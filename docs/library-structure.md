# Library structure

[Documentation](README.md)

Start with the public `poirot` crate. It re-exports the main workbook, evaluation
and file types so ordinary applications do not need to coordinate component
crates directly.

## Public workflow

```text
Spreadsheet / Workbook
        |
        v
Evaluation or Overlay + EvaluationContext
        |
        v
WorkbookChange values -> Workbook::apply
        |
        v
XLSX / XLS save plan, or Poirot snapshot
```

`Workbook` is the sparse source model: sheets, cells, formulas, styles, names and
date system. `Spreadsheet` adds file import/export and retained import warnings.
Evaluations hold edits separately and export owned changes for explicit application.

## Components and advanced APIs

| Public module | Responsibility | Component guide |
| --- | --- | --- |
| `model` | Typed cells, sparse workbook data, atomic change application | [Workbook model](../crates/poirot-model/README.md) |
| `workbook` | Spreadsheet facade and formula-aware source editing | [Workbook integration](../crates/poirot-workbook/README.md) |
| `eval` | Formula preparation, evaluation, policies, extensions and execution limits | [Evaluator](../crates/poirot-eval/README.md) |
| `parsing` | Formula, number, criteria and date/time grammars | [Parsing](../crates/poirot-parsing/README.md) |
| `format` | Excel number/date format compilation and display | [Formatting](../crates/poirot-format/README.md) |
| `io` | XLS/XLSX import and XLSX export | [File I/O](../crates/poirot-io/README.md) |
| `xls` | BIFF/XLS export and representation checks | [XLS](../crates/poirot-xls/README.md) |
| `artifact` | Versioned source and prepared-state persistence | [Snapshots](snapshots.md) |

For row, sheet or name topology changes, end workspace borrows before changing
the workbook. `edit::EditPlanner` and
`translation_edit::FormulaTranslationEditPlan` provide formula-aware structural
transactions. Use these APIs when formulas must be translated along with edits.

Spreadsheet function algorithms live in `poirot-functions-*` crates. They are
independent of workbook and evaluator implementations. `poirot-coercionv2` owns
argument conversion policies; `poirot-numeric`, `poirot-text` and `poirot-parsing`
provide shared primitives. Most users should call functions through evaluation,
which supplies references, localization, dependencies and budgets.

## Compatibility details

Formula support and file preservation are incomplete. Unknown formula functions
produce `#NAME?`; unsupported operational capabilities can fail the evaluation.
Builtin policy selection does not establish full Excel parity.

Consult [builtin policies](../crates/poirot-eval/docs/excel-builtin-policy.md),
[dynamic arrays](../crates/poirot-eval/docs/dynamic-array-results.md),
[text semantics](../crates/poirot-text/CONTRACT.md), and the
[SERIESSUM contract](../crates/poirot-functions-math/docs/seriessum.md)
when depending on edge-case behavior. The [test runner](../crates/poirot-test-runner/README.md)
explains the compatibility corpus, evidence sources and known gaps.
