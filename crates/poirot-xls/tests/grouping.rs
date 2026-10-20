//! Raw token structure and checked poirot readback; stale caches cannot prove grouping.
//! Standalone calamine currently cannot decode the source-required MemArea token.
use calamine::{Reader, Xls};
use poirot_model::{CellValue, Workbook};
use poirot_parsing::formula::{Expr, parse_formula};
use std::io::{Cursor, Read};

fn exported(formula: &str) -> (Vec<u8>, String) {
    let mut book = Workbook::new();
    book.add_sheet("Data").unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value("D1".parse().unwrap(), formula, CellValue::Number(-999.))
        .unwrap();
    let bytes = poirot_xls::write_xls_to_buffer(&book).unwrap();
    let mut file = cfb::CompoundFile::open(Cursor::new(bytes.clone())).unwrap();
    let mut stream = Vec::new();
    file.open_stream("/Workbook")
        .unwrap()
        .read_to_end(&mut stream)
        .unwrap();
    let mut position = 0;
    let mut tokens = None;
    while position + 4 <= stream.len() {
        let id = u16::from_le_bytes(stream[position..position + 2].try_into().unwrap());
        let len =
            u16::from_le_bytes(stream[position + 2..position + 4].try_into().unwrap()) as usize;
        if id == 6 {
            let record = &stream[position + 4..position + 4 + len];
            let count = u16::from_le_bytes(record[20..22].try_into().unwrap()) as usize;
            tokens = Some(record[22..22 + count].to_vec());
            break;
        }
        position += 4 + len;
    }
    // Preserve the independent backend gap rather than treating its diagnostic
    // string as a decoded formula. No POI/native execution is claimed here.
    let mut reader = Xls::new(Cursor::new(bytes.clone())).unwrap();
    let formulas = reader.worksheet_formula("Data").unwrap();
    let backend = formulas.get_value((0, 3)).unwrap();
    assert!(
        backend.contains("Unrecognized") && backend.contains("val: 38"),
        "{backend}"
    );

    let imported = poirot_io::read_workbook_bytes(&bytes).unwrap();
    let cell = imported
        .sheet(0)
        .unwrap()
        .get("D1".parse().unwrap())
        .unwrap();
    assert_eq!(cell.value, CellValue::Number(-999.));
    let reconstructed = cell.formula.clone().unwrap();
    assert_eq!(
        parse_formula(&reconstructed).unwrap(),
        parse_formula(formula).unwrap()
    );
    (tokens.unwrap(), reconstructed)
}

fn argument_count(formula: &str) -> usize {
    let Expr::Function { args, .. } = parse_formula(formula).unwrap() else {
        panic!("reader did not preserve outer function")
    };
    args.len()
}

#[test]
fn grouped_union_preserves_sum_argument_boundary() {
    let (tokens, formula) = exported("SUM(($A$1,$B$1),$C$1)");
    assert_eq!(
        tokens,
        [
            0x26, 0, 0, 0, 0, 11, 0, // MemArea: zero reserved field, 11-byte union
            0x24, 0, 0, 0, 0, // Reference-class absolute A1
            0x24, 0, 0, 1, 0, // Reference-class absolute B1
            0x10, 0x15, // Union then the grouped function argument
            0x24, 0, 0, 2, 0, // Reference-class absolute C1
            0x42, 2, 4, 0, // SUM, Value result, two arguments
        ]
    );
    assert_eq!(
        argument_count(&formula),
        2,
        "union must remain one function argument"
    );
    assert_eq!(
        &tokens[17..19],
        &[0x10, 0x15],
        "PtgUnion followed by PtgParen"
    );
    assert_eq!(&tokens[tokens.len() - 4..], &[0x42, 2, 4, 0]);
}

#[test]
fn grouped_union_preserves_lazy_if_argument_boundary() {
    let (tokens, formula) = exported("IF(TRUE,($A$1,$A$2),3)");
    assert_eq!(
        tokens,
        [
            0x1d, 1, // TRUE
            0x26, 0, 0, 0, 0, 11, 0, // MemArea: zero reserved field, 11-byte union
            0x24, 0, 0, 0, 0, // Reference-class absolute A1
            0x24, 1, 0, 0, 0, // Reference-class absolute A2
            0x10, 0x15, // Union then the grouped selected branch
            0x1e, 3, 0, // IntPtg(3)
            0x42, 3, 1, 0, // IF, requested Value result, three arguments
        ]
    );
    assert_eq!(
        argument_count(&formula),
        3,
        "union must remain the selected branch argument"
    );
    assert_eq!(
        &tokens[19..21],
        &[0x10, 0x15],
        "PtgUnion followed by PtgParen"
    );
    assert_eq!(&tokens[tokens.len() - 4..], &[0x42, 3, 1, 0]);
}
