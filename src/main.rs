mod cli;
mod probe;
mod storage;
mod tui;
mod version;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, CompareArgs, ExportFormat};
use probe::{format_report, run_probe_suite};
use serde::Serialize;
use storage::{
    compare_latest_runs, compare_reports, compare_run_ids, export_runs_csv, export_runs_json,
    format_compared_runs, format_comparison, format_history, latest_run, list_runs, save_run,
    ComparedRuns, RunComparison, StoredRun,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => {
            let output = run_and_store(args.probe.to_probe_options()?).await;
            print_output(&output, args.json)?;
        }
        Commands::Watch(args) => {
            let probe_options = args.probe.to_probe_options()?;
            let mut run_number = 1_u64;

            loop {
                let output = run_and_store(probe_options.clone()).await;
                println!(
                    "{} run #{run_number}\n",
                    version::short_banner(&probe_options.target)
                );
                print_output(&output, args.json)?;
                tokio::time::sleep(std::time::Duration::from_secs(args.interval)).await;
                run_number += 1;
            }
        }
        Commands::Tui(args) => {
            tui::run_tui(args.probe.to_probe_options()?, args.interval).await?;
        }
        Commands::History(args) => {
            let runs = list_runs(args.target.as_deref(), args.limit)?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                println!("{}", format_history(&runs));
            }
        }
        Commands::Export(args) => {
            let runs = list_runs(args.target.as_deref(), args.limit)?;
            let output = match args.format {
                ExportFormat::Json => export_runs_json(&runs)?,
                ExportFormat::Csv => export_runs_csv(&runs),
            };
            println!("{output}");
        }
        Commands::Compare(args) => {
            let output = build_compare_output(args)?;
            if output.json {
                println!("{}", serde_json::to_string_pretty(&output.compared_runs)?);
            } else {
                println!("{}", format_compared_runs(&output.compared_runs));
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct RunOutput {
    run: StoredRun,
    comparison: Option<RunComparison>,
}

async fn run_and_store(options: probe::ProbeOptions) -> Result<RunOutput> {
    let previous = latest_run(&options.target)?;
    let report = run_probe_suite(&options).await?;
    let run = save_run(&report)?;
    let comparison = previous.map(|previous| compare_reports(&previous.report, &report));

    Ok(RunOutput { run, comparison })
}

struct CompareCommandOutput {
    compared_runs: ComparedRuns,
    json: bool,
}

fn build_compare_output(args: CompareArgs) -> Result<CompareCommandOutput> {
    let compared_runs = match (args.target, args.previous_id, args.current_id) {
        (Some(target), None, None) => compare_latest_runs(&target)?.ok_or_else(|| {
            anyhow::anyhow!("need at least two saved runs for target {target} to compare")
        })?,
        (None, Some(previous_id), Some(current_id)) => compare_run_ids(&previous_id, &current_id)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "failed to find both saved runs for ids {previous_id} and {current_id}"
                )
            })?,
        (Some(_), Some(_), Some(_)) => {
            anyhow::bail!("use either --target or both --previous-id and --current-id")
        }
        _ => {
            anyhow::bail!("provide --target or both --previous-id and --current-id")
        }
    };

    Ok(CompareCommandOutput {
        compared_runs,
        json: args.json,
    })
}

fn print_output(output: &Result<RunOutput, anyhow::Error>, json: bool) -> Result<()> {
    if json {
        let output = match output {
            Ok(output) => serde_json::to_string_pretty(output)?,
            Err(error) => serde_json::to_string_pretty(&serde_json::json!({
                "error": error.to_string(),
            }))?,
        };
        println!("{output}");
        return Ok(());
    }

    match output {
        Ok(output) => {
            println!("{}", format_report(&output.run.report));
            println!("Saved run: {}", output.run.id);
            if let Some(comparison) = &output.comparison {
                println!("Comparison");
                println!("{}", format_comparison(comparison));
            }
        }
        Err(error) => eprintln!("Probe failed: {error}"),
    }

    Ok(())
}
