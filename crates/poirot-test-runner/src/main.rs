use clap::Parser;
use poirot_test_runner::Cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    match Cli::parse().execute(&mut std::io::stdout().lock()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("poirot-test-runner: {error}");
            ExitCode::FAILURE
        }
    }
}
