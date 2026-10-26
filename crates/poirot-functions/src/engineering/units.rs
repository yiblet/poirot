//! Independently encoded measurement definitions; provenance is in UNIT_SOURCES.md.
/// Incompatible physical quantities remain distinct even if factors coincide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dimension {
    Mass,
    Length,
    Time,
    Pressure,
    Force,
    Energy,
    Power,
    Magnetism,
    Temperature,
    Volume,
    Area,
    Information,
    Speed,
}
/// Prefix admission, including exponentiation for squared and cubed units.
#[derive(Clone, Copy)]
enum Prefix {
    None,
    Decimal(i32),
    Information,
}
/// Whether zero native quantity is the dimension's physical zero.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mapping {
    Linear,
    Affine,
}
/// Resolved unit metadata used to check dimensional compatibility and convert quantities.
/// Scale and the native reading at a shared reference point support both linear units and affine temperatures.
#[derive(Clone, Copy)]
struct Unit {
    dimension: Dimension,
    mapping: Mapping,
    scale: f64,
    origin: f64,
    prefix: Prefix,
}
fn linear(dimension: Dimension, scale: f64, prefix: Prefix) -> Unit {
    Unit {
        dimension,
        mapping: Mapping::Linear,
        scale,
        origin: 0.0,
        prefix,
    }
}
fn exact(token: &str) -> Option<Unit> {
    use Dimension::*;
    use Prefix::{Decimal as D, Information as I, None as No};
    const INCH: f64 = 0.0254;
    const FOOT: f64 = 12.0 * INCH;
    const MILE: f64 = 5280.0 * FOOT;
    const POUND: f64 = 0.453_592_37;
    const GRAVITY: f64 = 9.806_65;
    const GALLON: f64 = 231.0 * INCH * INCH * INCH;
    const UK_GALLON: f64 = 0.004_546_09;
    const LIGHT_YEAR: f64 = 299_792_458.0 * 31_557_600.0;
    const POINT: f64 = INCH / 72.0;
    let (dimension, scale, prefix) = match token {
        "g" => (Mass, 0.001, D(1)),
        "sg" => (Mass, POUND * GRAVITY / FOOT, No),
        "lbm" => (Mass, POUND, No),
        "u" => (Mass, 1.660_539_068_92e-27, D(1)),
        "ozm" => (Mass, POUND / 16.0, No),
        "grain" => (Mass, POUND / 7000.0, No),
        "cwt" | "shweight" => (Mass, POUND * 100.0, No),
        "uk_cwt" | "lcwt" | "hweight" => (Mass, POUND * 112.0, No),
        "stone" => (Mass, POUND * 14.0, No),
        "ton" => (Mass, POUND * 2000.0, No),
        "uk_ton" | "LTON" | "brton" => (Mass, POUND * 2240.0, No),
        "m" => (Length, 1.0, D(1)),
        "mi" => (Length, MILE, No),
        "Nmi" => (Length, 1852.0, No),
        "in" => (Length, INCH, No),
        "ft" => (Length, FOOT, No),
        "yd" => (Length, FOOT * 3.0, No),
        "ang" => (Length, 1e-10, D(1)),
        "ell" => (Length, 45.0 * INCH, No),
        "ly" => (Length, LIGHT_YEAR, D(1)),
        "parsec" | "pc" => (
            Length,
            149_597_870_700.0 * 648_000.0 / std::f64::consts::PI,
            D(1),
        ),
        "Picapt" | "Pica" => (Length, POINT, No),
        "pica" => (Length, INCH / 6.0, No),
        "survey_mi" => (Length, 5280.0 * 1200.0 / 3937.0, No),
        "yr" => (Time, 31_557_600.0, No),
        "day" | "d" => (Time, 86400.0, No),
        "hr" => (Time, 3600.0, No),
        "mn" | "min" => (Time, 60.0, No),
        "sec" | "s" => (Time, 1.0, D(1)),
        "Pa" | "p" => (Pressure, 1.0, D(1)),
        "atm" | "at" => (Pressure, 101325.0, D(1)),
        "mmHg" => (Pressure, 133.322_387_415, D(1)),
        "psi" => (Pressure, POUND * GRAVITY / (INCH * INCH), No),
        "Torr" => (Pressure, 101325.0 / 760.0, D(1)),
        "N" => (Force, 1.0, D(1)),
        "dyn" | "dy" => (Force, 1e-5, D(1)),
        "lbf" => (Force, POUND * GRAVITY, No),
        "J" => (Energy, 1.0, D(1)),
        "e" => (Energy, 1e-7, D(1)),
        "c" => (Energy, 4.184, D(1)),
        "cal" => (Energy, 4.1868, D(1)),
        "eV" | "ev" => (Energy, 1.602_176_634e-19, D(1)),
        "HPh" | "hh" => (Energy, 550.0 * FOOT * POUND * GRAVITY * 3600.0, No),
        "Wh" | "wh" => (Energy, 3600.0, D(1)),
        "flb" => (Energy, FOOT * POUND * GRAVITY, No),
        "BTU" | "btu" => (Energy, 1_055.055_852_62, No),
        "HP" | "h" => (Power, 550.0 * FOOT * POUND * GRAVITY, No),
        "PS" => (Power, 75.0 * GRAVITY, No),
        "W" | "w" => (Power, 1.0, D(1)),
        "T" => (Magnetism, 1.0, D(1)),
        "ga" => (Magnetism, 1e-4, D(1)),
        "C" | "cel" => {
            return Some(Unit {
                dimension: Temperature,
                mapping: Mapping::Affine,
                scale: 1.0,
                origin: 0.0,
                prefix: D(1),
            });
        }
        "F" | "fah" => {
            return Some(Unit {
                dimension: Temperature,
                mapping: Mapping::Affine,
                scale: 5.0 / 9.0,
                origin: 32.0,
                prefix: No,
            });
        }
        "K" | "kel" => {
            return Some(Unit {
                dimension: Temperature,
                mapping: Mapping::Linear,
                scale: 1.0,
                origin: 273.15,
                prefix: D(1),
            });
        }
        "Rank" => {
            return Some(Unit {
                dimension: Temperature,
                mapping: Mapping::Linear,
                scale: 5.0 / 9.0,
                origin: 491.67,
                prefix: No,
            });
        }
        "Reau" => {
            return Some(Unit {
                dimension: Temperature,
                mapping: Mapping::Affine,
                scale: 1.25,
                origin: 0.0,
                prefix: No,
            });
        }
        "tsp" => (Volume, GALLON / 768.0, No),
        "tspm" => (Volume, 5e-6, No),
        "tbs" => (Volume, GALLON / 256.0, No),
        "oz" => (Volume, GALLON / 128.0, No),
        "cup" => (Volume, GALLON / 16.0, No),
        "pt" | "us_pt" => (Volume, GALLON / 8.0, No),
        "uk_pt" => (Volume, UK_GALLON / 8.0, No),
        "qt" => (Volume, GALLON / 4.0, No),
        "uk_qt" => (Volume, UK_GALLON / 4.0, No),
        "gal" => (Volume, GALLON, No),
        "uk_gal" => (Volume, UK_GALLON, No),
        "l" | "L" | "lt" => (Volume, 0.001, D(1)),
        "ang3" | "ang^3" => (Volume, 1e-30, D(3)),
        "barrel" => (Volume, GALLON * 42.0, No),
        "bushel" => (Volume, 2150.42 * INCH * INCH * INCH, No),
        "ft3" | "ft^3" => (Volume, FOOT * FOOT * FOOT, No),
        "in3" | "in^3" => (Volume, INCH * INCH * INCH, No),
        "ly3" | "ly^3" => (Volume, LIGHT_YEAR * LIGHT_YEAR * LIGHT_YEAR, D(3)),
        "m3" | "m^3" => (Volume, 1.0, D(3)),
        "mi3" | "mi^3" => (Volume, MILE * MILE * MILE, No),
        "yd3" | "yd^3" => (Volume, 27.0 * FOOT * FOOT * FOOT, No),
        "Nmi3" | "Nmi^3" => (Volume, 1852.0 * 1852.0 * 1852.0, No),
        "Picapt3" | "Picapt^3" | "Pica3" | "Pica^3" => (Volume, POINT * POINT * POINT, No),
        "GRT" | "regton" => (Volume, 100.0 * FOOT * FOOT * FOOT, No),
        "MTON" => (Volume, 40.0 * FOOT * FOOT * FOOT, No),
        "uk_acre" => (Area, 43560.0 * FOOT * FOOT, No),
        "us_acre" => (Area, 43560.0 * (1200.0 / 3937.0) * (1200.0 / 3937.0), No),
        "ang2" | "ang^2" => (Area, 1e-20, D(2)),
        "ar" => (Area, 100.0, D(1)),
        "ft2" | "ft^2" => (Area, FOOT * FOOT, No),
        "ha" => (Area, 10000.0, No),
        "in2" | "in^2" => (Area, INCH * INCH, No),
        "ly2" | "ly^2" => (Area, LIGHT_YEAR * LIGHT_YEAR, D(2)),
        "m2" | "m^2" => (Area, 1.0, D(2)),
        "Morgen" => (Area, 2500.0, No),
        "mi2" | "mi^2" => (Area, MILE * MILE, No),
        "Nmi2" | "Nmi^2" => (Area, 1852.0 * 1852.0, No),
        "Picapt2" | "Picapt^2" | "Pica2" | "Pica^2" => (Area, POINT * POINT, No),
        "yd2" | "yd^2" => (Area, 9.0 * FOOT * FOOT, No),
        "bit" => (Information, 1.0, I),
        "byte" => (Information, 8.0, I),
        "admkn" => (Speed, 6080.0 * FOOT / 3600.0, No),
        "kn" => (Speed, 1852.0 / 3600.0, No),
        "m/h" | "m/hr" => (Speed, 1.0 / 3600.0, D(1)),
        "m/s" | "m/sec" => (Speed, 1.0, D(1)),
        "mph" => (Speed, MILE / 3600.0, No),
        _ => return None,
    };
    Some(linear(dimension, scale, prefix))
}
const DECIMAL: &[(&str, f64)] = &[
    ("da", 1e1),
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("e", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
];
const BINARY: &[(&str, u32)] = &[
    ("Yi", 80),
    ("Zi", 70),
    ("Ei", 60),
    ("Pi", 50),
    ("Ti", 40),
    ("Gi", 30),
    ("Mi", 20),
    ("ki", 10),
];
fn resolve(token: &str) -> Option<Unit> {
    if let Some(unit) = exact(token) {
        return Some(unit);
    }
    for &(prefix, power) in BINARY {
        if let Some(mut unit) = token.strip_prefix(prefix).and_then(exact)
            && matches!(unit.prefix, Prefix::Information)
        {
            unit.scale *= 2f64.powi(power as i32);
            return Some(unit);
        }
    }
    for &(prefix, factor) in DECIMAL {
        if let Some(mut unit) = token.strip_prefix(prefix).and_then(exact) {
            let exponent = match unit.prefix {
                Prefix::None => continue,
                Prefix::Decimal(n) => n,
                Prefix::Information => 1,
            };
            let multiplier = factor.powi(exponent);
            unit.scale *= multiplier;
            unit.origin /= multiplier;
            return Some(unit);
        }
    }
    None
}
/// Distinguish unknown/incompatible units from arithmetic overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionError {
    /// Unknown tokens, unsupported prefixes or different dimensions.
    NotAvailable,
    /// Nonfinite input or result.
    Number,
}
/// Convert a finite quantity with exact-token-first, case-sensitive unit resolution.
pub fn convert(number: f64, from: &str, to: &str) -> std::result::Result<f64, ConversionError> {
    let from = resolve(from).ok_or(ConversionError::NotAvailable)?;
    let to = resolve(to).ok_or(ConversionError::NotAvailable)?;
    if from.dimension != to.dimension {
        return Err(ConversionError::NotAvailable);
    }
    if !number.is_finite() {
        return Err(ConversionError::Number);
    }
    // Every admitted factor is between 1e-102 and 1e120, so their ratio is finite
    // and nonzero. Ratio-first scaling avoids overflow through canonical units.
    let ratio = from.scale / to.scale;
    let result = if from.mapping == Mapping::Linear && to.mapping == Mapping::Linear {
        number * ratio
    } else if from.scale == to.scale && from.origin == to.origin {
        number
    } else {
        (number - from.origin) * ratio + to.origin
    };
    if result.is_finite() {
        Ok(result)
    } else {
        Err(ConversionError::Number)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn near(n: f64, from: &str, to: &str, expected: f64) {
        let actual = convert(n, from, to).unwrap();
        assert!(
            (actual - expected).abs()
                <= if expected == 0.0 {
                    1e-13
                } else {
                    expected.abs() * 1e-13
                },
            "{from} {to}: {actual} {expected}"
        );
    }
    #[test]
    fn exact_definitions_affine_temperatures_and_powered_prefixes() {
        near(1., "in", "m", 0.0254);
        near(1., "ft", "in", 12.);
        near(1., "lbm", "g", 453.59237);
        near(1., "byte", "bit", 8.);
        near(1., "kibyte", "byte", 1024.);
        near(1., "kbyte", "byte", 1000.);
        near(0., "C", "F", 32.);
        near(0., "C", "K", 273.15);
        near(1000., "mC", "C", 1.);
        near(1e-20, "K", "mK", 1e-17);
        near(1e-20, "K", "Rank", 1.8e-20);
        near(1e-20, "Rank", "K", 1e-20 * 5.0 / 9.0);
        assert_eq!(convert(0.1, "K", "mK"), Ok(100.0));
        near(212., "F", "C", 100.);
        near(80., "Reau", "C", 100.);
        near(491.67, "Rank", "C", 0.);
        near(1., "cm2", "m2", 1e-4);
        near(1., "cm^3", "m3", 1e-6);
        near(1., "km/h", "m/s", 1. / 3.6);
        near(1., "h", "W", 745.6998715822702);
        near(1., "pica", "Picapt", 12.);
        near(1e300, "Yly3", "Yly3", 1e300);
        near(1e300, "ym3", "Ym3", 1e156);
        for (a, b) in [
            ("m", "s"),
            ("foo", "foo"),
            ("KiB", "byte"),
            ("kim", "m"),
            ("M", "m"),
            ("kkm", "m"),
            ("kft", "m"),
        ] {
            assert_eq!(convert(1., a, b), Err(ConversionError::NotAvailable));
        }
        assert_eq!(
            convert(f64::MAX, "Ym3", "ym3"),
            Err(ConversionError::Number)
        );
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;
    fn equal_quantity(n: f64, from: &str, to: &str, expected: f64) {
        let actual = convert(n, from, to).unwrap();
        assert!(
            (actual - expected).abs()
                <= if expected == 0.0 {
                    1e-12
                } else {
                    expected.abs() * 1e-12
                },
            "{from}->{to}: {actual} != {expected}"
        );
    }
    #[test]
    fn every_documented_alias_and_quantity_family_is_admitted() {
        // Factual alias identities from the Microsoft token contract. Scale
        // checks below are independent definitions rather than inverse calls.
        for aliases in [
            "cwt shweight",
            "uk_cwt lcwt hweight",
            "uk_ton LTON brton",
            "parsec pc",
            "Picapt Pica",
            "day d",
            "mn min",
            "sec s",
            "Pa p",
            "atm at",
            "dyn dy",
            "eV ev",
            "HPh hh",
            "Wh wh",
            "BTU btu",
            "HP h",
            "W w",
            "C cel",
            "F fah",
            "K kel",
            "pt us_pt",
            "l L lt",
            "ang3 ang^3",
            "ft3 ft^3",
            "in3 in^3",
            "ly3 ly^3",
            "m3 m^3",
            "mi3 mi^3",
            "yd3 yd^3",
            "Nmi3 Nmi^3",
            "Picapt3 Picapt^3 Pica3 Pica^3",
            "GRT regton",
            "ang2 ang^2",
            "ft2 ft^2",
            "in2 in^2",
            "ly2 ly^2",
            "m2 m^2",
            "mi2 mi^2",
            "Nmi2 Nmi^2",
            "Picapt2 Picapt^2 Pica2 Pica^2",
            "yd2 yd^2",
            "m/h m/hr",
            "m/s m/sec",
        ] {
            let mut tokens = aliases.split_whitespace();
            let canonical = tokens.next().unwrap();
            for alias in tokens {
                assert_eq!(convert(7.0, alias, canonical), Ok(7.0), "{alias}");
            }
        }
        for (from, to, expected) in [
            ("g", "kg", 0.001),
            ("sg", "lbm", 32.17404855643044),
            ("u", "kg", 1.66053906892e-27),
            ("ozm", "lbm", 1. / 16.),
            ("grain", "lbm", 1. / 7000.),
            ("cwt", "lbm", 100.),
            ("uk_cwt", "lbm", 112.),
            ("stone", "lbm", 14.),
            ("ton", "lbm", 2000.),
            ("uk_ton", "lbm", 2240.),
            ("mi", "m", 1609.344),
            ("Nmi", "m", 1852.),
            ("yd", "in", 36.),
            ("ang", "m", 1e-10),
            ("ell", "in", 45.),
            ("ly", "m", 9.4607304725808e15),
            ("parsec", "m", 3.085677581491367e16),
            ("pica", "in", 1. / 6.),
            ("survey_mi", "m", 1609.3472186944373),
            ("yr", "day", 365.25),
            ("hr", "min", 60.),
            ("day", "hr", 24.),
            ("min", "s", 60.),
            ("Pa", "N", f64::NAN), // Different quantity groups never cancel dimensions.
            ("atm", "Pa", 101325.),
            ("mmHg", "Pa", 133.322387415),
            ("psi", "Pa", 6894.757293168361),
            ("Torr", "Pa", 133.32236842105263),
            ("N", "dyn", 100000.),
            ("lbf", "N", 4.4482216152605),
            ("J", "e", 1e7),
            ("c", "J", 4.184),
            ("cal", "J", 4.1868),
            ("eV", "J", 1.602176634e-19),
            ("HPh", "Wh", 745.6998715822702),
            ("Wh", "J", 3600.),
            ("flb", "J", 1.3558179483314004),
            ("BTU", "J", 1055.05585262),
            ("PS", "W", 735.49875),
            ("T", "ga", 10000.),
            ("tsp", "tbs", 1. / 3.),
            ("tspm", "l", 0.005),
            ("oz", "cup", 1. / 8.),
            ("cup", "pt", 0.5),
            ("uk_pt", "uk_gal", 1. / 8.),
            ("qt", "gal", 0.25),
            ("uk_qt", "uk_gal", 0.25),
            ("gal", "in3", 231.),
            ("uk_gal", "l", 4.54609),
            ("barrel", "gal", 42.),
            ("bushel", "in3", 2150.42),
            ("ft3", "in3", 1728.),
            ("yd3", "ft3", 27.),
            ("mi3", "ft3", 147197952000.),
            ("Nmi3", "m3", 6352182208.),
            ("Picapt3", "in3", 1. / 373248.),
            ("GRT", "ft3", 100.),
            ("MTON", "ft3", 40.),
            ("uk_acre", "ft2", 43560.),
            ("us_acre", "m2", 4046.872609874252),
            ("ar", "m2", 100.),
            ("ha", "ar", 100.),
            ("in2", "m2", 0.00064516),
            ("Morgen", "m2", 2500.),
            ("mi2", "ft2", 27878400.),
            ("Nmi2", "m2", 3429904.),
            ("Picapt2", "in2", 1. / 5184.),
            ("yd2", "ft2", 9.),
            ("admkn", "ft/hr", f64::NAN),
            ("admkn", "m/s", 0.5147733333333333),
            ("kn", "m/h", 1852.),
            ("mph", "m/h", 1609.344),
        ] {
            if expected.is_nan() {
                assert_eq!(convert(1., from, to), Err(ConversionError::NotAvailable));
            } else {
                equal_quantity(1., from, to, expected);
            }
        }
        assert_eq!(convert(32., "F", "C"), Ok(0.));
        assert_eq!(convert(273.15, "K", "C"), Ok(0.));
        assert_eq!(convert(491.67, "Rank", "C"), Ok(0.));
        assert_eq!(convert(1e-300, "C", "C"), Ok(1e-300));
        equal_quantity(273150., "mK", "C", 0.);
    }
    #[test]
    fn all_prefixes_have_independent_decimal_or_bit_exponents() {
        for (token, exponent) in [
            ("Y", 24),
            ("Z", 21),
            ("E", 18),
            ("P", 15),
            ("T", 12),
            ("G", 9),
            ("M", 6),
            ("k", 3),
            ("h", 2),
            ("da", 1),
            ("e", 1),
            ("d", -1),
            ("c", -2),
            ("m", -3),
            ("u", -6),
            ("n", -9),
            ("p", -12),
            ("f", -15),
            ("a", -18),
            ("z", -21),
            ("y", -24),
        ] {
            for power in [1, 2, 3] {
                let suffix = if power == 1 {
                    "m"
                } else if power == 2 {
                    "m2"
                } else {
                    "m3"
                };
                let result = convert(1., &format!("{token}{suffix}"), suffix).unwrap();
                let expected = 10f64.powi(exponent * power);
                assert!((result / expected - 1.).abs() < 1e-12);
            }
        }
        for (token, power) in [
            ("ki", 10),
            ("Mi", 20),
            ("Gi", 30),
            ("Ti", 40),
            ("Pi", 50),
            ("Ei", 60),
            ("Zi", 70),
            ("Yi", 80),
        ] {
            assert_eq!(
                convert(1., &format!("{token}byte"), "bit"),
                Ok(2f64.powi(power + 3))
            );
            assert_eq!(
                convert(1., &format!("{token}m"), "m"),
                Err(ConversionError::NotAvailable)
            );
        }
    }
}
