use poirot_test_runner::fods::{
    corpus::{Baseline, BaselineCase, BaselineStatus, CorpusManifest, FixtureEntry},
    execute,
    plan::{self, RunOptions},
    report::CaseStatus,
    xml::{SOURCE_COMMIT, extract_bytes},
};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

/// Temporary corpus directory owned by one runner integration test.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "poirot-fods-runner-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(sheet: &str, formula: &str, expected: f64) -> String {
    format!(
        r#"<o:document xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:x="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><t:table t:name="{sheet}"><t:table-row><t:table-cell o:value-type="string"><x:p>Function</x:p></t:table-cell><t:table-cell o:value-type="string"><x:p>Expected</x:p></t:table-cell><t:table-cell o:value-type="string"><x:p>Correct</x:p></t:table-cell></t:table-row><t:table-row><t:table-cell t:formula="of:={formula}" o:value-type="float" o:value="999"/><t:table-cell o:value-type="float" o:value="{expected}"/><t:table-cell t:formula="of:=[.A2]=[.B2]" o:value-type="boolean" o:boolean-value="true"/></t:table-row></t:table></o:document>"#
    )
}
fn json(path: &Path, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn corpus(root: &Path) {
    std::fs::create_dir(root.join("fixtures")).unwrap();
    let mut fixtures = Vec::new();
    let mut cases = Vec::new();
    for (key, sheet, formula, expected, status) in [
        ("a", "Sheet1", "SUM(1;2)", 3., BaselineStatus::Pass),
        (
            "b",
            "Sheet1",
            "UNKNOWN_FODS_FUNCTION(1)",
            3.,
            BaselineStatus::Gap,
        ),
        ("c", "Sheet1", "SUM(1;2)", 4., BaselineStatus::Pass),
        ("d", "Sheet1", "SUM(1;2)", 3., BaselineStatus::Gap),
        ("e", "external/cache", "SUM(1;2)", 3., BaselineStatus::Gap),
    ] {
        let name = format!("{key}.fods");
        let upstream = format!("sc/qa/unit/data/functions/{name}");
        let bytes = source(sheet, formula, expected).into_bytes();
        let parsed = extract_bytes(&bytes, &upstream).unwrap();
        let case = &parsed.cases[0];
        fixtures.push(FixtureEntry {
            fixture: name.clone(),
            id: key.into(),
            source: upstream,
            sha256: parsed.sha256.clone(),
            cases: 1,
        });
        cases.push(BaselineCase {
            id: case.id.clone(),
            fixture: name.clone(),
            index: 0,
            sheet: Some(case.sheet_name.clone()),
            cell: Some(case.cell.clone()),
            classification: status,
            reason: None,
            gap_type: None,
        });
        std::fs::write(root.join("fixtures").join(name), bytes).unwrap();
    }
    json(
        &root.join("manifest.json"),
        &CorpusManifest {
            source_repository: "test-source".into(),
            source_commit: SOURCE_COMMIT.into(),
            license: "MPL-2.0".into(),
            fixtures,
            case_count: 5,
            omissions: vec![],
        },
    );
    json(
        &root.join("classifications.json"),
        &Baseline {
            source_commit: SOURCE_COMMIT.into(),
            cases,
        },
    );
}
fn options(root: &Path, jobs: usize, filter: Option<&str>) -> RunOptions {
    RunOptions {
        corpus: root.into(),
        jobs: NonZeroUsize::new(jobs).unwrap(),
        filter: filter.map(str::to_owned),
    }
}
fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_poirot-test-fods"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn countif_precision_gap_retains_libreoffice_oracle_and_is_still_executed() {
    use poirot_test_runner::fods::{
        corpus::{Corpus, default_corpus, load_fixture},
        gaps::GapKind,
        xml::model::InputValue,
    };
    let root = default_corpus();
    let corpus = Corpus::open(&root).unwrap();
    let id = "statistical__fods__countif2__00008";
    let baseline = corpus.baseline.get(id).unwrap();
    assert_eq!(baseline.classification, BaselineStatus::Gap);
    assert_eq!(baseline.gap_type, Some(GapKind::NumericMismatch));
    assert!(baseline.reason.as_deref().unwrap().contains("POI 5.4.1"));
    let fixture = corpus
        .manifest
        .fixtures
        .iter()
        .find(|entry| entry.fixture == baseline.fixture)
        .unwrap();
    // Loading also verifies the native fixture against its retained SHA-256.
    let source = load_fixture(&root, fixture).unwrap();
    let case = source.cases.iter().find(|case| case.id == id).unwrap();
    assert_eq!(case.formula, "COUNTIF(J10:J10,\"=\"&J10)");
    assert_eq!(case.expected.value, InputValue::number(1.0));
    let report =
        execute::execute(&plan::build(&options(&root, 1, Some("countif2"))).unwrap()).unwrap();
    let result = report.cases.iter().find(|case| case.id == id).unwrap();
    assert_eq!(result.status, CaseStatus::KnownGap);
    assert_eq!(
        result.failure.as_ref().unwrap().gap_type,
        GapKind::NumericMismatch
    );
    assert!(!report.successful(true));
}

#[test]
fn serial_and_rayon_results_match_and_distinguish_all_outcomes() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let serial = execute::execute(&plan::build(&options(&scratch.0, 1, None)).unwrap()).unwrap();
    let parallel = execute::execute(&plan::build(&options(&scratch.0, 4, None)).unwrap()).unwrap();
    assert_eq!(serial.cases, parallel.cases);
    assert_eq!(
        parallel.cases.iter().map(|c| c.status).collect::<Vec<_>>(),
        vec![
            CaseStatus::Pass,
            CaseStatus::KnownGap,
            CaseStatus::Regression,
            CaseStatus::Improvement,
            CaseStatus::KnownGap
        ]
    );
    assert_eq!(parallel.summary.harness_errors, 0);
    assert_eq!(parallel.summary.regressions, 1);
    assert_eq!(parallel.summary.passed, 2);
    assert!(!parallel.successful(false));
}

#[test]
fn every_completed_case_is_observed_once_and_output_errors_propagate() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let plan = plan::build(&options(&scratch.0, 3, None)).unwrap();
    let observed = std::sync::Mutex::new(Vec::new());
    let report = execute::execute_with_observer(&plan, &|case| {
        observed.lock().unwrap().push(case.id.clone());
        Ok(())
    })
    .unwrap();
    let mut observed = observed.into_inner().unwrap();
    observed.sort();
    let mut expected = report
        .cases
        .iter()
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(observed, expected);
    let broken = execute::execute_with_observer(&plan, &|_| {
        Err(poirot_test_runner::fods::Error::Output(
            std::io::Error::from(std::io::ErrorKind::BrokenPipe),
        ))
    });
    assert!(matches!(
        broken,
        Err(poirot_test_runner::fods::Error::Output(_))
    ));
}

#[test]
fn new_native_sources_get_execution_bindings_without_accepting_shared_baseline_entries() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    std::fs::remove_file(scratch.0.join("classifications.json")).unwrap();
    let plan = plan::build(&options(&scratch.0, 1, None)).unwrap();
    assert_eq!(plan.fixtures.len(), 5);
    assert!(
        plan.fixtures
            .iter()
            .flat_map(|fixture| &fixture.cases)
            .all(|case| case.baseline == BaselineStatus::Pass)
    );
    // This creates no accepted historical observation: the combined baseline stays unchanged.
    let shared = poirot_test_runner::baseline::Baseline::load(
        &poirot_test_runner::package_root().join("baseline.json"),
    )
    .unwrap();
    assert!(!shared.cases.contains_key("fods:a__00000"));
}

#[test]
fn combined_logs_report_typed_expected_and_actual_values_without_internal_cell_dumps() {
    use poirot_test_runner::{
        model::Source,
        report::Format,
        runner::{self, Options},
        selection::Selection,
    };
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let options = Options {
        fods_corpus: scratch.0.clone(),
        selection: Selection {
            source: vec![Source::Fods],
            cases: vec!["fods:c__00000".into()],
            ..Selection::default()
        },
        ..Options::default()
    };
    let mut output = Vec::new();
    let report = runner::run(&options, &mut output, Format::Text, false, false).unwrap();
    let log = String::from_utf8(output).unwrap();
    assert!(log.contains("FAIL  SUM(1,2)"));
    assert!(log.contains("Expected: Number(4.0)"));
    assert!(log.contains("Actual: Number(3.0)"));
    assert!(!log.contains("CellInput"));
    assert!(
        report.cases[0].details["failure"]["message"]
            .as_str()
            .unwrap()
            .contains("CellInput")
    );
}

#[test]
fn planning_rejects_stale_baseline_locations_and_duplicate_bindings() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let path = scratch.0.join("classifications.json");
    let mut baseline: Baseline = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    baseline.cases[0].cell = Some("A99".into());
    json(&path, &baseline);
    assert!(plan::build(&options(&scratch.0, 1, None)).is_err());
    baseline.cases[0].cell = Some("A2".into());
    baseline.cases.push(baseline.cases[0].clone());
    json(&path, &baseline);
    assert!(plan::build(&options(&scratch.0, 1, None)).is_err());
}

#[test]
fn cli_default_and_explicit_run_stream_results_with_correct_exit_codes() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let root = scratch.0.to_str().unwrap();
    let pass = cli(&["--corpus", root, "--filter", "a.fods", "--jobs", "2"]);
    assert_eq!(pass.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&pass.stdout).contains("test a__00000 ... ok"));
    let gap = cli(&["run", "--corpus", root, "--filter", "b.fods"]);
    assert_eq!(gap.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&gap.stdout).contains("FAILED (known gap: missing_function)"));
    let strict = cli(&["run", "--corpus", root, "--filter", "b.fods", "--strict"]);
    assert_eq!(strict.status.code(), Some(1));
    let regression = cli(&[
        "run", "--corpus", root, "--filter", "c.fods", "--format", "json",
    ]);
    assert_eq!(regression.status.code(), Some(1));
    let lines = String::from_utf8(regression.stdout).unwrap();
    let records = lines
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["status"], "regression");
    assert_eq!(records[1]["record"], "summary");
    assert_eq!(records[1]["successful"], false);
    assert_eq!(cli(&["run", "--jobs", "0"]).status.code(), Some(2));
}

#[test]
fn inspect_returns_original_expected_values_without_cached_results() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let root = scratch.0.to_str().unwrap();
    let output = cli(&[
        "inspect", "a.fods", "--corpus", root, "--case", "a__00000", "--format", "json",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let case: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(case["expected"]["value"]["value"].as_f64(), Some(3.));
    assert_eq!(case["original_formula"], "of:=SUM(1;2)");
    assert_eq!(
        cli(&["inspect", "a.fods", "--corpus", root, "--case", "missing"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn refresh_builds_a_runnable_corpus_and_failure_preserves_existing_bytes() {
    let scratch = Scratch::new();
    let source_root = scratch.0.join("source");
    let formulas = source_root.join("sc/qa/unit/data/functions");
    std::fs::create_dir_all(&formulas).unwrap();
    std::fs::write(source_root.join("revision.txt"), SOURCE_COMMIT).unwrap();
    std::fs::write(source_root.join("COPYING.MPL"), "test license").unwrap();
    let input = formulas.join("sum.fods");
    std::fs::write(&input, source("Sheet1", "SUM(1;2)", 3.)).unwrap();
    let destination = scratch.0.join("refreshed");
    let src = source_root.to_str().unwrap();
    let dest = destination.to_str().unwrap();
    let refresh = cli(&["refresh", "--source", src, "--corpus", dest, "--jobs", "2"]);
    assert!(
        refresh.status.success(),
        "{}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    assert_eq!(
        std::fs::read(destination.join("fixtures/sum.fods")).unwrap(),
        std::fs::read(&input).unwrap()
    );
    assert!(cli(&["run", "--corpus", dest]).status.success());
    let before = std::fs::read(destination.join("manifest.json")).unwrap();
    std::fs::write(&input, "<broken>").unwrap();
    assert_eq!(
        cli(&["refresh", "--source", src, "--corpus", dest])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        std::fs::read(destination.join("manifest.json")).unwrap(),
        before
    );
    std::fs::write(&input, source("Sheet1", "SUM(1;2)", 3.)).unwrap();
    std::fs::write(destination.join("unrelated.txt"), "keep this").unwrap();
    assert_eq!(
        cli(&["refresh", "--source", src, "--corpus", dest])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        std::fs::read_to_string(destination.join("unrelated.txt")).unwrap(),
        "keep this"
    );
}

#[test]
fn gap_types_distinguish_missing_functions_error_types_and_inactive_calls() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let plan = plan::build(&options(&scratch.0, 2, Some("b.fods"))).unwrap();
    let report = execute::execute(&plan).unwrap();
    let failure = report.cases[0].failure.as_ref().unwrap();
    assert_eq!(
        failure.gap_type,
        poirot_test_runner::fods::gaps::GapKind::MissingFunction
    );
    assert_eq!(failure.missing_functions, vec!["UNKNOWN_FODS_FUNCTION"]);

    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    let mut replace = |xml: String| {
        let path = scratch.0.join("fixtures/b.fods");
        std::fs::write(&path, &xml).unwrap();
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|e| e.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        json(&metadata, &manifest);
    };
    replace(
        source("Sheet1", "NA()", 3.)
            .replace("o:value=\"3\"", "t:formula=\"of:=1/0\" o:value=\"3\""),
    );
    let report =
        execute::execute(&plan::build(&options(&scratch.0, 2, Some("b.fods"))).unwrap()).unwrap();
    assert_eq!(
        report.cases[0].failure.as_ref().unwrap().gap_type,
        poirot_test_runner::fods::gaps::GapKind::WrongErrorType
    );

    replace(source(
        "Sheet1",
        "IF(TRUE;UndefinedName;UNKNOWN_UNUSED_FUNCTION())",
        3.,
    ));
    let report =
        execute::execute(&plan::build(&options(&scratch.0, 2, Some("b.fods"))).unwrap()).unwrap();
    assert_eq!(
        report.cases[0].failure.as_ref().unwrap().gap_type,
        poirot_test_runner::fods::gaps::GapKind::NameResolution
    );
}

#[test]
fn classify_adds_gap_labels_without_accepting_regressions() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let path = scratch.0.join("classifications.json");
    let before: Baseline = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let output = cli(&[
        "classify",
        "--corpus",
        scratch.0.to_str().unwrap(),
        "--jobs",
        "2",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after: Baseline = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for (before, after) in before.cases.iter().zip(&after.cases) {
        assert_eq!(before.classification, after.classification);
        assert_eq!(before.reason, after.reason);
    }
    assert_eq!(
        after.cases[1].gap_type,
        Some(poirot_test_runner::fods::gaps::GapKind::MissingFunction)
    );
    assert_eq!(after.cases[2].classification, BaselineStatus::Pass);
    assert_eq!(after.cases[2].gap_type, None);
    assert_eq!(
        after.cases[4].gap_type,
        Some(poirot_test_runner::fods::gaps::GapKind::UnsupportedWorkbookContext)
    );
}

#[test]
fn execution_evidence_preserves_error_literals_and_lazy_capabilities() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    let mut check = |formula: &str, expected: f64, correct: &str, dependency: Option<&str>| {
        let mut xml = source("Sheet1", formula, expected)
            .replace("of:=[.A2]=[.B2]", &format!("of:={correct}"));
        if let Some(dependency) = dependency {
            xml = xml.replace("</t:table>", &format!(
                "<t:table-row><t:table-cell t:formula=\"of:={dependency}\"/></t:table-row></t:table>"));
        }
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|e| e.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
        json(&metadata, &manifest);
        execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
            .unwrap()
            .cases
            .remove(0)
    };
    for (formula, dependency) in [
        ("#NAME?", None),
        ("[.A3]", Some("#NAME?")),
        ("IF(TRUE;#NAME?;UNUSED_MISSING())", None),
    ] {
        let result = check(formula, 5., "ERROR.TYPE([.A2])=[.B2]", dependency);
        assert_eq!(result.status, CaseStatus::Improvement, "{result:?}");
    }
    let result = check("IF(TRUE;3;UNUSED_MISSING())", 3., "[.A2]=[.B2]", None);
    assert_eq!(result.status, CaseStatus::Improvement);
    for (formula, dependency, function) in [
        ("IFERROR(MISSING_ACTUAL();3)", None, "MISSING_ACTUAL"),
        (
            "ERROR.TYPE([.A3])",
            Some("MISSING_DEPENDENT()"),
            "MISSING_DEPENDENT",
        ),
        ("ERROR.TYPE([.A3])", Some("DHFG()"), "DHFG"),
    ] {
        let result = check(formula, 3., "[.A2]=[.B2]", dependency);
        let failure = result.failure.unwrap();
        assert_eq!(
            failure.gap_type,
            poirot_test_runner::fods::gaps::GapKind::MissingFunction
        );
        assert_eq!(failure.missing_functions, vec![function]);
        assert!(failure.message.starts_with("actual:"));
    }
    let result = check("3", 3., "#NAME?", None);
    assert_ne!(
        result.failure.unwrap().gap_type,
        poirot_test_runner::fods::gaps::GapKind::UnsupportedCheck
    );
    let calls = (0..20)
        .map(|i| format!("IFERROR(MISSING_{i}();0)"))
        .collect::<Vec<_>>()
        .join(";");
    let result = check(&format!("SUM({calls})"), 0., "[.A2]=[.B2]", None);
    let failure = result.failure.unwrap();
    assert_eq!(failure.missing_functions.len(), 16);
    assert!(failure.message.contains("additional names omitted"));
    let result = check("3", 3., "IFERROR(MISSING_CHECK();TRUE)", None);
    let failure = result.failure.unwrap();
    assert_eq!(
        failure.gap_type,
        poirot_test_runner::fods::gaps::GapKind::UnsupportedCheck
    );
    assert_eq!(failure.missing_functions, vec!["MISSING_CHECK"]);
    assert!(failure.message.starts_with("Correct:"));
    let result = check("IFERROR(UndefinedName;3)", 3., "[.A2]=[.B2]", None);
    assert_eq!(result.status, CaseStatus::Improvement);
}

#[test]
fn executed_dependent_parser_failures_cannot_satisfy_error_checks() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    for (formula, expected, should_pass) in [
        ("ERROR.TYPE([.A3])", 3., false),
        ("IFERROR([.A3];3)", 3., false),
        ("ROW([.A3])", 3., true),
        ("AREAS([.A3])", 1., true),
        ("IF(TRUE;3;[.A3])", 3., true),
    ] {
        let xml = source("Sheet1", formula, expected).replace(
            "</t:table>",
            "<t:table-row><t:table-cell t:formula=\"of:=LAMBDA(x;x)(1)\"/></t:table-row></t:table>",
        );
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|e| e.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
        json(&metadata, &manifest);
        let result =
            execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
                .unwrap()
                .cases
                .remove(0);
        if should_pass {
            assert_eq!(
                result.status,
                CaseStatus::Improvement,
                "{formula}: {result:?}"
            );
        } else {
            let failure = result.failure.unwrap();
            assert_eq!(
                failure.gap_type,
                poirot_test_runner::fods::gaps::GapKind::UnsupportedFormulaSyntax
            );
            assert!(
                failure
                    .message
                    .starts_with("actual: executed unsupported source syntax:")
            );
        }
    }
}

#[test]
fn selected_name_parser_failures_and_shared_source_cycles_are_visible() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    for (formula, correct, syntax_phase) in [
        ("ERROR.TYPE(BrokenName)", "[.A2]=[.B2]", Some("actual:")),
        ("3", "IFERROR(BrokenName;TRUE)", Some("Correct:")),
        ("IF(TRUE;3;BrokenName)", "[.A2]=[.B2]", None),
    ] {
        let xml = source("Sheet1", formula, 3.)
            .replace("of:=[.A2]=[.B2]", &format!("of:={correct}"))
            .replace("</o:document>",
                "<t:named-expressions><t:named-expression t:name=\"BrokenName\" t:expression=\"of:=LAMBDA(x;x)(1)\" t:base-cell-address=\"$Sheet1.$A$1\"/></t:named-expressions></o:document>");
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|e| e.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
        json(&metadata, &manifest);
        let result =
            execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
                .unwrap()
                .cases
                .remove(0);
        if let Some(phase) = syntax_phase {
            let failure = result.failure.unwrap();
            assert_eq!(
                failure.gap_type,
                poirot_test_runner::fods::gaps::GapKind::UnsupportedFormulaSyntax
            );
            assert!(failure.message.starts_with(phase));
            assert!(failure.message.contains("defined name"));
        } else {
            assert_eq!(result.status, CaseStatus::Improvement);
        }
    }
    // Actual reads its own original checker, which reads Actual again. A fresh
    // formula probe at each phase hid this real source cycle by recomputation.
    let xml = source("Sheet1", "ISBLANK([.C2])", 0.)
        .replace("o:value=\"0\"", "t:formula=\"of:=FALSE()\" o:value=\"0\"");
    let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
    manifest
        .fixtures
        .iter_mut()
        .find(|e| e.fixture == "b.fods")
        .unwrap()
        .sha256 = parsed.sha256;
    std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
    json(&metadata, &manifest);
    let result = execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
        .unwrap()
        .cases
        .remove(0);
    assert_eq!(
        result.failure.as_ref().unwrap().gap_type,
        poirot_test_runner::fods::gaps::GapKind::PredicateMismatch
    );
    assert!(
        result
            .failure
            .unwrap()
            .message
            .contains("predicate=Error(Ref)")
    );
}

#[test]
fn omitted_linked_caches_do_not_block_local_checks_or_supply_external_values() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    let cache = r#"<t:table t:name="'file:///unavailable.ods'#Sheet1"><t:table-source xmlns:l="http://www.w3.org/1999/xlink" l:href="file:///unavailable.ods" t:mode="copy-results-only"/><t:table-row><t:table-cell o:value-type="float" o:value="3"/></t:table-row></t:table>"#;
    for (formula, unsupported) in [
        ("3", false),
        ("IF(TRUE;3;[.A3])", false),
        ("IFERROR([.A3];3)", true),
        ("ERROR.TYPE([.A3])", true),
    ] {
        let local = source("Sheet1", formula, 3.).replace("</t:table>", r#"<t:table-row><t:table-cell t:formula="of:=['file:///unavailable.ods'#$Sheet1.A1]"/></t:table-row></t:table>"#);
        for before in [true, false] {
            let xml = if before {
                local.replacen("<t:table ", &format!("{cache}<t:table "), 1)
            } else {
                local.replace("</o:document>", &format!("{cache}</o:document>"))
            };
            let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
            assert_eq!(parsed.sheets.len(), 1);
            manifest
                .fixtures
                .iter_mut()
                .find(|entry| entry.fixture == "b.fods")
                .unwrap()
                .sha256 = parsed.sha256;
            std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
            json(&metadata, &manifest);
            let mut report =
                execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
                    .unwrap();
            let result = report.cases.remove(0);
            if unsupported {
                assert_eq!(
                    result.failure.unwrap().gap_type,
                    poirot_test_runner::fods::gaps::GapKind::UnsupportedFormulaSyntax,
                    "{formula}"
                );
            } else {
                assert_eq!(
                    result.status,
                    CaseStatus::Improvement,
                    "{formula}: {result:?}"
                );
            }
        }
    }
}

#[test]
fn consumed_linked_table_names_remain_unsupported_even_when_ref_errors_are_caught() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    for (formula, unsupported) in [
        ("3", false),
        ("IF(TRUE;3;[Linked.A1])", false),
        ("IFERROR([Linked.A1];3)", true),
        ("IFERROR([linked.A1];3)", true),
    ] {
        let cache = r#"<t:table t:name="Linked"><t:table-source xmlns:l="http://www.w3.org/1999/xlink" l:href="file:///unavailable.ods" t:mode="copy-results-only"/><t:table-row><t:table-cell o:value-type="float" o:value="3"/></t:table-row></t:table>"#;
        let xml = source("Sheet1", formula, 3.)
            .replace("</o:document>", &format!("{cache}</o:document>"));
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|entry| entry.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
        json(&metadata, &manifest);
        let mut report =
            execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
                .unwrap();
        let result = report.cases.remove(0);
        if unsupported {
            let reason = result.failure.unwrap();
            assert_eq!(
                reason.gap_type,
                poirot_test_runner::fods::gaps::GapKind::UnsupportedWorkbookContext,
                "{formula}"
            );
            assert!(reason.message.contains("external linked table"));
        } else {
            assert_eq!(result.status, CaseStatus::Improvement, "{result:?}");
        }
    }
}

#[test]
fn ordinary_calc_formulas_use_scalar_intersection_without_enabling_cse() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    // LibreOffice's published implicit-intersection example: [7,95,5]+100
    // entered as an ordinary formula on row2 returns195, not a spill headed107.
    let xml = source("Sheet1", "[.D1:.D3]+100", 195.);
    let end = "</t:table-row>";
    let first = xml.find(end).unwrap();
    let mut xml = format!(
        "{}<t:table-cell o:value-type=\"float\" o:value=\"7\"/>{}",
        &xml[..first],
        &xml[first..]
    );
    let second = xml.find("</t:table-row></t:table>").unwrap();
    xml.insert_str(
        second,
        r#"<t:table-cell o:value-type="float" o:value="95"/>"#,
    );
    xml = xml.replace("</t:table>", r#"<t:table-row><t:table-cell t:number-columns-repeated="3"/><t:table-cell o:value-type="float" o:value="5"/></t:table-row></t:table>"#);
    let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
    manifest
        .fixtures
        .iter_mut()
        .find(|entry| entry.fixture == "b.fods")
        .unwrap()
        .sha256 = parsed.sha256;
    std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
    json(&metadata, &manifest);
    let report =
        execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap()).unwrap();
    assert_eq!(
        report.cases[0].status,
        CaseStatus::Improvement,
        "{:?}",
        report.cases[0]
    );
}

#[test]
fn selected_cse_continuations_are_not_blank_inputs_but_geometry_remains_usable() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    for (formula, expected, unsupported) in [
        ("[.D4]", 0., true),
        ("IFERROR([.D4];0)", 0., true),
        ("IF(TRUE;0;[.D4])", 0., false),
        ("COLUMN([.D4])", 4., false),
    ] {
        let xml = source("Sheet1", formula, expected).replace("</t:table>", r#"<t:table-row><t:table-cell t:number-columns-repeated="3"/><t:table-cell t:formula="of:={7|8}" t:number-matrix-rows-spanned="2" t:number-matrix-columns-spanned="1" o:value-type="float" o:value="999"/></t:table-row><t:table-row><t:table-cell t:number-columns-repeated="3"/><t:table-cell o:value-type="float" o:value="888"/></t:table-row></t:table>"#);
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|entry| entry.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
        json(&metadata, &manifest);
        let mut report =
            execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
                .unwrap();
        let result = report.cases.remove(0);
        if unsupported {
            assert_eq!(
                result.failure.unwrap().gap_type,
                poirot_test_runner::fods::gaps::GapKind::UnsupportedArrayContext,
                "{formula}"
            );
        } else {
            assert_eq!(
                result.status,
                CaseStatus::Improvement,
                "{formula}: {result:?}"
            );
        }
    }
}

#[test]
fn preparation_limits_are_resources_and_unused_limited_sources_stay_lazy() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let metadata = scratch.0.join("manifest.json");
    let mut manifest: CorpusManifest =
        serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
    let large = format!("SUM({})", vec!["1"; 1100].join(";"));
    for (formula, limited) in [
        (large.clone(), true),
        ("IFERROR([.A3];3)".into(), true),
        ("IF(TRUE;3;[.A3])".into(), false),
    ] {
        let xml = source("Sheet1", &formula, 3.).replace(
            "</t:table>",
            &format!(
                "<t:table-row><t:table-cell t:formula=\"of:={large}\"/></t:table-row></t:table>"
            ),
        );
        let parsed = extract_bytes(xml.as_bytes(), "sc/qa/unit/data/functions/b.fods").unwrap();
        manifest
            .fixtures
            .iter_mut()
            .find(|entry| entry.fixture == "b.fods")
            .unwrap()
            .sha256 = parsed.sha256;
        std::fs::write(scratch.0.join("fixtures/b.fods"), xml).unwrap();
        json(&metadata, &manifest);
        let result =
            execute::execute(&plan::build(&options(&scratch.0, 1, Some("b.fods"))).unwrap())
                .unwrap()
                .cases
                .remove(0);
        if limited {
            assert_eq!(
                result.failure.unwrap().gap_type,
                poirot_test_runner::fods::gaps::GapKind::ResourceLimit
            );
        } else {
            assert_eq!(result.status, CaseStatus::Improvement);
        }
    }
}
