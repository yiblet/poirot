use super::*;
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
/// Deliberate serial-60 conversion, independent of host calendars and clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhantomDayPolicy {
    /// Preserve Excel's fictional 1900-02-29 for display.
    DisplayFebruary29,
    /// POI DateUtil Gregorian conversion maps serial 60 to March 1.
    GregorianMarch1,
}
/// Selects how numeric date serials become display calendar dates.
/// Only finite, nonnegative serials through 9999-12-31 are admitted; the epoch and
/// fictional 1900 leap-day choice remain explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarPolicy {
    /// Workbook epoch and serial-day convention.
    pub date_system: DateSystem,
    /// How to display Excel's fictional 1900 leap day.
    pub phantom_day: PhantomDayPolicy,
}
impl CalendarPolicy {
    /// Construct the calendar policy used for spreadsheet display.
    pub const fn display(date_system: DateSystem) -> Self {
        Self {
            date_system,
            phantom_day: PhantomDayPolicy::DisplayFebruary29,
        }
    }
    /// Return the largest serial day admitted by this calendar policy.
    pub const fn maximum_day(self) -> i64 {
        match self.date_system {
            DateSystem::Excel1900 => 2_958_465,
            DateSystem::Excel1904 => 2_957_003,
        }
    }
    /// Reject serial values outside supported calendar/display bounds.
    pub fn validate(self, serial: f64) -> Result<(), FormatError> {
        if !serial.is_finite() || serial < 0. || serial >= (self.maximum_day() + 1) as f64 {
            Err(error(
                "date serial outside supported nonnegative calendar range",
            ))
        } else {
            Ok(())
        }
    }
    /// Convert an admitted serial day to calendar fields.
    pub fn date(self, serial: f64) -> Result<CalendarDate, FormatError> {
        self.validate(serial)?;
        Ok(self.day(serial.floor() as i64))
    }
    fn day(self, day: i64) -> CalendarDate {
        if self.date_system == DateSystem::Excel1900
            && day == 60
            && self.phantom_day == PhantomDayPolicy::DisplayFebruary29
        {
            return CalendarDate {
                year: 1900,
                month: 2,
                day: 29,
                weekday: 4,
                phantom: true,
            };
        }
        let days = if self.date_system == DateSystem::Excel1904 {
            day - 24107
        } else {
            day - 25568 - i64::from(day > 60)
        };
        let (year, month, date) = civil(days);
        CalendarDate {
            year,
            month,
            day: date,
            weekday: (days + 4).rem_euclid(7) as usize,
            phantom: false,
        }
    }
}
/// Calendar date produced from a serial for date-format rendering, including Excel's fictional leap day.
/// Weekdays are zero-based from Sunday; the phantom day retains the legacy Thursday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarDate {
    /// Displayed calendar year.
    pub year: i64,
    /// One-based calendar month.
    pub month: u32,
    /// One-based day of month.
    pub day: u32,
    /// Weekday index used by the date formatter.
    pub weekday: usize,
    /// Whether this represents Excel's fictional 1900-02-29.
    pub phantom: bool,
}
/// Compiled date/time tokens reused across value-dependent renders.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DatePlan {
    tokens: Vec<Token>,
    fractional_digits: usize,
    system_selector: Option<SystemDateTime>,
    system_tokens: Option<Vec<Token>>,
}
impl DatePlan {
    /// Validate date/time tokens and retain their rendering program.
    pub(super) fn compile(tokens: &[Token]) -> Result<Self, FormatError> {
        let mut system_selector = None;
        for token in tokens {
            if let Token::SystemDateTime(selector) = token
                && system_selector.replace(*selector).is_some()
            {
                return Err(error("multiple system date/time selectors"));
            }
        }
        if system_selector.is_some()
            && (!matches!(tokens.first(), Some(Token::SystemDateTime(_)))
                || tokens.iter().any(|t| {
                    matches!(
                        t,
                        Token::Digit(_)
                            | Token::General
                            | Token::Elapsed(..)
                            | Token::Text
                            | Token::Exponent { .. }
                            | Token::Percent
                    )
                }))
        {
            return Err(error("unsupported system date/time fallback layout"));
        }
        // SAFETY: The window width is the nonzero literal 2.
        let fractional_digits = tokens
            .windows(2)
            .enumerate()
            // SAFETY: windows(2) supplies two tokens; its enumerated start plus two is at most tokens.len().
            .filter(|(_, w)| matches!(w[0], Token::Date('s', _)) && matches!(w[1], Token::Decimal))
            .map(|(index, _)| {
                // SAFETY: windows(2) supplies two tokens; its enumerated start plus two is at most tokens.len().
                tokens[index + 2..]
                    .iter()
                    .take_while(|t| matches!(t, Token::Digit('0')))
                    .count()
            })
            .max()
            .unwrap_or(0);
        if fractional_digits > 6 {
            return Err(error("more than six fractional-second digits unsupported"));
        }
        Ok(Self {
            tokens: tokens.to_vec(),
            fractional_digits,
            system_selector,
            system_tokens: system_selector.map(system::configured_tokens),
        })
    }
    /// Render a serial with the selected epoch, locale, and resource policy.
    pub(super) fn render(
        &self,
        serial: f64,
        system: DateSystem,
        locale: FormatLocale,
    ) -> Result<RenderedText, FormatError> {
        if let Some(selector) = self.system_selector {
            if !matches!(locale, FormatLocale::English) {
                return Err(FormatError {
                    kind: FormatErrorKind::UnsupportedFidelity,
                    position: 0,
                    message: "unsupported system date/time locale".into(),
                });
            }
            // Policy resolution is value-independent, but occurs at rendering.
            // Retain the original fallback tokens in the immutable plan.
            let tokens = self
                .system_tokens
                .as_deref()
                .ok_or_else(|| error("missing compiled system pattern"))?;
            // Date-only display must not carry into tomorrow when seconds round.
            let selected_serial = match selector {
                SystemDateTime::LongDate => serial.floor(),
                SystemDateTime::LongTime => serial,
            };
            return render_plan(selected_serial, tokens, system, 0);
        }
        render_plan(serial, &self.tokens, system, self.fractional_digits)
    }
}
fn render_plan(
    serial: f64,
    tokens: &[Token],
    system: DateSystem,
    fractional_digits: usize,
) -> Result<RenderedText, FormatError> {
    if fractional_digits > 6 {
        return Err(error("more than six fractional-second digits unsupported"));
    }
    let policy = CalendarPolicy::display(system);
    policy.validate(serial)?;
    // SAFETY: The local check restricts fractional_digits to 0..=6; 10^6 fits u64.
    let factor = 10u64.pow(fractional_digits as u32);
    // Round only the day fraction: multiplying the entire serial loses microseconds
    // near year 9999. Reject a carry beyond the declared calendar boundary.
    let mut day = serial.floor() as i64;
    let ticks = ((serial - serial.floor()) * 86400. * factor as f64).round() as u64;
    let day_ticks = 86400 * factor;
    day += (ticks / day_ticks) as i64;
    if day > policy.maximum_day() {
        return Err(error("rounded date exceeds supported calendar range"));
    }
    let fraction = ticks % factor;
    let time = ticks % day_ticks / factor;
    let seconds = day as u64 * 86400 + time;
    let hour = time / 3600;
    let minute = time % 3600 / 60;
    let second = time % 60;
    let CalendarDate {
        year,
        month,
        day: date,
        weekday,
        ..
    } = policy.day(day);
    let ampm = tokens.iter().any(|t| matches!(t, Token::AmPm(_)));
    let mut out = RenderedText::default();
    let mut i = 0;
    while i < tokens.len() {
        // SAFETY: The enclosing loop checks i < tokens.len(); the lookahead additionally checks i + 1 < tokens.len().
        match &tokens[i] {
            Token::Date(c, width) => {
                let w = *width;
                match c {
                    'y' => out.push_str(&if w <= 2 {
                        format!("{:02}", year.rem_euclid(100))
                    } else {
                        format!("{year:04}")
                    }),
                    'd' => out.push_str(&match w {
                        1 => date.to_string(),
                        2 => format!("{date:02}"),
                        3 => DAYS
                            .get(weekday)
                            .and_then(|day| day.get(..3))
                            .ok_or_else(|| error("invalid weekday"))?
                            .into(),
                        _ => DAYS
                            .get(weekday)
                            .ok_or_else(|| error("invalid weekday"))?
                            .to_string(),
                    }),
                    'm' => {
                        // SAFETY: The enclosing loop checks i < tokens.len(); the lookahead additionally checks i + 1 < tokens.len().
                        let before = tokens[..i]
                            .iter()
                            .rev()
                            .find(|t| matches!(t, Token::Date(..) | Token::Elapsed(..)));
                        // SAFETY: The enclosing loop checks i < tokens.len(); the lookahead additionally checks i + 1 < tokens.len().
                        let after = tokens[i + 1..]
                            .iter()
                            .find(|t| matches!(t, Token::Date(..) | Token::Elapsed(..)));
                        let minutes =
                            matches!(before, Some(Token::Date('h', _) | Token::Elapsed('h', _)))
                                || matches!(
                                    after,
                                    Some(Token::Date('s', _) | Token::Elapsed('s', _))
                                );
                        if minutes {
                            out.push_str(&if w == 1 {
                                minute.to_string()
                            } else {
                                format!("{minute:02}")
                            })
                        } else {
                            out.push_str(&match w {
                                1 => month.to_string(),
                                2 => format!("{month:02}"),
                                3 => MONTHS
                                    .get((month as usize).wrapping_sub(1))
                                    .and_then(|month| month.get(..3))
                                    .ok_or_else(|| error("invalid month"))?
                                    .into(),
                                4 => MONTHS
                                    .get((month as usize).wrapping_sub(1))
                                    .ok_or_else(|| error("invalid month"))?
                                    .to_string(),
                                _ => MONTHS
                                    .get((month as usize).wrapping_sub(1))
                                    .and_then(|month| month.get(..1))
                                    .ok_or_else(|| error("invalid month"))?
                                    .into(),
                            })
                        }
                    }
                    'h' => {
                        let h = if ampm {
                            if hour.is_multiple_of(12) {
                                12
                            } else {
                                hour % 12
                            }
                        } else {
                            hour
                        };
                        out.push_str(&if w == 1 {
                            h.to_string()
                        } else {
                            format!("{h:02}")
                        })
                    }
                    's' => {
                        out.push_str(&if w == 1 {
                            second.to_string()
                        } else {
                            format!("{second:02}")
                        });
                        if i + 1 < tokens.len()
                            // SAFETY: The enclosing loop checks i < tokens.len(); the lookahead additionally checks i + 1 < tokens.len().
                            && matches!(tokens[i + 1], Token::Decimal)
                            && fractional_digits > 0
                        {
                            out.push_str(&format!(".{fraction:0fractional_digits$}"));
                            i += fractional_digits + 1
                        }
                    }
                    _ => return Err(error("unknown date token")),
                }
            }
            Token::Elapsed(c, width) => out.push_str(&format!(
                "{:0width$}",
                match c {
                    'h' => seconds / 3600,
                    'm' => seconds / 60,
                    _ => seconds,
                }
            )),
            Token::AmPm(full) => out.push_str(if hour < 12 {
                if *full { "AM" } else { "A" }
            } else if *full {
                "PM"
            } else {
                "P"
            }),
            Token::Digit(_) => return Err(error("numeric placeholders mixed with date format")),
            token => out.push_literal(token),
        }
        i += 1
    }
    Ok(out)
}
// Gregorian calendar conversion from days relative to 1970-01-01.
// Gregorian conversion adapts Howard Hinnant's days_from_civil/civil_from_days.
// MIT reference: HowardHinnant/date 5bdb7e6f31fac909c090a46dbd9fea27b6e609a4,
// include/date/date.h; see root LICENSES/ACKNOWLEDGEMENTS.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m as u32, d as u32)
}
