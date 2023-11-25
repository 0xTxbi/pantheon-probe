use clap::{App, Arg};
use std::io;

/// struct to hold CLI arguments.
pub struct CliArgs {
    pub target_host: String,
}

/// parse CLI arguments and return a CliArgs struct.
pub fn parse_cli_args() -> CliArgs {
    let matches = App::new("PantheonProbe")
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .value_name("HOST")
                .help("Sets the target host or IP address")
                .takes_value(true),
        )
        .get_matches();

    let target_host = matches
        .value_of("target")
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let mut target = String::new();
            println!("Enter your desired target host or IP address:");
            std::io::stdin()
                .read_line(&mut target)
                .expect("Oops! Failed to read line");
            target.trim().to_string()
        });

    CliArgs { target_host }
}

/// prompt the user to continue or not
pub fn should_continue() -> bool {
    println!("Do you wish to continue? (y/n)");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Oops! Failed to read line");
    let input = input.trim().to_lowercase();
    input == "y" || input == "yes"
}
