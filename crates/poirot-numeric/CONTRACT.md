# Shared POI numeric primitives

Pure binary64 comparison, number-text conversion and Java-compatible number
parsing. These functions are workbook-independent; callers supply provenance,
coercion and operational budgets.

## Policies

| Primitive | Numeric policy | Text policy | Rounding policy |
| --- | --- | --- | --- |
| `compare(f64, f64)` | Returns `CellError::Num` for any nonfinite argument. Signed zero orders negative before positive. Normal/subnormal threshold anomalies follow POI. | No text coercion. | POI 15-digit normalized decimal comparison; distinct binary64 inputs can compare equal. |
| `to_text(f64)` | Raw binary64 representation is passed through the existing POI-derived normalization. Consumers should use finite inputs; extraction retains historical nonfinite behavior without claiming complete POI special-value coverage. | Signed zero and signed subnormal display as `-0`/`0`. Uses the 20-character plain/scientific boundary, uppercase E, explicit exponent sign, and at least two exponent digits. This is POI numeric text, not locale/display formatting. | POI normalization and additional extreme-exponent rounding; does not use Rust shortest-roundtrip formatting. |
| `parse_hex(&str)` | Java hexadecimal binary64 syntax with optional sign, 0x/0X prefix, one optional point, mandatory p/P exponent, and decimal exponent digits. Overflow returns infinity; underflow preserves signed zero. | No whitespace trimming, decimal grammar, suffixes, locale, criterion operators, or cell coercion. Those belong to the caller. | Exact integer mantissa rounded once to binary64, ties to even, including subnormals. |
| `parse_java_number(&str)` | Preserves the database's Java decimal/hexadecimal grammar wrapper, including signed NaN/Infinity. | Trims characters U+0000 through U+0020; permits d/D/f/F suffixes on numeric literals. Decimal literals accept only ASCII digits, point, signs and e/E, then use the existing Rust binary64 parser. | Delegates hex to `parse_hex`; preserves existing decimal binary64 parsing. |

These policies are fixed by the named primitives, not configurable options.
Financial/statistical operations must retain their own input provenance,
coercion/error handling, final rounding, and reduction order.

## Pinned source evidence

POI revision: `732120980140d5ed64b482c470e0b625cdb1ab15`.
Paths are relative to `poi/src` in that upstream revision; no vendored checkout is required.

| Source | SHA-256 |
| --- | --- |
| `main/java/org/apache/poi/ss/util/NumberComparer.java` | `f5c63fa2f6c18d852267b2e70682fa1b10236a8574d667c4503a344f9d77cc89` |
| `main/java/org/apache/poi/ss/util/NumberToTextConverter.java` | `03859f50e1b4fc9e48cc62f6db8fc79942b738da01d088405cf616073425983f` |
| `main/java/org/apache/poi/ss/util/NormalisedDecimal.java` | `f4385334c4f89131d98993e4bb7c92eb5b2bf231e256d16f03536a4f1f646c97` |
| `main/java/org/apache/poi/ss/util/MutableFPNumber.java` | `ca0d21a70639f263c19433eb886df54948747e3ec465175f7a1f8aa54596883c` |
| `main/java/org/apache/poi/ss/util/ExpandedDouble.java` | `fb050e0887fd12864e3b4b164395e7c1ee042432c8e1eae2f2b5ed3dfe1d3072` |
| `test/java/org/apache/poi/ss/util/TestNumberComparer.java` | `1903ce40e83e26905f356f95ef111377601eceebf5d3b507432acdd346145200` |
| `test/java/org/apache/poi/ss/util/TestNumberToTextConverter.java` | `fc9311eb25f3a1cc987b5038d71815d7aaefc6f32cdb3b3ebe2523f9e0a2b6c9` |
| `main/java/org/apache/poi/ss/formula/functions/PercentRank.java` | `e5842466f17ba6ea0be4be53e43d0efd6aba2368d0d52596b7dea1ddaefde834` |

## Consumers

Database criteria, statistics and text algorithms share these primitives while
retaining their own grammar, argument-origin and error policies. Numeric text
conversion is separate from display formatting; use `poirot-format` for formatted
cell output.
