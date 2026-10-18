# Approximate MATCH wildcard limits

## Decision

Keep the four known gaps in
`../corpus/excel-compat-poi-cache-matchfunctiontestcasedata.json`.
No evaluator change is justified by this investigation. This is an assessment of
an existing limitation, not a claim that these cases have been fixed.

The four original formulas use ascending approximate mode (`match_type = 1`)
with `Albert, Charles, Ed, Greg, Ian, Cedric`. The final `Cedric` makes the vector
unsorted. Their lookup patterns and independent expected positions are:

| Pattern | Expected position | Current result |
| --- | ---: | --- |
| `*Ian*` | 5 | `#N/A` |
| `G*?eg` | 4 | `#N/A` |
| `G?eg` | 4 | `#N/A` |
| `??eg` | 4 | `#N/A` |

These expectations come from the pinned Apache POI source workbook's saved
caches and fresh LibreOffice recalculation. The saved creating application is
**not recorded**, and the caches are not a fresh native Excel execution. The
corpus expectations, identities, known-gap markers and source suite bytes remain
unchanged.

## Evidence and mechanism

[Microsoft's MATCH documentation](https://support.microsoft.com/en-gb/excel/functions/match-function)
requires ascending input for mode `1` and descending input for mode `-1`.
It documents wildcard matching and tilde escapes for exact mode `0`.
Thus the four examples do not establish a documented general Excel contract for
approximate wildcard matching on unordered data. This does not prove Excel would
reject these inputs; it limits what we can infer from the saved examples.

Current source separates exact first-match scanning, modern typed approximate
traversal, and pinned POI traversal. In
`poirot-eval/src/adapters/search/execution/classic_lookup_compare.rs`, wildcard
matches compare equal and every wildcard mismatch compares less. That is a
matching predicate, not an ordered comparison. In
`poirot-functions-lookup/src/classic_match.rs`, ascending approximate MATCH uses
bisection over candidate comparisons. A wildcard can match a later candidate
after earlier mismatches, so these comparisons need not be monotone even when
ordinary candidate text is sorted. Sorting alone does not repair this mismatch.
The pinned ascending MATCH path scans backwards; adopting it only for wildcard
keys would introduce a new traversal exception in the modern policy.

The bounded probe matrix contains 54 independently authored diagnostic
hypotheses: the four patterns across exact/ascending/descending modes and
unsorted/ascending/descending vectors, plus literal, escaped wildcard,
case-insensitivity, duplicate, scalar-type, error and unmatched controls.

- Fresh Calc observations confirm all four original production expectations.
- Calc agrees with 53 of 54 diagnostic hypotheses. The retained disagreement is
  descending wildcard duplicates: the first-match hypothesis predicts `2`, but
  Calc returns `3`.
- Poirot and Calc agree on 41 of 54 controls. Eleven disagreements concern
  ascending wildcard search, and two concern descending duplicate selection
  (one numeric, one wildcard). The numeric case demonstrates an existing
  traversal-policy difference; it is not new evidence for a wildcard exception.
- Exact wildcard and escape controls, ordered literal controls, and exact typed
  and error-candidate controls agree. Normal and overlay results and change
  lists agree for every Poirot control and all four production scenarios.

Diagnostic hypotheses are preserved as hypotheses, including the falsified one;
they are not new production expectations or claims about native Excel.

## Three options considered

1. **Keep the existing traversal and document the four gaps (selected).**
   This preserves a predictable model and the established source profiles,
   operational failures, budgets, dependencies and duplicate-selection rules.
   The limitation stays visible in the authored corpus. No new state or policy
   is needed. The benefit of fixing these four out-of-contract combinations
   does not currently justify a new exception.
2. **Make wildcard matching a separate candidate question and derive a new
   approximate selection contract.** This would stop pretending a predicate is
   an ordering, but the desired first/last-match and fallback behavior still
   needs independent Excel evidence. A richer reader protocol and traversal
   contract would carry conceptual and maintenance costs beyond these cases.
   Reconsider only if broader real usage and discriminating target evidence
   justify it.
3. **Use a wildcard-specific linear or pinned-search fallback.** This can make
   the four examples pass, but chooses a new modern-policy exception without
   establishing Excel's general behavior. It changes candidate reads, work and
   potential operational failures, and does not settle descending duplicates.
   A small diff would still be unintuitive code. Reject it.

## Reproduction and artifacts

Run from the repository root of this branch:

```sh
cargo run -p poirot-test-json -- --corpus crates/poirot-test-json/corpus/excel-compat-poi-cache-matchfunctiontestcasedata.json --format json
cargo run -p poirot-test-json -- --corpus crates/poirot-test-json/evidence/match-wildcard-controls.json --format json
/usr/bin/python3 tools/verify_json_scenarios_libreoffice.py crates/poirot-test-json/evidence/match-wildcard-controls.json crates/poirot-test-json/corpus/excel-compat-poi-cache-matchfunctiontestcasedata.json --output /tmp/match-wildcard-calc.json
```

The production run reports four known gaps and zero regressions. The diagnostic
Poirot command intentionally exits `1`: 42 hypotheses agree and 12 do not. The
Calc command also intentionally exits `1` for the retained descending-duplicate
hypothesis disagreement. Do not relabel these diagnostic mismatches as newly
accepted production bugs. No product code or conversion boundary changed, so no
new workspace or architecture run is necessary for this documentation change.

[Source-bound observations](match-wildcard-assessment.json) contain the raw Calc
results, source hashes, both Poirot execution modes and an engine comparison.
[Diagnostic inputs](match-wildcard-controls.json) live outside the production
corpus. No native Excel execution was available. The source production suite
SHA-256 remains
`6da23363694a10efb9b97d603fe6b443f888b4d9cfc53066dc7e7b5c66b7c37b`.
