# Deterministic system format selectors

`[$-F800]` means system long date and `[$-F400]` means system long time.
These are retained as typed selectors, not ordinary LCIDs, in the parsed program.
Compilation is pure and retains fallback syntax, conditions and colors. The fixed bounded US token patterns are compiled once; rendering borrows them
and resolves the selector using caller-controlled `FormatRenderContext.locale`.

ROI does not read the host regional settings. Its existing default English locale
pins US English names, Gregorian Excel calendar conversion, long date
`dddd, mmmm d, yyyy`, and long time `h:mm:ss AM/PM`. These patterns are a
deliberate reproducible configuration, not a claim about every Excel installation.
`FormatLocale::from_lcid(0x0409)` selects this policy; other LCIDs remain explicit
`UnsupportedLcid` values and system selector rendering fails with `UnsupportedFidelity`, an operational
capability refusal rather than a malformed-format spreadsheet error. Fallback date tokens
are preserved but do not override the configured system pattern. Multiple selectors,
selectors after fallback tokens and numeric placeholder mixtures are unsupported.
Workbook/model raw format strings are never rewritten. No ISO archive substitution
is performed: serial 46082 displays `Sunday, March 1, 2026`; 46447 displays
`Monday, March 1, 2027`. Those are display strings, not changed numeric values.

## Evidence and limits

- Microsoft Win32 `LOCALE_SLONGDATE` documents locale long date patterns and
  multiple configurable formats: https://learn.microsoft.com/en-us/windows/win32/intl/locale-slongdate
- Microsoft's `DateTime.ToLongDateString` example specifies the US pattern
  `dddd, MMMM d, yyyy` and long time `h:mm:ss tt`. Its remarks explicitly explain
  OS/version/user customization variability:
  https://learn.microsoft.com/en-us/dotnet/api/system.datetime.tolongdatestring
- Pinned POI `poi/src/main/java/org/apache/poi/ss/usermodel/DataFormatter.java`
  lines 110–112 says locale directives are ignored; lines 491–502 strip them.
  Its `poi/src/test/java/org/apache/poi/hssf/usermodel/TestHSSFDataFormatter.java`
  lines 88–108 includes F800 with creation-language fallback syntax.
  `poi/src/test/java/org/apache/poi/ss/format/TestCellFormat.java` lines 1005–1017
  exercises the F400 companion. POI therefore proves recognized syntax, not
  Windows regional selector fidelity; ROI's system semantics differ deliberately.

Microsoft's public official API documentation establishes regional configuration
and the pinned patterns. No primary normative Excel spec explicitly documenting
F800/F400 was located; this evidence limit must remain visible. An official-domain
community answer is not treated as normative primary evidence.
