# Coercion boundaries

Poirot preserves a value's type until the operation consuming that value requires
conversion. Consumers include operators, individual function parameters,
and origin-sensitive aggregate collectors. Reads, selections, memoization and
result publication preserve blank values. There is no universal numeric argument conversion.

For example, a successful lookup into an empty result cell returns `Blank`.
`MIN` ignores that value, arithmetic can use it as zero, concatenation can use it
as empty text, and `ISBLANK` can inspect it. A missing match remains `#N/A`.
An empty string, a numeric zero, and a blank are different values.

A completed formula or function call can return `Blank`, including through a
formula-cell reference, memoized evaluation or spill child. Publication does not
convert it to zero. This is an explicit project contract; it can differ from the
final numeric result exposed by Excel or source-compatible public APIs.

A computed inline blank and a blank selected from a cell have different numeric
aggregate admission rules. Materialized selectors retain source provenance in
`SelectedScalar` without pretending to return a reference (`ISREF` stays false).
The scalar argument channel remains distinct from that source: selected logical
and text values still use scalar admission, while a selected blank remains
ignorable under the existing lookup contract. An inline blank contributes zero
only when an aggregate requests that numeric argument policy.

## Explicit conversion names

Argument conversions live in `poirot-coercionv2` and use `coercev2`,
`coercev2_...`, and the `CoerceV2` trait. The name identifies the consuming rule.
Existing result/publication conversions retain their `coerce_...` names so a grep
can identify the remaining exceptions. Do not alias either prefix to generic
names such as `number`, `text`, or `logical`.

The central crate owns scalar dispatch, numeric grammars, logical admission,
text spelling, integer bounds, origin-sensitive collection rules, and date/time
parsing plans. Function/evaluator adapters own reference selection, traversal,
budget admission and dependency recording. Their named conversion wrappers call
central policies with fallible continuations for those host effects.

The migration preserves pinned source behavior, including differences between
finite-number consumers, arithmetic operands that preserve nonfinite payloads,
and parsing failures. It does not establish new Excel compatibility claims or
repair known compatibility gaps.

```sh
rg -n '\bcoerce_[A-Za-z0-9_]+\s*\(' crates --glob '*.rs'
rg -n 'TODO\(coercion-non-argument\)' crates
python3 tools/test_coercion_architecture.py
python3 tools/test_function_architecture.py
```

Mark conversions outside function **and operator** argument handling with an
adjacent, regexable comment:
`// TODO(coercion-non-argument): [HIGH|MEDIUM|LOW] <potential issue>`.
Reads, selection, and memoization preserve semantic value types. Numeric kernel
intermediates, publication/cache normalization and display representation are
separate consumers and retain those audit markers.

The Python guard catches common inline scalar conversions, legacy helper names,
and aliases hiding a prefix. It also requires audit markers on remaining legacy
conversion declarations and rejects inline argument conversion bodies outside the
central crate. It is a syntactic guard; reviewers must still identify new policies.

## Audit dispositions

This inventory covers semantic conversion entry points and the implemented
function families, rather than listing every numeric cast inside an algorithm.
“Retained” means conversion belongs to the named consumer or is an explicit
existing compatibility policy. It does not assert native Excel parity for every
edge case. The audit did not execute Excel or replace pinned POI policies with a
new general Excel specification.

| Subsystem / family | Boundary and disposition | Source |
|---|---|---|
| Typed scalar storage | **Retained:** blank, number, boolean, error and exact text variants. Constructors describe ingress types; they do not parse arbitrary text into numbers. | [value.rs](../crates/poirot-functions-core/src/value.rs), [model](../crates/poirot-model/src/lib.rs) |
| Formula parsing / omitted syntax | **Retained:** grammar identifies numbers, strings, booleans, errors and omitted arguments. Missing flags are syntax metadata, not inferred from an empty cell. The tree/VM legacy missing-as-blank instruction is not the host's missing identity. | [parser](../crates/poirot-formula/src/parser.rs), [AST](../crates/poirot-formula/src/ast.rs), [host operand](../crates/poirot-eval/src/host/operand.rs) |
| Tree and bytecode execution | **Retained:** literals enter typed channels; both execute the same operator/adaptor kernels. No generic function-return finalization. Omitted syntax is inspected by deferred argument acquisition before scalar conversion. | [tree](../crates/poirot-eval/src/expression_execution.rs), [VM](../crates/poirot-eval/src/bytecode_vm.rs) |
| Deferred host / memoization | **Retained:** preparation stores values/references/arrays and missing flags. Reads copy the selected member and retain direct/reference/array origin. Merely preparing or memoizing an argument does not request numeric conversion. | [host](../crates/poirot-eval/src/host.rs), [readers](../crates/poirot-eval/src/host/readers.rs), [operand](../crates/poirot-eval/src/host/operand.rs) |
| Scalar / reference projection | **Retained:** implicit intersection selects a member without replacing its scalar type. Legacy scalar selection maps missing to blank, while callers needing omission identity use `scalar_data`. Final missing results can be unsupported rather than invented zero. | [value projection](../crates/poirot-eval/src/value_projection.rs), [signed references](../crates/poirot-eval/src/signed_reference.rs) |
| Arithmetic operators | **Retained:** numeric conversion occurs after selection, at the arithmetic operator. Error propagation and finite/nonfinite handling remain explicit. | [binary](../crates/poirot-eval/src/value_projection.rs), [unary](../crates/poirot-eval/src/expression_execution.rs), [numeric outcomes](../crates/poirot-eval/src/numeric_outcome.rs) |
| Unary plus / implicit intersection | **Retained exceptions:** pinned unary plus preserves text; explicit `@` selects a scalar. Neither is replaced with a blanket numeric conversion. | [unary implementation](../crates/poirot-eval/src/expression_execution.rs) |
| Comparison operators | **Retained:** typed ordering; blank compares using the other operand's type. Numeric text is not generally identical to a number. Pinned signed-zero/numeric-precision behavior is independent of lookup result forwarding. | [comparison](../crates/poirot-eval/src/core_projection.rs), [host equality](../crates/poirot-eval/src/host/coercionv2.rs) |
| Text concatenation | **Retained:** text conversion belongs to `&`; numeric spelling uses spreadsheet precision, without mutating the original number or applying its display format. | [text execution](../crates/poirot-eval/src/adapters/text/execution.rs), [scalar text projection](../crates/poirot-eval/src/core_projection.rs) |
| Array operators | **Retained:** broadcasting and member selection retain scalar types; each selected member is then passed to the scalar operator consumer. Array construction does not finalize each nested value. | [array operators](../crates/poirot-eval/src/array_operator.rs) |
| HLOOKUP / VLOOKUP / MATCH | **Fixed in ticket 004:** H/V return the selected result unchanged. Removed premature final-result blank-to-zero conversion. Key/mode/index conversion remains owned by search; MATCH returns a numeric position by definition. | [search execution](../crates/poirot-eval/src/adapters/search/execution.rs) |
| LOOKUP / XLOOKUP / XMATCH | **Retained:** lookup results retain selected type; key wildcard conversion and mode/index conversions are search consumers. Existing pinned fallback evaluation/blank-fallback behavior remains source-specific, not a new general coercion rule. | [lookup adapter](../crates/poirot-eval/src/adapters/lookup/mod.rs), [search](../crates/poirot-functions-lookup/src/search.rs), [comparison](../crates/poirot-functions-lookup/src/comparison.rs) |
| INDEX / OFFSET / INDIRECT / reference functions | **Retained:** reference-valued results preserve identity for the next consumer. Numeric row/column/offset parameters and textual address parsing are function-specific inputs. ROW/COLUMN/ROWS/COLUMNS intrinsically return numbers. | [reference execution](../crates/poirot-eval/src/adapters/reference/execution.rs), [lookup reference helpers](../crates/poirot-functions-lookup/src/reference.rs), [index helpers](../crates/poirot-functions-lookup/src/index.rs) |
| IF / IFERROR / IFNA / CHOOSE | **Retained, regression coverage expanded:** selected result/reference forwarded; IF's condition and CHOOSE's index consume conversions. IF explicitly returns zero for an omitted selected branch and FALSE for an absent false branch. Error handlers catch designated spreadsheet errors, not operational failures. | [logical execution](../crates/poirot-eval/src/adapters/logical/execution.rs) |
| IFS / SWITCH / logical predicates | **Retained, regression coverage expanded:** selected operand retains value/reference/missing identity. Condition/equality/parity consumers request their own conversions. AND/OR use origin-sensitive collection; skipped branches remain deferred. | [logical policy](../crates/poirot-eval/src/adapters/logical/policy.rs), [logical kernel](../crates/poirot-functions-logical/src/lib.rs) |
| Information / explicit N and T | **Retained:** IS predicates classify without numeric coercion; TYPE produces a type code. N/T intentionally convert under their own contracts, including boolean-to-number and nontext-to-empty-text. | [information](../crates/poirot-eval/src/adapters/information/execution.rs) |
| Math | **Retained:** per-function scalar numeric parameters, missing defaults, booleans and text parsing request central profiles. ROUND's decimal normalization is its numeric algorithm, not a mutation of an upstream numeric value. | [math policy](../crates/poirot-eval/src/adapters/math/policy.rs), [math execution](../crates/poirot-eval/src/adapters/math/execution.rs), [math kernel](../crates/poirot-functions-math/src/lib.rs) |
| Aggregates / statistics | **Retained:** collectors distinguish direct, reference and array origins, ignore or admit blanks/booleans/text according to the selected operation, and distinguish missing. Paired numeric collections have different policies from scalar distribution parameters. No universal aggregate conversion is introduced. | [statistics coercion](../crates/poirot-eval/src/adapters/statistics/policy/coercionv2.rs), [collections](../crates/poirot-eval/src/adapters/statistics/policy/collections.rs), [statistics kernel](../crates/poirot-functions-statistics/src/lib.rs) |
| Financial | **Retained:** scalar rates/options, optional omitted defaults, cashflow collection and numeric-text grammar have distinct consumers. Numeric kernels receive already admitted numeric inputs. | [financial coercion](../crates/poirot-eval/src/adapters/financial/policy/coercionv2.rs), [financial policy](../crates/poirot-eval/src/adapters/financial/policy.rs), [financial kernel](../crates/poirot-functions-financial/src/lib.rs) |
| Engineering | **Retained:** radix/complex text conversion differs from numeric scalar conversion; parse-only numeric grammar differs from date-aware scalar coercion. Numeric/text outputs follow the selected function. | [engineering policy](../crates/poirot-eval/src/adapters/engineering/policy.rs), [engineering kernel](../crates/poirot-functions-engineering/src/lib.rs) |
| Dates / times | **Retained:** date serials are numbers, with separate display formatting. DATEVALUE/TIMEVALUE explicitly parse text. Calendar integer truncation, serial construction, date-system adjustments and clock-context reads are algorithm/consumer operations. | [dates](../crates/poirot-eval/src/adapters/dates/execution.rs), [date text](../crates/poirot-coercionv2/src/dates/text.rs) |
| Text / VALUE / TEXT | **Retained:** text functions own their string/numeric parameter rules. VALUE intentionally parses; TEXT intentionally produces text. UTF-16 unit operations preserve exact text; checked scalar projection fails explicitly when unsupported. | [text policy](../crates/poirot-eval/src/adapters/text/policy.rs), [unit consumers](../crates/poirot-eval/src/adapters/text/units.rs), [text kernel](../crates/poirot-functions-text/src/lib.rs) |
| Criteria / SUMIF(S) / COUNTIF(S) | **Retained:** criterion parsing and matching are consumers with their own blank/error/text rules; sum members are selected separately. Known COUNTIF compatibility gaps are not repaired by changing raw storage types. | [criteria adapter](../crates/poirot-eval/src/adapters/criteria/mod.rs), [criteria projection](../crates/poirot-eval/src/criteria_adapter.rs), [criteria kernel](../crates/poirot-functions-criteria/src/lib.rs) |
| SUBTOTAL | **Retained:** metadata exclusion precedes reading/aggregation. Filtered cells and nested subtotal exclusion are selection rules, not value coercion. Included members use the chosen aggregate's contract. | [subtotal host](../crates/poirot-eval/src/host/subtotal.rs), [subtotal kernel](../crates/poirot-functions-criteria/src/subtotal.rs) |
| Database functions | **Retained explicit pinned policy:** criteria/header conversion and selected numeric collection belong to database operations. DGET skips blanks and requires one nonblank selected value; empty-text handling is explicit. DMIN/DMAX's empty-result zero is intrinsic aggregate output. The bridge forwards selected record types before the kernel's operation-specific handling. | [database adapter](../crates/poirot-eval/src/adapters/database/mod.rs), [selection](../crates/poirot-functions-database/src/selection.rs), [aggregation](../crates/poirot-functions-database/src/aggregation.rs) |
| Matrix functions / TRANSPOSE | **Retained explicit profile differences:** documented-profile TRANSPOSE preserves text/booleans/errors/blanks. Pinned matrix collection compacts selected numeric members and pads zeros, including pinned TRANSPOSE; it is not a general selector rule. Determinant/inverse/multiply/trend/sumproduct each own collection rules. | [matrix adapter](../crates/poirot-eval/src/adapters/matrix/mod.rs), [matrix policy](../crates/poirot-eval/src/adapters/matrix/policy.rs), [matrix collection](../crates/poirot-functions-matrix/src/collection.rs) |
| Shared number / boolean / text helpers | **Retained:** helpers are requested conversions, not automatic producer hooks. Ordinary arithmetic, numeric scalar arguments, VALUE, and admitted direct aggregate text share the locale-parametric formatted-number grammar and lazy calendar continuation. Exact-number requests, deliberately narrow DecimalNumber requests, numeric-only collections, Java ingress, and NUMBERVALUE explicit-separator rules remain distinct. | [core projections](../crates/poirot-eval/src/core_projection.rs), [host coercion](../crates/poirot-eval/src/host/coercionv2.rs), [numeric grammar](../crates/poirot-eval/src/host/numeric_policy.rs), [date fallback](../crates/poirot-coercionv2/src/date_time.rs), [numeric library](../crates/poirot-numeric/src/lib.rs) |
| Numeric outcome / cache-setter policies | **Retained:** nonfinite validation and pinned numeric construction are explicit fidelity policies, separate from operand type conversion. Optional POI setter normalization records computed/stored identities; default publication preserves computed values. | [numeric outcome](../crates/poirot-eval/src/numeric_outcome.rs), [materialization](../crates/poirot-eval/src/materialization.rs) |
| Final worksheet and public evaluation | **Blank-preserving:** direct/bound/cell execution, spill cells, session memoization and public function calls publish the selected scalar unchanged. The former `coerce_cell_publication` blank-to-zero hook has been removed. | [value channels](../crates/poirot-eval/src/lib.rs), [session execution](../crates/poirot-eval/src/session_execution.rs), [public adapter call](../crates/poirot-eval/src/adapters/interface.rs) |
| Spill publication | **Retained:** final anchor and child cells publish normalized results; nested arrays/operator buffers retain their original element types until publication. | [spill publication](../crates/poirot-eval/src/spill_publication.rs) |
| Overlay / cache / retained observations | **Retained:** source values and computed results remain typed; dependency retention and reset do not parse display strings. Reads of formula cells use their already finalized results. | [overlay evaluation](../crates/poirot-eval/src/overlay/evaluation.rs), [source](../crates/poirot-eval/src/overlay/source.rs), [run overlay](../crates/poirot-eval/src/run_overlay.rs), [retention](../crates/poirot-eval/src/execution_retention.rs) |
| Extensions | **Retained:** scalar/member ports expose raw value and omission flags. Provider capability checks and explicit output validation remain separate; operational aborts do not turn into catchable spreadsheet errors. | [extension invocation](../crates/poirot-eval/src/extensions/invocation.rs) |
| XLS/XLSX import | **Retained:** decoded numeric/date-serial fields become model numbers, strings stay text, booleans stay boolean and empty stays blank. ISO date/duration types outside model fidelity fail explicitly. Integer-to-f64 is model representation, not text coercion. | [decoded cells](../crates/poirot-io/src/decoded_cells.rs), [import](../crates/poirot-io/src/lib.rs) |
| XLSX / XLS export | **Retained:** writer chooses typed wire encoding from `CellValue`; blank does not gain a numeric payload, text does not become numeric text, and booleans use boolean fields. Numeric finite/fidelity checks remain explicit. | [worksheet encoding](../crates/poirot-io/src/ooxml/worksheet.rs), [XLS value encoding](../crates/poirot-xls/src/workbook_encoding.rs), [XLS formula cache](../crates/poirot-xls/src/representation.rs) |
| Display formatting | **Retained:** renderer takes typed values plus independent format/context and returns display text; it does not replace raw values. Only explicit TEXT returns that display as an expression's text value. | [formatter](../crates/poirot-format/src/lib.rs), [evaluated output](../crates/poirot-eval/src/overlay/output.rs) |
| Artifact / public facade | **Retained:** facade reexports typed APIs; snapshot serialization preserves model/cache/policy identity rather than parsing rendered output. | [facade](../src/lib.rs), [artifact](../src/artifact.rs) |

## Deliberate exceptions and limits

The principle is consumer-owned conversion, not a prohibition on all producer
normalization. Numeric functions produce numbers; explicit conversion functions
produce converted values; final cell publication has its own contract. Omitted
optional parameters can have function-defined defaults. These are distinct from
replacing a blank returned by a nested lookup with zero.

Compatibility profiles remain explicit. In particular, pinned matrix numeric
compaction, DGET blank filtering, XLOOKUP fallback rules and special numeric
outcomes need their own source/specification evidence before changing behavior.
Their presence is recorded here rather than silently presented as universally
verified Excel rules.

UTF-8/UTF-16 encoding, numeric casts in calendar/matrix/radix algorithms, typed
wire serialization and numeric decimal rendering are representation/algorithm
operations. They were reviewed separately from semantic scalar coercion. Some
family ports require a checked Unicode scalar projection and can reject isolated
UTF-16 surrogates, including eagerly converted materialized lookup arrays. This
is an existing explicit capability limitation, not silent replacement text or a
new numeric coercion fix. Exact-unit text consumers retain their existing path.

## Regression and audit evidence

`selection_preserves_types_until_the_selected_consumer` exercises logical
forwarding through aggregate, arithmetic, text and type-inspection consumers,
including booleans, numeric text, empty text, errors, IF omission defaults and
INDEX array selection. `lookup_blanks_reach_consumers_before_formula_cell_normalization` covers
classic and modern lookup/reference selectors, raw blank versus zero/empty text,
inline expressions versus finalized formula-cell references, and both tree and
compiled evaluation. Public overlay/export coverage exercises the publication
boundary separately.

The audit searched conversion/coercion helper names, blank/missing match arms,
numeric parsers, text projections and finalization call sites across the
workspace, then followed the family adapters and typed storage/IO paths. The
production correction found was the H/V premature finalization (ticket 004).
The broader audit did not establish another premature numeric producer coercion;
it retains consumer-specific rules rather than making speculative changes.
The existing FODS corpus did not expose the nested blank-lookup defect: all
16,114 records were unchanged immediately after ticket 004. New focused
regressions cover that missing behavior; existing fixture expectations were not
rewritten to accept the change.

## Shared rule corrections after the v2 migration

Paired statistics now request numeric-only membership from the central crate.
A position contributes only when both values are already numbers, so numeric
text and booleans cannot add a sample or change the means. Original lengths still
validate input shape, while regression consumes the admitted sample size. This
rule is shared by correlation, covariance, slope/intercept and forecast; their
existing acquisition order, error precedence and operational failures remain.
The legacy explicit numeric-converting pair policy remains available for callers
that actually request it, rather than being selected by statistical datasets.

Text index/count consumers request a finite signed-32 target before performing
empty-string shortcuts or clamping to the string length. Checked admission uses
the truncated target and VALUE on overflow, preserving the raw fractional value
for each function's sign checks. Pinned text policy retains its own floor rule
and NUM error. This fixes common argument admission for MID and REPT without
special-casing those function names or the failing formulas.

The authored expected values remain unchanged. Resolved gap metadata is removed
only after normal and overlay results agree, and the affected suites have fresh
Calc evidence in `coercionv2-paired-libreoffice.json` and
`coercionv2-text-integers-libreoffice.json` under the JSON runner's evidence directory.

## Formatted numeric input

`NumberLocaleV2` stores validated literal decimal, grouping, currency, percent and
time tokens. `RunConfiguration::with_number_locale` configures direct execution;
`EvaluationContext::number_locale` configures workspaces. The default is explicit
US-English input syntax, independent of display currency settings and process
locale. Tokens are runtime strings, including Unicode and multi-character tokens.

The shared nom grammar recognizes decimal/exponent, suffix percent, currency,
accounting negatives, mixed fractions and elapsed times before constructing a
finite number. It validates complete alternatives before selecting one and never
strips punctuation from unrecognized text. Complete numeric forms with invalid
numeric values remain worksheet errors; operational admission failures escape
IFERROR. Complete numeric-duration candidates cannot fall through to a looser
clock parser after failing field rules. Calendar and AM/PM interpretation remains
an explicit lazy continuation, consulting the captured year only if needed.

`DependencyRead::NumberLocale` is emitted only when numeric text is parsed. Exact
locale identity survives retained snapshots and invalidates only readers and their
dependents. Raw snapshot wire version 6 and engine compatibility revision 7 reject older runtime snapshots whose
results lack the new numeric argument semantics and dependency traces; durable
workbook source remains recoverable.

Independent Calc observations establish suffix percent and full fractional-second
precision. The inspected ClosedXML helper also admits prefix percent and rounds
elapsed fractions to milliseconds; these uncorroborated extensions are not enabled
by the default grammar. The retained pinned calendar fallback still requires two
day digits in slash dates (`M/dd/yyyy`); broader calendar admission is tracked
separately, rather than changing the numeric grammar to compensate.


NUMBERVALUE under `ExcelDocumentation` uses its central explicit-separator grammar
and the configured numeric locale for omitted arguments. Explicit separator text
uses its first BMP character (supplementary separators are operationally unsupported
pending a verified UTF-16 contract); equal separators, empty decimal separators, repeated decimals,
and grouping after the decimal are worksheet VALUE errors. An explicit empty
grouping separator disables grouping, as corroborated by Calc; ClosedXML rejects
that argument, an explicit source disagreement. Decimal/exponent input
must be finite and must not already underflow a nonzero significand to zero.
Percent suffixes apply repeated division after parsing, allowing those operations
to underflow. The pinned Java grammar, split faults, hex doubles, and suffix doubles
remain selectable under `PinnedPoi`. Decimal syntax and the IEEE result domain
are distinct: the two ClosedXML range fixtures remain open pending native Excel
evidence; Calc's omitted-decimal bug does not establish those magnitude limits.
