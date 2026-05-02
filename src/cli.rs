use clap::{Args, Parser, Subcommand, ValueEnum};

use anyhow::Result;

use crate::probe::{
    resolve_probe_options, BandwidthProviderPreset, MeasurementProfile, ProbeOptions,
    ProbeOverrides,
};

const DEFAULT_INTERVAL_SECONDS: u64 = 15;
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
    #[arg(long, value_enum, default_value_t = MeasurementProfile::Standard)]
    pub profile: MeasurementProfile,
    #[arg(long, value_enum, default_value_t = BandwidthProviderPreset::Cloudflare)]
    pub provider: BandwidthProviderPreset,
    #[arg(short = 'n', long)]
    pub samples: Option<u32>,
    #[arg(long)]
    pub download_url: Option<String>,
    #[arg(long)]
    pub upload_url: Option<String>,
    #[arg(long)]
    pub download_size_bytes: Option<usize>,
    #[arg(long)]
    pub upload_size_bytes: Option<usize>,
    #[arg(long)]
    pub bandwidth_runs: Option<u32>,
    #[arg(long)]
    pub download_streams: Option<u32>,
    #[arg(long)]
    pub upload_streams: Option<u32>,
}

impl SharedProbeArgs {
    pub fn to_probe_options(&self) -> Result<ProbeOptions> {
        resolve_probe_options(ProbeOverrides {
            target: self.target.clone(),
            profile: self.profile,
            provider: self.provider,
            samples: self.samples,
            download_url: self.download_url.clone(),
            upload_url: self.upload_url.clone(),
            download_size_bytes: self.download_size_bytes,
            upload_size_bytes: self.upload_size_bytes,
            bandwidth_runs: self.bandwidth_runs,
            download_streams: self.download_streams,
            upload_streams: self.upload_streams,
        })
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
