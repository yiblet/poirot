# INDEX admission and representation contract

The original scenario names, formulas and authored expectations are unchanged.
The 20 INDEX scenarios formerly mixed into `closedxml-lookup.json` (17) and
`closedxml-exploration-upstream-selection.json` (3) now form `closedxml-index.json`.
The four scalar INDEX scenarios remain in `closedxml-extension-selection.json`.
Both INDEX suites explicitly request `ExcelDocumentation`. This preserves the
public default and the existing pinned ISREF tests.

## Independent evidence

- [Microsoft INDEX documentation](https://support.microsoft.com/en-us/excel/functions/index-function):
  zero row/column selects an entire dimension; indices outside the source produce
  REF; omitted area selects area one; reference results retain reference identity.
- [Pinned ClosedXML implementation](https://github.com/ClosedXML/ClosedXML/blob/09f2f21a60e821ce7fb18ca3576b272e5dd6d921/ClosedXML/Excel/CalcEngine/Functions/Lookup.cs#L127):
  rejects area below one with VALUE, rejects exhausted areas with REF, selects
  arrays separately from references, and returns array members as scalars.
- [Original ClosedXML assertions](https://github.com/ClosedXML/ClosedXML/blob/09f2f21a60e821ce7fb18ca3576b272e5dd6d921/ClosedXML.Tests/Excel/CalcEngine/LookupTests.cs#L368)
  remain linked in every scenario.
- `index-admission-libreoffice.json` binds all four modified suite files by SHA256:
  46 scenarios / 65 checks independently corroborated after the relocation.
- [Pinned POI INDEX](https://github.com/apache/poi/blob/732120980140d5ed64b482c470e0b625cdb1ab15/poi/src/main/java/org/apache/poi/ss/formula/functions/Index.java):
  rows/columns use OperandResolver integer flooring and return TwoDEval slices;
  the four-argument implementation throws an incomplete-code exception. The
  existing project four-argument extension was therefore inconsistent code,
  rather than an established POI scalar-area contract.

## Mechanism and alternatives

Scalar/array sources ignored area_num; unsigned and signed reference sources
separately validated it with a different zero error. Area admission now runs once
in coercionv2 before representation-specific selection. The pure lookup kernel
owns coordinate selection. Geometry, anchors, blank values and selected origins
remain with the source/result representation. Documented array-member selections
return selected scalars; the pinned TwoDEval shape remains unchanged.

The five identity scenarios already had correct ROWS/COLUMNS results; their only
failing checks were ISREF. Under ExcelDocumentation the shared ISREF consumer now
classifies actual reference channels, including unions, INDIRECT and references
forwarded by IF/CHOOSE. It no longer counts arrays as references. The default
PinnedPoi rule still counts TwoDEval arrays as reference-like.

Synthetic references were rejected: they would incorrectly make literal/computed
arrays pass ISREF. A universal view representation is unnecessary: existing array,
scalar, selected-scalar, signed-reference and reference channels retain the needed
information. Separate admission and pure selection fit the observed mechanism.

## Limits and deliberate retained behavior

A separate 62-case Calc probe varied source kinds, fractional/negative/large row,
column and area numbers, and selection consumers. Calc agrees on integer area
bounds and reference identity. It differs internally on fractional behavior:
`INDEX("Text",1,1,1.9)` returns REF, while corresponding arrays/references select
area one; negative fractional row/column values can be accepted for arrays while
references reject them. No representation-specific workaround was added.
The existing row/column flooring and signed-32 bound/error order are retained.
Positive area fractions retain the preexisting reference extension's truncation
rule; nonfinite numeric argument admission remains a consuming conversion.
Two-argument two-dimensional INDEX retains its existing REF contract.

Project blank preservation deliberately differs from native Excel/Calc numeric
publication: INDEX forwards blank; it does not replace it with zero. Raw evaluator
snapshot VERSION and retained COMPATIBILITY_REVISION both advance because cached
identity and area results change. Durable workbook source recovery is unchanged.

DynamicSpill execution of array-valued INDEX producers retains the preexisting
operational refusal when no supported static footprint bound can be established.
The ticket does not expand spill footprint planning; regression tests exercise
this refusal for literal/computed arrays. Reference-valued results retain the
existing scalar-intersection publication path (`value_projection::result_scalar` spills
only `Value::Array`), so they do not invent a spill child.
