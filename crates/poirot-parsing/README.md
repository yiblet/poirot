# poirot-parsing

Workbook-independent formula scanning, formula syntax, and spreadsheet text
grammars. Consumers import these modules directly; there is no `poirot-formula`
compatibility crate. The public root library exposes them as `poirot::parsing`.

- `formula::classified` streams borrowed source tokens for template hashing,
  exact normalized comparison, and reference rebinding. It does not construct an
  AST or validate a formula. Unsupported spellings still require full parsing.
- `formula` owns the full parser, AST, parse limits, spans, printing, and reference
  translation. The scanner and parser share lexical helpers. Tokens are transient;
  successful compiled exemplars remain the authority for template reuse.
- `number` provides locale-parametric formatted numbers and separate narrow,
  financial, statistical, classic criteria, and source numeric grammars.
  `decimal` and `source_float` are nom prefix parsers returning a remainder;
  complete entry points reject trailing input. Grammar profiles remain distinct.
- `java_number` retains Java decimal/type-suffix/hexadecimal spellings and exact
  binary64 rounding. Its nonfinite results are syntax results; consumers decide
  whether those results are admissible.
- `criteria` splits the comparison prefix and interprets classic operands using
  the numeric grammar. Text patterns stay borrowed. Criteria are not formulas.
- `date`, `datetime`, and `formatter` inspect date/time fields with their existing
  distinct source grammars. These modules do not read a clock or workbook.

`poirot-coercionv2` chooses an argument's grammar, admits storage and scanner work,
handles origin and omission, maps parse failures to spreadsheet errors, resolves
calendar/date-order/default-year policy, and records context effects. Matching and
function algorithms remain with their consumers. A grammar move does not expand
any argument's accepted spellings.

```rust
use poirot_parsing::{criteria, number};
assert_eq!(number::decimal("1.25;next"), Ok((";next", "1.25")));
assert_eq!(number::parse_finite_decimal("1.25;next"), None);
let syntax = criteria::parse_criterion(">=1.25");
assert_eq!(syntax.classic_operand(), criteria::Operand::Number(1.25));
```

See [formula contracts](FORMULA.md) for AST and translation details. Specialized
argument kernels (NUMBERVALUE separator admission, complex input policies, radix
bounds, and Roman numeral admission) remain in coercion; they use the shared
numeric primitives where their grammars agree.
