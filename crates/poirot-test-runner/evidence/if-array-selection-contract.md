# Positional IF array selection

## Mechanism and chosen scope

IF previously projected the condition to one scalar and forwarded one complete branch. That lost the correspondence between condition members and branch members, affecting eight aggregate consumers. The original independently authored 21 gap scenarios / 34 checks now pass, without changing their expected values or tolerances. The entire conditional-array consumer suite is independently recalculated by Calc: 24 scenarios / 40 checks agree; its refreshed source hash remains in `excel-compat-authored-boundaries-libreoffice.json`.

Three options were considered: reuse the existing rectangular traversal and deferred branch evaluation (chosen); automatically vectorize all functions (disproportionate); or patch the aggregate consumers (wrong layer). The implementation adds an IF-local selection loop and exposes existing rectangular helpers. It adds no persistent flags, provenance model, or generic selection framework.

For ExcelDocumentation, an array condition selects each corresponding branch member. Non-singleton condition axes determine output extent; singleton condition axes can expand to the extents of needed branches. Scalar/singleton branch axes broadcast. Entirely unused branches are not evaluated and do not determine output shape. Needed branches are evaluated once, including volatile expressions; reference member reads remain deferred. An exhausted non-singleton selected branch axis gives #N/A. The existing arithmetic operator mismatch remains #VALUE!.

A scalar condition still forwards the selected branch unchanged. An explicit array condition retains array identity even when its shape is 1x1. The independent falsifying pair `ISREF(IF(TRUE,A1:A3,0)) = TRUE` versus `ISREF(IF({TRUE},A1:A3,0)) = FALSE` ruled out treating singleton array conditions as scalar conditions. The shared builder therefore offers an array-preserving result; existing arithmetic operators retain their established singleton-collapse wrapper.

IF records its builtin-policy dependency for both profiles, so a retained pinned result cannot survive a switch to documented semantics or vice versa. Static spill bounds conservatively include IF's condition dimensions, enabling condition-derived and orthogonal spills, including child-before-anchor reads. Snapshot wire version and compatibility revision are bumped because retained results may change.

## Independent probes and falsifications

`if-array-selection-probes.json` retains all diagnostic source suites, exact source hashes, and fresh Calc typed results. Its first two runs deliberately used placeholder expected zeros; their `agrees: false` entries are observations, not claimed oracle agreement. These runs were used to discriminate shape and identity hypotheses, not to populate expectations from Poirot. The third run has independently authored selected-member sums: two direct-reference error-masking checks agree; the computed-reference branch check disagrees and is retained.

Examples independently observed in Calc:

- `SUM(IF({TRUE,FALSE},{1,2,3},9)) = 10`: a non-singleton condition axis is not extended by the longer branch.
- `SUM(IF({TRUE;FALSE},{1,2,3},{10,20,30})) = 66`: orthogonal singleton axes broadcast.
- `SUM(IF({TRUE,TRUE},2,{10,20,30})) = 4`: an unused branch does not extend the result.
- `SUM(IF({TRUE,FALSE,TRUE},{1,2},9)) = #N/A`: the selected branch is exhausted at the third coordinate.
- Error-valued reference members at unselected positions are masked. Valid direct-reference probes give 3 and 10.

The diagnostic array-expression literal `{1,1/0}` is invalid Excel constant-array syntax. Calc's imported #N/A for that probe does not establish error-member semantics. Error-valued array literals also produced an imported #N/A in this probe path; valid referenced-error probes were used instead. These observations are preserved rather than silently discarded.

## Limits deliberately retained

This change does not introduce generic reference-array demand. SUM retains the established scalar demand for nested reference operators; SUMPRODUCT requests array demand, as does dynamic execution. Tests use that distinction explicitly. In particular, Calc's scalar SUM with a computed reference branch can return #VALUE!; this is not used to justify broadening all aggregate argument demands.

Existing omission policies remain: an explicitly omitted selected IF slot is blank inside expressions; an absent false branch is FALSE. Calc diagnostic results show differences for omitted true slots and nested IFS omissions (for example, Calc counts the omitted true slot as zero). Fixing these would require a separately justified omission admission rule; this change does not add state to approximate them. Blank references, empty text, explicit zero, and existing false-branch controls remain distinct. Worksheet publication still converts a final blank to zero.

Needed compound branch expressions execute once as whole expressions; this is not a new coordinate-lazy expression interpreter. Unsupported nested operations and resource failures remain operational and are not caught by IFERROR. PinnedPoi retains its scalar condition selection. No native Excel was executed by the agent; existing Microsoft-derived fixtures and independent Calc evidence retain their stated provenance.
