use clap::Parser;
use poirot_test_runner::json::Cli;
use std::process::ExitCode;

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    Ok(if Cli::parse().run(&mut std::io::stdout().lock())? {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
