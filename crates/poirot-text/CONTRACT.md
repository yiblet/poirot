# Text unit policies

`TextSemanticsPolicy` names the source/version of counting and slicing rules.
It separates semantic units and source length ceilings from display width and
caller-owned allocation budgets. No normalization or replacement of malformed
text is implied.

| Source | LEN / indexing | Concatenation |
| --- | --- | --- |
| `PoirotF6c572d` | Unicode scalars | `&` limits UTF-8 bytes; CONCATENATE limits scalars to 32767; CONCAT unavailable in this legacy policy. |
| `Poi7321209` / `Poi7321209OpenJdk21Unicode15` | UTF-16 units | No source length ceiling. |
| `ExcelCompatibilityV2Documented` | Adjacent UTF-16 surrogate pairs count once | UTF-16 units without a source length ceiling. |
| `ExcelCompatibilityV2Unverified` | Explicit unsupported result | Explicit unsupported result. |

`length`, `slice`, `left`, `right` and `poi_mid` are pure UTF-8-view helpers.
A slice that would require isolated surrogates returns `Unsupported` rather than
substituting U+FFFD. The evaluator's lossless unit adapters can preserve such
outputs in `ExcelText`. `measure_concat` reports both allocation bytes and
source-semantic length; hosts must check their own budgets before allocation.
The Java integer-overflow boundary in `poi_mid` is an operational refusal.

The standalone `java_root_uppercase` helper supports ASCII. The evaluator's
separate OpenJDK mapping adapter admits Unicode 15 unconditional mappings and
bounded contextual sigma rules. Neither Rust's Unicode tables nor these source
profiles establish general Excel casing parity.

## Documented Excel Compatibility Version 2

`ExcelCompatibilityV2Documented` is a distinct, documentation-backed policy,
not an observed Excel runtime identity. Microsoft documents counting surrogate
pairs once for LEN/MID/LEFT/RIGHT/REPLACE, with variation selectors counted
separately:
- https://support.microsoft.com/en-us/excel/compatibility-versions
- https://support.microsoft.com/en-us/excel/functions/left-function
- https://support.microsoft.com/en-us/excel/functions/right-function
- https://support.microsoft.com/en-us/excel/functions/replace-function

The lossless adapter scans exact UTF-16 units and combines only adjacent valid
high/low pairs. Isolated surrogates remain separate, preserved units; combining
marks and variation selectors are independent characters. Work admission occurs
before boundary scans; output storage admission occurs before copying. REPLACE
uses the selected indexing units, including isolated-surrogate outputs in V1.
The default ROI replacement behavior and POI MID overflow contract are retained.

Other operations retain the pinned adapter behavior. Case conversion reuses the
explicit OpenJDK 21 Unicode 15 ROOT table; emoji plus ASCII conversion is covered
by regression tests, but this does not establish general Excel casing parity.
Contextual sigma restrictions remain operational failures. FIND/SEARCH are still
pinned UTF-16 adapters; their V2 indexing is outside this implementation's scope.
The original `ExcelCompatibilityV2Unverified` refusal is unchanged.
