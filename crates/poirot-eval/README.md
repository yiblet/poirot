# poirot-eval

The formula engine: compilation, builtin dispatch, incremental evaluation,
input overlays, and evaluation snapshots.

Use this crate when working on execution or when you need explicit budgets,
custom functions, dependency observations, or compilation controls. Start with
`FormulaCompiler`, `Evaluation`, or `Overlay`; inspect `registry` for supported
builtins and `extensions` for custom function contracts.

For everyday workbook workflows, use the [root library](../../README.md).
