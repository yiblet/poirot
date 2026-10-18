use clap::Parser;
use poirot_test_runner::fods::{
    Error,
    cli::{Cli, Command},
    commands,
};
use std::process::ExitCode;

fn dispatch(cli: Cli) -> Result<bool, Error> {
    match cli.command {
        Some(Command::Run(args)) => commands::run::run(&args),
        Some(Command::Inspect(args)) => commands::inspect::run(&args),
        Some(Command::Refresh(args)) => commands::refresh::run(&args),
        Some(Command::Classify(args)) => commands::classify::run(&args),
        None => commands::run::run(&cli.run),
    }
}

fn main() -> ExitCode {
    // clap reports invalid CLI usage with exit code 2 and help/version with 0.
    match dispatch(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("poirot-test-fods: {error}");
            ExitCode::from(1)
        }
    }
}
