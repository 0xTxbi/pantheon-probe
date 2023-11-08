use std::time::{Instant, Duration};
use std::io;
use std::process::{Command, Output};

fn measure_latency(target_host: &str) -> Option<Duration> {
    let ping_command = match cfg!(target_os = "windows") {
        true => "ping -n 1",
        false => "ping -c 1",
    };

    let output: Output = Command::new("sh")
        .arg("-c")
        .arg(format!("{} {}", ping_command, target_host))
        .output()
        .expect("Oops! Failed to execute the ping command");

    if output.status.success() {
        // Parse output to extract latency
        let output_str = String::from_utf8_lossy(&output.stdout);

        if let Some(latency) = extract_latency_from_ping_output(&output_str) {
            return Some(latency);
        }
    }

    None
}

// Extract latency from the ping's output
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

fn main() {
    println!("Welcome to PantheonProbe!");

    // prompt user for their desired target host
    let mut target_host = String::new();
    println!("Enter your desired target host or IP address:");
    io::stdin().read_line(&mut target_host).expect("Failed to read line");

    // trim trailing newline
    let target_host = target_host.trim();

    // measure latency
    if let Some(latency) = measure_latency(target_host) {
        println!("The latency to {} is {:?}", target_host, latency);
    } else {
        println!("Oops! Failed to measure the latency.");
    }
}
