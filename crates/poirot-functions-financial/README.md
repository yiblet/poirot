# poirot-functions-financial

Financial calculations for cash flows, loans, annuities, and rates, with
algorithms based on Apache POI.

Entry points include `pv`, `fv`, `pmt`, `nper`, `npv`, `irr`, `mirr`, and `rate`.
Supply numeric inputs and payment timing explicitly. The crate also provides
financial text helpers such as `fixed` and `dollar`; the evaluator handles
workbook references and argument evaluation.
