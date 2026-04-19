use clap::{Args, Parser, Subcommand};

use crate::probe::{BandwidthConfig, ProbeOptions};

const DEFAULT_INTERVAL_SECONDS: u64 = 15;
const DEFAULT_SAMPLE_COUNT: u32 = 5;
const DEFAULT_DOWNLOAD_URL: &str = "https://speed.cloudflare.com/__down?bytes=4000000";
const DEFAULT_UPLOAD_URL: &str = "https://speed.cloudflare.com/__up";
const DEFAULT_UPLOAD_SIZE_BYTES: usize = 1_000_000;

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
