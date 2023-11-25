use tokio::time::{self, Duration};

/// sleep for the specified interval in seconds.
pub fn sleep_for_interval(interval: u64) {
    let sleep_duration = Duration::from_secs(interval);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(time::sleep(sleep_duration));
}
