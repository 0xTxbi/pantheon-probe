pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn short_banner(target: &str) -> String {
    format!("PantheonProbe v{VERSION} | target: {target}")
}
