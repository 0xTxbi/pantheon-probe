use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::latency::measure_latency;

/// measure jitter by calculating the difference in latency over a series of measurements.
/// returns the average jitter in milliseconds.
pub fn measure_jitter(target_host: &str) -> Option<f64> {
    println!("Calculating jitter");

    let mut previous_latency: Option<Duration> = None;
    let mut jitter_sum: f64 = 0.0;
    let mut packet_count = 0;

    // instantiate new progress bar
    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .progress_chars("#>-"),
    );

    pb.set_message("Starting jitter measurement...");
    pb.inc(10); // Increment the progress bar by 10% after starting

    // measure latency for a certain number of packets
    for _ in 0..10 {
        if let Some(latency) = measure_latency(target_host) {
            pb.inc(10); // Increment the progress bar by 10% after each latency measurement
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
        pb.finish_with_message("Jitter measurement complete!");
        Some(average_jitter)
    } else {
        pb.finish_with_message("Failed to measure jitter.");
        None
    }
}
