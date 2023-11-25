use indicatif::{ProgressBar, ProgressStyle};
use std::process::{Command, Output};
use std::time::Duration;
use tokio;

/// measure packet loss to a target host using the ping command.
/// returns the measured packet loss percentage.
pub async fn measure_packet_loss(target_host: &str) -> Option<f64> {
    println!("Calculating packet loss");

    let packet_count = 10; // number of packets to send

    let ping_command = match cfg!(target_os = "windows") {
        true => format!("ping -n {}", packet_count),
        false => format!("ping -c {}", packet_count),
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

    pb.set_message("Starting packet loss measurement...");
    pb.inc(33); // Increment the progress bar by 33% after starting

    let output: Output = Command::new("sh")
        .arg("-c")
        .arg(format!("{} {}", ping_command, target_host))
        .output()
        .expect("Oops! Failed to execute the ping command");

    pb.set_message("Ping command executed...");
    pb.inc(33); // Increment the progress bar by 33% after executing the ping command

    if output.status.success() {
        // parse output to extract packet loss percentage
        let output_str = String::from_utf8_lossy(&output.stdout);

        pb.set_message("Parsing ping output...");
        pb.inc(34); // Increment the progress bar by 34% after parsing the output

        if let Some(packet_loss) = extract_packet_loss_from_ping_output(&output_str) {
            pb.finish_with_message("Packet loss measurement complete!");
            return Some(packet_loss);
        }
    }

    pb.finish_with_message("Failed to measure packet loss.");
    None
}

/// extract packet loss from the ping command's output.
/// returns the measured packet loss percentage.
fn extract_packet_loss_from_ping_output(output: &str) -> Option<f64> {
    let lines: Vec<&str> = output.lines().collect();
    for line in lines {
        if line.contains("packet loss") {
            if let Some(loss_start) = line.find("received, ") {
                let loss_end = line[loss_start + 10..].find("%").unwrap_or(line.len());
                let loss_str = &line[loss_start + 10..loss_start + 10 + loss_end];
                if let Ok(packet_loss) = loss_str.parse::<f64>() {
                    return Some(packet_loss);
                }
            }
        }
    }
    None
}
