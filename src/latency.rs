use indicatif::{ProgressBar, ProgressStyle};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// measure latency to a target host using the ping command.
/// returns the measured latency as a duration.
pub fn measure_latency(target_host: &str) -> Option<Duration> {
    let ping_command = match cfg!(target_os = "windows") {
        true => "ping -n 1",
        false => "ping -c 1",
    };

    // instantiate new progress bar
    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .progress_chars("#>-"),
    );

    pb.set_message("Starting latency measurement...");
    pb.inc(25); // Increment the progress bar by 25% after starting

    let output: Output = Command::new("sh")
        .arg("-c")
        .arg(format!("{} {}", ping_command, target_host))
        .output()
        .expect("Oops! Failed to execute the ping command");

    pb.set_message("Ping command executed...");
    pb.inc(25); // Increment the progress bar by 25% after executing the ping command

    if output.status.success() {
        // parse output to extract latency
        let output_str = String::from_utf8_lossy(&output.stdout);

        pb.set_message("Parsing ping output...");
        pb.inc(25); // Increment the progress bar by 25% after parsing the output

        if let Some(latency) = extract_latency_from_ping_output(&output_str) {
            pb.set_message("Latency measurement complete!");
            pb.inc(25); // Increment the progress bar by 25% after completing the measurement
            pb.finish();
            return Some(latency);
        }
    }

    pb.finish_with_message("Failed to measure latency.");
    None
}

/// measure latency to a target host using the ping command.
/// returns the measured latency as a duration.
fn extract_latency_from_ping_output(output: &str) -> Option<Duration> {
    let lines: Vec<&str> = output.lines().collect();
    for line in lines {
        if line.contains("time=") {
            if let Some(time_start) = line.find("time=") {
                let time_end = line[time_start + 5..].find(" ").unwrap_or(line.len());
                let latency_str = &line[time_start + 5..time_start + 5 + time_end];
                if let Ok(latency_ms) = latency_str.parse::<f64>() {
                    return Some(Duration::from_millis(latency_ms as u64));
                }
            }
        }
    }
    None
}
