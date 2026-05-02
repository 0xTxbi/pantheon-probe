use anyhow::{anyhow, Context, Result};
use clap::ValueEnum;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, ToSocketAddrs};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;

const CLOUDFLARE_UPLOAD_URL: &str = "https://speed.cloudflare.com/__up";

#[derive(Debug, Clone)]
pub struct ProbeOptions {
    pub target: String,
    pub profile: MeasurementProfile,
    pub samples: u32,
    pub bandwidth: BandwidthConfig,
}

#[derive(Debug, Clone)]
pub struct BandwidthConfig {
    pub provider: BandwidthProviderPreset,
    pub download_url: String,
    pub upload_url: String,
    pub download_size_bytes: usize,
    pub upload_size_bytes: usize,
    pub runs: u32,
    pub download_streams: u32,
    pub upload_streams: u32,
}

#[derive(Debug, Clone)]
pub struct ProbeOverrides {
    pub target: String,
    pub profile: MeasurementProfile,
    pub provider: BandwidthProviderPreset,
    pub samples: Option<u32>,
    pub download_url: Option<String>,
    pub upload_url: Option<String>,
    pub download_size_bytes: Option<usize>,
    pub upload_size_bytes: Option<usize>,
    pub bandwidth_runs: Option<u32>,
    pub download_streams: Option<u32>,
    pub upload_streams: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum MeasurementProfile {
    Quick,
    Standard,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum BandwidthProviderPreset {
    Cloudflare,
    Custom,
}

#[derive(Debug, Clone, Copy)]
struct ProfileDefaults {
    samples: u32,
    download_size_bytes: usize,
    upload_size_bytes: usize,
    bandwidth_runs: u32,
    download_streams: u32,
    upload_streams: u32,
}

impl MeasurementProfile {
    fn defaults(self) -> ProfileDefaults {
        match self {
            Self::Quick => ProfileDefaults {
                samples: 3,
                download_size_bytes: 2_000_000,
                upload_size_bytes: 500_000,
                bandwidth_runs: 1,
                download_streams: 1,
                upload_streams: 1,
            },
            Self::Standard => ProfileDefaults {
                samples: 5,
                download_size_bytes: 4_000_000,
                upload_size_bytes: 1_000_000,
                bandwidth_runs: 3,
                download_streams: 2,
                upload_streams: 2,
            },
            Self::Full => ProfileDefaults {
                samples: 7,
                download_size_bytes: 12_000_000,
                upload_size_bytes: 4_000_000,
                bandwidth_runs: 5,
                download_streams: 4,
                upload_streams: 4,
            },
        }
    }
}

impl fmt::Display for MeasurementProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Full => "full",
        })
    }
}

impl fmt::Display for BandwidthProviderPreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cloudflare => "cloudflare",
            Self::Custom => "custom",
        })
    }
}

pub fn resolve_probe_options(overrides: ProbeOverrides) -> Result<ProbeOptions> {
    let defaults = overrides.profile.defaults();
    let samples = overrides.samples.unwrap_or(defaults.samples).max(1);
    let download_size_bytes = overrides
        .download_size_bytes
        .unwrap_or(defaults.download_size_bytes)
        .max(1);
    let upload_size_bytes = overrides
        .upload_size_bytes
        .unwrap_or(defaults.upload_size_bytes)
        .max(1);
    let runs = overrides
        .bandwidth_runs
        .unwrap_or(defaults.bandwidth_runs)
        .max(1);
    let download_streams = overrides
        .download_streams
        .unwrap_or(defaults.download_streams)
        .max(1);
    let upload_streams = overrides
        .upload_streams
        .unwrap_or(defaults.upload_streams)
        .max(1);

    let has_download_override = overrides.download_url.is_some();
    let has_upload_override = overrides.upload_url.is_some();
    if has_download_override ^ has_upload_override {
        anyhow::bail!(
            "provide both --download-url and --upload-url when overriding bandwidth endpoints"
        );
    }

    let provider = if has_download_override {
        BandwidthProviderPreset::Custom
    } else {
        overrides.provider
    };

    let (download_url, upload_url) = match provider {
        BandwidthProviderPreset::Cloudflare => (
            build_cloudflare_download_url(download_size_bytes),
            CLOUDFLARE_UPLOAD_URL.to_string(),
        ),
        BandwidthProviderPreset::Custom => (
            overrides
                .download_url
                .ok_or_else(|| anyhow!("custom provider requires --download-url"))?,
            overrides
                .upload_url
                .ok_or_else(|| anyhow!("custom provider requires --upload-url"))?,
        ),
    };

    Ok(ProbeOptions {
        target: overrides.target,
        profile: overrides.profile,
        samples,
        bandwidth: BandwidthConfig {
            provider,
            download_url,
            upload_url,
            download_size_bytes,
            upload_size_bytes,
            runs,
            download_streams,
            upload_streams,
        },
    })
}

fn build_cloudflare_download_url(bytes: usize) -> String {
    format!("https://speed.cloudflare.com/__down?bytes={bytes}")
}

fn default_measurement_profile() -> MeasurementProfile {
    MeasurementProfile::Standard
}

fn default_bandwidth_provider_name() -> String {
    BandwidthProviderPreset::Cloudflare.to_string()
}

fn default_download_size_bytes() -> usize {
    MeasurementProfile::Standard.defaults().download_size_bytes
}

fn default_upload_size_bytes() -> usize {
    MeasurementProfile::Standard.defaults().upload_size_bytes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeReport {
    pub target: String,
    #[serde(default = "default_measurement_profile")]
    pub profile: MeasurementProfile,
    #[serde(default = "default_bandwidth_provider_name")]
    pub bandwidth_provider: String,
    pub samples: u32,
    pub created_at_unix_ms: u128,
    pub ping: ProbeOutcome<PingSummary>,
    pub dns: ProbeOutcome<DnsSummary>,
    pub bandwidth: ProbeOutcome<BandwidthSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOutcome<T> {
    pub value: Option<T>,
    pub error: Option<String>,
}

impl<T> ProbeOutcome<T> {
    fn success(value: T) -> Self {
        Self {
            value: Some(value),
            error: None,
        }
    }

    fn failure(error: anyhow::Error) -> Self {
        Self {
            value: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingSummary {
    pub sent: u32,
    pub received: u32,
    pub packet_loss_pct: f64,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub stddev_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub samples_ms: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsSummary {
    pub resolution_time_ms: f64,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthSummary {
    #[serde(default = "default_bandwidth_provider_name")]
    pub provider: String,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub download: MetricStats,
    pub upload: MetricStats,
    pub download_runs: Vec<TransferSample>,
    pub upload_runs: Vec<TransferSample>,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    #[serde(default = "default_download_size_bytes")]
    pub download_size_bytes: usize,
    #[serde(default = "default_upload_size_bytes")]
    pub upload_size_bytes: usize,
    pub runs: u32,
    pub download_streams: u32,
    pub upload_streams: u32,
    pub download_url: String,
    pub upload_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricStats {
    pub min: f64,
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
    pub stddev: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferSample {
    pub mbps: f64,
    pub bytes: u64,
    pub elapsed_ms: f64,
    pub streams: u32,
}

pub async fn run_probe_suite(options: &ProbeOptions) -> Result<ProbeReport> {
    let ping_result = measure_ping(&options.target, options.samples);
    let dns_result = measure_dns(&options.target);
    let bandwidth_result = measure_bandwidth(&options.bandwidth).await;

    Ok(ProbeReport {
        target: options.target.clone(),
        profile: options.profile,
        bandwidth_provider: options.bandwidth.provider.to_string(),
        samples: options.samples,
        created_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before unix epoch")?
            .as_millis(),
        ping: ping_result
            .map(ProbeOutcome::success)
            .unwrap_or_else(ProbeOutcome::failure),
        dns: dns_result
            .map(ProbeOutcome::success)
            .unwrap_or_else(ProbeOutcome::failure),
        bandwidth: bandwidth_result
            .map(ProbeOutcome::success)
            .unwrap_or_else(ProbeOutcome::failure),
    })
}

pub fn format_report(report: &ProbeReport) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "PantheonProbe v{} | target: {}\n",
        env!("CARGO_PKG_VERSION"),
        report.target
    ));
    output.push_str(&format!(
        "Profile: {} | Samples: {} | Bandwidth provider: {}\n\n",
        report.profile, report.samples, report.bandwidth_provider
    ));

    output.push_str("Ping\n");
    output.push_str(&format_outcome(&report.ping, |ping| {
        let latency = match (ping.min_ms, ping.avg_ms, ping.max_ms) {
            (Some(min), Some(avg), Some(max)) => {
                format!("min/avg/max: {:.2}/{:.2}/{:.2} ms", min, avg, max)
            }
            _ => "min/avg/max: unavailable".to_string(),
        };
        let jitter = ping
            .jitter_ms
            .map(|value| format!("{value:.2} ms"))
            .unwrap_or_else(|| "unavailable".to_string());

        [
            format!("  sent/received: {}/{}", ping.sent, ping.received),
            format!("  packet loss: {:.2}%", ping.packet_loss_pct),
            format!("  {latency}"),
            format!(
                "  median/p95/stddev: {}",
                format_optional_triplet(ping.median_ms, ping.p95_ms, ping.stddev_ms, "ms")
            ),
            format!("  jitter: {jitter}"),
        ]
        .join("\n")
    }));
    output.push('\n');
    output.push_str("\nDNS\n");
    output.push_str(&format_outcome(&report.dns, |dns| {
        [
            format!("  resolution time: {:.2} ms", dns.resolution_time_ms),
            format!("  addresses: {}", dns.addresses.join(", ")),
        ]
        .join("\n")
    }));
    output.push('\n');
    output.push_str("\nBandwidth\n");
    output.push_str(&format_outcome(&report.bandwidth, |bandwidth| {
        [
            format!(
                "  download: {:.2} Mbps median, {:.2} Mbps p95",
                bandwidth.download.median, bandwidth.download.p95
            ),
            format!(
                "  upload: {:.2} Mbps median, {:.2} Mbps p95",
                bandwidth.upload.median, bandwidth.upload.p95
            ),
            format!(
                "  runs/streams: {} runs, {} down streams, {} up streams",
                bandwidth.runs, bandwidth.download_streams, bandwidth.upload_streams
            ),
            format!(
                "  payload sizing: {} down bytes, {} up bytes",
                bandwidth.download_size_bytes, bandwidth.upload_size_bytes
            ),
            format!("  provider: {}", bandwidth.provider),
            format!("  download source: {}", bandwidth.download_url),
            format!("  upload source: {}", bandwidth.upload_url),
        ]
        .join("\n")
    }));
    output.push('\n');

    output
}

fn format_outcome<T>(outcome: &ProbeOutcome<T>, formatter: impl FnOnce(&T) -> String) -> String {
    match (&outcome.value, &outcome.error) {
        (Some(value), _) => formatter(value),
        (None, Some(error)) => format!("  error: {error}"),
        (None, None) => "  unavailable".to_string(),
    }
}

fn format_optional_triplet(
    first: Option<f64>,
    second: Option<f64>,
    third: Option<f64>,
    unit: &str,
) -> String {
    match (first, second, third) {
        (Some(first), Some(second), Some(third)) => {
            format!("{first:.2}/{second:.2}/{third:.2} {unit}")
        }
        _ => "unavailable".to_string(),
    }
}

fn measure_ping(target: &str, samples: u32) -> Result<PingSummary> {
    let sample_count = samples.max(1);
    let mut command = Command::new("ping");

    if cfg!(target_os = "windows") {
        command.args(["-n", &sample_count.to_string(), target]);
    } else {
        command.args(["-c", &sample_count.to_string(), target]);
    }

    let output = command
        .output()
        .with_context(|| format!("failed to execute ping against {target}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    parse_ping_output(&combined_output, sample_count)
}

fn parse_ping_output(output: &str, sent: u32) -> Result<PingSummary> {
    let samples_ms: Vec<f64> = output.lines().filter_map(extract_time_ms).collect();
    let received = samples_ms.len() as u32;
    let packet_loss_pct = if sent == 0 {
        0.0
    } else {
        ((sent - received) as f64 / sent as f64) * 100.0
    };

    if sent == 0 {
        return Err(anyhow!("ping sample count must be greater than zero"));
    }

    if received == 0 && !output.to_ascii_lowercase().contains("ttl") && output.trim().is_empty() {
        return Err(anyhow!("ping produced no parseable output"));
    }

    let (min_ms, avg_ms, median_ms, p95_ms, max_ms, stddev_ms, jitter_ms) = if samples_ms.is_empty()
    {
        (None, None, None, None, None, None, None)
    } else {
        let stats = calculate_stats(&samples_ms).context("failed to derive ping stats")?;
        let jitter_ms = calculate_jitter_ms(&samples_ms);

        (
            Some(stats.min),
            Some(stats.mean),
            Some(stats.median),
            Some(stats.p95),
            Some(stats.max),
            Some(stats.stddev),
            jitter_ms,
        )
    };

    Ok(PingSummary {
        sent,
        received,
        packet_loss_pct,
        min_ms,
        avg_ms,
        median_ms,
        p95_ms,
        max_ms,
        stddev_ms,
        jitter_ms,
        samples_ms,
    })
}

fn calculate_jitter_ms(samples_ms: &[f64]) -> Option<f64> {
    if samples_ms.len() < 2 {
        return None;
    }

    let jitter_total: f64 = samples_ms
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .sum();

    Some(jitter_total / (samples_ms.len() - 1) as f64)
}

fn extract_time_ms(line: &str) -> Option<f64> {
    extract_time_value(line, "time=").or_else(|| extract_time_value(line, "time<"))
}

fn extract_time_value(line: &str, marker: &str) -> Option<f64> {
    let start = line.find(marker)?;
    let mut digits = String::new();

    for character in line[start + marker.len()..].chars() {
        if character.is_ascii_digit() || character == '.' {
            digits.push(character);
        } else if !digits.is_empty() {
            break;
        } else if character == ' ' {
            continue;
        } else {
            return None;
        }
    }

    if digits.is_empty() {
        return None;
    }

    digits.parse::<f64>().ok()
}

fn measure_dns(target: &str) -> Result<DnsSummary> {
    let start = Instant::now();
    let addresses: Vec<IpAddr> = (target, 0)
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve {target}"))?
        .map(|addr| addr.ip())
        .collect();
    let elapsed = start.elapsed();

    if addresses.is_empty() {
        return Err(anyhow!("no IP addresses resolved for {target}"));
    }

    Ok(DnsSummary {
        resolution_time_ms: duration_to_ms(elapsed),
        addresses: addresses
            .into_iter()
            .map(|address| address.to_string())
            .collect(),
    })
}

async fn measure_bandwidth(config: &BandwidthConfig) -> Result<BandwidthSummary> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client for bandwidth probe")?;

    let runs = config.runs.max(1);
    let download_streams = config.download_streams.max(1);
    let upload_streams = config.upload_streams.max(1);
    let mut download_runs = Vec::with_capacity(runs as usize);
    let mut upload_runs = Vec::with_capacity(runs as usize);

    for _ in 0..runs {
        download_runs.push(
            download_sample(&client, &config.download_url, download_streams)
                .await
                .with_context(|| {
                    format!(
                        "download throughput check failed for {}",
                        config.download_url
                    )
                })?,
        );
        upload_runs.push(
            upload_sample(&client, config, upload_streams)
                .await
                .with_context(|| {
                    format!("upload throughput check failed for {}", config.upload_url)
                })?,
        );
    }

    let download_values: Vec<f64> = download_runs.iter().map(|sample| sample.mbps).collect();
    let upload_values: Vec<f64> = upload_runs.iter().map(|sample| sample.mbps).collect();
    let download = calculate_stats(&download_values).context("failed to derive download stats")?;
    let upload = calculate_stats(&upload_values).context("failed to derive upload stats")?;
    let download_bytes = download_runs.iter().map(|sample| sample.bytes).sum();
    let upload_bytes = upload_runs.iter().map(|sample| sample.bytes).sum();

    Ok(BandwidthSummary {
        provider: config.provider.to_string(),
        download_mbps: download.median,
        upload_mbps: upload.median,
        download,
        upload,
        download_runs,
        upload_runs,
        download_bytes,
        upload_bytes,
        download_size_bytes: config.download_size_bytes,
        upload_size_bytes: config.upload_size_bytes,
        runs,
        download_streams,
        upload_streams,
        download_url: config.download_url.clone(),
        upload_url: config.upload_url.clone(),
    })
}

async fn download_sample(client: &Client, url: &str, streams: u32) -> Result<TransferSample> {
    let started = Instant::now();
    let mut tasks = JoinSet::new();

    for _ in 0..streams {
        let client = client.clone();
        let url = url.to_string();
        tasks.spawn(async move { download_bytes(&client, &url).await });
    }

    let mut total_bytes = 0_u64;
    while let Some(result) = tasks.join_next().await {
        total_bytes += result.context("download worker failed to join")??;
    }

    let elapsed = started.elapsed();

    Ok(TransferSample {
        mbps: bytes_to_mbps(total_bytes, elapsed),
        bytes: total_bytes,
        elapsed_ms: duration_to_ms(elapsed),
        streams,
    })
}

async fn upload_sample(
    client: &Client,
    config: &BandwidthConfig,
    streams: u32,
) -> Result<TransferSample> {
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    let stream_payload_size = split_size(config.upload_size_bytes, streams);

    for _ in 0..streams {
        let client = client.clone();
        let upload_url = config.upload_url.clone();
        tasks.spawn(async move { upload_bytes(&client, &upload_url, stream_payload_size).await });
    }

    let mut total_bytes = 0_u64;
    while let Some(result) = tasks.join_next().await {
        total_bytes += result.context("upload worker failed to join")??;
    }

    let elapsed = started.elapsed();

    Ok(TransferSample {
        mbps: bytes_to_mbps(total_bytes, elapsed),
        bytes: total_bytes,
        elapsed_ms: duration_to_ms(elapsed),
        streams,
    })
}

async fn download_bytes(client: &Client, url: &str) -> Result<u64> {
    let mut response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to GET {url}"))?
        .error_for_status()
        .with_context(|| format!("download endpoint returned an error for {url}"))?;
    let mut total_bytes = 0_u64;

    while let Some(chunk) = response
        .chunk()
        .await
        .context("failed to stream download body")?
    {
        total_bytes += chunk.len() as u64;
    }

    Ok(total_bytes)
}

async fn upload_bytes(client: &Client, upload_url: &str, payload_size: usize) -> Result<u64> {
    let payload = vec![b'x'; payload_size];
    let payload_len = payload.len() as u64;

    client
        .post(upload_url)
        .body(payload)
        .send()
        .await
        .with_context(|| format!("failed to POST {upload_url}"))?
        .error_for_status()
        .with_context(|| format!("upload endpoint returned an error for {upload_url}"))?;

    Ok(payload_len)
}

fn bytes_to_mbps(bytes: u64, elapsed: Duration) -> f64 {
    if elapsed.is_zero() {
        return 0.0;
    }

    (bytes as f64 * 8.0) / elapsed.as_secs_f64() / 1_000_000.0
}

fn duration_to_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn calculate_stats(values: &[f64]) -> Option<MetricStats> {
    if values.is_empty() {
        return None;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);

    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let variance = sorted
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / sorted.len() as f64;

    Some(MetricStats {
        min: sorted[0],
        mean,
        median: percentile(&sorted, 50.0),
        p95: percentile(&sorted, 95.0),
        max: sorted[sorted.len() - 1],
        stddev: variance.sqrt(),
    })
}

fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.len() == 1 {
        return sorted_values[0];
    }

    let rank = (percentile / 100.0) * (sorted_values.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;

    if lower == upper {
        return sorted_values[lower];
    }

    let weight = rank - lower as f64;
    sorted_values[lower] * (1.0 - weight) + sorted_values[upper] * weight
}

fn split_size(total_size: usize, streams: u32) -> usize {
    let streams = streams.max(1) as usize;
    total_size.div_ceil(streams)
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_jitter_ms, calculate_stats, parse_ping_output, resolve_probe_options, split_size,
        BandwidthProviderPreset, MeasurementProfile, ProbeOverrides, CLOUDFLARE_UPLOAD_URL,
    };

    #[test]
    fn parses_unix_ping_output_into_structured_stats() {
        let output = "\
PING example.com (93.184.216.34): 56 data bytes
64 bytes from 93.184.216.34: icmp_seq=0 ttl=56 time=24.1 ms
64 bytes from 93.184.216.34: icmp_seq=1 ttl=56 time=21.4 ms
64 bytes from 93.184.216.34: icmp_seq=2 ttl=56 time=23.0 ms
";

        let parsed = parse_ping_output(output, 3).expect("ping output should parse");

        assert_eq!(parsed.sent, 3);
        assert_eq!(parsed.received, 3);
        assert!((parsed.packet_loss_pct - 0.0).abs() < f64::EPSILON);
        assert_eq!(parsed.samples_ms.len(), 3);
        assert_eq!(parsed.min_ms, Some(21.4));
        assert_eq!(parsed.max_ms, Some(24.1));
        assert!(parsed.avg_ms.expect("avg latency should exist") > 22.0);
        assert_eq!(parsed.median_ms, Some(23.0));
        assert!(parsed.p95_ms.expect("p95 latency should exist") > 23.0);
        assert!(parsed.jitter_ms.expect("jitter should exist") > 2.0);
    }

    #[test]
    fn parses_windows_ping_output_with_sub_millisecond_response() {
        let output = "\
Reply from 1.1.1.1: bytes=32 time<1ms TTL=59
Reply from 1.1.1.1: bytes=32 time=2ms TTL=59
";

        let parsed = parse_ping_output(output, 2).expect("windows ping should parse");

        assert_eq!(parsed.received, 2);
        assert_eq!(parsed.min_ms, Some(1.0));
        assert_eq!(parsed.max_ms, Some(2.0));
    }

    #[test]
    fn jitter_uses_absolute_latency_deltas() {
        let jitter = calculate_jitter_ms(&[10.0, 20.0, 15.0]).expect("jitter should exist");
        assert!((jitter - 7.5).abs() < f64::EPSILON);
    }

    #[test]
    fn calculates_distribution_stats() {
        let stats = calculate_stats(&[10.0, 20.0, 30.0, 40.0]).expect("stats should exist");

        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 40.0);
        assert_eq!(stats.mean, 25.0);
        assert_eq!(stats.median, 25.0);
        assert!((stats.p95 - 38.5).abs() < f64::EPSILON);
    }

    #[test]
    fn splits_upload_payload_across_streams() {
        assert_eq!(split_size(1_000_000, 2), 500_000);
        assert_eq!(split_size(1_000_001, 2), 500_001);
        assert_eq!(split_size(10, 0), 10);
    }

    #[test]
    fn resolves_standard_cloudflare_defaults() {
        let options = resolve_probe_options(ProbeOverrides {
            target: "1.1.1.1".to_string(),
            profile: MeasurementProfile::Standard,
            provider: BandwidthProviderPreset::Cloudflare,
            samples: None,
            download_url: None,
            upload_url: None,
            download_size_bytes: None,
            upload_size_bytes: None,
            bandwidth_runs: None,
            download_streams: None,
            upload_streams: None,
        })
        .expect("probe options should resolve");

        assert_eq!(options.profile, MeasurementProfile::Standard);
        assert_eq!(options.samples, 5);
        assert_eq!(
            options.bandwidth.provider,
            BandwidthProviderPreset::Cloudflare
        );
        assert_eq!(
            options.bandwidth.download_url,
            "https://speed.cloudflare.com/__down?bytes=4000000"
        );
        assert_eq!(options.bandwidth.upload_url, CLOUDFLARE_UPLOAD_URL);
    }

    #[test]
    fn explicit_urls_switch_to_custom_provider() {
        let options = resolve_probe_options(ProbeOverrides {
            target: "example.com".to_string(),
            profile: MeasurementProfile::Quick,
            provider: BandwidthProviderPreset::Cloudflare,
            samples: None,
            download_url: Some("https://downloads.example.test/file.bin".to_string()),
            upload_url: Some("https://uploads.example.test".to_string()),
            download_size_bytes: Some(9_000_000),
            upload_size_bytes: Some(2_000_000),
            bandwidth_runs: Some(2),
            download_streams: Some(3),
            upload_streams: Some(2),
        })
        .expect("probe options should resolve");

        assert_eq!(options.bandwidth.provider, BandwidthProviderPreset::Custom);
        assert_eq!(
            options.bandwidth.download_url,
            "https://downloads.example.test/file.bin"
        );
        assert_eq!(options.bandwidth.upload_url, "https://uploads.example.test");
    }

    #[test]
    fn custom_provider_requires_both_urls() {
        let error = resolve_probe_options(ProbeOverrides {
            target: "example.com".to_string(),
            profile: MeasurementProfile::Standard,
            provider: BandwidthProviderPreset::Custom,
            samples: None,
            download_url: Some("https://downloads.example.test/file.bin".to_string()),
            upload_url: None,
            download_size_bytes: None,
            upload_size_bytes: None,
            bandwidth_runs: None,
            download_streams: None,
            upload_streams: None,
        })
        .expect_err("custom provider should require both urls");

        assert!(error.to_string().contains("--upload-url"));
    }
}
