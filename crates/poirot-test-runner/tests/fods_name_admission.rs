//! Independently authored synthetic controls; no upstream fixture material is reused.
use poirot_test_runner::fods::{
    corpus::{Baseline, BaselineCase, BaselineStatus, CorpusManifest, FixtureEntry},
    execute,
    plan::{self, RunOptions, WorkbookPreparation},
    report::{CaseStatus, NameDisposition},
    xml::{SOURCE_COMMIT, extract_bytes},
};
use std::{
    num::NonZeroUsize,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

/// One independently authored temporary native fixture and its integrity metadata.
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn escape(source: &str) -> String {
    source
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn fixture(
    actual: &str,
    expected: u32,
    correct: &str,
    extra: &str,
    definitions: &[(&str, &str)],
) -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "poirot-names-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(directory.join("fixtures")).unwrap();
    let names = definitions.iter().map(|(name, formula)| format!(
        "<t:named-expression t:name=\"{}\" t:expression=\"{}\" t:base-cell-address=\"Main.A1\"/>", escape(name), escape(formula))).collect::<String>();
    let source = format!(
        r#"<o:document xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:x="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><t:named-expressions>{names}</t:named-expressions><t:table t:name="Main"><t:table-row><t:table-cell o:value-type="string"><x:p>Function</x:p></t:table-cell><t:table-cell o:value-type="string"><x:p>Expected</x:p></t:table-cell><t:table-cell o:value-type="string"><x:p>Correct</x:p></t:table-cell></t:table-row><t:table-row><t:table-cell t:formula="{}" o:value-type="float" o:value="987654"/><t:table-cell o:value-type="float" o:value="{expected}"/><t:table-cell t:formula="{}" o:value-type="boolean" o:boolean-value="true"/></t:table-row></t:table>{extra}</o:document>"#,
        escape(actual),
        escape(correct)
    );
    let parsed = extract_bytes(source.as_bytes(), "independent/names.fods").unwrap();
    assert_eq!(parsed.cases.len(), 1);
    assert!(
        parsed
            .sheets
            .iter()
            .flat_map(|s| &s.cells)
            .filter(|c| c.formula.is_some() || c.matrix.is_some())
            .all(|c| c.value.is_blank())
    );
    assert_eq!(
        parsed
            .names
            .iter()
            .map(|n| n.original_definition.as_deref().unwrap())
            .collect::<Vec<_>>(),
        definitions
            .iter()
            .map(|(_, source)| *source)
            .collect::<Vec<_>>()
    );
    let case = &parsed.cases[0];
    let entry = FixtureEntry {
        fixture: "names.fods".into(),
        id: "independent__names".into(),
        source: parsed.source.clone(),
        sha256: parsed.sha256.clone(),
        cases: 1,
    };
    let manifest = CorpusManifest {
        source_repository: "independently-authored-test".into(),
        source_commit: SOURCE_COMMIT.into(),
        license: "MPL-2.0".into(),
        fixtures: vec![entry],
        case_count: 1,
        omissions: vec![],
    };
    let baseline = Baseline {
        source_commit: SOURCE_COMMIT.into(),
        cases: vec![BaselineCase {
            id: case.id.clone(),
            fixture: "names.fods".into(),
            index: 0,
            sheet: Some(case.sheet_name.clone()),
            cell: Some(case.cell.clone()),
            classification: BaselineStatus::Gap,
            reason: None,
            gap_type: None,
        }],
    };
    std::fs::write(directory.join("fixtures/names.fods"), source).unwrap();
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("classifications.json"),
        serde_json::to_vec(&baseline).unwrap(),
    )
    .unwrap();
    Fixture(directory)
}
fn build(fixture: &Fixture) -> plan::RunPlan {
    plan::build(&RunOptions {
        corpus: fixture.0.clone(),
        filter: None,
        jobs: NonZeroUsize::new(1).unwrap(),
    })
    .unwrap()
}
fn extra_formula(formula: &str, matrix: bool) -> String {
    format!(
        "<t:table t:name=\"Unselected\"><t:table-row><t:table-cell t:formula=\"{}\" {}/></t:table-row></t:table>",
        escape(formula),
        if matrix {
            "t:number-matrix-rows-spanned=\"2\" t:number-matrix-columns-spanned=\"1\""
        } else {
            ""
        }
    )
}
#[test]
fn unused_rejection_is_explicit_and_both_actual_and_correct_execute() {
    for (expected, correct, passes) in [
        (42, "of:=[.A2]=[.B2]", true),
        (41, "of:=[.A2]=[.B2]", false),
        (42, "of:=FALSE()", false),
    ] {
        let fixture = fixture("of:=6*7", expected, correct, "", &[("GQ17", "of:=123")]);
        let plan = build(&fixture);
        assert!(matches!(
            plan.fixtures[0].workbook,
            WorkbookPreparation::Ready(_)
        ));
        let diagnostic = &plan.fixtures[0].preparation_diagnostics[0];
        assert_eq!(diagnostic.name, "GQ17");
        assert_eq!(diagnostic.original_definition.as_deref(), Some("of:=123"));
        assert_eq!(diagnostic.base_cell_address.as_deref(), Some("Main.A1"));
        assert_eq!(diagnostic.rejection, "invalid defined name");
        assert_eq!(diagnostic.disposition, NameDisposition::QuarantinedUnused);
        let report = execute::execute(&plan).unwrap();
        assert_eq!(
            report.cases[0].status == CaseStatus::Improvement,
            passes,
            "{report:?}"
        );
        let mut older = serde_json::to_value(&report.cases[0]).unwrap();
        older
            .as_object_mut()
            .unwrap()
            .remove("preparation_diagnostics");
        let decoded: poirot_test_runner::fods::report::CaseResult =
            serde_json::from_value(older).unwrap();
        assert!(decoded.preparation_diagnostics.is_empty());
        assert_eq!(
            report.cases[0].preparation_diagnostics,
            plan.fixtures[0].preparation_diagnostics
        );
        let mut json = Vec::new();
        poirot_test_runner::fods::report::render(
            &report,
            &mut json,
            poirot_test_runner::fods::cli::OutputFormat::Json,
            false,
        )
        .unwrap();
        assert!(
            String::from_utf8(json)
                .unwrap()
                .contains("quarantined_unused")
        );
    }
}
#[test]
fn every_potential_source_use_retains_the_workbook_blocker() {
    for source in [
        "of:=GQ17",
        "of:=gq17",
        "of:=$GQ$17",
        "of:=GQ$17",
        "of:=[Main.GQ17]",
        "of:='Main'.GQ17",
        "of:=[.GQ17]",
        "of:=\"GQ17\"",
        "of:=IF(TRUE();42;GQ17)",
        "of:=GQ170",
        "of:=INDIRECT(\"GQ\"&\"17\")",
        "of:=IF(TRUE();42;COM.MICROSOFT.INDIRECT(\"GQ\"&17))",
        "of:=_xlfn.INDIRECT(\"A1\")",
        "of:=EVALUATE(\"6*7\")",
        "of:=42+é",
        "of:=42@1",
        "of:=(42",
        "of:=\"unterminated",
    ] {
        for matrix in [false, true] {
            let fixture = fixture(
                "of:=6*7",
                42,
                "of:=[.A2]=[.B2]",
                &extra_formula(source, matrix),
                &[("GQ17", "of:=123")],
            );
            let plan = build(&fixture);
            assert!(
                matches!(
                    plan.fixtures[0].workbook,
                    WorkbookPreparation::Unsupported(_)
                ),
                "{source}"
            );
            assert_eq!(
                plan.fixtures[0].preparation_diagnostics[0].disposition,
                NameDisposition::BlocksPreparation,
                "{source}"
            );
            let report = execute::execute(&plan).unwrap();
            assert_eq!(report.cases[0].status, CaseStatus::KnownGap, "{source}");
            assert!(
                report.cases[0]
                    .failure
                    .as_ref()
                    .unwrap()
                    .message
                    .contains("non-use not proved")
            );
        }
    }
}
#[test]
fn definitions_are_inspected_transitively_and_valid_names_keep_their_contract() {
    let fixture = fixture(
        "of:=6*7",
        42,
        "of:=[.A2]=[.B2]",
        "",
        &[("GQ17", "of:=123"), ("bridge", "of:=GQ17")],
    );
    assert!(matches!(
        build(&fixture).fixtures[0].workbook,
        WorkbookPreparation::Unsupported(_)
    ));
    let fixture = self::fixture(
        "of:=factor*7",
        42,
        "of:=[.A2]=[.B2]",
        "",
        &[("GQ17", "of:=123"), ("factor", "of:=6")],
    );
    let plan = build(&fixture);
    let report = execute::execute(&plan).unwrap();
    assert_eq!(report.cases[0].status, CaseStatus::Improvement);
    let fixture = self::fixture(
        "of:=6*7",
        42,
        "of:=[.A2]=[.B2]",
        "",
        &[("factor", "of:=6"), ("factor", "of:=7")],
    );
    let plan = build(&fixture);
    assert!(matches!(
        plan.fixtures[0].workbook,
        WorkbookPreparation::Unsupported(_)
    ));
    assert!(plan.fixtures[0].preparation_diagnostics.is_empty());
}
#[test]
fn quarantine_does_not_weaken_public_model_name_validation() {
    let mut book = poirot::model::Workbook::new();
    assert!(book.define_name("GQ17", "123", None).is_err());
    assert!(book.define_name("factor", "6", None).is_ok());
}

#[test]
fn omitted_linked_formula_sources_are_retained_and_inspected() {
    let linked = r#"<t:table t:name="Linked"><t:table-source xmlns:l="http://www.w3.org/1999/xlink" l:href="file:///unavailable.ods" t:mode="copy-results-only"/><t:table-row><t:table-cell t:formula="of:=GQ17"/></t:table-row></t:table>"#;
    let fixture = fixture(
        "of:=6*7",
        42,
        "of:=[.A2]=[.B2]",
        linked,
        &[("GQ17", "of:=123")],
    );
    let plan = build(&fixture);
    assert!(matches!(
        plan.fixtures[0].workbook,
        WorkbookPreparation::Unsupported(_)
    ));
    assert!(
        plan.fixtures[0].preparation_diagnostics[0]
            .reason
            .contains("potential rejected-name")
    );
}

#[test]
fn complete_linked_source_inspection_does_not_install_external_caches() {
    let linked = r#"<t:table t:name="Linked"><t:table-source xmlns:l="http://www.w3.org/1999/xlink" l:href="file:///unavailable.ods" t:mode="copy-results-only"/><t:table-row><t:table-cell t:formula="of:=7*8" o:value-type="float" o:value="999"/></t:table-row></t:table>"#;
    let fixture = fixture(
        "of:=6*7",
        42,
        "of:=[.A2]=[.B2]",
        linked,
        &[("GQ17", "of:=123")],
    );
    let plan = build(&fixture);
    let WorkbookPreparation::Ready(book) = &plan.fixtures[0].workbook else {
        panic!("unused declaration remains blocked")
    };
    assert_eq!(book.sheets().len(), 1);
    assert_eq!(
        execute::execute(&plan).unwrap().cases[0].status,
        CaseStatus::Improvement
    );
}
