# Workbook test runner

One crate runs the original LibreOffice FODS checks and authored JSON workbook
scenarios. Selection, grouping, skips, logs, saved reports, and comparison against
the checked-in [`baseline.json`](baseline.json) are shared. Expected values remain
in the source fixtures: updating the baseline never changes those expectations.

```sh
cargo run -p poirot-test-runner -- list
cargo run -p poirot-test-runner -- list --source json --group spreadsheet --cases
cargo run -p poirot-test-runner -- run --group text
cargo run -p poirot-test-runner -- run --filter XLOOKUP --filter XMATCH
cargo run -p poirot-test-runner -- run --case 'fods:statistical__fods__rank__00004'
cargo run -p poirot-test-runner -- run --strict --jobs 4
cargo test -p poirot-test-runner
```

No subcommand means `run`. Empty selections fail. `list` reads and validates source
metadata without compiling or evaluating workbooks. Suite workers stream completed
groups; JSONL case order reflects completion order. Baselines and final in-memory
reports are sorted by stable ID. Run-wide results do not depend on completion order.

## Fixture storage and compact baseline

Fixtures remain text files so Git can compress and share their repeated content.
The documented Excel specification table now lives at `corpus/json/cases.json`.
The JSON adapter retains its richer schema, including evidence, names, settings,
steps and array assertions, and checks each case in normal and overlay modes.
Use `--source json --group excel_specs` to select that table. Its authoring and
coverage tools remain in `../poirot-test-excel-specs/` and use the shared table;
there is no duplicate normative case file.

Baseline schema 2 stores passing case IDs only and groups identical nonpassing
observations and reasons. It retains the full inventory for new/removed test
comparison without repeating suite metadata or passing observations. FODS bindings
come from hash-checked native fixtures. Schema 1 baselines remain readable; new
baseline candidates use schema 2. Normal runs never update the baseline.

## Selection and groups

Tests are grouped by source, family, and suite. FODS families come from their native
paths. [`groups.json`](groups.json) assigns the existing JSON suites to families
without editing their fixture bytes. A JSON suite can supply an explicit `group`;
otherwise an uncatalogued suite uses its filename stem.

- `--source fods` / `--source json`: choose sources; repeated sources are OR.
- `--group text`: exact family or suite path component, ignoring case; repeated
  groups are OR. For example, `--group lookup` also selects suites with a `lookup`
  filename component, while `--group spreadsheet` selects their wider family.
- `--filter text`: case-insensitive substring search over IDs, suite paths,
  scenario names, locations, and formulas; repeated filters are OR.
- `--case ID`: exact stable identity; repeat to select several cases.
- Different dimensions intersect. `list` and `run` use the identical selector.

FODS IDs retain their original source identity with a `fods:` prefix. JSON IDs are
`json:<relative suite path>:<scenario name>`. They do not contain absolute checkout
paths or scenario indices. Renaming a scenario produces a new and a removed ID.
Use the full default corpus root for consistent IDs when comparing runs. The
`--json-corpus` and `--fods-corpus` options support custom research corpora; such
runs cannot produce replacement baselines or report global removals.

## Reading results

Execution and historical comparison are separate:

| Observation | Meaning |
| --- | --- |
| Passed | The original assertion completed and passed |
| Mismatch | Executed assertion disagreed with its expected behavior |
| Blocked | A missing function, parser/context limitation, resource limit, or unavailable checker prevented a reliable assertion |
| Skipped | An explicit rule prevented execution, with a recorded reason |
| HarnessError | Infrastructure or evaluator operation failed |

The baseline comparison reports unchanged outcomes, regressions, improvements,
new tests, changed gaps, new skips, and changed skip reasons. A blocker becoming
an executed mismatch is a changed gap, not a pass. A skipped test resuming execution
is assessed as a new observation, not called an improvement. Full unfiltered runs
also report removed baseline IDs; filtered/custom runs explicitly do not assess
removals.

Normal runs fail on regressions, changed gaps/skips, new failing or blocked tests,
new skips, removals, and harness errors. New passing tests and improvements are
visible and succeed. `--strict` additionally fails all nonpassing observations.
JSON's stale known-gap annotations always fail, even if manually baselined.

Default text output summarizes suites and repeated diagnostic categories and expands
failures and skips. `--verbose` expands every case; `--quiet` suppresses unchanged
suite output while retaining failures and the summary. Expanded failures include
source identity, authority, location, formula, baseline/current behavior, and a
shell-quoted rerun command.

## Explicit skips

Known gaps still execute. Selecting fewer cases does not count as skipping.
Pass a reviewed file explicitly with `--skip-file skips.json`:

```json
{
  "rules": [
    {
      "case": "json:example.json:locale-dependent rendering",
      "reason": "Requires a locale unavailable in this environment",
      "tracking": "issue-123"
    }
  ]
}
```

`tracking` is optional. Reasons must be nonempty; duplicate and unknown case IDs
are errors. Rules for an unselected source are checked when that source is
discovered, so a JSON-only run need not open the FODS corpus. The runner does not infer skips from missing capabilities or change
the classification of LibreOffice/Excel disagreements automatically.

## Saved results and offline viewer

```sh
cargo run -p poirot-test-runner -- run --format json > run.jsonl
cargo run -p poirot-test-runner -- show \
  'fods:statistical__fods__rank__00004' --results run.jsonl
cargo run -p poirot-test-runner -- report run.jsonl --html report.html
```

The HTML file works offline with no network dependencies. It filters by source,
family, outcome, baseline change, and free text. Expand suites and cases for
evidence, expected/actual diagnostics, rerun commands, and native typed results.
Both JSON modes and their raw ordered change lists remain available in details.
Source text is rendered as data, not executable HTML.

JSONL contains a versioned header with the baseline digest, selection, worker and
strict-mode settings, source policies/evidence, and input digests in its inventory.
Case records follow, then removed IDs and a final summary. Readers reject incomplete
runs, duplicate/missing cases, inconsistent comparisons, and invalid summaries.
A failed test run still produces a complete report; an interrupted/infrastructure-
aborted write does not. Capture the report even when the process exits nonzero.

## Reviewing baseline updates

Normal runs never write `baseline.json`. Generate a candidate from a completed,
unfiltered run of both default corpora:

```sh
cargo run -p poirot-test-runner -- baseline update \
  --results run.jsonl --output baseline.next.json \
  --reason 'Reviewed observations for the runner migration'
git diff --no-index crates/poirot-test-runner/baseline.json baseline.next.json
```

The command refuses existing output paths, incomplete/filtered/custom runs, stale
baseline digests, harness errors, and stale JSON annotations. Review the candidate
before replacing the checked-in file. Baseline changes can explicitly accept a
reviewed new observation; they never establish Excel correctness.

The initial migration preserves FODS's previous pass/gap verdicts, including its
three existing regressions, and JSON's authored known-gap expectations. Diagnostic
fingerprints establish the initial shared comparison for already accepted failing
observations. Previously known FODS gaps that now pass remain improvements. Native
standalone FODS `classifications.json` is supported for import tools and custom
corpora; the checked-in corpus has one shared baseline, not a competing native one.

## Source contracts and maintenance

- FODS retains native predicates, source hashes, provenance, parser/capability
  tracing, and missing-checker diagnostics. This is LibreOffice compatibility
  evidence, not native Excel verification. See the [source reference](docs/fods-README.md).
- JSON retains authored evidence, narrow known-gap matching, persistent normal
  and overlay execution, and exact final ordered change-list comparison.
  See [authoring and oracle evidence](docs/json-README.md).
  Historical oracle records retain their original paths and hashes; relocated
  fixture and evidence files remain byte-for-byte identical.
- `fods inspect FIXTURE --case NATIVE_ID` displays the original source record.
  Inspection uses native IDs without the combined `fods:` prefix.
- `fods refresh --source PATH --corpus SEPARATE_PATH` extracts a standalone pinned
  corpus for review; it cannot overwrite the checked-in corpus through this CLI.
  See [refresh details](docs/fods-REFRESH.md).

The `poirot-test-fods` and `poirot-test-json` binary names remain transitional
native-tool entry points **inside this crate** for existing workflows and harness
regression tests. They use their historical output contracts. Use
`cargo run -p poirot-test-runner -- ...` for shared selection and baseline reporting.

## Code boundaries

`sources.rs` adapts `FodsSuite` and `JsonSuite` behind `TestSuite`. Each backend owns
source parsing and assertion semantics. `runner.rs` handles selection, explicit
skips, bounded suite scheduling, and aggregation. `baseline.rs` compares independent
observations and constructs review candidates. `report.rs` and `report.html` consume
those results; neither evaluates formulas nor changes expected values.

`cargo test` retains the authored JSON corpus gate and focused native harness
tests. The full FODS corpus is an explicit runner invocation, as before. No office
installation, browser, or network access is required by Cargo tests.
