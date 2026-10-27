//! Customer coordinate inputs stay checked across reads and edits.
use poirot::model::CellStyle;
use poirot::{Cell, CellAddress, CellValue, Workbook, Worksheet};

#[test]
fn natural_coordinates_cover_generated_and_authored_data() -> Result<(), Box<dyn std::error::Error>>
{
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data")?;
    sheet.set_value((0, 0), 1.)?;
    for (row, values) in [[2., 3.], [4., 5.]].iter().enumerate() {
        for (col, value) in values.iter().enumerate() {
            sheet.set_value((row, col), *value)?;
        }
    }
    let address = CellAddress::new(0, 0)?;
    let owned = String::from("$a$1");
    let borrowed = &address;
    assert_eq!(sheet.get(address)?, sheet.get(borrowed)?);
    assert_eq!(sheet.get(&owned)?, sheet.get(owned.clone())?);
    assert_eq!(
        sheet.get(owned.as_str())?.ok_or("missing input")?.value,
        CellValue::Number(2.)
    );
    sheet.set_formula("C1", "A1+B1")?;
    sheet.set_style(
        (0usize, 2usize),
        CellStyle {
            number_format: Some("0.00".into()),
            ..Default::default()
        },
    )?;
    sheet.set_formula_with_cached_value("C1", "A1+B1", CellValue::Number(5.))?;
    sheet.get_mut("C1")?.ok_or("missing formula")?.value = CellValue::Number(6.);
    sheet.set("D1", Cell::default())?;
    assert!(sheet.get("D1")?.is_some());
    assert!(sheet.get("E1")?.is_none());
    assert!(sheet.remove(String::from("D1"))?.is_some());
    assert!(sheet.remove("D1")?.is_none());
    assert!(book.stored_cell("Missing", "A1")?.is_none());
    assert_eq!(
        book.stored_cell("Data", (0, 0))?
            .ok_or("missing stored input")?
            .value,
        CellValue::Number(2.)
    );
    Ok(())
}

#[test]
fn invalid_coordinates_fail_without_mutating_the_sheet() {
    let mut sheet = Worksheet::new("Data").unwrap();
    sheet
        .set_formula_with_cached_value("A1", "1+1", CellValue::Number(2.))
        .unwrap();
    sheet
        .set_style(
            "A1",
            CellStyle {
                number_format: Some("0.00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let original = sheet.clone();
    for address in ["", "A0", "XFE1", "A1048577", "Data!A1", "💥1"] {
        assert!(sheet.get(address).is_err());
        assert!(sheet.get_mut(address).is_err());
        assert!(sheet.set(address, Cell::default()).is_err());
        assert!(sheet.set_value(address, 9.).is_err());
        assert!(sheet.set_formula(address, "9").is_err());
        assert!(
            sheet
                .set_formula_with_cached_value(address, "9", CellValue::Number(9.))
                .is_err()
        );
        assert!(sheet.set_style(address, CellStyle::default()).is_err());
        assert!(sheet.remove(address).is_err());
        assert_eq!(sheet, original);
    }
    let invalid = CellAddress {
        row: u32::MAX,
        col: 0,
    };
    assert!(sheet.get(invalid).is_err());
    let borrowed_invalid = &invalid;
    assert!(sheet.get_mut(borrowed_invalid).is_err());
    assert!(sheet.set_value(invalid, 9.).is_err());
    assert!(sheet.remove(invalid).is_err());
    assert!(sheet.set_value((-1, 0), 9.).is_err());
    assert!(sheet.set_value((usize::MAX, 0usize), 9.).is_err());
    assert!(sheet.set_value((0, 16_384), 9.).is_err());
    assert_eq!(sheet, original);
    let book = Workbook::new();
    assert!(book.stored_cell("Missing", invalid).is_err());
    assert!(book.stored_cell("Missing", "A0").is_err());
}
