# poirot-functions-engineering

Engineering calculations for spreadsheet functions.

Includes binary, octal, and hexadecimal conversion, complex-number helpers, and
Bessel calculations. Call operations such as `hex2dec`, `dec2bin`, `complex`,
and `besselj` with supplied values.

The caller resolves workbook references and evaluates arguments. Shared argument
parsing lives in [poirot-coercionv2](https://docs.rs/poirot-coercionv2).
