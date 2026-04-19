mod cli;
mod probe;
mod tui;
mod version;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use probe::{format_report, run_probe_suite};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => {
            let report = run_probe_suite(&args.probe.to_probe_options()).await;
            print_report(&report, args.json)?;
        }
        Commands::Watch(args) => {
            let probe_options = args.probe.to_probe_options();
            let mut run_number = 1_u64;

            loop {
                let report = run_probe_suite(&probe_options).await;
                println!(
                    "{} run #{run_number}\n",
                    version::short_banner(&probe_options.target)
                );
                print_report(&report, args.json)?;
                tokio::time::sleep(std::time::Duration::from_secs(args.interval)).await;
                run_number += 1;
            }
        }
        Commands::Tui(args) => {
            tui::run_tui(args.probe.to_probe_options(), args.interval).await?;
        }
    }

    Ok(())
}

fn print_report(report: &Result<probe::ProbeReport, anyhow::Error>, json: bool) -> Result<()> {
    if json {
        let output = match report {
            Ok(report) => serde_json::to_string_pretty(report)?,
            Err(error) => serde_json::to_string_pretty(&serde_json::json!({
                "error": error.to_string(),
            }))?,
        };
        println!("{output}");
        return Ok(());
    }

    match report {
        Ok(report) => println!("{}", format_report(report)),
        Err(error) => eprintln!("Probe failed: {error}"),
    }

    Ok(())
}
