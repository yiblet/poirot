# Typed value conversions v2

`poirot-coercionv2` is the new, workbook-independent home for requested value
conversions. It owns operator and function argument conversion policies. The migration
initially preserved existing behavior; explicit policy corrections now resolve
verified compatibility gaps without converting stored or published blanks.

All new conversion names use `coercev2`, `coercev2_…`, `CoerceV2`, or
`CoercionV2…`. Do not rename old implementations to make the migration appear
complete. To find outstanding old names across Rust source:

```sh
rg --pcre2 -ni '\bcoerc(?:ion|e)(?!v2)' crates --glob '*.rs'
```

The architecture check enforces the v2 spelling inside this crate. Argument adapters use the central policies. Unsuffixed result/publication
conversions retain adjacent, risk-rated audit comments outside this crate.

## Simple typed call sites

```rust
use poirot_coercionv2::{coercev2_args, DecimalNumber};

type DecimalPair = (DecimalNumber, DecimalNumber);
// inputs retain scalar values, omission, and Direct/Reference/Array provenance.
let (left, right) = coercev2_args::<DecimalPair>(inputs, context)?;
// The algorithm receives two f64 values, without type-tag dispatch.
```

Rules implement `CoerceV2<'a>` with an associated `Output`. Tuples implement
`Signature<'a>` for arities zero through eight. Both entry points return
statically typed results. Text rules can borrow input storage without allocating.
An ordinary consumer selects a contract and writes its algorithm.

| Rule | Accepted inputs and result |
| --- | --- |
| `Identity` | Original borrowed `Input`, including origin, omission, errors and exact text |
| `ExactNumber` | Existing numeric payload, preserving IEEE bits; finiteness is not a type test |
| `ExactBoolean` | Existing boolean |
| `ExactText` | Existing borrowed `TextRef`, including isolated UTF-16 surrogates |
| `ExactError` | Worksheet error as data rather than a propagated failure |
| `DecimalNumber` | Finite numbers, blank as zero, booleans as zero/one, finite decimal text |
| `LogicalBoolean` | Boolean, finite numeric truthiness, blank as false, ASCII TRUE/FALSE text |
| `FormulaText` | Unicode scalar text, blank as empty text, uppercase boolean text, explicit numeric renderer |
| `TextCount` | Nonnegative `DecimalNumber` whose truncated integer fits i32 |
| `Optional<R>` | Only omitted syntax becomes `None`; present values still request R |
| `Direct<R>`, `Referenced<R>`, `ArrayMember<R>` | Admit one origin before requesting R |

`DecimalNumber` deliberately accepts only decimal/exponent text with outer ASCII
whitespace. It rejects percentages, fractions, grouping, accounting parentheses,
dates, times, NaN, and infinity spellings. It is **not** advertised as full Excel
arithmetic or VALUE behavior. Existing date-aware and other consumer grammars live in the explicit
`compatibility`, `date_time`, and `dates` modules. Decimal
text follows binary64 parsing, including underflow to zero; overflow is a mismatch.
An existing nonfinite number instead produces a terminal numeric worksheet error
when a finite rule consumes it. No incomplete policy is named `ArithmeticNumber`.

`LogicalBoolean` does not trim text. `TextCount` is one explicit argument policy,
not the universal index rule for all text functions. `FormulaText` uses the
context's selected `NumberTextSource`; it never reads a cell display format.

## Ordered whole signatures

```rust
use poirot_coercionv2::{Choice, DecimalNumber, ExactText, FirstOf, coercev2_args};

type Accepted = FirstOf<((ExactText, ExactText), (DecimalNumber, DecimalNumber))>;
match coercev2_args::<Accepted>(inputs, context)? {
    Choice::First((left, right)) => { /* two exact texts */ }
    Choice::Second((left, right)) => { /* two numbers */ }
}
```

The first complete signature wins. If one argument rejects a candidate, the next
candidate starts from the original immutable inputs. Each candidate processes
arguments left to right and stops at its first failure. This is an explicit
precedence contract; functions that inspect every error before interpreting text
will need a separately named contract. Rejected alternatives retain work already
performed and text reservations; resource usage is never rolled back.

Only `CoercionV2Failure::Mismatch` permits fallback, including an arity mismatch.
Worksheet errors, resource exhaustion, and unsupported Unicode scalar projection
are terminal. Use `Identity` or `ExactError` explicitly when errors are data.
The caller decides how an exhausted mismatch becomes a function-specific error;
the library does not guess or silently treat an invalid value as zero.

`FirstOf` is for actual overloads, not a universal function wrapper. Named aliases
hide its machinery. Larger choices can nest it behind an alias; complex consumers
can implement a named `Signature` returning a meaningful enum.

## Boundaries and resources

The host selects a scalar from a reference/array before constructing `Input`.
This crate does not implicitly intersect, broadcast, publish formula results,
evaluate deferred branches, or turn referenced values into direct literals.
Hosts retain array/reference shapes and traversal. Origin-sensitive aggregate
admission and consumer grammars live here; final-result publication remains
separate and is marked by the repository audit comments.

`CoercionV2Context` supplies cumulative work/text limits and numeric rendering
policy. Its default is unbounded. Hosts must provide limits reflecting their
remaining resources and account for `usage()` on success **and** failure.
Numeric rendering reserves its conservative bound before allocating. Exact text
borrows either representation; Unicode scalar projection admits UTF-8 output
before allocating and never substitutes replacement characters.

The implementation depends only on shared scalar types and numeric primitives.
Pure numeric rendering is reused from `poirot-numeric`. All migrated rules and
consumer contracts are defined here rather than recreated in an adapter.

## Required locale-aware parsing extension

The central crate must also support locale-aware numeric input, including plain
numbers such as `"1.22"` and currency forms such as `"$1.44"`. Locale is an
explicit input, never an ambient process setting. A future request must declare
its decimal/grouping separators, recognized currency symbols and placement,
sign/parentheses rules, accepted whitespace, and localized exponent/percent forms.
Consumers select which forms they accept; currency support must not silently
broaden every arithmetic or collection grammar. Ambiguous strings must follow
the selected locale rather than guessing from their punctuation.

This migration retains the existing grammars and does not yet implement this
broader locale-aware input rule. Existing NUMBERVALUE accepts explicit separators,
but that alone is not a complete locale/currency parser.

## Existing consumers and host effects

Typed `CoerceV2` rules are the small, self-contained request interface. Existing
adapters also use explicit central compatibility profiles to retain their source
behavior. `NumberPolicy::Finite` rejects existing nonfinite numeric values with
NUM; `Preserve` carries the original payload. Arithmetic operands use their own
entry point, preserving existing numeric bits before result-outcome handling.
These profiles must not silently become the narrower `DecimalNumber` grammar.

Host continuations process only requests requiring host effects: text admission,
context-aware numeric parsing, numeric rendering, and omitted-argument capability
failures. The central rule performs scalar dispatch; hosts preserve budget checks,
work charging, dependency reads, and their operational error types. Continuations
are never called for an existing number, boolean, blank, or worksheet error in a
numeric request. An operational refusal is returned unchanged.

Owned text requests transfer an existing String buffer; borrowed requests retain
exact text representations until projection is requested. Statistical collections,
lookup scalars, criteria/SUBTOTAL members, matrix members, NUMBERVALUE, and date/time
plans share the central implementation. Function crates retain their algorithms,
shape validation, and traversal. The old date fallback crate is a compatibility
facade, not a second parser implementation.

```sh
cargo test -p poirot-coercionv2
cargo clippy -p poirot-coercionv2 --all-targets -- -D warnings
python3 tools/test_coercion_architecture.py
python3 tools/test_function_architecture.py
```

## Corrected shared argument rules

Paired statistical datasets use `PairCollection::NumericOnly`: retain an aligned
observation only when both coordinates are existing numbers. Numeric-looking
text, logical values and blank cells do not become observations; zero does.
`NumericPairs::sample_len()` validates alignment and reports the admitted count,
while `original_len` preserves the original input shape. Regression means use the
admitted count. Error acquisition order and host traversal stay explicit.

Text integer arguments use `coercev2_text_integer_number` before any clamping or
empty-text shortcut. The checked policy validates the truncated signed-32 target
and reports VALUE on overflow; it preserves the original fraction so consuming
functions retain their own sign checks and error order. The pinned policy keeps
its floor-before-range rule and NUM overflow result. `TextCount` shares checked
range admission and additionally rejects negative inputs.

The corrected paired and text-boundary corpus suites were freshly recalculated
in isolated LibreOffice instances; their hash-bound reports are in
`poirot-test-runner/evidence/coercionv2-*-libreoffice.json`. These observations do not
claim native Excel execution.

## Blank admission and scalar channels

Reads, selection and publication retain blank values. Numeric argument policies
can admit a blank as zero when requested; a selector does not request that policy
merely by forwarding a value. Numeric aggregates distinguish a directly computed
blank from an ignorable blank selected from a cell or array.

Materialized lookup results retain source origin separately from the scalar
argument channel. Statistical `Origin::SelectedScalar` expresses that distinction:
its blank is ignorable, while its logical/text values retain scalar admission.
Actual reference and array operands retain their own collection filtering rules.
`ExtremaA` ignores referenced blanks and counts referenced text as zero.

## General positional radix arguments

`radix` owns BASE numeric and padding domains, context-deferred numeric text,
and DECIMAL scalar spelling and unsigned ASCII digit admission. All scalar
origins share these explicit consumer policies; blank and omitted syntax are
separately admitted as zero/empty. BASE number is below 2^53 and padding at most
255. DECIMAL text is at most 255 UTF-16 units and finite binary64 accumulation
permits precision loss above 2^53. Numeric spelling reserves the shared numeric
formatter's `ALLOCATION_BOUND`; hosts admit that storage before conversion.
The checked mathematical alphabet is in `poirot-numeric::radix`; BASE rendering
is in `poirot-functions-math::radix`. Engineering signed-width grammars remain
separate. Independent observations and documented Calc differences are in
`poirot-test-runner/evidence/radix-domains-contract.md`.

## Roman numeral arguments

`roman` owns ROMAN optional-form and boolean admission plus ARABIC scalar and
text decoding. ROMAN numeric inputs keep the contextual host conversion. ARABIC
preserves errors, treats blank as empty text, rejects numeric/logical/missing
inputs, and counts at most 255 UTF-16 units before trimming ASCII outer spaces.
Its bounded symbol scan admits case-insensitive symbols and one leading minus;
it does not require formatter canonicality. Undocumented noncanonical spellings
remain unverified against native Excel. See the independent contract in
`poirot-test-runner/evidence/roman-numerals-contract.md`.
