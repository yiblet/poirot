use crate::fods::{FodsError, cli::RunArgs, execute, plan, report};
use std::io::Write;

/// Prepare and execute the selected corpus, then return its process exit status.
pub fn run(args: &RunArgs) -> Result<bool, FodsError> {
    if args.format == crate::fods::cli::OutputFormat::Text {
        eprintln!("Preparing FODS fixtures...");
    }
    let plan = plan::build(&plan::RunOptions {
        corpus: args.corpus.clone(),
        filter: args.filter.clone(),
        jobs: args.jobs,
    })?;
    let output = std::sync::Mutex::new(std::io::stdout());
    if args.format == crate::fods::cli::OutputFormat::Text {
        println!(
            "running {} tests",
            plan.fixtures
                .iter()
                .map(|fixture| fixture.cases.len())
                .sum::<usize>()
        );
    }
    let result = execute::execute_with_observer(&plan, &|case| {
        let mut writer = output
            .lock()
            .map_err(|_| FodsError::PoisonedProgressWriter)?;
        report::render_case(case, &mut *writer, args.format, args.strict)
    })?;
    let mut output = output
        .lock()
        .map_err(|_| FodsError::PoisonedProgressWriter)?;
    report::render_summary(&result, &mut *output, args.format, args.strict)?;
    output.flush().map_err(FodsError::Output)?;
    Ok(result.successful(args.strict))
}
