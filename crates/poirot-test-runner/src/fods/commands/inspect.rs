use crate::fods::{
    FodsError,
    cli::{InspectArgs, OutputFormat},
    corpus::{self, Corpus},
};
use std::io::Write;

/// Load and render the selected fixture or source case.
pub fn run(args: &InspectArgs) -> Result<bool, FodsError> {
    let corpus = Corpus::open(&args.corpus)?;
    let entry = corpus
        .manifest
        .fixtures
        .iter()
        .find(|entry| entry.fixture == args.fixture)
        .ok_or_else(|| FodsError::UnknownFixture {
            fixture: args.fixture.clone(),
        })?;
    let workbook = corpus::load_fixture(&corpus.root, entry)?;
    let selected = args
        .case
        .as_ref()
        .map(|id| {
            workbook
                .cases
                .iter()
                .find(|case| case.id == *id)
                .ok_or_else(|| FodsError::UnknownCase {
                    fixture: args.fixture.clone(),
                    case: id.clone(),
                })
        })
        .transpose()?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match args.format {
        OutputFormat::Json => {
            if let Some(case) = selected {
                serde_json::to_writer(&mut output, case).map_err(json_output_error)?;
            } else {
                serde_json::to_writer(&mut output, &workbook).map_err(json_output_error)?;
            }
            writeln!(output).map_err(FodsError::Output)?;
        }
        OutputFormat::Text => {
            writeln!(
                output,
                "{}: {} sheets, {} cases, {} omissions",
                entry.fixture,
                workbook.sheets.len(),
                workbook.cases.len(),
                workbook.omissions.len()
            )
            .map_err(FodsError::Output)?;
            for case in workbook
                .cases
                .iter()
                .filter(|case| selected.is_none_or(|selected| case.id == selected.id))
            {
                writeln!(
                    output,
                    "{} @ {}!{}: {}",
                    case.id, case.sheet_name, case.cell, case.original_formula
                )
                .map_err(FodsError::Output)?;
                writeln!(
                    output,
                    "  expected: {:?}; check: {}",
                    case.expected.value, case.original_correct_formula
                )
                .map_err(FodsError::Output)?;
            }
            if selected.is_none() {
                for omission in &workbook.omissions {
                    writeln!(
                        output,
                        "  omitted {}!{}: {}",
                        omission.sheet.as_deref().unwrap_or("?"),
                        omission.cell.as_deref().unwrap_or("?"),
                        omission.reason
                    )
                    .map_err(FodsError::Output)?;
                }
            }
        }
    }
    output.flush().map_err(FodsError::Output)?;
    Ok(true)
}

fn json_output_error(error: serde_json::Error) -> FodsError {
    FodsError::JsonOutput(error)
}
