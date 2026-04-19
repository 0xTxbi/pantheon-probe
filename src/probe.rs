use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Serialize;
use std::net::{IpAddr, ToSocketAddrs};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ProbeOptions {
    pub target: String,
    pub samples: u32,
    pub bandwidth: BandwidthConfig,
}

#[derive(Debug, Clone)]
pub struct BandwidthConfig {
    pub download_url: String,
    pub upload_url: String,
    pub upload_size_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub target: String,
    pub samples: u32,
    pub created_at_unix_ms: u128,
    pub ping: ProbeOutcome<PingSummary>,
    pub dns: ProbeOutcome<DnsSummary>,
    pub bandwidth: ProbeOutcome<BandwidthSummary>,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct PingSummary {
    pub sent: u32,
    pub received: u32,
    pub packet_loss_pct: f64,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub samples_ms: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsSummary {
    pub resolution_time_ms: f64,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BandwidthSummary {
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_url: String,
    pub upload_url: String,
}

pub async fn run_probe_suite(options: &ProbeOptions) -> Result<ProbeReport> {
    let ping_result = measure_ping(&options.target, options.samples);
    let dns_result = measure_dns(&options.target);
    let bandwidth_result = measure_bandwidth(&options.bandwidth).await;

    Ok(ProbeReport {
        target: options.target.clone(),
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
    output.push_str(&format!("Samples: {}\n\n", report.samples));

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
            format!("  download: {:.2} Mbps", bandwidth.download_mbps),
            format!("  upload: {:.2} Mbps", bandwidth.upload_mbps),
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

    let (min_ms, avg_ms, max_ms, jitter_ms) = if samples_ms.is_empty() {
        (None, None, None, None)
    } else {
        let min_ms = samples_ms
            .iter()
            .copied()
            .reduce(f64::min)
            .context("failed to derive minimum ping latency")?;
        let max_ms = samples_ms
            .iter()
            .copied()
            .reduce(f64::max)
            .context("failed to derive maximum ping latency")?;
        let avg_ms = samples_ms.iter().sum::<f64>() / samples_ms.len() as f64;
        let jitter_ms = calculate_jitter_ms(&samples_ms);

        (Some(min_ms), Some(avg_ms), Some(max_ms), jitter_ms)
    };

    Ok(PingSummary {
        sent,
        received,
        packet_loss_pct,
        min_ms,
        avg_ms,
        max_ms,
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

    let (download_bytes, download_elapsed) = download_bytes(&client, &config.download_url)
        .await
        .with_context(|| {
            format!(
                "download throughput check failed for {}",
                config.download_url
            )
        })?;

    let (upload_bytes, upload_elapsed) = upload_bytes(&client, config)
        .await
        .with_context(|| format!("upload throughput check failed for {}", config.upload_url))?;

    Ok(BandwidthSummary {
        download_mbps: bytes_to_mbps(download_bytes, download_elapsed),
        upload_mbps: bytes_to_mbps(upload_bytes, upload_elapsed),
        download_bytes,
        upload_bytes,
        download_url: config.download_url.clone(),
        upload_url: config.upload_url.clone(),
    })
}

async fn download_bytes(client: &Client, url: &str) -> Result<(u64, Duration)> {
    let started = Instant::now();
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

    Ok((total_bytes, started.elapsed()))
}

async fn upload_bytes(client: &Client, config: &BandwidthConfig) -> Result<(u64, Duration)> {
    let payload = vec![b'x'; config.upload_size_bytes];
    let payload_len = payload.len() as u64;
    let started = Instant::now();

    client
        .post(&config.upload_url)
        .body(payload)
        .send()
        .await
        .with_context(|| format!("failed to POST {}", config.upload_url))?
        .error_for_status()
        .with_context(|| {
            format!(
                "upload endpoint returned an error for {}",
                config.upload_url
            )
        })?;

    Ok((payload_len, started.elapsed()))
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

#[cfg(test)]
mod tests {
    use super::{calculate_jitter_ms, parse_ping_output};

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
}
