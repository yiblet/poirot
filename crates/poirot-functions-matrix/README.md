# poirot-functions-matrix

Matrix and array calculations for spreadsheet functions.

Use `Matrix` with `transpose`, `multiply`, `determinant`, or `inverse`. Other
operations include `frequency`, `sumproduct`, and `trend`.

The caller supplies arrays and a `Budget` for cell counts and estimated work.
Exceeding a budget is reported separately from spreadsheet errors.

Modern regression uses a shared, scaled column-pivoted least-squares fit. It
returns original-order coefficients, centered predictions and optional typed
statistics without forming normal equations. See [regression contracts and
numerical policy](REGRESSION_SOURCES.md) for rank selection and compatibility
limits. The pinned TREND path retains its reference behavior.
