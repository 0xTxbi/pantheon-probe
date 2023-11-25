mod bandwidth;
mod cli;
mod dns_resolution_time;
mod jitter;
mod latency;
mod packet_loss;
mod utils;
mod version;

#[tokio::main]
async fn main() {
    version::print_version();

    // parse CLI arguments
    let cli_args = cli::parse_cli_args();

    let target_host = cli_args.target_host;

    loop {
        latency::measure_latency(&target_host);
        dns_resolution_time::measure_dns_resolution_time(&target_host);
        packet_loss::measure_packet_loss(&target_host).await;
        jitter::measure_jitter(&target_host);

        if let Some(bandwidth) = bandwidth::measure_bandwidth().await {
            println!("Download Speed: {} Mbps", bandwidth.0);
            println!("Upload Speed: {} Mbps", bandwidth.1);
        } else {
            println!("Failed to measure bandwidth.");
        }

        if !cli::should_continue() {
            break;
        }
    }
}
