# Roman numeral admission and decoding

ROMAN(999) failed at three two-argument admission checks, while ARABIC("mcmxii") had no handler. These are missing documented argument admission and function support, not scalar identity loss. The existing text-family formatter remains unchanged. No math-family relocation or new dependency is needed.

## Independent expectations and options

Microsoft's [ROMAN](https://support.microsoft.com/en-us/excel/functions/roman-function) and [ARABIC](https://support.microsoft.com/en-us/excel/functions/arabic-function) pages were reviewed on 2026-10-17. ROMAN documents omitted form zero, forms 0..4, TRUE classic and FALSE simplified, and the nonnegative 0..3999 interval. ARABIC documents case-insensitive Roman text, empty text zero, leading minus, outer spaces and a maximum argument length of 255 characters / largest positive result 255000. Its MXMVII = 1997 example demonstrates that formatter canonicality is not a prerequisite.

Three options were compared before implementation:

1. Selected: retain the existing text formatter and add bounded central admission/decoding. A small symbol scan handles documented signs, case, spaces and simplified Roman spelling without canonical syntax validation. Mathematical symbol values are so small that sharing the formatter's mixed single/subtractive token table would couple different grammars without useful reuse.
2. A numeral syntax tree would add structure not needed by the observed domain and risk imposing canonical formatter constraints on accepted text.
3. Porting a separate upstream parser/formatter would duplicate working rendering and add provenance and behavioral coupling without independent native Excel evidence. No source was ported.

Before implementation, 61 Calc probes covered optional/missing forms, all numeric forms and booleans, negative/fractional bounds, case, empty text, signed/repeated/subtractive spellings, invalid symbols/whitespace/lengths and scalar errors. A further XIL probe distinguishes adjacent-pair arithmetic from running-maximum subtraction. Raw source probes and typed observations are retained in StateMCP board issue 023 evidence paths. Their placeholder expected zero values are exploration devices, not authored compatibility claims. No native Excel installation is available.

The decoder consumes a smaller symbol and its immediately following larger symbol as one difference; otherwise it adds that symbol. This covers documented MXMVII and every existing formatter form. Native behavior for undocumented malformed/noncanonical sequences such as IIV, XIL, VV, IXC and XIXI is unverified. Tests for those sequences assert only bounded totality, and the exhaustive formatter/decoder inverse check is explicitly an implementation invariant, not an independent compatibility oracle.

## Policies and preserved contracts

Pinned ROMAN still requires exactly two arguments, floors numeric operands, applies numeric boolean form styles, masks first spreadsheet coercion/selection errors to VALUE and second errors to NUM, and skips form evaluation when number is zero. `ExcelDocumentation` changes optional form admission and preserves its previously explicit logical styles. Absent and explicitly missing form use the same central policy. Number/form numeric text keeps the existing locale-aware contextual conversion and dependency reads; no separate Rust float parser is introduced. Existing floor behavior gives the independently observed 3999.9 and 4.9 results and rejects negative fractions.

ARABIC is a new operation under both builtin policies. It accepts text or blank, preserves original scalar errors and rejects number/logical/missing inputs with VALUE. ASCII outer spaces are trimmed; tabs, newlines, nonbreaking spaces, Unicode numeral glyphs, internal spaces, plus and misplaced signs are rejected. The 255 UTF-16-unit limit is checked before trimming. Microsoft does not specify every whitespace and malformed-sequence detail; those exact grammar decisions are bounded implementation scope, not native parity claims.

The complete 14-scenario closedxml-extension-numeric-domains suite now selects ExcelDocumentation: its retained domains and expectations form a coherent documented contract, and all pass without expected-value edits. This policy selection resolves the ROMAN original through new admission; it is not credited as a formatter correction. closedxml-math already selected ExcelDocumentation; registering ARABIC resolves its original missing function. The two exact original scenario names remain in those corpus files. Only their resolved known-gap annotations were removed; tolerances are unchanged.

## Calc disagreements retained

The source-hash-bound roman-numerals-libreoffice.json recalculates 14 numeric-domain extension scenarios, 57 math scenarios and 36 new Roman-domain neighbors: 99 of 107 checks agree. All original source checks agree. The eight disagreements remain visible with unchanged authored expectations:

- ROMAN(499,TRUE) and ROMAN(499,FALSE): Calc uses numeric styles 1/0, while Microsoft documents classic/simplified.
- ROMAN(0,1/0), ROMAN(1,1/0), ROMAN(1/0,0): Calc eagerly returns DIV0; this project deliberately preserves its existing lazy zero and VALUE/NUM masking contract under both policies.
- ARABIC("-MMXI"), ARABIC(" X "), and ARABIC with 255 M symbols: Calc returns VALUE, contrary to Microsoft's explicit signs, outer spaces and 255000 domain.

Exploration also found Calc rejects many noncanonical sequences and more than three M symbols. These observations do not justify inventing canonicality exceptions in a decoder intended to cover Microsoft's broader domain. Calc agreement never serves as a claim of native Excel parity.

## Execution and validation

The adapter admits text storage and work before projection or parsing. No unbounded collection or auxiliary syntax tree is allocated. Checked arithmetic preserves failure channels; actual text/work/collection refusals remain operational and escape IFERROR. References use existing lazy scalar selection and dependency tracking. Snapshot restoration, reference edits, locale changes and builtin-policy changes are exercised together. Raw snapshot VERSION advances 24 to 25 and COMPATIBILITY_REVISION 25 to 26; old raw snapshots are rejected and durable source recovery is retained. CommonFunction ARABIC is appended as ID 262, leaving existing IDs stable. Registry/spec metadata declares bounded support and leaves exhaustive native obligations open.

Focused tests cover independent documented/Calc examples, all 20000 formatter inverse combinations, numeric locale/currency inputs, scalar origins, blanks, errors, lazy skipping and operational budgets. Full JSON passes 1439 of 1444 scenarios with five remaining known gaps, zero failures, unexpected passes or harness errors, and equal normal/overlay outputs and change lists.

The text FODS slice contains 1287 exact parsed records: 371 pass, 894 known gaps, 22 improvements and zero regressions/harness errors. Against the saved absence baseline, the only 17 changes are text__fods__decimal__00000 through text__fods__decimal__00016, all gap-to-improvement changes attributable to the preceding radix ticket. No record changed for this ticket's Roman behavior; affected ARABIC/ROMAN FODS fixtures remain limited by imported workbook context. This does not claim new Roman compatibility coverage from those blocked fixtures.
