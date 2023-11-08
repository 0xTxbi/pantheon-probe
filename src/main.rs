use clap::{App, Arg};
use reqwest::Client;
use std::io;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use tokio;
use tokio::time;

// measure bandwidth
async fn measure_bandwidth() -> Option<(u64, u64)> {
    let client = Client::new();
    let start_time = Instant::now();

    // simulate a download process
    let download_url =
        "https://drive.google.com/uc?id=1ie1FhaN5ZzwCqc8E0Mz8hS_x9LYMRCk5&export=download";
    let response = match client.get(download_url).send().await {
        Ok(response) => response,
        Err(_) => return None,
    };
    let download_size = match response.bytes().await {
        Ok(bytes) => bytes.len() as u64,
        Err(_) => return None,
    };

    // simulate an upload process
    let upload_url = "https://example.com/upload_endpoint";
    let response = match client.post(upload_url).body(Vec::new()).send().await {
        Ok(response) => response,
        Err(_) => return None,
    };
    let upload_size = response.content_length().unwrap_or_default();

    let end_time = Instant::now();

    // calculate download and upload speeds in Mbps
    let elapsed_time = end_time.duration_since(start_time).as_secs_f64();
    let download_speed = (download_size as f64 / elapsed_time) * 8.0 / 1_000_000.0; // Mbps
    let upload_speed = (upload_size as f64 / elapsed_time) * 8.0 / 1_000_000.0; // Mbps

    Some((download_speed as u64, upload_speed as u64))
}

// measure latency
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
        // parse output to extract latency
        let output_str = String::from_utf8_lossy(&output.stdout);

        if let Some(latency) = extract_latency_from_ping_output(&output_str) {
            return Some(latency);
        }
    }

    None
}

// extract latency from the ping's output
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

// measure packet loss
async fn measure_packet_loss(target_host: &str) -> Option<f64> {
    let packet_count = 10; // number of packets to send

    let ping_command = match cfg!(target_os = "windows") {
        true => format!("ping -n {}", packet_count),
        false => format!("ping -c {}", packet_count),
    };

    let output: Output = Command::new("sh")
        .arg("-c")
        .arg(format!("{} {}", ping_command, target_host))
        .output()
        .expect("Oops! Failed to execute the ping command");

    if output.status.success() {
        // parse output to extract packet loss percentage
        let output_str = String::from_utf8_lossy(&output.stdout);

        if let Some(packet_loss) = extract_packet_loss_from_ping_output(&output_str) {
            return Some(packet_loss);
        }
    }

    None
}

// extract packet loss from the ping's output
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

// measure jitter
fn measure_jitter(target_host: &str) -> Option<f64> {
    let mut previous_latency: Option<Duration> = None;
    let mut jitter_sum: f64 = 0.0;
    let mut packet_count = 0;

    // measure latency for a certain number of packets
    for _ in 0..10 {
        if let Some(latency) = measure_latency(target_host) {
            if let Some(previous) = previous_latency {
                if let Some(latency_diff) = latency.checked_sub(previous) {
                    let latency_diff_ms = latency_diff.as_secs_f64() * 1000.0;
                    jitter_sum += latency_diff_ms;
                    packet_count += 1;
                }
            }
            previous_latency = Some(latency);
        }
    }

    // compute average jitter
    if packet_count > 0 {
        let average_jitter = jitter_sum / packet_count as f64;
        Some(average_jitter)
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    // define cli options
    let matches = App::new("PantheonProbe")
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .value_name("HOST")
                .help("Sets the target host or IP address")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("interval")
                .short("i")
                .long("interval")
                .value_name("SECONDS")
                .help("Sets the testing interval in seconds")
                .takes_value(true),
        )
        .get_matches();

    // utilise provided target or prompt the user
    let target_host = matches
        .value_of("target")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let mut target = String::new();
            println!("Enter your desired target host or IP address:");
            io::stdin()
                .read_line(&mut target)
                .expect("Oops! Failed to read line");
            target.trim().to_string()
        });

    // trim trailing newline
    let target_host = target_host.trim();

    // get the testing interval from command-line options or use a default value
    let interval = matches
        .value_of("interval")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10); // default interval of 10 seconds

    loop {
        // measure latency
        if let Some(latency) = measure_latency(&target_host) {
            println!("The latency to {} is {:?}", target_host, latency);
        } else {
            println!("Oops! Failed to measure the latency.");
        }

        // measure packet loss
        if let Some(packet_loss) = measure_packet_loss(&target_host).await {
            println!("Packet Loss: {}%", packet_loss);
        } else {
            println!("Failed to measure packet loss.");
        }

        // measure jitter
        if let Some(jitter) = measure_jitter(&target_host) {
            println!("Jitter: {} ms", jitter);
        } else {
            println!("Failed to measure jitter.");
        }

        // measure bandwidth
        if let Some((download, upload)) = measure_bandwidth().await {
            println!("Download Speed: {} Mbps", download);
            println!("Upload Speed: {} Mbps", upload);
        } else {
            println!("Failed to measure bandwidth.");
        }

        // sleep for the specified interval
        time::sleep(Duration::from_secs(interval)).await;

        // prompt the user to continue or not
        println!("Do you wish to continue? (y/n)");
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Oops! Failed to read line");
        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            break;
        }
    }
}
