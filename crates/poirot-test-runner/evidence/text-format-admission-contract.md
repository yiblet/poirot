# TEXT contextual value admission

The original saved Microsoft Excel worksheet contains TEXT(F5,"H:MM:ss AM/PM")
where F5 is the text result "15:43:09" of another TEXT call. The expected value
"3:43:09 PM" is independently saved upstream and corroborated by fresh Calc.
Direct numeric controls already render AM/PM correctly; the defect is admission
of numeric text before rendering, not the renderer.

Three options were compared:

1. Compose TEXT's centralized value policy with the existing metered locale,
   formatted-number and calendar parser for ExcelDocumentation. Selected: the
   same rule covers numeric, currency, percent, scientific, negative and date/time
   text in either text encoding without adding a grammar or value state.
2. Add a separate TEXT parser or formatter. Rejected: no evidence requires a new
   grammar or rendering behavior, and it would duplicate existing contracts.
3. Patch AM/PM strings or the individual pattern. Rejected: it would preserve the
   admission defect across the neighboring input and format distribution.

PinnedPoi retains the arithmetic UTF8 fast path and its checked UTF16 projection.
ExcelDocumentation defers both encodings to existing contextual admission. Rejected
numeric text remains the original typed text, including malformed UTF16. Blank is
converted only because TEXT requests its numeric value. Booleans retain their
existing direct spelling; errors propagate. Operational limits remain terminal
and cannot be caught as a literal-text fallback or by IFERROR. Existing locale,
calendar, clock and epoch dependency effects remain owned by the reused parser.
BuiltinPolicy reads on both paths protect profile changes and retained results.
Raw snapshot version and compatibility revision advance with execution semantics.

The diagnostic report retains all 20 en-US observations: 18 corroborate authored
hypotheses. Calc formats boolean TRUE and the text "TRUE" as 1.00; this work does
not change Poirot's existing boolean/literal contract or claim native Excel agrees
with it. Four additional locale hypotheses remain inconclusive: changing the
application locale alone left imported OOXML interpretation English. They are
retained as disagreements, not asserted as independent locale evidence. Locale,
date-order and epoch regressions instead verify composition with the established
parametric parser contract, plus retained invalidation, profile changes, source
edits and snapshot restoration. No native Excel execution was available.

`text-format-admission-libreoffice.json` freshly corroborates all three checks in
the changed source suite. The original shared report retains its former hash in
`source_reverification`; expected values, formulas and tolerances are unchanged.
