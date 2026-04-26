use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::probe::ProbeReport;

const APP_DIR_NAME: &str = ".pantheon-probe";
const RUNS_DIR_NAME: &str = "runs";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRun {
    pub id: String,
    pub target: String,
    pub report: ProbeReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunComparison {
    pub previous_run_id: String,
    pub previous_created_at_unix_ms: u128,
    pub ping_avg_delta_ms: Option<f64>,
    pub ping_median_delta_ms: Option<f64>,
    pub packet_loss_delta_pct: Option<f64>,
    pub dns_delta_ms: Option<f64>,
    pub download_delta_mbps: Option<f64>,
    pub upload_delta_mbps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComparedRuns {
    pub current: StoredRun,
    pub previous: StoredRun,
    pub comparison: RunComparison,
}

pub fn save_run(report: &ProbeReport) -> Result<StoredRun> {
    let id = build_run_id(report.created_at_unix_ms, &report.target);
    let stored_run = StoredRun {
        id: id.clone(),
        target: report.target.clone(),
        report: report.clone(),
    };
    let path = runs_dir()?.join(format!("{id}.json"));
    let bytes = serde_json::to_vec_pretty(&stored_run).context("failed to serialize stored run")?;

    fs::write(&path, bytes)
        .with_context(|| format!("failed to write run file {}", path.display()))?;

    Ok(stored_run)
}

pub fn list_runs(target: Option<&str>, limit: usize) -> Result<Vec<StoredRun>> {
    let runs_dir = runs_dir()?;
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();
    for entry in
        fs::read_dir(&runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let entry = entry.context("failed to read run directory entry")?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let stored_run = load_run(&path)?;
        if target.is_none_or(|target| stored_run.target == target) {
            runs.push(stored_run);
        }
    }

    runs.sort_by(|left, right| {
        right
            .report
            .created_at_unix_ms
            .cmp(&left.report.created_at_unix_ms)
            .then_with(|| right.id.cmp(&left.id))
    });

    if limit < runs.len() {
        runs.truncate(limit);
    }

    Ok(runs)
}

pub fn latest_run(target: &str) -> Result<Option<StoredRun>> {
    Ok(list_runs(Some(target), 1)?.into_iter().next())
}

pub fn get_run(id: &str) -> Result<Option<StoredRun>> {
    let path = runs_dir()?.join(format!("{id}.json"));
    if !path.exists() {
        return Ok(None);
    }

    Ok(Some(load_run(&path)?))
}

pub fn compare_latest_runs(target: &str) -> Result<Option<ComparedRuns>> {
    let mut runs = list_runs(Some(target), 2)?;
    if runs.len() < 2 {
        return Ok(None);
    }

    let previous = runs.pop().expect("two runs are available");
    let current = runs.pop().expect("two runs are available");

    Ok(Some(build_compared_runs(previous, current)))
}

pub fn compare_run_ids(previous_id: &str, current_id: &str) -> Result<Option<ComparedRuns>> {
    let previous = match get_run(previous_id)? {
        Some(run) => run,
        None => return Ok(None),
    };
    let current = match get_run(current_id)? {
        Some(run) => run,
        None => return Ok(None),
    };

    Ok(Some(build_compared_runs(previous, current)))
}

pub fn compare_reports(previous: &ProbeReport, current: &ProbeReport) -> RunComparison {
    RunComparison {
        previous_run_id: build_run_id(previous.created_at_unix_ms, &previous.target),
        previous_created_at_unix_ms: previous.created_at_unix_ms,
        ping_avg_delta_ms: difference(
            previous.ping.value.as_ref().and_then(|value| value.avg_ms),
            current.ping.value.as_ref().and_then(|value| value.avg_ms),
        ),
        ping_median_delta_ms: difference(
            previous
                .ping
                .value
                .as_ref()
                .and_then(|value| value.median_ms),
            current
                .ping
                .value
                .as_ref()
                .and_then(|value| value.median_ms),
        ),
        packet_loss_delta_pct: difference(
            previous
                .ping
                .value
                .as_ref()
                .map(|value| value.packet_loss_pct),
            current
                .ping
                .value
                .as_ref()
                .map(|value| value.packet_loss_pct),
        ),
        dns_delta_ms: difference(
            previous
                .dns
                .value
                .as_ref()
                .map(|value| value.resolution_time_ms),
            current
                .dns
                .value
                .as_ref()
                .map(|value| value.resolution_time_ms),
        ),
        download_delta_mbps: difference(
            previous
                .bandwidth
                .value
                .as_ref()
                .map(|value| value.download_mbps),
            current
                .bandwidth
                .value
                .as_ref()
                .map(|value| value.download_mbps),
        ),
        upload_delta_mbps: difference(
            previous
                .bandwidth
                .value
                .as_ref()
                .map(|value| value.upload_mbps),
            current
                .bandwidth
                .value
                .as_ref()
                .map(|value| value.upload_mbps),
        ),
    }
}

pub fn format_history(runs: &[StoredRun]) -> String {
    if runs.is_empty() {
        return "No saved runs.".to_string();
    }

    runs.iter()
        .map(|run| {
            format!(
                "{} | {} | target: {} | ping avg: {} | download: {} | upload: {}",
                run.id,
                run.report.created_at_unix_ms,
                run.target,
                format_optional(
                    run.report
                        .ping
                        .value
                        .as_ref()
                        .and_then(|value| value.avg_ms),
                    "ms"
                ),
                format_optional(
                    run.report
                        .bandwidth
                        .value
                        .as_ref()
                        .map(|value| value.download_mbps),
                    "Mbps"
                ),
                format_optional(
                    run.report
                        .bandwidth
                        .value
                        .as_ref()
                        .map(|value| value.upload_mbps),
                    "Mbps"
                ),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_compared_runs(compared_runs: &ComparedRuns) -> String {
    [
        format!(
            "Current run: {} ({})",
            compared_runs.current.id, compared_runs.current.report.created_at_unix_ms
        ),
        format!(
            "Previous run: {} ({})",
            compared_runs.previous.id, compared_runs.previous.report.created_at_unix_ms
        ),
        format_comparison(&compared_runs.comparison),
    ]
    .join("\n")
}

pub fn format_comparison(comparison: &RunComparison) -> String {
    [
        format!(
            "Compared with run {} ({})",
            comparison.previous_run_id, comparison.previous_created_at_unix_ms
        ),
        format!(
            "  ping avg delta: {}",
            format_signed(comparison.ping_avg_delta_ms, "ms")
        ),
        format!(
            "  ping median delta: {}",
            format_signed(comparison.ping_median_delta_ms, "ms")
        ),
        format!(
            "  packet loss delta: {}",
            format_signed(comparison.packet_loss_delta_pct, "pct")
        ),
        format!(
            "  dns delta: {}",
            format_signed(comparison.dns_delta_ms, "ms")
        ),
        format!(
            "  download delta: {}",
            format_signed(comparison.download_delta_mbps, "Mbps")
        ),
        format!(
            "  upload delta: {}",
            format_signed(comparison.upload_delta_mbps, "Mbps")
        ),
    ]
    .join("\n")
}

pub fn export_runs_json(runs: &[StoredRun]) -> Result<String> {
    serde_json::to_string_pretty(runs).context("failed to serialize run export as JSON")
}

pub fn export_runs_csv(runs: &[StoredRun]) -> String {
    let mut output = String::from(
        "created_at_unix_ms,target,ping_avg_ms,ping_median_ms,ping_p95_ms,packet_loss_pct,dns_resolution_ms,download_mbps,upload_mbps\n",
    );

    for run in runs {
        output.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            run.report.created_at_unix_ms,
            csv_escape(&run.target),
            csv_number(
                run.report
                    .ping
                    .value
                    .as_ref()
                    .and_then(|value| value.avg_ms)
            ),
            csv_number(
                run.report
                    .ping
                    .value
                    .as_ref()
                    .and_then(|value| value.median_ms)
            ),
            csv_number(
                run.report
                    .ping
                    .value
                    .as_ref()
                    .and_then(|value| value.p95_ms)
            ),
            csv_number(
                run.report
                    .ping
                    .value
                    .as_ref()
                    .map(|value| value.packet_loss_pct)
            ),
            csv_number(
                run.report
                    .dns
                    .value
                    .as_ref()
                    .map(|value| value.resolution_time_ms)
            ),
            csv_number(
                run.report
                    .bandwidth
                    .value
                    .as_ref()
                    .map(|value| value.download_mbps)
            ),
            csv_number(
                run.report
                    .bandwidth
                    .value
                    .as_ref()
                    .map(|value| value.upload_mbps)
            ),
        ));
    }

    output
}

fn load_run(path: &Path) -> Result<StoredRun> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn runs_dir() -> Result<PathBuf> {
    let root = data_dir()?;
    let runs = root.join(RUNS_DIR_NAME);
    fs::create_dir_all(&runs).with_context(|| format!("failed to create {}", runs.display()))?;
    Ok(runs)
}

fn data_dir() -> Result<PathBuf> {
    let home = env::var_os("PANTHEON_PROBE_HOME")
        .or_else(|| env::var_os("HOME"))
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| anyhow!("failed to resolve a home directory for PantheonProbe storage"))?;

    let path = PathBuf::from(home).join(APP_DIR_NAME);
    fs::create_dir_all(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(path)
}

fn build_compared_runs(previous: StoredRun, current: StoredRun) -> ComparedRuns {
    let comparison = compare_reports(&previous.report, &current.report);

    ComparedRuns {
        current,
        previous,
        comparison,
    }
}

fn build_run_id(created_at_unix_ms: u128, target: &str) -> String {
    format!("{created_at_unix_ms}-{}", sanitize_target(target))
}

fn sanitize_target(target: &str) -> String {
    let sanitized: String = target
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => character,
            _ => '-',
        })
        .collect();

    sanitized.trim_matches('-').to_lowercase()
}

fn difference(previous: Option<f64>, current: Option<f64>) -> Option<f64> {
    Some(current? - previous?)
}

fn format_optional(value: Option<f64>, unit: &str) -> String {
    value
        .map(|value| format!("{value:.2} {unit}"))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn format_signed(value: Option<f64>, unit: &str) -> String {
    value
        .map(|value| format!("{value:+.2} {unit}"))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn csv_number(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.4}")).unwrap_or_default()
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_run_id, compare_reports, csv_escape, export_runs_csv, format_compared_runs,
        format_history, sanitize_target, ComparedRuns, StoredRun,
    };
    use crate::probe::{
        BandwidthSummary, DnsSummary, MetricStats, PingSummary, ProbeOutcome, ProbeReport,
        TransferSample,
    };

    #[test]
    fn sanitizes_targets_for_storage_ids() {
        assert_eq!(sanitize_target("1.1.1.1"), "1-1-1-1");
        assert_eq!(sanitize_target("Example Host"), "example-host");
        assert_eq!(build_run_id(42, "example.com"), "42-example-com");
    }

    #[test]
    fn formats_csv_rows() {
        let runs = vec![fixture_run(42)];
        let csv = export_runs_csv(&runs);

        assert!(csv.contains("created_at_unix_ms,target"));
        assert!(csv.contains("42,example.com,"));
    }

    #[test]
    fn formats_history_rows_with_run_ids() {
        let history = format_history(&[fixture_run(42)]);
        assert!(history.contains("42-example-com"));
        assert!(history.contains("target: example.com"));
    }

    #[test]
    fn escapes_csv_cells() {
        assert_eq!(csv_escape("example"), "example");
        assert_eq!(csv_escape("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn compares_reports_by_metric_delta() {
        let previous = fixture_run(1).report;
        let mut current = fixture_run(2).report;
        if let Some(ping) = current.ping.value.as_mut() {
            ping.avg_ms = Some(20.0);
            ping.median_ms = Some(19.0);
            ping.packet_loss_pct = 5.0;
        }

        let comparison = compare_reports(&previous, &current);
        assert!(comparison.ping_avg_delta_ms.is_some());
        assert!(comparison.packet_loss_delta_pct.is_some());
    }

    #[test]
    fn formats_compared_runs() {
        let previous = fixture_run(1);
        let current = fixture_run(2);
        let compared_runs = ComparedRuns {
            comparison: compare_reports(&previous.report, &current.report),
            previous,
            current,
        };

        let formatted = format_compared_runs(&compared_runs);
        assert!(formatted.contains("Current run: 2-example-com"));
        assert!(formatted.contains("Previous run: 1-example-com"));
    }

    fn fixture_run(created_at_unix_ms: u128) -> StoredRun {
        StoredRun {
            id: format!("{created_at_unix_ms}-example-com"),
            target: "example.com".to_string(),
            report: ProbeReport {
                target: "example.com".to_string(),
                samples: 5,
                created_at_unix_ms,
                ping: ProbeOutcome {
                    value: Some(PingSummary {
                        sent: 5,
                        received: 5,
                        packet_loss_pct: 0.0,
                        min_ms: Some(10.0),
                        avg_ms: Some(12.0),
                        median_ms: Some(11.0),
                        p95_ms: Some(14.0),
                        max_ms: Some(15.0),
                        stddev_ms: Some(1.5),
                        jitter_ms: Some(1.0),
                        samples_ms: vec![10.0, 11.0, 12.0, 13.0, 15.0],
                    }),
                    error: None,
                },
                dns: ProbeOutcome {
                    value: Some(DnsSummary {
                        resolution_time_ms: 1.2,
                        addresses: vec!["93.184.216.34".to_string()],
                    }),
                    error: None,
                },
                bandwidth: ProbeOutcome {
                    value: Some(BandwidthSummary {
                        download_mbps: 50.0,
                        upload_mbps: 20.0,
                        download: MetricStats {
                            min: 45.0,
                            mean: 50.0,
                            median: 50.0,
                            p95: 54.0,
                            max: 55.0,
                            stddev: 3.0,
                        },
                        upload: MetricStats {
                            min: 18.0,
                            mean: 20.0,
                            median: 20.0,
                            p95: 21.5,
                            max: 22.0,
                            stddev: 1.4,
                        },
                        download_runs: vec![TransferSample {
                            mbps: 50.0,
                            bytes: 4_000_000,
                            elapsed_ms: 640.0,
                            streams: 2,
                        }],
                        upload_runs: vec![TransferSample {
                            mbps: 20.0,
                            bytes: 1_000_000,
                            elapsed_ms: 400.0,
                            streams: 2,
                        }],
                        download_bytes: 4_000_000,
                        upload_bytes: 1_000_000,
                        runs: 1,
                        download_streams: 2,
                        upload_streams: 2,
                        download_url: "https://speed.cloudflare.com/__down?bytes=4000000"
                            .to_string(),
                        upload_url: "https://speed.cloudflare.com/__up".to_string(),
                    }),
                    error: None,
                },
            },
        }
    }
}
