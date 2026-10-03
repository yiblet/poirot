use poirot_test_fods::corpus::{CorpusManifest, FixtureEntry, load_fixture};
use poirot_test_fods::fods::model::{CellInput, FodsWorkbook, InputValue};
use poirot_test_fods::fods::{extract_bytes, fixture_data, translate};
use std::path::PathBuf;

fn document(body: &str) -> String {
    format!(
        r#"<o:document xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:x="urn:oasis:names:tc:opendocument:xmlns:text:1.0">{body}</o:document>"#
    )
}
fn parse(body: &str) -> FodsWorkbook {
    extract_bytes(
        document(body).as_bytes(),
        "sc/qa/unit/data/functions/test.fods",
    )
    .unwrap()
}
fn cells(data: &FodsWorkbook) -> &[CellInput] {
    &data.sheets[0].cells
}
fn entries() -> Vec<FixtureEntry> {
    let manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(corpus().join("manifest.json")).unwrap()).unwrap();
    manifest.fixtures
}
fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

#[test]
fn formula_punctuation_preserves_literals_and_array_axes() {
    assert_eq!(
        translate(r#"of:=IF([.A1];"a;b|c";{1;2|3;4})"#),
        r#"IF(A1,"a;b|c",{1,2;3,4})"#
    );
    assert_eq!(
        translate(r#"of:=CONCAT("a"";b";[.$B$2])"#),
        r#"CONCAT("a"";b",$B$2)"#
    );
    assert_eq!(
        translate("of:=COM.MICROSOFT.SUMIFS([$'Input data'.$A$1:.$A$5];[.$B$1:.$B$5];1)"),
        "SUMIFS('Input data'!$A$1:$A$5,$B$1:$B$5,1)"
    );
}

#[test]
fn namespace_prefixes_and_odf_whitespace_are_semantic() {
    let data = parse(
        r#"<t:table t:name="Text"><t:table-row><t:table-cell o:value-type="string"><x:p><x:s x:c="3"/>a<x:tab/>b<x:line-break/>c<x:s/></x:p></t:table-cell></t:table-row></t:table>"#,
    );
    assert_eq!(
        cells(&data)[0].value,
        InputValue::Text {
            value: "   a\tb\nc ".into()
        }
    );
}

#[test]
fn literal_repetitions_preserve_row_and_column_coordinates() {
    let data = parse(
        r#"<t:table t:name="Repeated"><t:table-row t:number-rows-repeated="2"><t:table-cell t:number-columns-repeated="3"/><t:table-cell t:number-columns-repeated="2" o:value-type="float" o:value="7"/></t:table-row></t:table>"#,
    );
    let positions: Vec<_> = cells(&data).iter().map(|c| (c.row, c.col)).collect();
    assert_eq!(positions, vec![(0, 3), (0, 4), (1, 3), (1, 4)]);
}

#[test]
fn trailing_blank_padding_beyond_sheet_bounds_is_not_materialized() {
    // The pinned dec2bin workbook contains blank padding two rows past Excel's limit.
    let data = parse(
        r#"<t:table t:name="Padding"><t:table-row><t:table-cell o:value-type="float" o:value="7"/></t:table-row><t:table-row t:number-rows-repeated="1048577"><t:table-cell/></t:table-row></t:table>"#,
    );
    assert_eq!(cells(&data).len(), 1);
    assert_eq!(cells(&data)[0].value, InputValue::number(7.0));
}

#[test]
fn expected_literal_and_correct_predicate_are_independent_of_target_cache() {
    let data = parse(
        r#"<t:table t:name="Cases"><t:table-row><t:table-cell o:value-type="string"><x:p>Function</x:p></t:table-cell><t:table-cell o:value-type="string"><x:p>Expected</x:p></t:table-cell><t:table-cell o:value-type="string"><x:p>Correct</x:p></t:table-cell></t:table-row><t:table-row><t:table-cell t:formula="of:=ROUND(1.25;1)" o:value-type="float" o:value="999"/><t:table-cell o:value-type="float" o:value="1.3"/><t:table-cell t:formula="of:=ABS([.A2]-[.B2])&lt;0.00001" o:value-type="boolean" o:boolean-value="true"/></t:table-row></t:table>"#,
    );
    assert_eq!(data.cases.len(), 1);
    assert_eq!(data.cases[0].expected.value, InputValue::number(1.3));
    assert_eq!(
        data.cases[0].original_correct_formula,
        "of:=ABS([.A2]-[.B2])<0.00001"
    );
    assert_eq!(
        cells(&data)
            .iter()
            .find(|c| c.row == 1 && c.col == 0)
            .unwrap()
            .value,
        InputValue::Blank
    );
}

#[test]
fn matrix_continuation_cache_is_not_literal_input() {
    let data = parse(
        r#"<t:table t:name="Array"><t:table-row><t:table-cell t:formula="of:=TRANSPOSE([.D1:.D2])" t:number-matrix-columns-spanned="2" t:number-matrix-rows-spanned="1" o:value-type="float" o:value="123"/><t:table-cell o:value-type="float" o:value="456"/><t:table-cell/><t:table-cell o:value-type="float" o:value="3"/></t:table-row><t:table-row><t:table-cell t:number-columns-repeated="3"/><t:table-cell o:value-type="float" o:value="4"/></t:table-row></t:table>"#,
    );
    assert!(
        cells(&data)
            .iter()
            .all(|c| c.value != InputValue::number(123.0) && c.value != InputValue::number(456.0))
    );
    assert_eq!(
        cells(&data).iter().find(|c| c.col == 1).unwrap().value,
        InputValue::Blank
    );
    let inputs: Vec<_> = cells(&data)
        .iter()
        .filter(|c| c.col == 3)
        .map(|c| c.value.clone())
        .collect();
    assert_eq!(
        inputs,
        vec![InputValue::number(3.0), InputValue::number(4.0)]
    );
}

#[test]
fn default_calc_date_origin_and_absolute_named_reference_are_retained() {
    let data = parse(
        r#"<t:table t:name="Cases"><t:table-row><t:table-cell o:value-type="date" o:date-value="1900-02-28"/></t:table-row></t:table><t:named-expressions><t:named-range t:name="source" t:base-cell-address="$Cases.$D$3" t:cell-range-address="$Cases.$D$3"/></t:named-expressions>"#,
    );
    assert!(matches!(
        cells(&data)[0].value,
        InputValue::Number { value: 60.0, .. }
    ));
    assert_eq!(data.names[0].formula, "Cases!$D$3");
}

#[test]
fn native_mmult_loader_preserves_source_and_blanks_all_computed_cells() {
    let data = fixture_data(
        &corpus().join("fixtures/array/fods/mmult.fods"),
        "sc/qa/unit/data/functions/array/fods/mmult.fods",
    )
    .unwrap();
    assert_eq!(
        data.source,
        "sc/qa/unit/data/functions/array/fods/mmult.fods"
    );
    let computed: Vec<_> = data
        .sheets
        .iter()
        .flat_map(|s| &s.cells)
        .filter(|c| c.formula.is_some() || c.matrix.is_some())
        .collect();
    assert!(computed.iter().any(|c| c.matrix_continuation == Some(true)));
    assert!(computed.iter().all(|c| c.value.is_blank()));
}

#[test]
fn malformed_native_xml_returns_error() {
    assert!(extract_bytes(b"<broken>", "malformed.fods").is_err());
}

#[test]
fn runtime_loader_rejects_tampered_native_bytes() {
    let root = corpus();
    let entry = entries()
        .into_iter()
        .find(|e| e.fixture == "text/fods/trim.fods")
        .unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temporary = std::env::temp_dir().join(format!(
        "poirot-fods-integrity-{}-{stamp}",
        std::process::id()
    ));
    let path = temporary.join("fixtures/text/fods/trim.fods");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut bytes = std::fs::read(root.join("fixtures/text/fods/trim.fods")).unwrap();
    bytes.push(b'\n');
    std::fs::write(&path, bytes).unwrap();
    let result = load_fixture(&temporary, &entry);
    std::fs::remove_dir_all(temporary).unwrap();
    assert!(
        result.is_err(),
        "changed pinned bytes must fail integrity checking"
    );
}

#[test]
fn runtime_loader_rejects_manifest_case_count_drift() {
    let root = corpus();
    let mut entry = entries()
        .into_iter()
        .find(|e| e.fixture == "text/fods/trim.fods")
        .unwrap();
    entry.cases += 1;
    assert!(load_fixture(&root, &entry).is_err());
}
