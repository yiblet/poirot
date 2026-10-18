# Coercion boundaries

Poirot preserves a value's type until the operation consuming that value requires
conversion. Consumers include operators, individual function parameters,
and origin-sensitive aggregate collectors. Reads, selections, memoization and
intermediate forwarding preserve blank values. There is no universal numeric argument conversion.

For example, a successful lookup into an empty result cell returns `Blank`.
`MIN` ignores that value, arithmetic can use it as zero, concatenation can use it
as empty text, and `ISBLANK` can inspect it. A missing match remains `#N/A`.
An empty string, a numeric zero, and a blank are different values.

A completed worksheet formula publishes a remaining `Blank` as numeric zero,
including every generated spill member, before memoization and downstream reads.
Empty text remains text. Inline expressions, ad hoc prepared/bound evaluation and
public function calls preserve blank results because they do not publish a
worksheet cell. Physical empty cells remain blank.

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
| Unary plus / implicit intersection | **Value operators:** default unary plus preserves the selected scalar type (including blank and error) and array member types, while dropping reference identity. The explicit pinned numeric policy retains its text-only exception and numeric admission for other types. Explicit `@` selects a scalar. | [unary implementation](../crates/poirot-eval/src/expression_execution.rs) |
| Comparison operators | **Retained:** typed ordering; blank compares using the other operand's type. Numeric text is not generally identical to a number. Pinned signed-zero/numeric-precision behavior is independent of lookup result forwarding. | [comparison](../crates/poirot-eval/src/core_projection.rs), [host equality](../crates/poirot-eval/src/host/coercionv2.rs) |
| Text concatenation | **Retained:** CONCAT traverses arguments and their members in row-major order, preserving types, origin and omission until central text admission. Legacy CONCAT treats omission and blank as empty text; pinned text sources keep their distinct omission contracts. Collection traversal does not itself request array expression demand. Text conversion belongs to `&`; numeric spelling uses spreadsheet precision, without mutating the original number or applying its display format. | [text execution](../crates/poirot-eval/src/adapters/text/execution.rs), [scalar text projection](../crates/poirot-eval/src/core_projection.rs) |
| Array operators | **Retained:** broadcasting and member selection retain scalar types; each selected member is then passed to the scalar operator consumer. Explicit arrays retain shape independently of final scalar/spill publication. Matrix consumers request array expression execution for their array roles before argument acquisition (TREND keeps its const role scalar); ordinary scalar formulas retain reference intersection, including reference operands paired with literal arrays. Scalar function parameters retain scalar argument contracts; automatic scalar-function vectorization is a separate capability. Lazy selection result roles inherit their caller's demand. Argument memoization fixes each role before any projection, and dependency formulas reset to their own worksheet demand. Array construction does not finalize each nested value. | [array operators](../crates/poirot-eval/src/array_operator.rs) |
| HLOOKUP / VLOOKUP / MATCH | **Fixed in ticket 004:** H/V return the selected result unchanged. Removed premature final-result blank-to-zero conversion. Key/mode/index conversion remains owned by search; MATCH returns a numeric position by definition. | [search execution](../crates/poirot-eval/src/adapters/search/execution.rs) |
| LOOKUP / XLOOKUP / XMATCH | **Retained:** lookup results retain selected type; key wildcard conversion and mode/index conversions are search consumers. Existing pinned fallback evaluation/blank-fallback behavior remains source-specific, not a new general coercion rule. | [lookup adapter](../crates/poirot-eval/src/adapters/lookup/mod.rs), [search](../crates/poirot-functions-lookup/src/search.rs), [comparison](../crates/poirot-functions-lookup/src/comparison.rs) |
| INDEX / OFFSET / INDIRECT / reference functions | **Retained:** reference-valued results preserve identity for the next consumer. Numeric row/column/offset parameters and textual address parsing are function-specific inputs. ROW/COLUMN/ROWS/COLUMNS intrinsically return numbers. | [reference execution](../crates/poirot-eval/src/adapters/reference/execution.rs), [lookup reference helpers](../crates/poirot-functions-lookup/src/reference.rs), [index helpers](../crates/poirot-functions-lookup/src/index.rs) |
| IF / IFERROR / IFNA / CHOOSE | **Retained, regression coverage expanded:** selected result/reference forwarded; IF's condition and CHOOSE's index consume conversions. IF retains blank for an explicitly omitted selected branch and FALSE for an absent false branch; worksheet publication converts a remaining blank to zero. ExcelDocumentation IF selects array conditions by position, preserving array identity even for a singleton condition, broadcasting singleton axes, and returning #N/A for exhausted selected branch axes. ExcelDocumentation CHOOSE selects each retained index using its existing scalar numeric admission policy, sharing IFS/SWITCH singleton-axis broadcasting and deferred selected-member reads; incompatible needed non-singleton extents return #VALUE!. Scalar CHOOSE retains selected references and omitted blanks. PinnedPoi retains scalar condition/index selection. Error handlers catch designated spreadsheet errors, not operational failures. | [logical execution](../crates/poirot-eval/src/adapters/logical/execution.rs) |
| IFS / SWITCH / logical predicates | **Retained, regression coverage expanded:** selected scalar operand retains value/reference/missing identity. ExcelDocumentation IFS selects first-true results per retained array position using the same logical-test policy, singleton-axis broadcasting, and deferred member reads; incompatible non-singleton dimensions return #VALUE!, without inspecting unused argument shapes. ExcelDocumentation SWITCH selects first matching cases/default per retained position using the existing typed scalar equality policy, with the same bounded shape rule; PinnedPoi remains scalar. Materialized omission retains the existing rectangular-reader limitation. Condition/equality/parity consumers request their own conversions. AND/OR use origin-sensitive collection; skipped branches remain deferred. | [logical policy](../crates/poirot-eval/src/adapters/logical/policy.rs), [logical kernel](../crates/poirot-functions-logical/src/lib.rs) |
| Information / explicit N and T | **Exact type inspection:** ExcelDocumentation maps ISERROR/ISERR/ISNA/ISNUMBER/ISTEXT/ISBLANK/ISLOGICAL/ISNONTEXT over explicit arrays and array-demand references, preserving rectangular shape and omission metadata. Scalar projection and PinnedPoi retain the existing host selection path. No numeric or text conversion is requested. TYPE remains scalar and produces a type code. N/T intentionally convert under their own contracts, including boolean-to-number and nontext-to-empty-text. | [information](../crates/poirot-eval/src/adapters/information/execution.rs) |
| Math | **Retained:** per-function scalar numeric parameters, missing defaults, booleans and text parsing request central profiles. ROUND's decimal normalization is its numeric algorithm, not a mutation of an upstream numeric value. | [math policy](../crates/poirot-eval/src/adapters/math/policy.rs), [math execution](../crates/poirot-eval/src/adapters/math/execution.rs), [math kernel](../crates/poirot-functions-math/src/lib.rs) |
| Aggregates / statistics | **Retained:** collectors distinguish direct, reference and array origins, ignore or admit blanks/booleans/text according to the selected operation, and distinguish missing. Paired numeric collections have different policies from scalar distribution parameters. No universal aggregate conversion is introduced. | [statistics coercion](../crates/poirot-eval/src/adapters/statistics/policy/coercionv2.rs), [collections](../crates/poirot-eval/src/adapters/statistics/policy/collections.rs), [statistics kernel](../crates/poirot-functions-statistics/src/lib.rs) |
| Financial | **Retained:** scalar rates/options, optional omitted defaults, cashflow collection and numeric-text grammar have distinct consumers. Numeric kernels receive already admitted numeric inputs. | [financial coercion](../crates/poirot-eval/src/adapters/financial/policy/coercionv2.rs), [financial policy](../crates/poirot-eval/src/adapters/financial/policy.rs), [financial kernel](../crates/poirot-functions-financial/src/lib.rs) |
| Engineering | **Retained:** radix/complex text conversion differs from numeric scalar conversion; parse-only numeric grammar differs from date-aware scalar coercion. Numeric/text outputs follow the selected function. | [engineering policy](../crates/poirot-eval/src/adapters/engineering/policy.rs), [engineering kernel](../crates/poirot-functions-engineering/src/lib.rs) |
| Dates / times | **Retained:** date serials are numbers, with separate display formatting. DATEVALUE/TIMEVALUE explicitly parse text. Calendar integer truncation, serial construction, date-system adjustments and clock-context reads are algorithm/consumer operations. | [dates](../crates/poirot-eval/src/adapters/dates/execution.rs), [date text](../crates/poirot-coercionv2/src/dates/text.rs) |
| Text / VALUE / TEXT | **Retained:** text functions own their string/numeric parameter rules. VALUE intentionally parses; TEXT intentionally produces text. Under ExcelDocumentation, TEXT value admission reuses the existing locale/formatted-number/calendar parser for both text encodings; PinnedPoi retains arithmetic text parsing. Rejected numeric text stays typed text, booleans retain direct spelling, and operational failures escape. UTF-16 unit operations preserve exact text; checked scalar projection fails explicitly when unsupported. | [text policy](../crates/poirot-eval/src/adapters/text/policy.rs), [unit consumers](../crates/poirot-eval/src/adapters/text/units.rs), [text kernel](../crates/poirot-functions-text/src/lib.rs) |
| Criteria / SUMIF(S) / COUNTIF(S) / MINIFS / MAXIFS | **Explicit admission:** after scalar selection and omission handling, ExcelDocumentation requests `coercev2_criteria_excel_operand`: a supplied blank criterion becomes numeric zero, including blanks forwarded through IF/INDEX/arrays. Empty text stays text. PinnedPoi retains BlankEval's nonmatching predicate. For ordered comparisons (`<`, `<=`, `>`, `>=`), `coercev2_criteria_ordered_threshold` offers remaining text to the existing contextual number/calendar service. Rejected numeric text stays literal and operational failures escape. Equality and inequality retain literal matching; formatted equality/inequality is deliberately not broadened, with seven parenthesized-negative `<>` compatibility gaps retained. Existing boolean/error/plain-number precedence and candidate range matching remain unchanged; missing criteria retain their operational unsupported result. | [criteria admission](../crates/poirot-coercionv2/src/criteria.rs), [criteria adapter](../crates/poirot-eval/src/adapters/criteria/mod.rs), [criteria kernel](../crates/poirot-functions-criteria/src/lib.rs) |
| SUBTOTAL | **Retained:** metadata exclusion precedes reading/aggregation. Filtered cells and nested subtotal exclusion are selection rules, not value coercion. Included members use the chosen aggregate's contract. | [subtotal host](../crates/poirot-eval/src/host/subtotal.rs), [subtotal kernel](../crates/poirot-functions-criteria/src/subtotal.rs) |
| Database functions | **Retained explicit pinned policy:** criteria/header conversion and selected numeric collection belong to database operations. DGET skips blanks and requires one nonblank selected value; empty-text handling is explicit. DMIN/DMAX's empty-result zero is intrinsic aggregate output. The bridge forwards selected record types before the kernel's operation-specific handling. | [database adapter](../crates/poirot-eval/src/adapters/database/mod.rs), [selection](../crates/poirot-functions-database/src/selection.rs), [aggregation](../crates/poirot-functions-database/src/aggregation.rs) |
| Matrix functions / TRANSPOSE | **Retained explicit profile differences:** documented-profile TRANSPOSE preserves text/booleans/errors/blanks. Documented MDETERM/MINVERSE require square geometry and numeric members at every coordinate; `MatrixAdmission` in coercionv2 scans row-major, rejects invalid geometry or any nonnumber before embedded errors, then propagates the first embedded error. No rejected matrix reaches a kernel, and singleton numeric matrices are valid. Boolean members are rejected by the numeric-only contract even though Calc admits them. Pinned matrix collection compacts selected numeric members and pads zeros, including pinned TRANSPOSE; it is not a general selector rule. Determinant/inverse/multiply/trend/sumproduct each own collection rules. | [matrix adapter](../crates/poirot-eval/src/adapters/matrix/mod.rs), [matrix policy](../crates/poirot-eval/src/adapters/matrix/policy.rs), [matrix collection](../crates/poirot-functions-matrix/src/collection.rs) |
| Shared number / boolean / text helpers | **Retained:** helpers are requested conversions, not automatic producer hooks. Ordinary arithmetic, numeric scalar arguments, VALUE, and admitted direct aggregate text share the locale-parametric formatted-number grammar and lazy calendar continuation. Exact-number requests, deliberately narrow DecimalNumber requests, numeric-only collections, Java ingress, and NUMBERVALUE explicit-separator rules remain distinct. | [core projections](../crates/poirot-eval/src/core_projection.rs), [host coercion](../crates/poirot-eval/src/host/coercionv2.rs), [numeric grammar](../crates/poirot-eval/src/host/numeric_policy.rs), [date fallback](../crates/poirot-coercionv2/src/date_time.rs), [numeric library](../crates/poirot-numeric/src/lib.rs) |
| Numeric outcome / cache-setter policies | **Retained:** nonfinite validation and pinned numeric construction are explicit fidelity policies, separate from operand type conversion. Optional POI setter normalization records computed/stored identities; default publication preserves computed values. | [numeric outcome](../crates/poirot-eval/src/numeric_outcome.rs), [materialization](../crates/poirot-eval/src/materialization.rs) |
| Final worksheet and public evaluation | **Worksheet publication:** stored formula results and spill members convert remaining blanks to zero before memo admission. Direct/bound expression evaluation and public function calls preserve blanks. `coerce_worksheet_formula_result` owns this nonargument policy. | [value channels](../crates/poirot-eval/src/lib.rs), [session execution](../crates/poirot-eval/src/session_execution.rs), [public adapter call](../crates/poirot-eval/src/adapters/interface.rs) |
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
INDEX array selection. `lookup_blanks_reach_consumers_before_formula_cell_publication` covers
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

Documented FIND/SEARCH starts additionally use `coercev2_text_search_start`:
truncate first, then require a position from one through the text source's length.
The consuming adapter supplies its existing scalar-character or UTF-16 length.
Pinned builtin domains retain their end-position behavior, independently of the
text source's wildcard/literal matching and Unicode case expansion. Calc confirms
fractional last-position starts and the empty-haystack error; it disagrees with
Microsoft's FIND empty-needle rule for nonempty text. Those disagreements remain
recorded; no expectation is derived from Poirot. See `search-domains-libreoffice.json`.

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
by the default grammar. The retained pinned calendar fallback requires two day
digits in slash dates (`M/dd/yyyy`). Under `ExcelDocumentation`, the shared
numeric calendar continuation instead requests `coercev2_calendar_text_plan`:
variable-width civil dates use the explicit date order and workbook epoch, and
omitted years use the captured clock only after the fields can form a valid date.
`coercev2_calendar_serial` validates civil bounds and explicitly includes the
1900 calendar's phantom slot after February 28. This does not change DATEVALUE,
TIMEVALUE, or the explicitly pinned formatter grammar.

Modern EDATE and WEEKDAY request central date serial admission; modern WEEKDAY
options preserve conversion errors before numeric mode validation. Missing and
blank options still convert to zero, which is an invalid WEEKDAY mode; an absent
option defaults to one. The modern civil-date continuation validates an optional
clock completely, retains admitted millisecond fractions, and advances 24:00 to
the next day. Its clock suffix retains the source's three-digit fractional field;
it does not claim all Calc time spellings. Named dates and standalone clocks
outside the civil-date plan retain the pinned formatter continuation. Calendar
reads include date order only for ambiguous slash dates, year/offset only for
omitted years, and workbook date system for civil serial conversion. Raw snapshot
version 14 and compatibility revision 15 invalidate retained results from earlier
semantics; durable workbook source remains recoverable.


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


Statistical sample requirements run after origin-sensitive admission and ordered
error acquisition. `aggregate_with_policy` and `paired_with_policy` select explicit
`SamplePolicy` contracts: documented population covariance/variance/deviation
admit one observation, while sample variance/deviation/covariance require two and
return `#DIV/0!` when insufficient. Original pair shape checks remain separate.
The existing kernel entry points and global evaluator default retain pinned
POI/StatsLib minimum counts and errors. External documented suites select
`ExcelDocumentation` for the entire suite; value conversion remains in coercionv2.

Documented TREND selects `TrendFitPolicy::Determined`: prediction can solve a
full-rank system when the observation count equals its coefficient count, without
requiring a spare observation for residual statistics. The existing Householder
QR calculation is unchanged. Origin-only fits solve their coefficient even for
constant predictors. The existing constant-predictor/intercept mean shortcut is
retained as a Poirot compatibility behavior; Calc rejects some such inputs.
General rank-revealing regression and redundant-predictor removal remain outside
this policy, so this does not establish complete Excel collinearity compatibility.
Pinned TREND retains its original minimum counts, source exceptions and shortcut.

### Kth rank admission

LARGE and SMALL request `coercev2_statistics_kth_rank` after scalar numeric
coercion and before collection reads. `DocumentedLarge` admits positive subunit
ranks and rounds upward; `PinnedPoi` rejects ranks below one, as Apache POI's
AggregateFunction does. SMALL retains this existing source rule in both profiles:
Calc floors SMALL ranks, so its behavior does not justify extending LARGE's
correction. Percentile interpolation, quartile truncation, scalar type/origin
handling, omission failures and rank-first error/budget order remain unchanged.
Raw snapshot version 12 and envelope compatibility revision 13 invalidate older
computed state while preserving source recovery.

Business-day holiday arguments retain container/selection origin through traversal.
`coercev2_holiday_member` selects explicit `PinnedPoi`, `NetworkDays`, or `WorkDay`
member admission before requesting date conversion. Modern NETWORKDAYS ignores
referenced text and logical members; WORKDAY requests date conversion for text and
logicals. Blank worksheet members and explicit omission remain distinct. The host
owns bounded date admission, dependency reads, allocation and materialization
charges; the calendar consumer truncates and deduplicates admitted serial days.


Documented IPMT/PPMT admission uses `coercev2_financial_payment_number` to admit
explicit positional Missing operands as zero, preserving the existing consumer
contract. Arity admission separately rejects absent required positions. Omitted
future-value and timing slots default to zero. Blank references remain distinct
until numeric admission. `coercev2_financial_payment_timing` admits zero or one
after numeric conversion. Resolution still precedes conversion for all used
operands, including their errors and resource charges. The pinned entry point
and global default remain unchanged.


### Complex coefficients and result spelling

`poirot-coercionv2::complex` owns complex argument conversion and computed
result spelling. Its `ComplexNumber` retains real and imaginary coefficients and
an explicit lowercase i/j suffix. Under `ExcelDocumentation`, component calls
return numeric coefficients directly; numeric arguments keep their full f64
value without a text round trip. Direct, reference and selected-array origins
share this policy. Booleans are rejected, actual blanks retain the established
zero component admission, and missing slots retain host capability failures.

The documented text grammar uses nom's complete decimal/scientific parser with
full consumption and finite results. It recognizes omitted unit coefficients
and distinguishes pair separators from exponent signs. Whitespace, currency,
locale separators, date syntax and nonfinite values are rejected. Existing
`#NUM!` invalid-complex failures are retained. Computed results use the existing
number formatter with explicit negative unit signs and canonical text zero.
The established empty suffix default remains i.

`PinnedPoi` preserves the source-pinned permissive regex, textual component
results and historical renderer behavior; public defaults stay pinned. Those
legacy helper APIs delegate to central conversion ownership. The engineering
kernel's typed component accessors only consume the representation.

Microsoft's [COMPLEX](https://support.microsoft.com/en-us/excel/functions/complex-function),
[IMREAL](https://support.microsoft.com/en-us/excel/functions/imreal-function) and
[IMAGINARY](https://support.microsoft.com/en-us/excel/functions/imaginary-function)
contracts motivate the documented policy. The hash-bound
`json-gaps-complex-libreoffice.json` corroborates all original authored checks.
Exploratory Calc probes additionally corroborate exponent signs and fractional
coefficients, but differ on invalid-spelling error codes, boolean admission,
decimal commas and negative-zero result spelling. These disagreements do not
justify inheriting locale grammar or changing established Poirot errors.


Approximate classic MATCH under `ExcelDocumentation` preserves candidate scalar
types and original positions. The lookup family owns an inclusive-bound ascending
bisection and a descending scan that remembers the last comparable candidate,
stops at the first lower value, and returns the first exact match. Ascending exact
duplicate runs return their last contiguous position. Blank and spreadsheet-error
candidates are noncomparable; operational read failures remain fatal. This follows
the typed ClosedXML lookup test contract, including its explicitly authored
unordered trajectories. It does not promise a nearest-value result on unordered
vectors. `PinnedPoi` retains its existing traversal, and HLOOKUP, VLOOKUP, LOOKUP,
XMATCH and exact MATCH retain their existing contracts. No new argument conversion
policy is introduced. The host records builtin-policy dependencies in both
approximate MATCH profiles, so retained results cannot cross profiles.

The independent evidence in
`crates/poirot-test-runner/evidence/classic-match-probes-libreoffice.json` records
actual Calc disagreements on duplicates, type coercion, and error entries. The
source-authored expectations remain unchanged; native Excel was unavailable.

### General radix consumers

BASE and DECIMAL request the central `poirot-coercionv2::radix` policies after
lazy scalar selection. BASE's numeric arguments defer text to the existing
context-aware numeric parser; DECIMAL's positional text grammar never uses that
parser. Blank/missing/logical admission remains explicit and origin-independent
at this consumer. BASE number/padding and DECIMAL radix/length/finite-overflow
checks retain distinct domains. `poirot-numeric::radix::Radix` shares checked
ASCII digit admission and uppercase spelling without importing engineering
signed-width rules. BASE result rendering is an audited non-argument conversion.
See the [independent contract and limitations](../crates/poirot-test-runner/evidence/radix-domains-contract.md).

### Roman numeral consumers

`poirot-coercionv2::roman` owns ROMAN arity and optional/boolean form defaults,
retaining the pinned two-argument policy. Numeric text uses existing contextual
host conversion, including locale dependency reads. ARABIC accepts exact text
or blank at scalar selection; numbers, booleans and missing syntax are not
stringified. Host text/work admission precedes projection and bounded decoding.
The 255-unit signed ASCII grammar is independent of the existing text-family
formatter. See the [independent contract and limits](../crates/poirot-test-runner/evidence/roman-numerals-contract.md).

SUMPRODUCT in the documented profile requests uniform text/blank-zero admission
for every conformable argument, including direct scalars and 1x1 references/arrays.
It reuses centralized `coercev2_sumproduct_member` with zero-admitting admission;
geometry does not select member conversion. Pinned POI dispatch remains distinct.
Boolean-to-number and explicit Missing rejection remain existing policies pending
stronger target evidence; see [the evidence and limits](../crates/poirot-test-runner/evidence/sumproduct-admission-contract.md).

The [integrated compatibility reconciliation](../crates/poirot-test-runner/evidence/master-compat-reconciliation.md) records the exact original gap identities, current source-bound evidence, preserved expectations and deliberately retained limitations.
