//! TIMEVALUE's POI source arithmetic, isolated from workbook calendar policy.
use super::calendar::{Date, epoch};

/// Convert a date using TIMEVALUE's pinned 1900-calendar arithmetic.
pub fn source_day(date: Date) -> f64 {
    if date.y < 1900 && !(date.y == 1899 && date.m == 12 && date.d == 31) {
        return -1.;
    }
    let day = epoch(date) + 25568;
    (day + i64::from(day >= 60)) as f64
}

/// Collapse source-recognized whitespace runs before time parsing.
pub fn normalize(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut whitespace = false;
    for character in input.chars() {
        let current = matches!(character, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c');
        if !current || !whitespace {
            output.push(if current { ' ' } else { character });
        }
        whitespace = current;
    }
    output
}

// DateUtil adds second-of-day to its resolved date, then TimeValue subtracts
// DateParser's independently resolved date. Keep these operations ordered.
/// Preserve source operation order when subtracting a resolved date from date-time.
pub fn fraction(date_time_day: f64, seconds: u32, date_day: f64) -> f64 {
    (date_time_day + f64::from(seconds) / 86400.) - date_day
}

#[cfg(test)]
mod tests {
    use super::*;
    use poirot_functions_core::{CellError, CellError::Value as VALUE_ERROR};

    /// Test parser result retaining optional date text and midnight rollover.
    pub struct ParsedTime<'a> {
        /// Date prefix preceding the clock, if supplied.
        pub date_text: Option<&'a str>,
        /// Parsed seconds since midnight.
        pub seconds: u32,
        /// Whether a valid 24:00 spelling advances the date.
        pub next_day: bool,
    }

    /// Parse the controlled clock spellings used by TIMEVALUE source-order tests.
    pub fn parse(input: &str) -> Result<ParsedTime<'_>, CellError> {
        let (raw, marker) = if let Some(raw) = input.strip_suffix(" AM") {
            (raw, Some(false))
        } else if let Some(raw) = input.strip_suffix(" PM") {
            (raw, Some(true))
        } else {
            (input, None)
        };
        let (date_text, clock) = match raw.rsplit_once(' ') {
            Some((date, clock)) => (Some(date), clock),
            None => (None, raw),
        };
        let mut parts = clock.split(':');
        let hour_text = parts.next().ok_or(VALUE_ERROR)?;
        let minute_text = parts.next().ok_or(VALUE_ERROR)?;
        let second_text = parts.next();
        if parts.next().is_some() {
            return Err(VALUE_ERROR);
        }
        fn component(text: &str) -> Result<u32, CellError> {
            if text.is_empty() || text.len() > 2 || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(VALUE_ERROR);
            }
            text.parse().map_err(|_| VALUE_ERROR)
        }
        let mut hour = component(hour_text)?;
        // .SSS is exactly three digits; Java's LocalTime discards nanos when
        // converted to second-of-day. The optional fraction also follows minutes
        // in DateUtil's h:m[:s][.SSS] pattern.
        let tail = second_text.unwrap_or(minute_text);
        let (whole, nanos) = tail.split_once('.').unwrap_or((tail, ""));
        if tail.contains('.') && (nanos.len() != 3 || !nanos.bytes().all(|b| b.is_ascii_digit())) {
            return Err(VALUE_ERROR);
        }
        let minute = component(if second_text.is_some() {
            minute_text
        } else {
            whole
        })?;
        let second = if second_text.is_some() {
            component(whole)?
        } else {
            0
        };
        if let Some(pm) = marker {
            if !(1..=12).contains(&hour) {
                return Err(VALUE_ERROR);
            }
            hour = hour % 12 + if pm { 12 } else { 0 };
        }
        let next_day = marker.is_none()
            && hour == 24
            && minute == 0
            && second == 0
            && nanos.bytes().all(|b| b == b'0');
        if (hour > 23 && !next_day) || minute > 59 || second > 59 {
            return Err(VALUE_ERROR);
        }
        Ok(ParsedTime {
            date_text,
            seconds: if next_day {
                0
            } else {
                hour * 3600 + minute * 60 + second
            },
            next_day,
        })
    }

    fn fixture_fraction(day: f64, time: &ParsedTime<'_>) -> f64 {
        fraction(day + f64::from(time.next_day), time.seconds, day)
    }

    #[test]
    fn retained_timevalue_and_source_fraction_use_exact_binary64() {
        let time = parse("12:30:15").unwrap();
        assert_eq!(
            fixture_fraction(36526., &time).to_bits(),
            0.5210069444437977_f64.to_bits()
        );
        assert_ne!(
            fixture_fraction(36526., &time).to_bits(),
            (45015_f64 / 86400.).to_bits()
        );
        // POI TestTimeValue's explicit expected value and fractional regression.
        for text in ["12:03:45", "12:03:45.386"] {
            assert_eq!(
                fixture_fraction(36526., &parse(text).unwrap()).to_bits(),
                0.5026041666642413_f64.to_bits()
            );
        }
    }

    #[test]
    fn source_order_tracks_date_exponent_and_preserves_day_rollover() {
        assert_eq!(
            source_day(Date {
                y: 1900,
                m: 2,
                d: 28
            }),
            59.
        );
        assert_eq!(
            source_day(Date {
                y: 1900,
                m: 3,
                d: 1
            }),
            61.
        );
        assert_eq!(
            source_day(Date {
                y: 1899,
                m: 12,
                d: 31
            }),
            0.
        );
        assert_eq!(
            source_day(Date {
                y: 1899,
                m: 12,
                d: 30
            }),
            -1.
        );
        let time = parse("12:30:15").unwrap();
        for day in [59_f64, 61., 36526., 2958465.] {
            let added = day + 45015_f64 / 86400.;
            assert_eq!(
                fixture_fraction(day, &time).to_bits(),
                (added - day).to_bits()
            );
        }
        assert_eq!(
            fixture_fraction(36526., &parse("24:00:00.000").unwrap()).to_bits(),
            1_f64.to_bits()
        );
        assert_eq!(
            fixture_fraction(36526., &parse("12:00 AM").unwrap()).to_bits(),
            0_f64.to_bits()
        );
        assert_eq!(
            fixture_fraction(36526., &parse("12:00 PM").unwrap()).to_bits(),
            0.5_f64.to_bits()
        );
        assert_eq!(
            parse("1/01/2000 6:00 PM").unwrap().date_text,
            Some("1/01/2000")
        );
        for text in [
            "24:00:00.001",
            "24:01",
            "25:00",
            "12:60",
            "12:30:60",
            "1:00 pm",
            "12:03:45.3",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }
}
