# Business-day serial-calendar contract

The `ExcelDocumentation` policy counts integer serial days on Excel's continuous
seven-day cycle. `PinnedPoi` retains the existing Gregorian calculation, including
its dual serial/civil coordinates and historical reversed-endpoint error. Public
defaults do not change. `closedxml-extension-workdays.json` now explicitly selects
the modern contract for its entire coherent WORKDAY suite; date-domains already
selected that contract in the calendar work.

## Independent evidence

- [Microsoft WORKDAY.INTL](https://support.microsoft.com/en-us/excel/functions/workday-intl-function)
  specifies signed offsets, integer truncation, zero returning the start date,
  Monday-first binary weekend masks, unordered holiday sets, and NUM for a holiday
  outside the workbook date base. All-weekend WORKDAY masks are invalid. The
  implementation does not add NETWORKDAYS.INTL, which is separately unsupported.
- The original [ClosedXML date tests](https://github.com/ClosedXML/ClosedXML/blob/09f2f21a60e821ce7fb18ca3576b272e5dd6d921/ClosedXML.Tests/Excel/CalcEngine/DateAndTimeTests.cs)
  supply all retained source expectations, including 12 early-serial WORKDAY
  cases, two reversed NETWORKDAYS intervals, and numeric text array holidays.
- `business-days-libreoffice.json` binds both changed suite hashes to fresh Calc
  evaluations. All 28 checks corroborate the unchanged source expectations.
- `business-days-calc-probes.json` and `business-days-calc-origin-probes.json`
  contain raw independent OOXML/Calc results in both 1900 and 1904 workbooks.
  Probes cover equal/forward/reversed endpoints, serials 0/1/59/60/61, signed/zero
  offsets, duplicate/fraction/outside holidays, scalar/array/reference text,
  logicals, blanks, errors and weekend strings. The probe script uses the same
  minimal OOXML writer and typed-cache reader as the checked-in verification tool.

Calc is corroboration, not a native Excel oracle. Its early serials share the
observed weekday cycle, but civil-date identity before March 1900 must not be
inferred. Calc accepts negative dates and out-of-base holidays; Microsoft explicitly
requires bounds, so Poirot retains NUM. Calc imports literal error cells and array
error constants imperfectly (REF becomes NAME/N/A); Poirot propagates the original
spreadsheet error. Calc rejects direct scalar NETWORKDAYS holiday text but converts
text arrays; existing direct date-text admission remains intact rather than copying
that unverified inconsistency. NETWORKDAYS ignores referenced text and logical
holiday members. WORKDAY converts text and logicals across origins. A blank
worksheet member is absent; an explicit omitted holiday argument retains zero.
No native Excel is available, so direct scalar holiday-text compatibility beyond
these retained contracts remains unverified.

## Mechanism, alternatives and attribution

The previous modern path reused the pinned civil calendar, rejected reversed
NETWORKDAYS with NAME, subtracted duplicate holidays, and rounded holidays relative
to fractional start dates. This is a consumer calendar contract mismatch with local
count/admission defects. It does not require changing workbook value representation.

1. Selected: keep pinned behavior explicit and implement bounded modern serial-day
   count/advancement beside it. Centralize origin-sensitive holiday admission in
   coercionv2. Whole-week jumps retain efficient traversal and CalendarStep charges.
2. Enumerating every day would simplify traversal but makes million-day offsets
   consume millions of steps and changes operational limits.
3. A reusable business-calendar abstraction would add indirection without another
   demonstrated consumer requirement; existing helper boundaries express both
   contracts completely.

The 12 original WORKDAY cases require the coherent modern suite selection plus
serial-coordinate traversal. The three NETWORKDAYS cases require signed endpoint
counting and the serial weekday cycle. Dependencies supplied explicit weekday
helpers and date admission but had not resolved these 15 original cases.
Holidays are validated, truncated, sorted and deduplicated before either algorithm.
Inclusive counts use complete weeks plus at most six remainder checks; traversal
jumps complete holiday-free weeks and checks each resulting serial against bounds.
Versions advance from raw 14 / compatibility 15 to raw 15 / compatibility 16 because
retained execution results under an existing modern configuration change.
