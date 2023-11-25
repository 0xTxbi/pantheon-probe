use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::time;

/// measure bandwidth by simulating a download and upload process.
/// returns a tuple containing download and upload speeds in mbps.
pub async fn measure_bandwidth() -> Option<(u64, u64)> {
    println!("Measuring bandwidth");

    let client = reqwest::Client::new();
    let start_time = Instant::now();

    // instantiate new progress bar with an arbitrary initial length
    let pb = ProgressBar::new(100);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .progress_chars("#>-"));

    // simulate a download process
    let download_url =
        "https://drive.google.com/uc?id=1ie1FhaN5ZzwCqc8E0Mz8hS_x9LYMRCk5&export=download";
    let download_result = download_task(client.clone(), download_url, pb.clone()).await;

    // simulate an upload process
    let upload_url = "https://example.com/upload_endpoint";
    let upload_result = upload_task(client.clone(), upload_url).await;

    // Handle errors from both tasks
    if let (Ok(download_size), Ok(upload_size)) = (download_result, upload_result) {
        let end_time = Instant::now();
        let elapsed_time = end_time.duration_since(start_time).as_secs_f64();

        // calculate download and upload speeds in Mbps
        let download_speed = (download_size as f64 / elapsed_time) * 8.0 / 1_000_000.0; // Mbps
        let upload_speed = (upload_size as f64 / elapsed_time) * 8.0 / 1_000_000.0; // Mbps

        pb.finish_with_message("Download and upload complete!");

        Some((download_speed as u64, upload_speed as u64))
    } else {
        None
    }
}

async fn download_task(client: Client, url: &str, pb: ProgressBar) -> Result<u64, reqwest::Error> {
    let mut response = client.get(url).send().await?;
    let total_size = response.content_length().unwrap_or_default();
    pb.set_length(total_size);

    let mut download_size = 0u64;
    while let Some(chunk) = response.chunk().await? {
        download_size += chunk.len() as u64;
        pb.set_position(download_size);
    }

    Ok(download_size)
}

async fn upload_task(client: Client, url: &str) -> Result<u64, reqwest::Error> {
    let response = client.post(url).body(Vec::new()).send().await?;
    Ok(response.content_length().unwrap_or_default())
}
