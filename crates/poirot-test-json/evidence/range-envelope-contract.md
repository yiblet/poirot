# Colon range envelopes and lexical admission

The three original scenario names, formulas and expected values remain in
`closedxml-extension-reference-ranges.json`. Only their resolved gap annotations
were removed. No suites moved and no builtin policy changed.

Independent evidence:

- [Pinned ClosedXML assertions](https://github.com/ClosedXML/ClosedXML/blob/09f2f21a60e821ce7fb18ca3576b272e5dd6d921/ClosedXML.Tests/Excel/CalcEngine/ReferenceOperatorsTests.cs#L98)
  explicitly assert chained and union-operand bounding rectangles. The upstream
  chain `Sheet1!B2:C5:Sheet1!D3` expects twelve cells; both union orders expect ten.
- `range-envelope-libreoffice.json` binds the edited suite by SHA256 and independently
  corroborates all eleven expected values through fresh Calc workbook snapshots.
- Fourteen independent Calc disambiguation observations, with exact input fixtures
  and typed results, are retained in StateMCP namespace `board` at
  `/boards/poirot-json-gaps/evidence/017-range-ambiguity-probe.json`. Dummy expected
  zeros in those research inputs were observation sentinels, not proposed results.

The shared colon operator previously required one area on each side, rejecting
unions before any rectangle could be built. Both unsigned and signed reference
kernels now combine all admitted same-sheet areas into one bounding rectangle.
They preserve signed bounds without clipping, produce area provenance, and read
no cells. Additional union members consume reference-pair work; the existing
single-area operation retains its budget. Empty operands and mixed sheets retain
REF. Union and intersection themselves keep their existing semantics.

The chain had a separate lexical cause. The parser consumed `C5:S!D3` as a sheet
span inside `S!B2:C5:S!D3`, losing C5 as a cell endpoint before the operator ran.
Calc confirms the complete chain is twelve cells with or without a sheet named
C5. Bare `C5:S!D3` is nevertheless a valid 3D reference when that sheet exists.
An explicit parser context therefore gives an unquoted cell token priority on a
direct colon RHS. Parentheses reset that context; quoted span atoms remain spans.
The parser constructs its ordinary operators without any AST rewriting or
workbook-dependent name resolution.

This evidence supports shared envelope geometry plus narrow lexical admission.
A parser rewrite would not repair union geometry; a new reference-algebra
representation would add no required information. Existing area channels suffice.

Deliberate retained contracts and limits:

- Cross-sheet colon operands retain the project's REF guard, even where Calc
  returns a combined result (the explicit parenthesized C5:S probe yields 106).
- Pinned missing-first-sheet fallback remains for actual 3D atoms. Calc reports
  NAME for absent C5; the pinned project contract selects the first sheet.
- Existing signed OFFSET dimensions and blank-preserving INDEX publication remain
  intact. No scalar or argument conversion was introduced.
- Raw evaluator snapshot VERSION advances from 18 to 19 and retained compatibility
  revision from 19 to 20. Old prepared results are rejected; durable workbook source
  recovery is unchanged.

Regression tests cover association, swaps, overlaps and disjoint union members,
unions on both sides, sheets, error priority, signed bounds and anchors, empty
operands, blank identity, row-major rectangle dependencies, edits and cache
invalidation, pure shape consumers, large lazy references and resource failures.
