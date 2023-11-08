use std::time::{Instant, Duration};
use std::io;
use std::process::{Command, Output};
use reqwest::Client;
use tokio;
use tokio::time;


// measure bandwidth
async fn measure_bandwidth() -> Option<(u64, u64)> {
    let client = Client::new();
    let start_time = Instant::now();

    // Simulate a download process
    let download_url = "https://www.strem.io/download?four=4";
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
    let response = match client.post(upload_url)
        .body(Vec::new())
        .send()
        .await {
        Ok(response) => response,
        Err(_) => return None,
    };
    let upload_size = response.content_length().unwrap_or_default();

    let end_time = Instant::now();

    // calculate download and upload speeds in Mbps
    let elapsed_time = end_time.duration_since(start_time).as_secs_f64();
    let download_speed = (download_size as f64 / elapsed_time) * 8.0 / 1_000_000.0; // Mbps
    let upload_speed = (upload_size as f64 / elapsed_time) * 8.0 / 1_000_000.0;     // Mbps

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
        // Parse output to extract latency
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

#[tokio::main]
async fn main() {
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

    // measure bandwidth
    if let Some((download, upload)) = measure_bandwidth().await {
        println!("Download Speed: {} Mbps", download);
        println!("Upload Speed: {} Mbps", upload);
    } else {
        println!("Failed to measure bandwidth.");
    }

    // sleep for a while to give time for the asynchronous task to complete
    time::sleep(Duration::from_secs(2)).await;
}
