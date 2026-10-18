> Source-specific authoring reference. Use the [combined runner](../README.md) for execution, grouping and baseline comparison.

# JSON workbook scenarios

A growing corpus of authored compatibility and regression tests. Each JSON file
starts with one workbook definition and a list of named scenarios. Every scenario
starts from that workbook independently and executes its actions in order, once
through `Evaluation` and once through a persistent `Overlay`.

Both modes check the authored expectations. The runner also compares their
observations and **raw, ordered change lists after the last action**. It does not
run an additional evaluation, sort the lists, or remove cache changes. Agreement
between modes alone cannot establish correctness.

```sh
cargo run -p poirot-test-runner -- run --source json
cargo run -p poirot-test-runner -- run --source json --filter lookup
cargo run -p poirot-test-runner -- run --source json --json-corpus path/to/suite.json --format json
cargo run -p poirot-test-runner -- run --source json --strict
cargo test -p poirot-test-runner
```

The library exposes the serde schema, `parse_suite`, `run_suite`, and clap's
`Cli` parser. The binary only parses arguments, writes results, and sets the exit
status. `--json-corpus` accepts a file or a directory searched recursively. JSON output
is JSONL: one scenario record containing both modes' checks and change lists,
then a summary. An empty selection fails. `cargo test` from the workspace root
includes this crate and runs its corpus regression gate.

## Authoring a scenario

Add a JSON file under `corpus/`, or append a scenario to a related workbook. No
Rust registration or generated baseline is needed. For example:

```json
{
  "schema_version": 1,
  "workbook": {
    "sheets": ["S"],
    "cells": [
      {"cell": "S!A1", "content": {"type": "number", "value": 3}},
      {"cell": "S!B1", "content": {"formula": "A1*2"}}
    ]
  },
  "scenarios": [
    {
      "name": "recalculate after an input edit",
      "evidence": ["Multiplication: 3*2=6 and 4*2=8."],
      "actions": [
        {"action": "check", "cell": "S!B1", "expected": {"type": "number", "value": 6}},
        {"action": "set", "cell": "S!A1", "content": {"type": "number", "value": 4}},
        {"action": "check", "cell": "S!B1", "expected": {"type": "number", "value": 8}}
      ]
    }
  ]
}
```

Addresses are worksheet-qualified A1 references. The workbook lists worksheet
names in order; `cells` defaults to empty. Duplicate initial cell addresses and
scenario names are rejected. Unknown fields and unsupported schema versions are
errors.

Actions:

- `set`: replace a cell's content with a typed value or `{"formula": "..."}`.
  It preserves existing formatting.
- `format`: set `number_format` to an Excel format string, or `null` to clear it.
- `check`: evaluate a cell and compare its typed `expected` value. Optional
  `number_format` checks the effective format string; `display` checks rendered
  text. An optional nonnegative `tolerance` applies only to numeric comparisons
  against the expected value. Its default is zero.

Typed values are `number`, `text`, `boolean`, `error` (Excel spelling such as
`#N/A`), `utf16` (code units), and `blank` (no `value` field). Blank, empty text,
zero, and false remain distinct. Initial cells can also specify `number_format`.
Scenario edits do not affect later scenarios. An operational/evaluation failure
stops that mode's scenario and is reported as a harness error, not a worksheet
error value.

## Expectations and known gaps

Each scenario needs a short `evidence` list supporting its expectations. Cite a
specific documented rule, an independent engine reproduction, or an explicit
project contract. Label POI/LibreOffice observations accurately; do not claim
native Excel verification without it. Never derive expectations from Poirot's
own output.

A check can document a narrowly recognized gap:

```json
"known_gap": {
  "reason": "Explain the mismatch and the independent evidence.",
  "actual": {"type": "boolean", "value": false}
}
```

Keep the correct `expected` value intact. Only that exact observed `actual` is
accepted as the gap. A different wrong result fails; matching the expectation is
an unexpected pass and fails until the stale annotation is removed. Format,
display, mode, and change-list differences always fail. `--strict` additionally
fails recognized gaps. There is no automatic baseline refresh.

The seed corpus covers rounding precision, number-to-text coercion, blank lookup
consumers, numeric formatting, and the unresolved inline missing-IF-result gap.
These are small authored scenarios informed by the prior compatibility research,
not an imported third-party test suite. Extend the same structure as new behaviors
are investigated.

## Excel compatibility expansion

The complete corpus now contains **2,400 scenarios / 3,176 checks**:
**2,254 passing scenarios and 146 precisely recognized gap scenarios** (159 gap
checks). Both execution modes agree on observations and raw final change lists.
These counts include all authored suites, rather than only the ClosedXML collection.

The `excel-compat-*.json` suites add **936 scenarios / 1,106 checks** on top of
master's 1,464 scenarios. Every new suite explicitly uses
`"builtin_policy": "ExcelDocumentation"`. The additions have **792 passing scenarios
and 144 known-gap scenarios** (157 gap checks), including 110 multi-check scenarios.
No expected value was derived from Poirot output.

| Batch | Scenarios | Checks | Evidence and purpose |
| --- | ---: | ---: | --- |
| Authored boundaries | 308 | 466 | Pinned Apache POI regressions and Microsoft documentation; array criteria, conditional selectors, predicate arrays, comparison number grammars, broadcasting, distribution/rank domains |
| Saved value semantics | 390 | 390 | Saved Excel regression-workbook caches; typed operators, numeric/text/date/engineering admission, blank result materialization |
| Saved consumer semantics | 238 | 250 | Saved lookup, array, aggregate, financial and formatting regressions; complete array components are consolidated into contrasting checks |

Sources include Apache POI revision `dc577a8cbcdec22ae5547615502d63b2051f35a6`,
[its formula regression tests](https://github.com/apache/poi/tree/dc577a8cbcdec22ae5547615502d63b2051f35a6/poi/src/test/java/org/apache/poi/ss/formula),
[its saved spreadsheet fixtures](https://github.com/apache/poi/tree/dc577a8cbcdec22ae5547615502d63b2051f35a6/test-data/spreadsheet),
and the Microsoft function pages linked in each scenario. Saved workbooks are
independent evidence, not proof of a fresh native Excel execution. Their original
application metadata is recorded when present; some files do not record it.

The [saved-cache provenance](evidence/excel-compat-saved-cache-provenance.json)
records source file SHA-256, original formula and typed cached value, worksheet and
cell, reconstructed expression, and semantic prototype. Reconstruction expands
BIFF add-in names and named ranges, includes the bounded dependency closure, and
uses explicit INDEX coordinates to observe legacy array components. Source types,
blank inputs and coordinates are preserved. No whole upstream workbook is imported
into the executable corpus.

Every retained check was also recalculated through fresh isolated en-US LibreOffice
snapshots. The exact installed suites and action indices are bound by SHA-256 in
[authored-boundary evidence](evidence/excel-compat-authored-boundaries-libreoffice.json),
[saved-value evidence](evidence/excel-compat-saved-values-libreoffice.json), and
[saved-consumer evidence](evidence/excel-compat-saved-consumers-libreoffice.json).
Reproduce with the same optional verifier described below, passing
`corpus/excel-compat-*.json` and a fresh output path.

The [selection audit](evidence/excel-compat-selection.json) records 2,101 screened
candidates, rejected expectations, comparisons with existing master semantic
prototypes, and consolidation membership. Selection excludes locale mismatches,
ordinary solver roundoff, repeated missing functions, operational failures and mode
or change-list disagreements. Equivalent aliases, false-branch type observations,
and individual elements of one BIFF array are grouped as controls rather than
counted separately. Cached cases are selected across consumers, prioritizing
independently corroborated gaps rather than a numeric parameter sweep.

Four proposed multi-output observation sequences exposed normal/overlay final
change-list disagreements and were excluded from scalar gap coverage. The retained
matrix component checks reuse one observer cell; their explicit source INDEX
coordinates and independently authored expectations stay unchanged. A plain INDEX
predicate observer that did not establish array context in Calc was also removed;
the retained positional SUMPRODUCT checks establish array context explicitly.

## ClosedXML edge cases

The original collection contained **1,200 scenarios and 1,669 checks**. Subsequent
master fixes and regressions have grown the `closedxml-*.json` suites to **1,219
scenarios / 1,763 checks**, with 1,217 passing scenarios and two recognized gaps.
The historical tranche counts below describe collection-time screening, not the
current unresolved-gap count; later source-bound evidence supersedes older hashes
when a suite has changed. The seed and other regression suites are counted in the
complete corpus total above.

The original 600 scenarios derive from assertions at ClosedXML revision
`09f2f21a60e821ce7fb18ca3576b272e5dd6d921` and explicitly described controls.
The first tranche contains 300 scenarios / 388 checks selected from 588 candidates;
the extension adds 300 scenarios / 375 checks selected from 718 more candidates.

The exploration adds **600 scenarios / 906 checks**, selected from 1,011 candidates.
It transfers known issues to sibling consumers and reviews official Microsoft
function documentation for implicit rules. These cases cite the relevant upstream
assertion or documentation and explain how their expected values were derived.
All retained checks are independently corroborated by LibreOffice 26.2.6.3.
This is a Calc reproduction, not a claim of native Excel execution.

| Exploration batch | Scenarios | Checks | Distinct rules |
| --- | ---: | ---: | --- |
| Origins and selection | 168 | 425 | Selected text/blank contributions, numeric grammar, type inspection, reference composition, source domain boundaries |
| Criteria and text | 224 | 234 | Row-aligned conditions and edits, wildcard escapes, range geometry, text indices, locale-independent parsing, arithmetic versus SUMPRODUCT filtering |
| Statistics and other consumers | 208 | 247 | Paired-data filtering, rank ceilings, database AND/OR and duplicate matches, typed result fields, frequency endpoints, signed radix widths, complex signs and result types |

At collection time, the exploration had 552 passing scenarios and 48 known-gap scenarios (64 gap
checks), including 100 scenarios with multiple checks. Repeated type observations,
argument-position errors, and population-covariance aliases are consolidated into
contrasting checks rather than counted separately. Numeric magnitude and flag
sweeps, nominal examples, repeated missing functions, uncorroborated expectations,
and operational/mode disagreements are excluded. The
[selection record](../evidence/closedxml-exploration-selection.json) retains the
screening exclusions and consolidation membership.

The first tranche covers:

| Area | Scenarios | Boundaries covered |
| --- | ---: | --- |
| Scalar origins and selectors | 17 | Unary plus, blank versus empty text, IF/IFERROR, type predicates, reference edits |
| Text | 50 | UTF-16 lengths, index/count bounds, wildcard escaping, coercion, array traversal |
| Number parsing | 45 | Distinct NUMBERVALUE and arithmetic grammars; signs, fractions, percentages, time spans |
| Math | 57 | Negative rounding, arbitrary nonzero modes, zero significance, decimal multiples |
| Calendar | 45 | Negative fractional components, extreme DATE arguments, leap dates, month clipping |
| Lookup | 34 | Typed MATCH, unsorted bisection, INDEX shapes, reference identity, area bounds |
| Criteria | 20 | Escaped wildcards and SUMIF predicates across typed edits |
| Aggregates | 12 | COUNT origins, COUNTBLANK reference restrictions, errors, SUMPRODUCT |
| Array operators | 3 | Singleton row/column broadcasting in both operand orders |
| Financial | 17 | Zero/negative rates, fractional/negative periods, absent principal and future values |

The first tranche collection-time result was **227 passing scenarios and 73 known-gap scenarios**
(77 precisely annotated failing checks). Only seven checks represent wholly
unimplemented functions; repeated `#NAME?` results were excluded. Thirty-eight
scenarios contain multiple checks, including typed edits and contrasting consumers.
Numeric tolerances are local to checks with rounded source expectations or
floating-point arithmetic; NUMBERVALUE underflow/overflow boundaries are exact.

The first batch was re-audited. Uncorroborated assertions about parity functions,
IFERROR reference identity, logical scalar text, ROWS scalars, and descending MATCH
duplicates were removed or narrowed. Agreement with ClosedXML alone does not make
a Poirot mismatch an accepted gap. Operational errors and normal/overlay change-list
differences cannot be annotated away.

```sh
cargo run -p poirot-test-runner -- run --source json --filter ClosedXML
cargo run -p poirot-test-runner -- run --source json --filter ClosedXML --strict
cargo test -p poirot-test-runner
```

### Reproducing the independent evidence

The [original record](../evidence/closedxml-libreoffice.json) and
[extension record](../evidence/closedxml-extension-libreoffice.json) cover the original
600 scenarios. Exploration evidence is split into
[origins](../evidence/closedxml-exploration-origins-libreoffice.json),
[criteria and text](../evidence/closedxml-exploration-criteria-text-libreoffice.json), and
[statistics and other consumers](../evidence/closedxml-exploration-statistics-libreoffice.json).
Each record contains typed Calc results and SHA-256 hashes of the exact suites
checked. Reproduce the combined corpus into a fresh record with the
optional research verifier (requires LibreOffice and Python UNO,
usually supplied by `python3-uno`):

```sh
/usr/bin/python3 tools/verify_json_scenarios_libreoffice.py \
  crates/poirot-test-runner/corpus/json/closedxml-*.json \
  --output /tmp/closedxml-all-libreoffice.json
```

The verifier starts its own headless Calc process with a temporary en-US profile.
It rebuilds each check's authored workbook state as minimal OOXML, recalculates,
exports, and reads the typed cached result. Booleans, errors, and text are not
inferred from display strings. It never uses Poirot results to set expectations
or update known-gap annotations. LibreOffice is not required by `cargo test`.

Calc discards literal empty text cells on OOXML import, so the verifier represents
such inputs with `=""`, preserving their scalar type for reference consumers.
Formula-origin introspection is rejected. CR characters are XML-escaped so UTF-16
lengths survive import. Blank-reference scenarios seed an explicit blank cell;
absent-cell change publication is outside this selection. The oracle snapshots
verify values after edits; the JSON runner separately exercises persistent
invalidation and compares normal/overlay final change lists.

## Extension from 300 to 600

The extension favors new semantic boundaries over ordinary function demonstrations.
It adds 300 scenarios across 25 families, including 45 scenarios with multiple
checks and input edit/restore controls. The combined result is **472 passing
scenarios and 128 known-gap scenarios** (132 precise known-gap checks).

The additions cover matrix member validation and full-array consumers, reference
union multiplicity and chained-range bounding areas, implicit intersection,
lookup result-type forwarding, scalar INDEX area bounds, aggregate origin and
error policies, backward workdays, ISO week-year rollover, day-count bases,
TEXTJOIN traversal after edits, and optional financial arguments. The explicit
missing IF false-result check contrasts with the correctly handled omitted
false-result argument. No repeated wholly unimplemented-function examples were
added.

The [selection audit](../evidence/closedxml-extension-selection.json) records why
candidates were excluded. In particular, source nested-SUBTOTAL examples require
metadata unavailable to the current runner; their operational failures are not
accepted scalar gaps. Cross-engine disagreements and mode/change-list differences
were also excluded. A research control that edited a cell outside a lookup table
was rejected by Calc and removed before inclusion.

Reproduce just the new tranche:

```sh
/usr/bin/python3 tools/verify_json_scenarios_libreoffice.py \
  crates/poirot-test-runner/corpus/json/closedxml-extension-*.json \
  --output /tmp/closedxml-extension-libreoffice.json
```

Modern function names used by the oracle are written with their OOXML `_xlfn.`
prefix where required (including MINIFS/MAXIFS and COVARIANCE.P/S). A failed
import of a function name is not evidence of its worksheet semantics.


Suites may set `"builtin_policy": "ExcelDocumentation"` to test that explicit
public contract. Omitted policy metadata retains `PinnedPoi`; both normal and
overlay execution use the same selected policy, reported in each mode result.
This is suite metadata, not an override on individual expected values. Unknown
policy names are rejected during schema parsing.
