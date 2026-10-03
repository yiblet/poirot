# Numeric datetime fallback

`parse_date_time(text, Context)` is a pure implementation of the numeric string
fallback in the pinned Apache POI source. It is separate from DATEVALUE and from
ordinary floating-point string parsing. The host must try its source numeric
parser first. `Valid` carries a number and context effects, `Rejected` identifies
an invalid string (spreadsheet VALUE in the host), and `Unsupported` identifies an
operational capability limitation, which the host must retain distinctly.

The source revision is `732120980140d5ed64b482c470e0b625cdb1ab15`:

- `OperandResolver.java:271-345`, SHA-256
  `82c3faf09404b54a0e717a44619ad46e936dd3c4ad5c3dfadac5781216ea677a`.
- `DateUtil.java:81-88,989-1003`, SHA-256
  `6f4be2cf77194cc40c4059caa237caf3fff19dd58c13a2cb1ad0520d452cc359`.

Four builder patterns concatenate. Optional sections restore both input and
parsed fields on failure; successful sections do not backtrack. Repeated fields
must agree. English full and abbreviated month names and AM/PM are case
sensitive. Java's default ASCII regex whitespace is collapsed to one space,
without trimming or accepting Unicode whitespace. SMART resolution clamps days
29 through 31 to the actual end of a month, rejects day 32, and carries 24:00 into
the date when one is present. Exactly three fractional digits parse, but the
source's `toSecondOfDay` drops milliseconds. Time-only 24:00 returns zero.

The serial uses the source 1900 convention, including its fictitious leap-day
adjustment, regardless of workbook date1904. Pre-1900 dates return -1 before the
time fraction is added. Default year is an explicit frozen context input instead
of the source formatter's class-initialization clock read. Explicit-year dates
and standalone times do not report default-year use. Month names/AMPM report
locale use; date conversion reports timezone use. The supported timezone policy
means the source instant conversion and calendar extraction share the same fixed
offset; it does not claim DST, independently differing JVM and user timezones,
or arbitrary locale support.

Signed years above four digits parse as formatter fields but return explicit
unsupported for calendar conversion above 10000. An omitted default year outside
1 through 10000 is also unsupported. Textual year zero is rejected. These limits
avoid guessing the source's `Date`/GregorianCalendar conversion at extreme years.

Unsigned four-digit years greedily consume digit runs and reject more than four
digits, following the source formatter's EXCEEDS_PAD rule.
