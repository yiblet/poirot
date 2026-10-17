# Text family integration contract

Pinned POI revision: `732120980140d5ed64b482c470e0b625cdb1ab15`.

Source SHA256:

| File | SHA256 |
| --- | --- |
| functions/NumberValueFunction.java | f7b0715e34ecbd64edc4093d21b43e8475adf71f5ef8588ee5a290b791b39d52 |
| functions/Roman.java | 9b59a58e32c2e47656f00b6eb5ffcf402a320c68ac28d18ab9af0ee85a4bd058 |
| functions/TextFunction.java | 91aa31ac6feed5cc4fefb540a9e86e103d9a3586bb556e4f9987f89f28cdcd9f |
| atp/TextJoinFunction.java | a8e5e8af37f117f2ec3342b809cc500feb6c38f2dfe728418b622120aef21ef4 |

Typed registry identifiers dispatch through `Call<Expression>` and
`Host<Expression>` over an evaluation context. Family must control arity/error timing; do not pre-evaluate
arguments or eagerly flatten arrays. ROMAN requires exactly two arguments in pinned
POI (despite its documentation describing the second as optional). Zero number skips
form resolution. NUMBERVALUE outside 1..3 args returns zero in source. TEXTJOIN allows
3..254 args, traverses text areas row-major, takes delimiter area's final row and cycles
those delimiters. It has no source text-length cap. Missing delimiter becomes empty;
missing text is POI unchecked coercion failure, not a guessed spreadsheet result.

Use engine explicit locale decimal/group separators, default policy is ROOT/US.
Number conversion is POI `NumberToTextConverter`, integer coercion floors, and boolean
coercion accepts numeric nonzero and case-insensitive TRUE/FALSE text. Reference errors
must propagate through getSingleValue; ROMAN masks first coercion errors to VALUE and
second to NUM. `values` must reject cell errors before subsequent argument resolution.

PROPER processes Java UTF16 characters; supplementary characters remain unchanged
and reset capitalization state. Host Unicode policy must expose Java Character.isLetter
and Locale.ROOT single-char case conversion. Modern Rust Unicode data differs from
the Java runtime baseline; such drift is explicit pending oracle validation.

Empty separators and invalid Java regex separators are typed source exceptions.
NUMBERVALUE's Java regex class split drops trailing empty fields, so `12..` yields 12.
Java `Double.parseDouble` routes through shared `poirot_numeric::parse_java_number`,
shared with database criteria parsing.


The central module also exposes `coercev2_number_value_excel` for the evaluator's
explicit `ExcelDocumentation` policy. It uses decimal/exponent syntax and
literal separator tokens with locale defaults, rather than Java split/double
extensions. Empty decimal and conflicting separators are worksheet VALUE errors;
an empty grouping argument disables grouping (Calc agrees; ClosedXML differs).
The pinned entry point above retains its source faults and numeric spellings.
Supplementary explicit separators remain an operational unsupported contract
until native UTF-16 admission is established. The two disputed ClosedXML numeric
range expectations remain visible; no source-specific magnitude cutoffs are
applied to the documented kernel.

## Resource and failure boundary

`Failure<SpreadsheetError, OperationalError>` separates spreadsheet results,
operational failures, unsupported capability, and source exceptions. ROMAN masks
only the Spreadsheet variant. Budget denial and unavailable capabilities bypass
all POI error conversion.

Host must supply non-evaluating `values_bound` and `string_bound` plans before
allocation. Bounds can be conservative; absent bounds return Unsupported before
rendering. Methods must honor their advertised bounds (including internal coercion
allocations). `check_text`, `check_collection`, and `charge_work` are mandatory;
checks run before adapter-owned allocation, Unicode expansion, join rendering,
NUMBERVALUE transformations, and ROMAN rendering. Host checks must account for
live reservations and vector capacity overhead according to engine budget policy.
Do not treat resource limits as spreadsheet VALUE/NUM. Values/resolution and scalar
coercion errors already use the explicit Failure sum type.

PROPER custom renderers must override `proper_output_bound` if they exceed the
modern Unicode renderer's conservative 3-times-input UTF8 bound. Public standalone
algorithm helpers are pure, unbudgeted helpers; evaluator calls preflight them.

The immutable Unicode mapping assets live beside their loaders in `src/` with
`UNICODE-LICENSE.txt`. Regeneration scripts are in `tools/`; supply the pinned
OpenJDK 21.0.8 Unicode 15 source files explicitly. The simple-case generator
checks the UnicodeData source hash before producing an asset.
