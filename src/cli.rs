use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::probe::{BandwidthConfig, ProbeOptions};

const DEFAULT_INTERVAL_SECONDS: u64 = 15;
const DEFAULT_SAMPLE_COUNT: u32 = 5;
const DEFAULT_DOWNLOAD_URL: &str = "https://speed.cloudflare.com/__down?bytes=4000000";
const DEFAULT_UPLOAD_URL: &str = "https://speed.cloudflare.com/__up";
const DEFAULT_UPLOAD_SIZE_BYTES: usize = 1_000_000;
const DEFAULT_BANDWIDTH_RUNS: u32 = 3;
const DEFAULT_DOWNLOAD_STREAMS: u32 = 2;
const DEFAULT_UPLOAD_STREAMS: u32 = 2;
const DEFAULT_HISTORY_LIMIT: usize = 10;

#[derive(Debug, Parser)]
#[command(
    name = "pantheon-probe",
    version,
    about = "Measure latency, jitter, packet loss, DNS resolution, and HTTP transfer throughput."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Run(RunArgs),
    Watch(WatchArgs),
    Tui(TuiArgs),
    History(HistoryArgs),
    Export(ExportArgs),
    Compare(CompareArgs),
}

#[derive(Debug, Clone, Args)]
pub struct SharedProbeArgs {
    #[arg(short, long)]
    pub target: String,
    #[arg(short = 'n', long, default_value_t = DEFAULT_SAMPLE_COUNT)]
    pub samples: u32,
    #[arg(long, default_value = DEFAULT_DOWNLOAD_URL)]
    pub download_url: String,
    #[arg(long, default_value = DEFAULT_UPLOAD_URL)]
    pub upload_url: String,
    #[arg(long, default_value_t = DEFAULT_UPLOAD_SIZE_BYTES)]
    pub upload_size_bytes: usize,
    #[arg(long, default_value_t = DEFAULT_BANDWIDTH_RUNS)]
    pub bandwidth_runs: u32,
    #[arg(long, default_value_t = DEFAULT_DOWNLOAD_STREAMS)]
    pub download_streams: u32,
    #[arg(long, default_value_t = DEFAULT_UPLOAD_STREAMS)]
    pub upload_streams: u32,
}

impl SharedProbeArgs {
    pub fn to_probe_options(&self) -> ProbeOptions {
        ProbeOptions {
            target: self.target.clone(),
            samples: self.samples,
            bandwidth: BandwidthConfig {
                download_url: self.download_url.clone(),
                upload_url: self.upload_url.clone(),
                upload_size_bytes: self.upload_size_bytes,
                runs: self.bandwidth_runs,
                download_streams: self.download_streams,
                upload_streams: self.upload_streams,
            },
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub probe: SharedProbeArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct HistoryArgs {
    #[arg(short, long)]
    pub target: Option<String>,
    #[arg(short, long, default_value_t = DEFAULT_HISTORY_LIMIT)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ExportArgs {
    #[arg(short, long)]
    pub target: Option<String>,
    #[arg(short, long, default_value_t = DEFAULT_HISTORY_LIMIT)]
    pub limit: usize,
    #[arg(short, long, value_enum, default_value_t = ExportFormat::Json)]
    pub format: ExportFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone, Args)]
pub struct CompareArgs {
    #[arg(short, long)]
    pub target: Option<String>,
    #[arg(long)]
    pub previous_id: Option<String>,
    #[arg(long)]
    pub current_id: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct WatchArgs {
    #[command(flatten)]
    pub probe: SharedProbeArgs,
    #[arg(short, long, default_value_t = DEFAULT_INTERVAL_SECONDS)]
    pub interval: u64,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TuiArgs {
    #[command(flatten)]
    pub probe: SharedProbeArgs,
    #[arg(short, long, default_value_t = DEFAULT_INTERVAL_SECONDS)]
    pub interval: u64,
}
