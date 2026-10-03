# LibreOffice Calc FODS corpus

The 516 original FODS fixtures come from
[`LibreOffice/core` at `cd35bc0879ed5e4b8138b724033ea5937751a84d`](https://github.com/LibreOffice/core/tree/cd35bc0879ed5e4b8138b724033ea5937751a84d/sc/qa/unit/data/functions).
FODS is Flat OpenDocument Spreadsheet XML. These files retain Calc's source
formulas, inputs and `Expected`/`Correct` checks for the Rust formula harness.

`fixtures/` holds unchanged upstream files. `manifest.json` records their source
revision, paths, hashes and extracted case inventory; `classifications.json`
records the pass/known-gap baseline used to detect regressions. The upstream
files are MPL-2.0; see [COPYING.MPL](COPYING.MPL).

See [the runner](../README.md) and [how to refresh](../REFRESH.md).
