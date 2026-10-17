# General radix conversion contract

The two original closedxml-math scenarios failed because BASE and DECIMAL had no registry handlers. This is missing function support, not a loss of scalar identity. New functions use one documented contract under both builtin policies; existing public policy defaults are unchanged.

## Independent evidence and option decision

Microsoft's [BASE](https://support.microsoft.com/en-us/excel/functions/base-function) and [DECIMAL](https://support.microsoft.com/en-us/excel/functions/decimal-function) pages were reviewed on 2026-10-17. BASE accepts nonnegative numbers below 2^53, radices 2..36 and padding 0..255; fractional arguments truncate. Out-of-domain values produce NUM. DECIMAL accepts case-insensitive alphanumeric digits, text at most 255 characters, and finite values above 2^53 with possible precision loss. The documented FF/16, 111/2 and zap/36 examples provide independent positional results.

Three options were compared before implementation:

1. Selected: a checked positional digit alphabet in poirot-numeric shared by BASE rendering and DECIMAL admission, with distinct central argument policies in poirot-coercionv2. This expresses the inverse relationship without conflating numeric bounds, padding and text grammar.
2. Separate algorithms would duplicate alphabet and radix validation and provide no useful contract separation beyond the distinct admission functions already selected.
3. Workbook number-format parsing/rendering (or engineering's private signed ten-digit codecs) has different grammars and result guarantees. Neither contract is imported into the new positional digit parser. Numeric BASE/radix/padding text uses the existing calculation-context numeric parser, including its locale admission; it does not use DECIMAL's digit grammar.

Before product implementation, 74 discriminating Calc probes were independently recalculated with LibreOffice 26.2.6.3. Their raw typed observations are retained in radix-investigation-calc.json. They corroborate both originals, uppercase/lowercase digits, fractional numeric arguments, numeric text, logical, blank and omitted scalars, leading ASCII spaces and empty DECIMAL text. Signs, trailing spaces and invalid digits are rejected. Error operands precede later domain validation: BASE(-1,1/0), BASE(1,1,1/0) and DECIMAL("?",1/0) yield DIV0. The selected codec was tested against documented positional examples, independently calculated ZAP = 35*36^2+10*36+25 = 45745, and valid inverse domains before integrated corpus checks.

## Deliberate differences and limits

Calc is an independent implementation, not native Excel. Native Excel is unavailable. Calc often returns VALUE where Microsoft's explicit BASE domain rule requires NUM. Calc admits BASE(2^53,16), BASE padding 256 and DECIMAL text longer than 255; this implementation retains Microsoft's explicit upper bounds. Microsoft does not fully specify sign/whitespace/empty/logical/omission details; the unsigned grammar and those scalar details use the above Calc observations. Only leading ASCII spaces are admitted; signs, trailing whitespace and non-ASCII digits are rejected. Invalid DECIMAL digits/domain/length/overflow use NUM, while numeric argument text failures use the existing contextual parser's error.

The fresh source-hash-bound radix-domains-libreoffice.json covers all 57 closedxml-math checks and 32 new neighbor checks: 88/89 agree. The sole discrepancy is DECIMAL("9007199254740992",10): Calc's exported cached number is 9007199254740990, while positional arithmetic gives exactly representable binary64 2^53 = 9007199254740992. Its expectation remains independently derived, without tolerance changes. This tests the documented allowance for large DECIMAL results with potential precision loss; it is not a native Excel exact-bit claim.

DECIMAL accumulates in finite binary64 with overflow checks; results beyond the exact-integer range may lose precision. A checked ASCII alphabet is shared with BASE, while engineering's signed-width and Java character rules remain separate. No custom package or workbook dependency was added.

The host admits input text work and output collection/text storage before copies and rendering. BASE validates huge padding before allocation and returns documented NUM; IFERROR may catch that spreadsheet error. Actual work/text/collection budget failures remain operational and escape IFERROR. DECIMAL length is counted in UTF-16 units and bounded at 255; arbitrarily large input is still admitted and charged by the host before grammar work. Arity rejects before reads. Reference selection uses the existing lazy scalar host, preserving dependencies, origin, blank and omission until consumer admission.

New registry execution semantics invalidate retained results: raw snapshot VERSION advances 23 to 24 and COMPATIBILITY_REVISION 24 to 25, preserving durable source recovery. Formula name IDs are appended; existing IDs remain stable. Spec inventory registers the new functions and explicitly leaves exhaustive native contract obligations open.
