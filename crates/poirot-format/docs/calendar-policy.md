# Explicit calendar policy

`CalendarPolicy` is a pure, host-clock-independent, finite/nonnegative calendar
boundary helper. Last whole days are 2958465 (1900) and 2957003 (1904).
All fractions of those last days validate. Display rounding that carries into
10000-01-01 returns an error, rather than printing an out-of-policy year.
Elapsed formats retain the same bounded nonnegative policy; unlimited durations
and negative calendar displays are not implemented.

`PhantomDayPolicy::DisplayFebruary29` preserves the existing fictional serial-60
1900-02-29 display and Thursday weekday. `GregorianMarch1` explicitly selects
POI DateUtil's Gregorian mapping of serial 60 to 1900-03-01. The latter selects
conversion semantics only: ROI's upper bound is deliberately stricter than POI's
`isValidExcelDate` predicate. No timezone or current-date defaults are used.

Source: Apache POI `poi/src/main/java/org/apache/poi/ss/usermodel/DateUtil.java`
at revision `732120980140d5ed64b482c470e0b625cdb1ab15`,
SHA256 6f4be2cf77194cc40c4059caa237caf3fff19dd58c13a2cb1ad0520d452cc359.
`setCalendar` documents March 1 mapping and 1904 base adjustment. These are
source-derived expectations; this change does not claim an actual Excel run.
Existing fake-day display is an intentional ROI policy, not an assertion that
POI renders it. WORKDAY.INTL mask divergences belong to evaluator evidence and
remain unaffected by these formatter helpers.
