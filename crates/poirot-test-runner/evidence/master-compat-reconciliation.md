# Integrated compatibility reconciliation

The original 144 new compatibility gaps now comprise **133 fixed scenarios / 146
checks** and **11 deliberately retained scenarios / 11 checks**. The full corpus
has **2,400 scenarios / 3,176 checks: 2,387 passing and 13 known gaps**, with zero
failures, unexpected passes or harness errors. The other two gaps are inherited
NUMBERVALUE range disputes outside this work's scope.

[Exact reconciliation](master-compat-reconciliation.json) records all 146 original
gap identities, formulas, action indices, expected and actual values, unique
ownership, focused commit mappings, current suite hashes, historical reports,
commands, exit codes and log hashes. Verification used base
`7629b681bcfb1b99e8e813558c9593a753323bb5` and semantic head
`019fcc242740649a6b2dae6ace9e9f0ceb164987`. Diagnostic controls remain evidence;
they were not counted as additional production scenarios.

Three integration options were considered: reconcile exact identities and rerun
integrated gates (selected); compare totals only (could hide replacement cases or
regressions); rewrite baselines (would conceal limitations). Every original new
scenario retains its actions, formulas, independent expectations and tolerances.
The complete corpus definitions differ beyond evidence and resolved gap markers
only in the separately authorized `lookup-blanks.json` project contract: “missing
IF result remains blank through a cell reference” became “missing IF result
publishes zero through a cell reference.” This supersedes an old internal contract,
not an independently sourced compatibility expectation. All 2,400 runtime scenario
identities are reconciled with that explicit name mapping, and both execution
modes agree on observations and raw final change lists.

| Class | Original gaps fixed | Retained | Semantic commit |
| --- | ---: | ---: | --- |
| Completed worksheet blank publication | 12 | 0 | `4ba68f5` |
| Contextual ordered criteria thresholds | 42 | 7 | `539ddac` |
| Selected AVERAGEIF result errors | 1 | 0 | `489f8d8` |
| Positional IF arrays | 21 / 34 checks | 0 | `f39905f` |
| Exact information-predicate arrays | 10 | 0 | `8d0052e` |
| Positional IFS arrays | 16 | 0 | `742291a` |
| Positional SWITCH arrays | 16 | 0 | `146455f` |
| Positional CHOOSE arrays | 13 | 0 | `63e597f` |
| Approximate MATCH wildcard assessment | 0 | 4 | `aa5b957` |
| Uniform SUMPRODUCT argument admission | 1 | 0 | `5367b57` |
| Contextual TEXT numeric-text admission | 1 | 0 | `734c630` |

The final allocation audit led to focused commit `019fcc2`: ordinary fallible
reservation replaces six potentially overflowing allocations, and local comments
prove six fixed nonzero iterator steps. Two unit tests verify allocation-capacity
overflow is returned rather than panicking. Exact before/after comparisons found
all 2,400 JSON scenario records and all 16,114 FODS case records unchanged.

## Retained limits and evidence

The seven parenthesized-negative inequality criteria gaps (inventory 30, 36, 42,
48, 54, 60 and 66) remain because broad numeric interpretation also breaks literal
matching. No dual text/numeric criterion state was introduced. The four approximate
MATCH wildcard cases (86–89) use an unsorted vector and a mode whose documented
ordering contract does not establish wildcard traversal semantics. A special
traversal exception was not justified. Original known-gap actuals are unchanged.
See [criteria controls](criteria-threshold-controls-libreoffice.json) and the
[MATCH assessment](match-wildcard-contract.md).

Complexity includes surprising exceptions to established patterns, even small
ones. More evidence that machinery could fix a case does not by itself make the
lasting conceptual and maintenance cost worthwhile. These retained bugs remain
reproducible rather than being counted as fixes.

The supported arrays are bounded and policy specific. IF, IFS, SWITCH and CHOOSE
retain explicit arrays or references under inherited array demand; singleton axes
broadcast, scalar reference/omission paths remain intact, and PinnedPoi retains
its scalar selectors. Exact information predicates inspect retained types without
conversion. This is not general function vectorization. Named/computed producers
without static bounds remain unsupported. Materialized omission, needed compound
expression execution, incompatible non-singleton dimensions and Calc's boolean,
padding and unused-shape differences remain explicit. See the individual
[IF](if-array-selection-contract.md), [information](information-array-contract.md),
[IFS](ifs-array-selection-contract.md), [SWITCH](switch-array-selection-contract.md)
and [CHOOSE](choose-array-selection-contract.md) contracts.

Blank remains typed inside expressions. A completed worksheet formula publishes
remaining blank as numeric zero, including spill members, before memoization.
Empty text stays text and physical empty cells stay blank. This follows the
user-confirmed Excel rule; Calc's inherited-empty behavior is separately retained
in the historical probes rather than modeled with extra value states.

Every changed owned suite has a current SHA-256-bound independent report listed
in the JSON reconciliation. Earlier reports intentionally retain superseded
hashes and observations; the current mapping identifies which refresh applies.
Saved source-cache provenance and diagnostic falsifications are preserved.
Independent sources comprise published contracts, saved workbook caches and fresh
isolated Calc recalculation; **no fresh native Excel execution is claimed**.

## Integrated verification

All commands below ran on `019fcc2`. Full logs and SHA-256 digests are recorded in
the JSON evidence under `/tmp/master-compat-final-*`; the pre-allocation-correction
run is retained separately under `/tmp/master-compat-pre-safety-*`.

| Command | Result |
| --- | --- |
| `cargo test --workspace` | 1,287 passed, one ignored, zero failed; 165 result groups, including the Excel workbook E2E runner |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/test_function_architecture.py` | Passed, one test |
| `python3 tools/test_coercion_architecture.py` | Passed, eight tests |
| `python3 tools/maintain.py --check` in `crates/poirot-test-excel-specs` | Passed; 2,004 authored cases / 2,065 assertions; this authoring check does not execute the engine |
| `cargo run -p poirot-test-json -- --format json` | 2,387 passing / 13 known gaps; zero failed, unexpected passes or harness errors; normal/overlay parity |
| `cargo run -p poirot-test-fods -- run --format json` | Expected exit 1: 2,006 passing / 14,105 known gaps / 52 improvements / three inherited regressions; zero harness errors |

FODS retains every original one of its 16,114 case identities across 516 fixtures.
The same inherited regression IDs remain:
`lambda__fods__isomitted__00005`, `lambda__fods__isomitted__00009` and
`statistical__fods__rank__00004`.

Exactly one original status changes: `text__fods__textjoin__00011` moves from
improvement to known gap. Its J13 formula `=F13`, with F13 physically absent, now
publishes numeric zero; `TEXTJOIN("-",1,I13:K13)` yields `A-0-C`, while Calc expects
`A-C`. This is the accepted Excel publication difference, not a TEXTJOIN-specific
exception. Thirty-one already recognized gaps change actual Blank to Number(0),
including 28 type-to-numeric mismatch classifications; their exact before/after
records are preserved. No FODS baseline or expectation was rewritten, and no new
regression is concealed by the totals.
