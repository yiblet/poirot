use super::*;
use poirot_model::{ExcelText, TextRef};

fn resource(error: &XlsError) -> bool {
    match error {
        XlsError::ResourceLimit(_) => true,
        XlsError::Context { source, .. } => resource(source),
        _ => false,
    }
}

fn records(bytes: &[u8]) -> Vec<(u16, &[u8])> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let id = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let size = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap()) as usize;
        result.push((id, &bytes[offset + 4..offset + 4 + size]));
        offset += 4 + size;
    }
    result
}

#[test]
fn lone_units_and_valid_pairs_have_exact_little_endian_records() {
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd800, 65],
        vec![0xd800, 0xd800, 0xdc00],
        vec![0xfffd],
    ] {
        let mut continued = strings::Continued::new(0x0207);
        continued.string(TextRef::Utf16(&units)).unwrap();
        let mut bytes = Vec::new();
        continued.finish(&mut bytes).unwrap();
        let parsed = records(&bytes);
        assert_eq!(parsed[0].0, 0x0207);
        let mut expected = (units.len() as u16).to_le_bytes().to_vec();
        expected.push(1);
        for unit in units {
            expected.extend(unit.to_le_bytes());
        }
        assert_eq!(parsed[0].1, expected);
    }
}

#[test]
fn continuation_preserves_pairs_and_lone_terminal_high() {
    // Prefix places a valid pair at the record boundary with only two bytes free.
    let units = [vec![65; 4109], vec![0xd83d, 0xde00, 0xd800]].concat();
    let mut continued = strings::Continued::new(0x0207);
    continued.string(TextRef::Utf16(&units)).unwrap();
    let mut bytes = Vec::new();
    continued.finish(&mut bytes).unwrap();
    let parsed = records(&bytes);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].1.len(), 8221);
    assert_eq!(parsed[1], (0x003c, &[1, 0x3d, 0xd8, 0, 0xde, 0, 0xd8][..]));
}

#[test]
fn headers_continue_without_character_option_byte() {
    let mut continued = strings::Continued::new(0x00fc);
    continued.data.resize(MAX_RECORD - 2, 0);
    continued.string(TextRef::Utf16(&[0xd800])).unwrap();
    let mut bytes = Vec::new();
    continued.finish(&mut bytes).unwrap();
    let parsed = records(&bytes);
    assert_eq!(parsed[1], (0x003c, &[1, 0, 1, 0, 0xd8][..]));
}

#[test]
fn literal_sst_cache_and_budget_are_unit_exact() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Data").unwrap();
    sheet
        .set_value(
            "A1".parse().unwrap(),
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    sheet
        .set_value("A2".parse().unwrap(), CellValue::Text("a".into()))
        .unwrap();
    sheet
        .set_value(
            "A3".parse().unwrap(),
            CellValue::TextUtf16(ExcelText::from_units(vec![97])),
        )
        .unwrap();
    sheet
        .set_formula_with_cached_value(
            "B1".parse().unwrap(),
            "A1",
            CellValue::TextUtf16(ExcelText::from_units(vec![0xdc00])),
        )
        .unwrap();
    let plan = XlsExportPlan::new(&workbook, XlsExportPolicy::default()).unwrap();
    let bytes = plan.encode_workbook_stream().unwrap();
    assert_eq!(bytes.len(), plan.predicted_stream_bytes());
    let parsed = records(&bytes);
    let sst = parsed.iter().find(|(id, _)| *id == 0x00fc).unwrap().1;
    assert_eq!(
        sst,
        &[3, 0, 0, 0, 2, 0, 0, 0, 1, 0, 1, 0, 0xd8, 1, 0, 1, 97, 0]
    );
    let cache = parsed.iter().find(|(id, _)| *id == 0x0006).unwrap().1;
    assert_eq!(&cache[6..14], &[0, 0, 0, 0, 0, 0, 0xff, 0xff]);
    assert_eq!(
        parsed.iter().find(|(id, _)| *id == 0x0207).unwrap().1,
        &[1, 0, 1, 0, 0xdc]
    );
    let mut policy = XlsExportPolicy {
        max_stream_bytes: bytes.len() - 1,
        ..XlsExportPolicy::default()
    };
    assert!(resource(
        &XlsExportPlan::new(&workbook, policy).err().unwrap()
    ));
    policy.max_stream_bytes = bytes.len();
    assert!(XlsExportPlan::new(&workbook, policy).is_ok());
}

#[test]
fn raw_text_limit_is_admitted_before_encoding() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .unwrap()
        .set_value(
            "A1".parse().unwrap(),
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800; 32768])),
        )
        .unwrap();
    assert!(resource(
        &XlsExportPlan::new(&workbook, XlsExportPolicy::default())
            .err()
            .unwrap()
    ));
}

#[test]
fn continuation_sizes_are_admitted_for_literal_and_formula_cache() {
    for units in [
        vec![0xd800; 9000],
        [vec![65; 4109], vec![0xd83d, 0xde00, 0xd800]].concat(),
    ] {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("Data").unwrap();
        let value = CellValue::TextUtf16(ExcelText::from_units(units));
        sheet
            .set_value("A1".parse().unwrap(), value.clone())
            .unwrap();
        sheet
            .set_formula_with_cached_value("B1".parse().unwrap(), "A1", value)
            .unwrap();
        let plan = XlsExportPlan::new(&workbook, XlsExportPolicy::default()).unwrap();
        let bytes = plan.encode_workbook_stream().unwrap();
        assert_eq!(plan.predicted_stream_bytes(), bytes.len());
        let parsed = records(&bytes);
        assert!(parsed.iter().filter(|(id, _)| *id == 0x003c).count() >= 2);
        let policy = XlsExportPolicy {
            max_stream_bytes: bytes.len() - 1,
            ..XlsExportPolicy::default()
        };
        assert!(resource(
            &XlsExportPlan::new(&workbook, policy).err().unwrap()
        ));
    }
}
