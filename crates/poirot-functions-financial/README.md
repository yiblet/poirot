# poirot-functions-financial

Financial calculations for spreadsheet functions.

Includes loan payments, present and future values, cash-flow returns, and
financial text formatting. Call operations such as `pv`, `fv`, `pmt`, `npv`,
and `irr` with numeric inputs; no workbook is required.

For periodic calculations, rates are fractions per payment period: use
`0.06 / 12.0` for a nominal annual rate of 6% with monthly payments. Period counts
use the same unit. Cash paid and cash received have opposite signs.
Where an operation takes `beginning`, `true` means payment at the start of each
period and `false` means payment at the end.

`fixed` and `dollar` format financial text. They are separate from cell display
formatting in [poirot-format](https://docs.rs/poirot-format).
