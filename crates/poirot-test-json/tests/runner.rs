use poirot_test_json::{Cli, OutputFormat, Status, parse_suite, run_suite};
use serde_json::{Value, json};

fn suite(scenarios: Value) -> Value {
    json!({"schema_version":1, "workbook":{"sheets":["S"], "cells":[{"cell":"S!A1", "content":{"type":"number","value":1}}]}, "scenarios":scenarios})
}
fn scenario(name: &str, actions: Value) -> Value {
    json!({"name":name,"evidence":["Harness test with independently authored literal expectations"],"actions":actions})
}
fn run(value: Value) -> Vec<poirot_test_json::ScenarioResult> {
    run_suite(&parse_suite(&value.to_string()).unwrap(), None).unwrap()
}
fn check(expected: Value) -> Value {
    json!({"action":"check","cell":"S!A1","expected":expected})
}

#[test]
fn corpus_scenarios_gate_regressions_in_both_modes() {
    let mut output = Vec::new();
    assert!(
        Cli {
            corpus: concat!(env!("CARGO_MANIFEST_DIR"), "/corpus").into(),
            filter: None,
            strict: false,
            format: OutputFormat::Json
        }
        .run(&mut output)
        .unwrap()
    );
    let records: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(records.iter().any(|r| r["type"] == "scenario"));
    assert_eq!(records.last().unwrap()["success"], true);
    for record in records.iter().filter(|r| r["type"] == "scenario") {
        assert_eq!(record["result"]["outputs_equal"], true);
        assert_eq!(record["result"]["changes_equal"], true);
        assert!(record["result"]["normal"]["changes"].is_array());
    }
}

#[test]
fn scenarios_start_fresh_and_actions_retain_edits() {
    let results = run(suite(json!([
        scenario(
            "edit",
            json!([
                {"action":"set","cell":"S!A1","content":{"type":"number","value":2}},
                check(json!({"type":"number","value":2}))
            ])
        ),
        scenario("fresh", json!([check(json!({"type":"number","value":1}))]))
    ])));
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.status == Status::Passed));
    assert!(!results[0].normal.changes.is_empty());
    assert!(results[1].normal.changes.is_empty());
}

#[test]
fn typed_values_and_spreadsheet_errors_remain_distinct() {
    for content in [
        json!({"type":"blank"}),
        json!({"type":"text","value":""}),
        json!({"type":"boolean","value":false}),
        json!({"type":"error","value":"#N/A"}),
        json!({"type":"utf16","value":[55296]}),
    ] {
        let results = run(suite(json!([scenario(
            "typed value",
            json!([
                {"action":"set","cell":"S!A1","content":content}, check(content)
            ])
        )])));
        assert_eq!(results[0].status, Status::Passed);
    }
    let results = run(suite(json!([scenario(
        "no coercion",
        json!([check(json!({"type":"text","value":"1"}))])
    )])));
    assert_eq!(results[0].status, Status::Failed);
}

#[test]
fn known_gaps_are_narrow_and_unexpected_passes_fail() {
    for (actual, expected_status) in [(1, Status::KnownGap), (2, Status::Failed)] {
        let mut assertion = check(json!({"type":"number","value":3}));
        assertion["known_gap"] =
            json!({"reason":"documented mismatch","actual":{"type":"number","value":actual}});
        let results = run(suite(json!([scenario("gap", json!([assertion]))])));
        assert_eq!(results[0].status, expected_status);
        assert_eq!(results[0].fails(false), expected_status == Status::Failed);
        assert!(results[0].fails(true));
    }
    let mut assertion = check(json!({"type":"number","value":1}));
    assertion["known_gap"] = json!({"reason":"stale gap","actual":{"type":"number","value":2}});
    let results = run(suite(json!([scenario("fixed gap", json!([assertion]))])));
    assert_eq!(results[0].status, Status::UnexpectedPass);
    assert!(results[0].fails(false));
}

#[test]
fn malformed_scenarios_are_rejected() {
    let valid = suite(json!([scenario(
        "check",
        json!([check(json!({"type":"number","value":1}))])
    )]));
    let mut invalids = Vec::new();
    let mut v = valid.clone();
    v["schema_version"] = json!(99);
    invalids.push(v);
    let mut v = valid.clone();
    v["unknown"] = json!(true);
    invalids.push(v);
    let mut v = valid.clone();
    v["scenarios"][0]["actions"][0]["tolerance"] = json!(-1);
    invalids.push(v);
    let mut v = valid.clone();
    v["scenarios"][0]["evidence"] = json!([]);
    invalids.push(v);
    let mut v = valid.clone();
    v["scenarios"][0]["actions"] = json!([]);
    invalids.push(v);
    let mut v = valid.clone();
    v["scenarios"]
        .as_array_mut()
        .unwrap()
        .push(valid["scenarios"][0].clone());
    invalids.push(v);
    let mut v = valid.clone();
    v["scenarios"][0]["actions"][0]["expected"] = json!({"type":"error","value":"nonsense"});
    invalids.push(v);
    for invalid in invalids {
        assert!(parse_suite(&invalid.to_string()).is_err(), "{invalid}");
    }
}

#[test]
fn invalid_targets_are_harness_errors_and_do_not_hide_as_known_gaps() {
    let mut assertion = check(json!({"type":"number","value":3}));
    assertion["cell"] = json!("Missing!A1");
    assertion["known_gap"] =
        json!({"reason":"not a setup exception","actual":{"type":"number","value":1}});
    let results = run(suite(json!([scenario("bad target", json!([assertion]))])));
    assert_eq!(results[0].status, Status::HarnessError);
    assert!(
        results[0]
            .normal
            .error
            .as_ref()
            .unwrap()
            .contains("action 0")
    );
}

#[test]
fn clap_filters_and_empty_selections_fail() {
    use clap::Parser;
    let args = Cli::try_parse_from([
        "test-json",
        "--filter",
        "no such scenario",
        "--strict",
        "--format",
        "json",
    ])
    .unwrap();
    assert!(args.strict);
    let mut output = Vec::new();
    assert!(!args.run(&mut output).unwrap());
}
