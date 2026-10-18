use super::*;
fn tokens(w: &Workbook, formula: &str, budget: usize) -> Result<Vec<u8>, XlsError> {
    compile(
        w,
        0,
        &poirot_parsing::formula::parse_formula(formula).unwrap(),
        0x20,
        budget,
    )
}
#[test]
fn static_area_bytes_remain_exact_and_dynamic_wrappers_have_exact_lengths() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    assert_eq!(
        tokens(&w, "A1:B3", 9).unwrap(),
        vec![0x45, 0, 0, 2, 0, 0, 0xc0, 1, 0xc0]
    );
    let bytes = tokens(&w, "OFFSET(B3,0,0,-2,-2):C4", 1024).unwrap();
    assert_eq!(bytes[0], 0x29);
    assert_eq!(
        u16::from_le_bytes([bytes[1], bytes[2]]) as usize,
        bytes.len() - 3
    );
    assert_eq!(*bytes.last().unwrap(), 0x11);
    assert!(tokens(&w, "OFFSET(B3,0,0,-2,-2):C4", bytes.len() - 1).is_err());
    let grouped = tokens(&w, "(A1:B3) (B2:C4)", 1024).unwrap();
    assert_eq!(grouped[0], 0x26);
    assert_eq!(&grouped[1..5], &[0; 4]);
    assert_eq!(
        u16::from_le_bytes([grouped[5], grouped[6]]) as usize,
        grouped.len() - 7
    );
    assert_eq!(*grouped.last().unwrap(), 0x0f);
}
#[test]
fn named_range_tokens_keep_scoped_identity_and_static_failures_do_not_fallback() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    w.define_name("Start", "A1", None).unwrap();
    w.define_name("End", "B3", None).unwrap();
    w.define_name("Z", "C4", None).unwrap();
    let bytes = tokens(&w, "Start:End", 1024).unwrap();
    assert_eq!(&bytes[3..8], &[0x23, 1, 0, 0, 0]);
    assert_eq!(&bytes[8..13], &[0x23, 2, 0, 0, 0]);
    assert_eq!(*bytes.last().unwrap(), 0x11);
    assert!(tokens(&w, "A1:Z", 1024).is_ok());
    w.define_name("End", "C5", Some(0)).unwrap();
    let scoped = tokens(&w, "Start:End", 1024).unwrap();
    assert_eq!(&scoped[8..13], &[0x23, 4, 0, 0, 0]);
    w.add_sheet("Other").unwrap();
    for source in ["A1:XFD1", "Data!A1:Other!B2", "'Data'!Start:End"] {
        assert!(tokens(&w, source, 1024).is_err(), "{source}");
    }
}

#[test]
fn array_operand_metadata_transports_references_without_enabling_array_constants() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    let bytes = tokens(&w, "ROWS(OFFSET(B3,0,0,-2,-2):C4)", 1024).unwrap();
    assert_eq!(bytes[0], 0x29);
    assert_eq!(&bytes[bytes.len() - 3..], &[0x41, 76, 0]);
    assert!(tokens(&w, "ROWS({1,2})", 1024).is_err());
    assert!(tokens(&w, "ROWS(TRANSPOSE(A1:B2))", 1024).is_err());
}
