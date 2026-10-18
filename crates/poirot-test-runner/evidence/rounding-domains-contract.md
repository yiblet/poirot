# Rounding modes and zero domains

The complete `closedxml-math.json` suite selects `ExcelDocumentation`. Its
57 authored expectations fit that profile; four unrelated known gaps remain.
Expected values and tolerances are unchanged. The matching source hash and typed
Calc results are in [rounding-domains-libreoffice.json](rounding-domains-libreoffice.json).
Older corroboration in `closedxml-libreoffice.json` is retained.

## Mechanism and policy

After normal scalar numeric admission, the Excel profile interprets every
nonzero MATH mode as a direction flag. CEILING.MATH reverses negative-number
rounding away from zero; FLOOR.MATH reverses it toward zero. Significance is
absolute in both MATH and PRECISE kernels. Positive numbers and exact multiples
are unaffected. The pinned profile retains its negative-only mode flag, and the
numeric kernels retain their existing pinned entry points. Scalar conversion,
origin, missing states, reference selection and budgets stay in their existing
central admission paths.

The implementation reuses the existing CEILING.MATH normalization for FLOOR.MATH.
The two original CEILING.MATH cases resolve by suite policy activation, not by a
new CEILING algorithm. The two FLOOR.MATH cases resolve through the new shared
normalization plus policy activation. Basic FLOOR's independent zero-significance
branch now admits zero/zero in both profiles; nonzero/zero remains #DIV/0.
CEILING's zero significance, sign checks and basic quotient direction remain
separate from MATH/PRECISE rules.

Microsoft's [CEILING.MATH documentation](https://support.microsoft.com/en-gb/excel/functions/ceiling-math-function)
provides both positive-mode prose and a negative-mode example. Its
[FLOOR.MATH documentation](https://support.microsoft.com/en-us/excel/functions/floor-math-function)
explicitly describes a nonzero mode in the worked example, although the surrounding
prose is imprecise. The [FLOOR documentation](https://support.microsoft.com/en-us/excel/functions/floor-function)
supports retaining domain distinctions between basic and modern rounders.

Independent Apache POI 5.4.1 release source
[`NumericFunction.java` lines 150–151](https://github.com/apache/poi/blob/REL_5_4_1/poi/src/main/java/org/apache/poi/ss/formula/functions/NumericFunction.java#L150)
returns zero for FLOOR(0,0) before returning DIV0 for other zero-significance
operands. Both the fetched release and the locally available Maven sources jar
have SHA-256 `a3c3336060d5e85c42bf51c7217fb80d97f4b136c39d4fe43956363d310e8b97`.
This differs from the historical repository `SOURCES.sha256` entry
`7466c67653f69b6f261d3dc726661446004be7aa72a946f218e7e833e774388c`.
The exact historical bytes were not recovered; calling this a defect relative to
that precise pin is therefore an inference. The historical manifest is unchanged.
The correction is supported independently by this identified release, the
ClosedXML assertion already cited in the corpus, and Calc's zero/zero observation.
The existing authored FLOOR(0,0) check previously recorded #DIV/0 as a known gap;
no explicit passing Rust assertion for that old result was found.

## Discriminating probes and limits

172 independently authored probes cover signs, zero, both significance signs,
mode 0/±0.1/10, omitted and missing arguments, exact multiples, booleans, errors,
and large/small magnitudes. LibreOffice 26.2.6.3 corroborates 164 of these initial
hypotheses. Raw probes and report are preserved in StateMCP namespace `board`:
`/boards/poirot-json-gaps/evidence/021-rounding-probes.json` and
`021-rounding-probes-calc.json`. These eight disagreements are retained as evidence:

- Explicitly missing MATH significance defaults to one in Calc; Poirot preserves
  its unsupported missing-numeric capability rather than collapsing missing to blank.
- Calc evaluates a MATH mode error after zero significance; Poirot retains its
  existing lazy zero-significance return before mode resolution.
- Calc reports #VALUE for basic CEILING/FLOOR with positive number and negative
  significance, and for basic FLOOR with nonzero number and zero significance.
  Poirot retains its documented/source domain errors (#NUM and #DIV/0).

Calc agrees with all 57 original suite expectations, including all five assigned
resolutions. Native Excel was unavailable; no native Excel observation is claimed.
Ordinary Rust tests separately pin the deliberate missing/error-order differences,
blank references, operational aborts through IFERROR, and both policy profiles.
Raw snapshot version 23 and compatibility revision 24 reject retained results
computed under the previous semantics; durable workbook source is unaffected.

## Alternatives

1. Selected: normalize the already admitted numeric direction flag at the existing
   policy boundary and correct FLOOR's independent zero domain branch. This
   explains the observed distribution without changing pinned MATH entry points.
2. A new direction enum across all rounders would add migration without recovering
   any lost distinction: current policy plus numeric kernels already express the rules.
3. Separate compatibility kernels would duplicate the same decimal rounding
   algorithms. The demonstrated difference is flag selection, not kernel arithmetic.
