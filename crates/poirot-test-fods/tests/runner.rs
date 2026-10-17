use poirot_test_fods::{
    corpus::{Baseline, BaselineCase, BaselineStatus, CorpusManifest, FixtureEntry},
    execute,
    fods::{SOURCE_COMMIT, extract_bytes},
    plan::{self, RunOptions},
    report::CaseStatus,
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
            sheet: case.sheet_name.clone(),
            cell: case.cell.clone(),
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
    use poirot_test_fods::{
        corpus::{Corpus, default_corpus, load_fixture},
        fods::model::InputValue,
        gaps::GapKind,
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
        Err(poirot_test_fods::Error::Output(std::io::Error::from(
            std::io::ErrorKind::BrokenPipe,
        )))
    });
    assert!(matches!(broken, Err(poirot_test_fods::Error::Output(_))));
}

#[test]
fn planning_rejects_stale_baseline_locations_and_duplicate_bindings() {
    let scratch = Scratch::new();
    corpus(&scratch.0);
    let path = scratch.0.join("classifications.json");
    let mut baseline: Baseline = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    baseline.cases[0].cell = "A99".into();
    json(&path, &baseline);
    assert!(plan::build(&options(&scratch.0, 1, None)).is_err());
    baseline.cases[0].cell = "A2".into();
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
        poirot_test_fods::gaps::GapKind::MissingFunction
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
        poirot_test_fods::gaps::GapKind::WrongErrorType
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
        poirot_test_fods::gaps::GapKind::NameResolution
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
        Some(poirot_test_fods::gaps::GapKind::MissingFunction)
    );
    assert_eq!(after.cases[2].classification, BaselineStatus::Pass);
    assert_eq!(after.cases[2].gap_type, None);
    assert_eq!(
        after.cases[4].gap_type,
        Some(poirot_test_fods::gaps::GapKind::UnsupportedWorkbookContext)
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
            poirot_test_fods::gaps::GapKind::MissingFunction
        );
        assert_eq!(failure.missing_functions, vec![function]);
        assert!(failure.message.starts_with("actual:"));
    }
    let result = check("3", 3., "#NAME?", None);
    assert_ne!(
        result.failure.unwrap().gap_type,
        poirot_test_fods::gaps::GapKind::UnsupportedCheck
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
        poirot_test_fods::gaps::GapKind::UnsupportedCheck
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
                poirot_test_fods::gaps::GapKind::UnsupportedFormulaSyntax
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
                poirot_test_fods::gaps::GapKind::UnsupportedFormulaSyntax
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
        poirot_test_fods::gaps::GapKind::PredicateMismatch
    );
    assert!(
        result
            .failure
            .unwrap()
            .message
            .contains("predicate=Error(Ref)")
    );
}
