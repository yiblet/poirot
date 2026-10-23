//! Change A1 in the first sheet of a workbook, recalculate, and save an XLSX file.
//! Run with INPUT.xlsx OUTPUT.xlsx; A1 should be an input in the chosen workbook.
//! Cell/cache edits preserve untouched XLSX package features through retained output.
use poirot::Workbook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .ok_or("usage: public_workbook INPUT.xlsx OUTPUT.xlsx")?;
    let output = args
        .next()
        .ok_or("usage: public_workbook INPUT.xlsx OUTPUT.xlsx")?;
    if args.next().is_some() {
        return Err("usage: public_workbook INPUT.xlsx OUTPUT.xlsx".into());
    }

    let mut book = Workbook::open(input)?;
    if let Some(metadata) = &book.file_metadata {
        for warning in &metadata.import_warnings {
            eprintln!("{warning}");
        }
    }

    let sheet = book.sheet_mut(0).ok_or("the workbook has no sheets")?;
    sheet.set_value("A1".parse()?, 24.)?;
    book.recalculate()?;
    book.save_xlsx().write_file(output)?;
    Ok(())
}
