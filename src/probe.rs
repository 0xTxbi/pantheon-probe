use anyhow::{anyhow, Context, Result};
use clap::ValueEnum;
use reqwest::{header::RANGE, Client};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, ToSocketAddrs};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;

const CLOUDFLARE_UPLOAD_URL: &str = "https://speed.cloudflare.com/__up";
const DEFAULT_ENDPOINT_NAME: &str = "global";

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
    pub endpoint: Option<String>,
    pub endpoints: Vec<BandwidthEndpoint>,
    pub download_size_bytes: usize,
    pub upload_size_bytes: usize,
    pub runs: u32,
    pub warmup_runs: u32,
    pub transfer_attempts: u32,
    pub download_streams: u32,
    pub upload_streams: u32,
    pub target_transfer_duration_ms: u64,
    pub max_download_size_bytes: usize,
    pub max_upload_size_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ProbeOverrides {
    pub target: String,
    pub profile: MeasurementProfile,
    pub provider: BandwidthProviderPreset,
    pub endpoint: Option<String>,
    pub samples: Option<u32>,
    pub download_urls: Vec<String>,
    pub upload_urls: Vec<String>,
    pub download_size_bytes: Option<usize>,
    pub upload_size_bytes: Option<usize>,
    pub bandwidth_runs: Option<u32>,
    pub bandwidth_warmup_runs: Option<u32>,
    pub transfer_attempts: Option<u32>,
    pub download_streams: Option<u32>,
    pub upload_streams: Option<u32>,
    pub target_transfer_duration_ms: Option<u64>,
    pub max_download_size_bytes: Option<usize>,
    pub max_upload_size_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthEndpoint {
    pub name: String,
    pub download_url: String,
    pub upload_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCatalogEntry {
    pub provider: BandwidthProviderPreset,
    pub endpoints: Vec<String>,
    pub profiles: Vec<MeasurementProfile>,
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
    bandwidth_warmup_runs: u32,
    transfer_attempts: u32,
    download_streams: u32,
    upload_streams: u32,
    target_transfer_duration_ms: u64,
    max_download_size_bytes: usize,
    max_upload_size_bytes: usize,
}

impl MeasurementProfile {
    fn defaults(self) -> ProfileDefaults {
        match self {
            Self::Quick => ProfileDefaults {
                samples: 3,
                download_size_bytes: 2_000_000,
                upload_size_bytes: 500_000,
                bandwidth_runs: 1,
                bandwidth_warmup_runs: 1,
                transfer_attempts: 2,
                download_streams: 1,
                upload_streams: 1,
                target_transfer_duration_ms: 1_000,
                max_download_size_bytes: 8_000_000,
                max_upload_size_bytes: 2_000_000,
            },
            Self::Standard => ProfileDefaults {
                samples: 5,
                download_size_bytes: 4_000_000,
                upload_size_bytes: 1_000_000,
                bandwidth_runs: 3,
                bandwidth_warmup_runs: 1,
                transfer_attempts: 2,
                download_streams: 2,
                upload_streams: 2,
                target_transfer_duration_ms: 2_500,
                max_download_size_bytes: 32_000_000,
                max_upload_size_bytes: 8_000_000,
            },
            Self::Full => ProfileDefaults {
                samples: 7,
                download_size_bytes: 12_000_000,
                upload_size_bytes: 4_000_000,
                bandwidth_runs: 5,
                bandwidth_warmup_runs: 2,
                transfer_attempts: 3,
                download_streams: 4,
                upload_streams: 4,
                target_transfer_duration_ms: 4_000,
                max_download_size_bytes: 128_000_000,
                max_upload_size_bytes: 32_000_000,
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

pub fn provider_catalog() -> Vec<ProviderCatalogEntry> {
    vec![
        ProviderCatalogEntry {
            provider: BandwidthProviderPreset::Cloudflare,
            endpoints: cloudflare_endpoints(
                MeasurementProfile::Standard.defaults().download_size_bytes,
            )
            .into_iter()
            .map(|endpoint| endpoint.name)
            .collect(),
            profiles: vec![
                MeasurementProfile::Quick,
                MeasurementProfile::Standard,
                MeasurementProfile::Full,
            ],
        },
        ProviderCatalogEntry {
            provider: BandwidthProviderPreset::Custom,
            endpoints: vec!["custom-1".to_string()],
            profiles: vec![
                MeasurementProfile::Quick,
                MeasurementProfile::Standard,
                MeasurementProfile::Full,
            ],
        },
    ]
}

pub fn format_provider_catalog(entries: &[ProviderCatalogEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let endpoints = entry.endpoints.join(", ");
            let profiles = entry
                .profiles
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");

            format!(
                "{} | endpoints: {} | profiles: {}",
                entry.provider, endpoints, profiles
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    let warmup_runs = overrides
        .bandwidth_warmup_runs
        .unwrap_or(defaults.bandwidth_warmup_runs);
    let transfer_attempts = overrides
        .transfer_attempts
        .unwrap_or(defaults.transfer_attempts)
        .max(1);
    let download_streams = overrides
        .download_streams
        .unwrap_or(defaults.download_streams)
        .max(1);
    let upload_streams = overrides
        .upload_streams
        .unwrap_or(defaults.upload_streams)
        .max(1);
    let target_transfer_duration_ms = overrides
        .target_transfer_duration_ms
        .unwrap_or(defaults.target_transfer_duration_ms)
        .max(1);
    let max_download_size_bytes = overrides
        .max_download_size_bytes
        .unwrap_or(defaults.max_download_size_bytes)
        .max(download_size_bytes);
    let max_upload_size_bytes = overrides
        .max_upload_size_bytes
        .unwrap_or(defaults.max_upload_size_bytes)
        .max(upload_size_bytes);

    let has_download_overrides = !overrides.download_urls.is_empty();
    let has_upload_overrides = !overrides.upload_urls.is_empty();
    if has_download_overrides ^ has_upload_overrides {
        anyhow::bail!(
            "provide both --download-url and --upload-url when overriding bandwidth endpoints"
        );
    }

    if has_download_overrides && overrides.download_urls.len() != overrides.upload_urls.len() {
        anyhow::bail!("provide the same number of --download-url and --upload-url values");
    }

    let provider = if has_download_overrides {
        BandwidthProviderPreset::Custom
    } else {
        overrides.provider
    };

    let endpoints = match provider {
        BandwidthProviderPreset::Cloudflare => cloudflare_endpoints(download_size_bytes),
        BandwidthProviderPreset::Custom => {
            custom_endpoints(&overrides.download_urls, &overrides.upload_urls)?
        }
    };

    Ok(ProbeOptions {
        target: overrides.target,
        profile: overrides.profile,
        samples,
        bandwidth: BandwidthConfig {
            provider,
            endpoint: overrides.endpoint,
            endpoints,
            download_size_bytes,
            upload_size_bytes,
            runs,
            warmup_runs,
            transfer_attempts,
            download_streams,
            upload_streams,
            target_transfer_duration_ms,
            max_download_size_bytes,
            max_upload_size_bytes,
        },
    })
}

fn build_cloudflare_download_url(bytes: usize) -> String {
    format!("https://speed.cloudflare.com/__down?bytes={bytes}")
}

fn cloudflare_endpoints(download_size_bytes: usize) -> Vec<BandwidthEndpoint> {
    vec![BandwidthEndpoint {
        name: DEFAULT_ENDPOINT_NAME.to_string(),
        download_url: build_cloudflare_download_url(download_size_bytes),
        upload_url: CLOUDFLARE_UPLOAD_URL.to_string(),
    }]
}

fn custom_endpoints(
    download_urls: &[String],
    upload_urls: &[String],
) -> Result<Vec<BandwidthEndpoint>> {
    if download_urls.is_empty() {
        anyhow::bail!("custom provider requires --download-url");
    }

    Ok(download_urls
        .iter()
        .zip(upload_urls.iter())
        .enumerate()
        .map(|(index, (download_url, upload_url))| BandwidthEndpoint {
            name: format!("custom-{}", index + 1),
            download_url: download_url.clone(),
            upload_url: upload_url.clone(),
        })
        .collect())
}

fn default_measurement_profile() -> MeasurementProfile {
    MeasurementProfile::Standard
}

fn default_bandwidth_provider_name() -> String {
    BandwidthProviderPreset::Cloudflare.to_string()
}

fn default_endpoint_name() -> String {
    DEFAULT_ENDPOINT_NAME.to_string()
}

fn default_download_size_bytes() -> usize {
    MeasurementProfile::Standard.defaults().download_size_bytes
}

fn default_upload_size_bytes() -> usize {
    MeasurementProfile::Standard.defaults().upload_size_bytes
}

fn default_target_transfer_duration_ms() -> u64 {
    MeasurementProfile::Standard
        .defaults()
        .target_transfer_duration_ms
}

fn default_transfer_attempts() -> u32 {
    1
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
    #[serde(default = "default_endpoint_name")]
    pub endpoint: String,
    #[serde(default)]
    pub endpoint_latency_ms: Option<f64>,
    #[serde(default)]
    pub endpoint_candidates: Vec<EndpointHealth>,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub download: MetricStats,
    pub upload: MetricStats,
    pub download_runs: Vec<TransferSample>,
    pub upload_runs: Vec<TransferSample>,
    #[serde(default)]
    pub warmup_download_runs: Vec<TransferSample>,
    #[serde(default)]
    pub warmup_upload_runs: Vec<TransferSample>,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    #[serde(default = "default_download_size_bytes")]
    pub download_size_bytes: usize,
    #[serde(default = "default_upload_size_bytes")]
    pub upload_size_bytes: usize,
    #[serde(default = "default_download_size_bytes")]
    pub calibrated_download_size_bytes: usize,
    #[serde(default = "default_upload_size_bytes")]
    pub calibrated_upload_size_bytes: usize,
    #[serde(default = "default_target_transfer_duration_ms")]
    pub target_transfer_duration_ms: u64,
    #[serde(default)]
    pub bandwidth_elapsed_ms: f64,
    pub runs: u32,
    #[serde(default)]
    pub warmup_runs: u32,
    #[serde(default = "default_transfer_attempts")]
    pub transfer_attempts: u32,
    pub download_streams: u32,
    pub upload_streams: u32,
    pub download_url: String,
    pub upload_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointHealth {
    pub name: String,
    pub download_url: String,
    pub upload_url: String,
    pub latency_ms: Option<f64>,
    pub error: Option<String>,
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
    #[serde(default)]
    pub target_bytes: usize,
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
                "  download: {:.2} Mbps median, {:.2} Mbps p95, {:.2} Mbps stddev",
                bandwidth.download.median, bandwidth.download.p95, bandwidth.download.stddev
            ),
            format!(
                "  upload: {:.2} Mbps median, {:.2} Mbps p95, {:.2} Mbps stddev",
                bandwidth.upload.median, bandwidth.upload.p95, bandwidth.upload.stddev
            ),
            format!(
                "  runs/streams: {} warmup, {} measured, {} attempts, {} down streams, {} up streams",
                bandwidth.warmup_runs,
                bandwidth.runs,
                bandwidth.transfer_attempts,
                bandwidth.download_streams,
                bandwidth.upload_streams
            ),
            format!(
                "  configured sizing: {} down bytes, {} up bytes",
                bandwidth.download_size_bytes, bandwidth.upload_size_bytes
            ),
            format!(
                "  calibrated sizing: {} down bytes, {} up bytes, target {} ms",
                bandwidth.calibrated_download_size_bytes,
                bandwidth.calibrated_upload_size_bytes,
                bandwidth.target_transfer_duration_ms
            ),
            format!(
                "  bandwidth elapsed: {}",
                format_optional_value(Some(bandwidth.bandwidth_elapsed_ms), "ms")
            ),
            format!(
                "  provider/endpoint: {}/{}",
                bandwidth.provider, bandwidth.endpoint
            ),
            format!(
                "  endpoint latency: {}",
                format_optional_value(bandwidth.endpoint_latency_ms, "ms")
            ),
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

fn format_optional_value(value: Option<f64>, unit: &str) -> String {
    value
        .map(|value| format!("{value:.2} {unit}"))
        .unwrap_or_else(|| "unavailable".to_string())
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

    let selected = select_bandwidth_endpoint(&client, config).await?;
    let bandwidth_started = Instant::now();
    let runs = config.runs.max(1);
    let download_streams = config.download_streams.max(1);
    let upload_streams = config.upload_streams.max(1);
    let mut download_runs = Vec::with_capacity(runs as usize);
    let mut upload_runs = Vec::with_capacity(runs as usize);
    let mut warmup_download_runs = Vec::with_capacity(config.warmup_runs as usize);
    let mut warmup_upload_runs = Vec::with_capacity(config.warmup_runs as usize);

    for _ in 0..config.warmup_runs {
        warmup_download_runs.push(
            download_sample_with_retries(
                &client,
                &selected.endpoint.download_url,
                config.download_size_bytes,
                download_streams,
                config.transfer_attempts,
            )
            .await
            .with_context(|| {
                format!(
                    "download warmup failed for {}",
                    selected.endpoint.download_url
                )
            })?,
        );
        warmup_upload_runs.push(
            upload_sample_with_retries(
                &client,
                &selected.endpoint.upload_url,
                config.upload_size_bytes,
                upload_streams,
                config.transfer_attempts,
            )
            .await
            .with_context(|| {
                format!("upload warmup failed for {}", selected.endpoint.upload_url)
            })?,
        );
    }

    let calibrated_download_size_bytes = calibrate_transfer_size(
        &warmup_download_runs,
        config.download_size_bytes,
        config.max_download_size_bytes,
        config.target_transfer_duration_ms,
    );
    let calibrated_upload_size_bytes = calibrate_transfer_size(
        &warmup_upload_runs,
        config.upload_size_bytes,
        config.max_upload_size_bytes,
        config.target_transfer_duration_ms,
    );

    for _ in 0..runs {
        download_runs.push(
            download_sample_with_retries(
                &client,
                &selected.endpoint.download_url,
                calibrated_download_size_bytes,
                download_streams,
                config.transfer_attempts,
            )
            .await
            .with_context(|| {
                format!(
                    "download throughput check failed for {}",
                    selected.endpoint.download_url
                )
            })?,
        );
        upload_runs.push(
            upload_sample_with_retries(
                &client,
                &selected.endpoint.upload_url,
                calibrated_upload_size_bytes,
                upload_streams,
                config.transfer_attempts,
            )
            .await
            .with_context(|| {
                format!(
                    "upload throughput check failed for {}",
                    selected.endpoint.upload_url
                )
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
        endpoint: selected.endpoint.name,
        endpoint_latency_ms: selected.latency_ms,
        endpoint_candidates: selected.candidates,
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
        calibrated_download_size_bytes,
        calibrated_upload_size_bytes,
        target_transfer_duration_ms: config.target_transfer_duration_ms,
        bandwidth_elapsed_ms: duration_to_ms(bandwidth_started.elapsed()),
        warmup_download_runs,
        warmup_upload_runs,
        runs,
        warmup_runs: config.warmup_runs,
        transfer_attempts: config.transfer_attempts,
        download_streams,
        upload_streams,
        download_url: sized_download_url(
            &selected.endpoint.download_url,
            calibrated_download_size_bytes,
        ),
        upload_url: selected.endpoint.upload_url,
    })
}

struct SelectedEndpoint {
    endpoint: BandwidthEndpoint,
    latency_ms: Option<f64>,
    candidates: Vec<EndpointHealth>,
}

async fn select_bandwidth_endpoint(
    client: &Client,
    config: &BandwidthConfig,
) -> Result<SelectedEndpoint> {
    let requested_endpoint = config.endpoint.as_deref();
    let candidates = match requested_endpoint {
        Some(endpoint) => config
            .endpoints
            .iter()
            .filter(|candidate| candidate.name == endpoint)
            .cloned()
            .collect::<Vec<_>>(),
        None => config.endpoints.clone(),
    };

    if candidates.is_empty() {
        let requested = requested_endpoint.unwrap_or("auto");
        let available = format_endpoint_names(&config.endpoints);
        anyhow::bail!(
            "no bandwidth endpoints are available for provider {} and endpoint {}; available endpoints: {}",
            config.provider,
            requested,
            available
        );
    }

    let mut health = Vec::with_capacity(candidates.len());
    for endpoint in &candidates {
        health.push(check_endpoint_health(client, endpoint).await);
    }

    let selected = health
        .iter()
        .filter_map(|candidate| candidate.latency_ms.map(|latency| (candidate, latency)))
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(candidate, latency)| (candidate.name.clone(), latency));

    let (endpoint_name, latency_ms) = selected.ok_or_else(|| {
        let errors = format_health_errors(&health);
        anyhow!("all bandwidth endpoint health checks failed: {errors}")
    })?;

    let endpoint = candidates
        .into_iter()
        .find(|candidate| candidate.name == endpoint_name)
        .expect("selected endpoint exists in candidate list");

    Ok(SelectedEndpoint {
        endpoint,
        latency_ms: Some(latency_ms),
        candidates: health,
    })
}

fn format_endpoint_names(endpoints: &[BandwidthEndpoint]) -> String {
    if endpoints.is_empty() {
        return "none".to_string();
    }

    endpoints
        .iter()
        .map(|endpoint| endpoint.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_health_errors(health: &[EndpointHealth]) -> String {
    let errors = health
        .iter()
        .filter_map(|candidate| {
            candidate
                .error
                .as_ref()
                .map(|error| format!("{}: {}", candidate.name, error))
        })
        .collect::<Vec<_>>()
        .join("; ");

    if errors.is_empty() {
        "no endpoint health results were collected".to_string()
    } else {
        errors
    }
}

fn calibrate_transfer_size(
    warmup_runs: &[TransferSample],
    minimum_bytes: usize,
    maximum_bytes: usize,
    target_duration_ms: u64,
) -> usize {
    let minimum_bytes = minimum_bytes.max(1);
    let maximum_bytes = maximum_bytes.max(minimum_bytes);
    let median_mbps = median_sample_mbps(warmup_runs);
    let target_bytes = median_mbps
        .map(|mbps| {
            let target_seconds = target_duration_ms.max(1) as f64 / 1_000.0;
            ((mbps * 1_000_000.0 / 8.0) * target_seconds).round() as usize
        })
        .unwrap_or(minimum_bytes);

    target_bytes.clamp(minimum_bytes, maximum_bytes)
}

fn median_sample_mbps(samples: &[TransferSample]) -> Option<f64> {
    let values = samples
        .iter()
        .filter(|sample| sample.mbps.is_finite() && sample.mbps > 0.0)
        .map(|sample| sample.mbps)
        .collect::<Vec<_>>();

    calculate_stats(&values).map(|stats| stats.median)
}

fn sized_download_url(download_url: &str, target_bytes: usize) -> String {
    let Some(bytes_position) = download_url.find("bytes=") else {
        return download_url.to_string();
    };
    let value_start = bytes_position + "bytes=".len();
    let value_end = download_url[value_start..]
        .find('&')
        .map(|offset| value_start + offset)
        .unwrap_or(download_url.len());

    format!(
        "{}{}{}",
        &download_url[..value_start],
        target_bytes,
        &download_url[value_end..]
    )
}

async fn check_endpoint_health(client: &Client, endpoint: &BandwidthEndpoint) -> EndpointHealth {
    let started = Instant::now();
    let result = client
        .get(&endpoint.download_url)
        .header(RANGE, "bytes=0-1023")
        .send()
        .await
        .with_context(|| format!("failed to reach {}", endpoint.download_url))
        .and_then(|response| {
            response.error_for_status().map(|_| ()).with_context(|| {
                format!(
                    "health check returned an error for {}",
                    endpoint.download_url
                )
            })
        });

    match result {
        Ok(()) => EndpointHealth {
            name: endpoint.name.clone(),
            download_url: endpoint.download_url.clone(),
            upload_url: endpoint.upload_url.clone(),
            latency_ms: Some(duration_to_ms(started.elapsed())),
            error: None,
        },
        Err(error) => EndpointHealth {
            name: endpoint.name.clone(),
            download_url: endpoint.download_url.clone(),
            upload_url: endpoint.upload_url.clone(),
            latency_ms: None,
            error: Some(error.to_string()),
        },
    }
}

async fn download_sample_with_retries(
    client: &Client,
    url: &str,
    target_bytes: usize,
    streams: u32,
    attempts: u32,
) -> Result<TransferSample> {
    let mut last_error = None;
    for attempt in 1..=attempts.max(1) {
        match download_sample(client, url, target_bytes, streams).await {
            Ok(sample) => return Ok(sample),
            Err(error) => {
                last_error = Some(error);
                if attempt < attempts {
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
            }
        }
    }

    Err(last_error.expect("at least one transfer attempt runs"))
}

async fn upload_sample_with_retries(
    client: &Client,
    upload_url: &str,
    upload_size_bytes: usize,
    streams: u32,
    attempts: u32,
) -> Result<TransferSample> {
    let mut last_error = None;
    for attempt in 1..=attempts.max(1) {
        match upload_sample(client, upload_url, upload_size_bytes, streams).await {
            Ok(sample) => return Ok(sample),
            Err(error) => {
                last_error = Some(error);
                if attempt < attempts {
                    tokio::time::sleep(retry_delay(attempt)).await;
                }
            }
        }
    }

    Err(last_error.expect("at least one transfer attempt runs"))
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_millis(150 * u64::from(attempt))
}

async fn download_sample(
    client: &Client,
    url: &str,
    target_bytes: usize,
    streams: u32,
) -> Result<TransferSample> {
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    let download_url = sized_download_url(url, target_bytes);

    for _ in 0..streams {
        let client = client.clone();
        let url = download_url.clone();
        tasks.spawn(async move { download_bytes(&client, &url).await });
    }

    let mut total_bytes = 0_u64;
    while let Some(result) = tasks.join_next().await {
        total_bytes += result.context("download worker failed to join")??;
    }

    let elapsed = started.elapsed();

    Ok(TransferSample {
        target_bytes,
        mbps: bytes_to_mbps(total_bytes, elapsed),
        bytes: total_bytes,
        elapsed_ms: duration_to_ms(elapsed),
        streams,
    })
}

async fn upload_sample(
    client: &Client,
    upload_url: &str,
    upload_size_bytes: usize,
    streams: u32,
) -> Result<TransferSample> {
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    let stream_payload_size = split_size(upload_size_bytes, streams);

    for _ in 0..streams {
        let client = client.clone();
        let upload_url = upload_url.to_string();
        tasks.spawn(async move { upload_bytes(&client, &upload_url, stream_payload_size).await });
    }

    let mut total_bytes = 0_u64;
    while let Some(result) = tasks.join_next().await {
        total_bytes += result.context("upload worker failed to join")??;
    }

    let elapsed = started.elapsed();

    Ok(TransferSample {
        target_bytes: upload_size_bytes,
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
        calculate_jitter_ms, calculate_stats, calibrate_transfer_size, format_provider_catalog,
        parse_ping_output, provider_catalog, resolve_probe_options, select_bandwidth_endpoint,
        sized_download_url, split_size, BandwidthConfig, BandwidthEndpoint,
        BandwidthProviderPreset, MeasurementProfile, ProbeOverrides, ProbeReport, TransferSample,
        CLOUDFLARE_UPLOAD_URL,
    };
    use reqwest::Client;

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
    fn calibrates_transfer_size_from_warmup_median() {
        let warmup = vec![
            TransferSample {
                target_bytes: 1_000,
                mbps: 80.0,
                bytes: 10_000_000,
                elapsed_ms: 1_000.0,
                streams: 1,
            },
            TransferSample {
                target_bytes: 1_000,
                mbps: 120.0,
                bytes: 15_000_000,
                elapsed_ms: 1_000.0,
                streams: 1,
            },
        ];

        assert_eq!(
            calibrate_transfer_size(&warmup, 1_000_000, 20_000_000, 2_000),
            20_000_000
        );
        assert_eq!(
            calibrate_transfer_size(&[], 1_000_000, 20_000_000, 2_000),
            1_000_000
        );
    }

    #[test]
    fn rewrites_cloudflare_download_size_without_touching_custom_urls() {
        assert_eq!(
            sized_download_url(
                "https://speed.cloudflare.com/__down?bytes=4000000",
                8_000_000
            ),
            "https://speed.cloudflare.com/__down?bytes=8000000"
        );
        assert_eq!(
            sized_download_url("https://downloads.example.test/file.bin", 8_000_000),
            "https://downloads.example.test/file.bin"
        );
    }

    #[test]
    fn resolves_standard_cloudflare_defaults() {
        let options = resolve_probe_options(ProbeOverrides {
            target: "1.1.1.1".to_string(),
            profile: MeasurementProfile::Standard,
            provider: BandwidthProviderPreset::Cloudflare,
            endpoint: None,
            samples: None,
            download_urls: Vec::new(),
            upload_urls: Vec::new(),
            download_size_bytes: None,
            upload_size_bytes: None,
            bandwidth_runs: None,
            bandwidth_warmup_runs: None,
            transfer_attempts: None,
            download_streams: None,
            upload_streams: None,
            target_transfer_duration_ms: None,
            max_download_size_bytes: None,
            max_upload_size_bytes: None,
        })
        .expect("probe options should resolve");

        assert_eq!(options.profile, MeasurementProfile::Standard);
        assert_eq!(options.samples, 5);
        assert_eq!(options.bandwidth.warmup_runs, 1);
        assert_eq!(options.bandwidth.transfer_attempts, 2);
        assert_eq!(options.bandwidth.target_transfer_duration_ms, 2_500);
        assert_eq!(options.bandwidth.max_download_size_bytes, 32_000_000);
        assert_eq!(options.bandwidth.max_upload_size_bytes, 8_000_000);
        assert_eq!(
            options.bandwidth.provider,
            BandwidthProviderPreset::Cloudflare
        );
        assert_eq!(
            options.bandwidth.endpoints[0].download_url,
            "https://speed.cloudflare.com/__down?bytes=4000000"
        );
        assert_eq!(
            options.bandwidth.endpoints[0].upload_url,
            CLOUDFLARE_UPLOAD_URL
        );
    }

    #[test]
    fn explicit_urls_switch_to_custom_provider() {
        let options = resolve_probe_options(ProbeOverrides {
            target: "example.com".to_string(),
            profile: MeasurementProfile::Quick,
            provider: BandwidthProviderPreset::Cloudflare,
            endpoint: None,
            samples: None,
            download_urls: vec!["https://downloads.example.test/file.bin".to_string()],
            upload_urls: vec!["https://uploads.example.test".to_string()],
            download_size_bytes: Some(9_000_000),
            upload_size_bytes: Some(2_000_000),
            bandwidth_runs: Some(2),
            bandwidth_warmup_runs: Some(0),
            transfer_attempts: Some(4),
            download_streams: Some(3),
            upload_streams: Some(2),
            target_transfer_duration_ms: Some(1_500),
            max_download_size_bytes: Some(12_000_000),
            max_upload_size_bytes: Some(4_000_000),
        })
        .expect("probe options should resolve");

        assert_eq!(options.bandwidth.provider, BandwidthProviderPreset::Custom);
        assert_eq!(
            options.bandwidth.endpoints[0].download_url,
            "https://downloads.example.test/file.bin"
        );
        assert_eq!(
            options.bandwidth.endpoints[0].upload_url,
            "https://uploads.example.test"
        );
    }

    #[test]
    fn custom_provider_accepts_multiple_endpoint_candidates() {
        let options = resolve_probe_options(ProbeOverrides {
            target: "example.com".to_string(),
            profile: MeasurementProfile::Quick,
            provider: BandwidthProviderPreset::Custom,
            endpoint: Some("custom-2".to_string()),
            samples: None,
            download_urls: vec![
                "https://downloads.example.test/a.bin".to_string(),
                "https://downloads.example.test/b.bin".to_string(),
            ],
            upload_urls: vec![
                "https://uploads.example.test/a".to_string(),
                "https://uploads.example.test/b".to_string(),
            ],
            download_size_bytes: None,
            upload_size_bytes: None,
            bandwidth_runs: None,
            bandwidth_warmup_runs: None,
            transfer_attempts: None,
            download_streams: None,
            upload_streams: None,
            target_transfer_duration_ms: None,
            max_download_size_bytes: None,
            max_upload_size_bytes: None,
        })
        .expect("probe options should resolve");

        assert_eq!(options.bandwidth.endpoint.as_deref(), Some("custom-2"));
        assert_eq!(options.bandwidth.endpoints.len(), 2);
        assert_eq!(options.bandwidth.endpoints[1].name, "custom-2");
    }

    #[tokio::test]
    async fn requested_endpoint_error_lists_available_candidates() {
        let client = Client::new();
        let config = BandwidthConfig {
            provider: BandwidthProviderPreset::Custom,
            endpoint: Some("custom-3".to_string()),
            endpoints: vec![
                BandwidthEndpoint {
                    name: "custom-1".to_string(),
                    download_url: "https://downloads.example.test/a.bin".to_string(),
                    upload_url: "https://uploads.example.test/a".to_string(),
                },
                BandwidthEndpoint {
                    name: "custom-2".to_string(),
                    download_url: "https://downloads.example.test/b.bin".to_string(),
                    upload_url: "https://uploads.example.test/b".to_string(),
                },
            ],
            download_size_bytes: 1,
            upload_size_bytes: 1,
            runs: 1,
            warmup_runs: 0,
            transfer_attempts: 1,
            download_streams: 1,
            upload_streams: 1,
            target_transfer_duration_ms: 1_000,
            max_download_size_bytes: 1,
            max_upload_size_bytes: 1,
        };

        let error = match select_bandwidth_endpoint(&client, &config).await {
            Ok(_) => panic!("unknown endpoint should fail before probing"),
            Err(error) => error,
        };
        let message = error.to_string();

        assert!(message.contains("endpoint custom-3"));
        assert!(message.contains("available endpoints: custom-1, custom-2"));
    }

    #[test]
    fn saved_runs_without_endpoint_metadata_use_defaults() {
        let report: ProbeReport = serde_json::from_str(
            r#"{
                "target": "1.1.1.1",
                "samples": 5,
                "created_at_unix_ms": 1777210123095,
                "ping": { "value": null, "error": "skipped" },
                "dns": { "value": null, "error": "skipped" },
                "bandwidth": {
                    "value": {
                        "download_mbps": 42.0,
                        "upload_mbps": 10.0,
                        "download": {
                            "min": 42.0,
                            "mean": 42.0,
                            "median": 42.0,
                            "p95": 42.0,
                            "max": 42.0,
                            "stddev": 0.0
                        },
                        "upload": {
                            "min": 10.0,
                            "mean": 10.0,
                            "median": 10.0,
                            "p95": 10.0,
                            "max": 10.0,
                            "stddev": 0.0
                        },
                        "download_runs": [],
                        "upload_runs": [],
                        "download_bytes": 4000000,
                        "upload_bytes": 1000000,
                        "runs": 1,
                        "download_streams": 1,
                        "upload_streams": 1,
                        "download_url": "https://speed.cloudflare.com/__down?bytes=4000000",
                        "upload_url": "https://speed.cloudflare.com/__up"
                    },
                    "error": null
                }
            }"#,
        )
        .expect("old saved run should deserialize");

        let bandwidth = report
            .bandwidth
            .value
            .expect("bandwidth result should exist");

        assert_eq!(report.profile, MeasurementProfile::Standard);
        assert_eq!(report.bandwidth_provider, "cloudflare");
        assert_eq!(bandwidth.provider, "cloudflare");
        assert_eq!(bandwidth.endpoint, "global");
        assert_eq!(bandwidth.endpoint_latency_ms, None);
        assert!(bandwidth.endpoint_candidates.is_empty());
        assert!(bandwidth.warmup_download_runs.is_empty());
        assert!(bandwidth.warmup_upload_runs.is_empty());
        assert_eq!(bandwidth.download_size_bytes, 4_000_000);
        assert_eq!(bandwidth.upload_size_bytes, 1_000_000);
        assert_eq!(bandwidth.calibrated_download_size_bytes, 4_000_000);
        assert_eq!(bandwidth.calibrated_upload_size_bytes, 1_000_000);
        assert_eq!(bandwidth.target_transfer_duration_ms, 2_500);
        assert_eq!(bandwidth.bandwidth_elapsed_ms, 0.0);
        assert_eq!(bandwidth.warmup_runs, 0);
        assert_eq!(bandwidth.transfer_attempts, 1);
    }

    #[test]
    fn formats_provider_catalog() {
        let formatted = format_provider_catalog(&provider_catalog());

        assert!(formatted.contains("cloudflare | endpoints: global"));
        assert!(formatted.contains("custom | endpoints: custom-1"));
        assert!(formatted.contains("profiles: quick, standard, full"));
    }

    #[test]
    fn custom_provider_requires_both_urls() {
        let error = resolve_probe_options(ProbeOverrides {
            target: "example.com".to_string(),
            profile: MeasurementProfile::Standard,
            provider: BandwidthProviderPreset::Custom,
            endpoint: None,
            samples: None,
            download_urls: vec!["https://downloads.example.test/file.bin".to_string()],
            upload_urls: Vec::new(),
            download_size_bytes: None,
            upload_size_bytes: None,
            bandwidth_runs: None,
            bandwidth_warmup_runs: None,
            transfer_attempts: None,
            download_streams: None,
            upload_streams: None,
            target_transfer_duration_ms: None,
            max_download_size_bytes: None,
            max_upload_size_bytes: None,
        })
        .expect_err("custom provider should require both urls");

        assert!(error.to_string().contains("--upload-url"));
    }
}
