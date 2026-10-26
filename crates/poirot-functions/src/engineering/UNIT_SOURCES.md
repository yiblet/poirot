# Measurement definitions used by CONVERT

The table and conversion code were written independently for Poirot. No library
implementation, fixture, dataset arrangement or test was copied or adapted.
Only factual identifiers and definitions were consulted. These sources do not
establish Excel's undisclosed historical coefficients or final-bit rounding.
Access date: 2026-10-22.

[Microsoft CONVERT](https://support.microsoft.com/en-us/excel/functions/convert-function)
is the token/alias, case sensitivity, quantity-group and prefix contract.
`units.rs::exact` covers every identifier there, including parenthetical aliases,
and both spellings of square/cubic units. Decimal prefixes are applied once;
squared/cubed length prefixes have exponent two/three. Binary prefixes are
restricted to information units. Exact tokens take precedence over prefixes.

## Factor ledger

Canonical quantities are kg, m, s, Pa, N, J, W, T, cubic m, square m, bit and
m/s. Temperature scale is Kelvin per native degree, with native readings at the
same physical reference point. Algebraic powers and ratios derive other factors.
Exact definitions still incur ordinary binary64 rounding.

- [NIST SP 811, Appendix B.8 (2008 edition)](https://www.nist.gov/pml/special-publication-811/nist-guide-si-appendix-b-conversion-factors/nist-guide-si-appendix-b8)
  supplies customary measures, standard gravity, calories, magnetism and time.
  Pound = 0.45359237 kg; inch = 0.0254 m; foot = 12 inches; yard = 3 feet;
  mile = 5280 feet; nautical mile = 1852 m. These are exact definitions.
  Slug is pound × standard gravity / foot; ounce, grain, hundredweights, stone
  and tons use their integer pound ratios. Dyne and erg use exact decimal powers.
  Thermochemical/IT calories are 4.184/4.1868 J. Mechanical horsepower is
  550 foot-pounds-force per second; metric horsepower is 75 kg-force m/s.
  BTU uses the IT definition, 1055.05585262 J. Conventional mercury pressure is
  133.322387415 Pa; Torr is standard atmosphere/760. Those selected conventions
  and the displayed NIST precision are distinct from a native Excel verification.
- [NIST SP 811, Appendix B](https://www.nist.gov/pml/special-publication-811/nist-guide-si-appendix-b-conversion-factors)
  defines the historical US survey foot as 1200/3937 m. Survey mile/acre derive
  from that foot; international acre derives from the international foot.
- US liquid gallon is 231 cubic inches; imperial gallon is 4.54609 litres.
  Cup, fluid ounce, pint, quart, tablespoon and customary teaspoon derive from
  the US gallon. Bushel is 2150.42 cubic inches; oil barrel is 42 US gallons.
  These are definitions, not rounded factors copied from a conversion table.
- [NIST Handbook 44, 2026, Appendix B](https://www.nist.gov/system/files/documents/2025/12/30/appb-26-HB44-20251210.pdf)
  distinguishes the 100 cubic-foot register ton from the 40 cubic-foot
  measurement ton. Their factors derive from the international foot.
- [CODATA 2022, Table XXXV](https://physics.nist.gov/cuu/pdf/JPCRD2022CODATA.pdf)
  supplies the approximate atomic mass unit 1.66053906892e-27 kg, the exact
  electron volt 1.602176634e-19 J and exact light speed 299792458 m/s.
- [IAU 2015 Resolution B2, footnote 4](https://www.iau.org/static/resolutions/IAU2015_English.pdf)
  defines parsec as (648000/pi) astronomical units. The astronomical unit is
  149597870700 m. Light-year uses light speed and a Julian year of 365.25 days;
  year/day/hour/minute/second are fixed durations, not calendar arithmetic.
- [Nebraska energy office, E factors](https://dwee.nebraska.gov/state-energy-information/energy-statistics/miscellaneous/conversion-factors/e-conversionequivalency-factors)
  identifies the English ell as 45 inches. Microsoft distinguishes the 1/72-inch
  point (`Picapt`/`Pica`) from the 1/6-inch pica (`pica`).
- [FDA metric household measures (1993)](https://www.fda.gov/regulatory-information/search-fda-guidance-documents/guidance-industry-guidelines-determining-metric-equivalents-household-measures)
  gives the modern metric teaspoon as 5 mL. It is separate from customary `tsp`.
- [PTB, Gesetzliche Einheiten, area table](https://www.ptb.de/cms/fileadmin/internet/Themenrundgaenge/hueterin_der_einheiten/einheiten_d.pdf)
  gives the metric quarter-hectare Morgen convention, 2500 square metres;
  historical regional Morgen definitions differ. This selection needs native
  Excel confirmation. Hectare/are are exact decimal areas; SI powers give
  angstrom and squared/cubed metric units. Gauss = 1e-4 T. Bit/byte use 8 bits.
- [BIPM SI Brochure, eighth edition, table 3 note (a)](https://www.bipm.org/documents/20126/41483022/si_brochure_8.pdf)
  permits SI prefixes on Celsius. Celsius/Fahrenheit/Kelvin/Rankine/Reaumur use reference readings
  0/32/273.15/491.67/0, with degree ratios 1, 5/9, 1, 5/9, 5/4.
  Absolute Kelvin/Rankine maps use linear scaling; offset maps use the
  shared reference reading. Negative absolute temperatures are accepted.

## Compatibility limits to verify

Microsoft does not mark prefix eligibility per individual row. Poirot admits
decimal prefixes for SI/CGS units, astronomical lengths, atmospheres/mercury/Torr,
Celsius/Kelvin, litres, are, metric speed and information; customary units, calendar
names and already prefixed hectare do not accept extra prefixes. Binary prefixes
are accepted only for bit/byte, including Microsoft's lowercase `ki` spelling.
This finite policy, metric Morgen, Julian year, astronomical constants, mercury,
BTU, atomic mass updates and detailed coercion/error ordering remain native
verification items. No claim of bit-exact historical Excel coefficients is made.
