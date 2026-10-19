# poirot-functions-math

Numeric spreadsheet algorithms, including rounding, factorials, combinations,
ceiling/floor variants, and transcendental functions. Many kernels follow
Apache POI behavior.

Call helpers such as `round`, `fact`, or `ceiling_math` with numeric operands.
The `transcendental` module contains shared source-compatible kernels. Argument
conversion, workbook references, and function dispatch belong to the evaluator.
