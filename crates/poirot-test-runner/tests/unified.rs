use poirot_test_runner::{
    baseline::{self, Baseline, Entry},
    model::{CaseMetadata, Change, Observation, Outcome, Source},
    report::{self, Format},
    runner::{self, Options},
    selection::Selection,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

/// Temporary authored JSON corpus and baseline used to exercise unified discovery,
/// execution, saved reports, and baseline transitions through public runner APIs.
struct Fixture {
    root: PathBuf,
    options: Options,
}
impl Fixture {
    fn new(expected: u8, known_gap: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from("/tmp/codex/poirot-test-runner/tests").join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("fods")).unwrap();
        std::fs::create_dir_all(root.join("json")).unwrap();
        let revision = poirot_test_runner::fods::xml::SOURCE_COMMIT;
        for (name, value) in [
            (
                "manifest.json",
                json!({"source_repository":"test", "source_commit":revision,
                "license":"test", "fixtures":[], "case_count":0}),
            ),
            (
                "classifications.json",
                json!({"source_commit":revision,"cases":[]}),
            ),
        ] {
            std::fs::write(root.join("fods").join(name), value.to_string()).unwrap();
        }
        let baseline = Baseline {
            schema_version: 2,
            fods_source_commit: revision.into(),
            cases: BTreeMap::new(),
        };
        std::fs::write(
            root.join("baseline.json"),
            serde_json::to_vec_pretty(&baseline).unwrap(),
        )
        .unwrap();
        let options = Options {
            fods_corpus: root.join("fods"),
            json_corpus: root.join("json"),
            baseline: root.join("baseline.json"),
            ..Options::default()
        };
        let fixture = Self { root, options };
        fixture.suite(expected, known_gap);
        fixture
    }

    fn suite(&self, expected: u8, known_gap: bool) {
        let mut check =
            json!({"action":"check", "cell":"S!B1", "expected":{"type":"number","value":expected}});
        if known_gap {
            check["known_gap"] = json!({"reason":"independent test contract", "actual":{"type":"number","value":if expected == 1 {2} else {1}}});
        }
        let suite = json!({"schema_version":1,"group":"lookup", "workbook":{"sheets":["S"],"cells":[
            {"cell":"S!B1","content":{"formula":"SUM(1)"}}]}, "scenarios":[
            {"name":"selected result", "evidence":["Literal arithmetic: SUM(1) = 1"],"actions":[check]}]});
        std::fs::write(self.root.join("json/suite.json"), suite.to_string()).unwrap();
    }

    fn run(&self) -> runner::EvaluationReport {
        runner::run(&self.options, &mut Vec::new(), Format::Json, false, false).unwrap()
    }

    fn accept(&self, report: &runner::EvaluationReport) {
        let mut baseline = Baseline::load(&self.options.baseline).unwrap();
        for result in &report.cases {
            baseline.cases.insert(
                result.case.id.clone(),
                Entry {
                    source: result.case.source,
                    group: result.case.group.clone(),
                    suite: result.case.suite.clone(),
                    observation: result.observation.clone(),
                    reason: Some("reviewed test observation".into()),
                    fods: None,
                },
            );
        }
        std::fs::write(
            &self.options.baseline,
            serde_json::to_vec_pretty(&baseline).unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn new_tests_known_gaps_and_changed_observations_have_distinct_verdicts() {
    let fixture = Fixture::new(2, true);
    let first = fixture.run();
    assert_eq!(first.cases[0].change, Change::NewTest);
    assert!(!first.summary.successful);
    fixture.accept(&first);
    let second = fixture.run();
    assert_eq!(second.cases[0].change, Change::Unchanged);
    assert!(second.summary.successful);
    fixture.suite(3, true);
    let third = fixture.run();
    assert_eq!(third.cases[0].change, Change::ChangedGap);
    assert!(!third.summary.successful);
    fixture.suite(1, false);
    let fixed = fixture.run();
    assert_eq!(fixed.cases[0].change, Change::Improvement);
    assert!(fixed.summary.successful);
    fixture.accept(&fixed);
    fixture.suite(2, false);
    let regression = fixture.run();
    assert_eq!(regression.cases[0].change, Change::Regression);
    assert!(!regression.summary.successful);
}

#[test]
fn filtering_previews_execution_and_combines_dimensions_consistently() {
    let mut fixture = Fixture::new(1, false);
    fixture.options.selection = Selection {
        source: vec![Source::Json],
        group: vec!["LOOKUP".into()],
        filter: vec!["does not match".into(), "SUM(1)".into()],
        cases: vec![],
    };
    let cases = runner::list(&fixture.options).unwrap();
    let report = fixture.run();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].id, report.cases[0].case.id);
    fixture.options.selection.source = vec![Source::Fods];
    assert!(runner::list(&fixture.options).is_err());
    assert!(
        runner::run(
            &fixture.options,
            &mut Vec::new(),
            Format::Json,
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn explicit_skips_are_not_executed_or_reported_as_improvements() {
    let mut fixture = Fixture::new(1, false);
    let initial = fixture.run();
    fixture.accept(&initial);
    let id = &initial.cases[0].case.id;
    let skips = fixture.root.join("skips.json");
    std::fs::write(&skips, json!({"rules":[{"case":id,"reason":"requires unavailable locale","tracking":"issue-123"}]}).to_string()).unwrap();
    fixture.options.skips = Some(skips.clone());
    let skipped = fixture.run();
    assert_eq!(skipped.cases[0].observation.outcome, Outcome::Skipped);
    assert_eq!(skipped.cases[0].change, Change::NewSkip);
    assert!(skipped.cases[0].details.is_null());
    assert!(skipped.cases[0].reason.contains("issue-123"));
    assert!(!skipped.summary.successful);
    fixture.accept(&skipped);
    fixture.options.skips = None;
    let resumed = fixture.run();
    assert_eq!(resumed.cases[0].change, Change::NewTest);
    std::fs::write(
        &skips,
        json!({"rules":[{"case":id,"reason":" "}]}).to_string(),
    )
    .unwrap();
    fixture.options.skips = Some(skips);
    assert!(
        runner::run(
            &fixture.options,
            &mut Vec::new(),
            Format::Text,
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn normal_runs_do_not_mutate_baseline_and_partial_reports_cannot_replace_it() {
    let fixture = Fixture::new(1, false);
    let bytes = std::fs::read(&fixture.options.baseline).unwrap();
    let report = fixture.run();
    assert_eq!(std::fs::read(&fixture.options.baseline).unwrap(), bytes);
    assert!(baseline::candidate(&report, "reviewed").is_err());
    let mut full = report;
    full.header.complete_inventory = true;
    let candidate = baseline::candidate(&full, "reviewed arithmetic contract").unwrap();
    let path = fixture.root.join("baseline.next.json");
    baseline::write_candidate(&path, &candidate).unwrap();
    assert!(baseline::write_candidate(&path, &candidate).is_err());
    full.cases[0].observation.outcome = Outcome::HarnessError;
    assert!(baseline::candidate(&full, "cannot waive infrastructure errors").is_err());
}

#[test]
fn reports_round_trip_and_reject_truncation_duplicate_cases_and_invalid_summaries() {
    let fixture = Fixture::new(1, false);
    let mut bytes = Vec::new();
    runner::run(&fixture.options, &mut bytes, Format::Json, false, false).unwrap();
    let path = fixture.root.join("run.jsonl");
    std::fs::write(&path, &bytes).unwrap();
    let report = report::read(&path).unwrap();
    assert_eq!(report.cases.len(), 1);
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<_> = text.lines().collect();
    std::fs::write(&path, lines[..lines.len() - 1].join("\n")).unwrap();
    assert!(report::read(&path).is_err());
    let duplicate = format!("{}\n{}\n{}\n{}\n", lines[0], lines[1], lines[1], lines[2]);
    std::fs::write(&path, duplicate).unwrap();
    assert!(report::read(&path).is_err());
    let mut summary: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    summary["summary"]["successful"] = json!(false);
    std::fs::write(&path, format!("{}\n{}\n{}\n", lines[0], lines[1], summary)).unwrap();
    assert!(report::read(&path).is_err());
}

#[test]
fn viewer_and_terminal_treat_fixture_content_as_data() {
    let fixture = Fixture::new(1, false);
    let mut report = fixture.run();
    report.cases[0].case.name = "</script><img src=x onerror=alert(1)>\u{1b}[31m".into();
    let html = report::html(&report).unwrap();
    assert!(!html.contains("</script><img"));
    assert!(html.contains("\\u003c/script>"));
    assert!(!report::single_line(&report.cases[0].case.name).contains('\u{1b}'));
}

#[test]
fn strict_mode_and_stale_annotations_cannot_be_baselined_away() {
    let mut fixture = Fixture::new(2, true);
    fixture.accept(&fixture.run());
    fixture.options.strict = true;
    assert!(!fixture.run().summary.successful);
    fixture.options.strict = false;
    fixture.suite(1, true);
    let report = fixture.run();
    assert_eq!(
        report.cases[0].observation.category.as_deref(),
        Some("stale_annotation")
    );
    fixture.accept(&report);
    assert!(!fixture.run().summary.successful);
}

#[test]
fn comparison_never_calls_a_blocker_or_skip_a_pass() {
    let observation = |outcome, category: &str| Observation {
        outcome,
        category: Some(category.into()),
        fingerprint: None,
    };
    let blocked = observation(Outcome::Blocked, "array_context");
    let mismatch = observation(Outcome::Mismatch, "numeric_mismatch");
    assert_eq!(
        baseline::compare(Some(&blocked), &mismatch),
        Change::ChangedGap
    );
    assert_eq!(
        baseline::compare(Some(&Observation::passed()), &blocked),
        Change::Regression
    );
    assert_eq!(
        baseline::compare(Some(&blocked), &Observation::passed()),
        Change::Improvement
    );
}

#[test]
fn output_write_errors_propagate_without_a_successful_report() {
    /// Writer that always fails, used to verify that report-output errors propagate
    /// instead of returning a successful run report.
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let fixture = Fixture::new(1, false);
    assert!(runner::run(&fixture.options, &mut Broken, Format::Json, false, false).is_err());
}

#[test]
fn ids_do_not_depend_on_scenario_position_or_absolute_checkout() {
    let fixture = Fixture::new(1, false);
    let cases: Vec<CaseMetadata> = runner::list(&fixture.options).unwrap();
    assert_eq!(cases[0].id, "json:suite.json:selected result");
}

#[test]
fn source_selection_does_not_open_an_unrelated_corpus() {
    let mut fixture = Fixture::new(1, false);
    fixture.options.selection.source = vec![Source::Json];
    fixture.options.fods_corpus = fixture.root.join("does-not-exist");
    assert_eq!(runner::list(&fixture.options).unwrap().len(), 1);
    assert!(fixture.run().summary.successful);
}

#[test]
fn duplicate_baseline_keys_are_rejected_instead_of_silently_overwritten() {
    let fixture = Fixture::new(1, false);
    let text = format!(
        r#"{{"schema_version":2,"fods_source_commit":"{}","passing":["json:suite.json:selected result","json:suite.json:selected result"],"nonpassing":[]}}"#,
        poirot_test_runner::fods::xml::SOURCE_COMMIT,
    );
    std::fs::write(&fixture.options.baseline, text).unwrap();
    let poirot_test_runner::RunnerError::Json(source) =
        Baseline::load(&fixture.options.baseline).unwrap_err()
    else {
        panic!("wrong baseline error category")
    };
    assert!(source.is_data());
    assert!(source.to_string().contains("duplicate baseline"));
}

#[test]
fn removed_tests_fail_the_summary_and_are_never_counted_as_passes() {
    let fixture = Fixture::new(1, false);
    let mut report = fixture.run();
    report.header.complete_inventory = true;
    report.removed.push("json:removed.json:old check".into());
    report.summarize();
    assert_eq!(report.summary.outcomes.get(&Outcome::Passed), Some(&1));
    assert_eq!(report.summary.removed, 1);
    assert!(!report.summary.successful);
}

#[cfg(unix)]
#[test]
fn rerun_quoting_preserves_literal_shell_syntax_and_control_characters() {
    let id = "json:suite.json:a ' quote $(false) \\ slash\nnext line\u{1b}";
    let result = std::process::Command::new("bash")
        .args(["-c", &format!("printf %s {}", report::shell_quote(id))])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, id.as_bytes());
}
