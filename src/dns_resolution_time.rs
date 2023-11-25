use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::time::{Duration, Instant};

// measure dns resolution time
// returns time taken to resolve the target host's domain name to an IP address
pub fn measure_dns_resolution_time(host: &str) -> Option<Duration> {
    println!("Measuring DNS Resolution Time for {}", host);

    let start_time = Instant::now();

    // attempt resolving the hostname to IP addresses
    let ips: Vec<_> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => match host.parse::<Ipv4Addr>() {
            Ok(ipv4) => vec![IpAddr::V4(ipv4)],
            Err(_) => match (host, 0).to_socket_addrs() {
                Ok(addrs) => addrs.map(|a| a.ip()).collect(),
                Err(err) => {
                    eprintln!("Error resolving {}: {}", host, err);
                    return None;
                }
            },
        },
    };

    let end_time = Instant::now();
    let elapsed_time = end_time.duration_since(start_time);

    if ips.is_empty() {
        eprintln!("Failed to resolve IP address for {}", host);
        None
    } else {
        println!("DNS Resolution Time for {}: {:?}", host, elapsed_time);
        Some(elapsed_time)
    }
}
