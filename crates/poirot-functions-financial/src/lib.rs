//! Rust adaptations of Apache POI (Apache-2.0); class bindings are described below.
//! <https://github.com/apache/poi/tree/732120980140d5ed64b482c470e0b625cdb1ab15/poi/src/main/java/org/apache/poi/ss/formula/functions>
//! Pure financial calculations. Scalar/reference coercion belongs to the evaluator adapter.
//! Algorithms are based on pinned POI FinanceLib, Finance, Irr, Mirr and Rate;
//! formatting follows ExcelArithmetic's 15 significant decimal digits.

/// Spreadsheet failures produced by financial kernels after adapter coercion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinancialError {
    /// An operand cannot satisfy the function's value contract.
    Value,
    /// Numeric domain, overflow, or convergence failure.
    Num,
    /// A financial expression has a zero denominator.
    DivZero,
}
/// Financial kernel result, with spreadsheet value/domain failures in [`FinancialError`].
pub type Result<T> = std::result::Result<T, FinancialError>;
fn checked(x: f64) -> Result<f64> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(FinancialError::Num)
    }
}
fn finite(xs: &[f64]) -> Result<()> {
    if xs.iter().all(|x| x.is_finite()) {
        Ok(())
    } else {
        Err(FinancialError::Num)
    }
}

/// Compute future value for constant-rate periodic payments.
pub fn fv(r: f64, n: f64, payment: f64, present: f64, beginning: bool) -> Result<f64> {
    finite(&[r, n, payment, present])?;
    checked(if r == 0.0 {
        -(present + n * payment)
    } else {
        let power = (1.0 + r).powf(n);
        (1.0 - power) * if beginning { 1.0 + r } else { 1.0 } * payment / r - present * power
    })
}
/// Compute present value for constant-rate periodic payments.
pub fn pv(r: f64, n: f64, payment: f64, future: f64, beginning: bool) -> Result<f64> {
    finite(&[r, n, payment, future])?;
    checked(if r == 0.0 {
        -(n * payment + future)
    } else {
        let power = (1.0 + r).powf(n);
        ((1.0 - power) / r * if beginning { 1.0 + r } else { 1.0 } * payment - future) / power
    })
}
/// Compute a constant periodic payment for an annuity.
pub fn pmt(r: f64, n: f64, present: f64, future: f64, beginning: bool) -> Result<f64> {
    finite(&[r, n, present, future])?;
    checked(if r == 0.0 {
        -(future + present) / n
    } else {
        let power = (1.0 + r).powf(n);
        (future + present * power) * r / (if beginning { 1.0 + r } else { 1.0 } * (1.0 - power))
    })
}
/// Compute the number of payment periods at a constant rate.
pub fn nper(r: f64, payment: f64, present: f64, future: f64, beginning: bool) -> Result<f64> {
    finite(&[r, payment, present, future])?;
    checked(if r == 0.0 {
        -(future + present) / payment
    } else {
        let a = if beginning { 1.0 + r } else { 1.0 } * payment / r;
        let (first, second) = if a - future < 0.0 {
            (future - a, -present - a)
        } else {
            (a - future, present + a)
        };
        (first.ln() - second.ln()) / (1.0 + r).ln()
    })
}
/// POI IPMT/PPMT use integer periods and the four-argument Finance algorithm.
/// This preserves its zero-rate #NUM behavior, rather than silently substituting PMT.
pub fn ipmt(rate: f64, period: i32, periods: i32, present: f64) -> Result<f64> {
    finite(&[rate, present])?;
    let payment = finance_payment(rate, periods, present);
    // Finance.ipmt subtracts in Java's signed integer domain before Finance.fv.
    let b = (1.0 + rate).powf(f64::from(period.wrapping_sub(1)));
    checked(-(present * b + payment * (1.0 + rate * 0.0) * (b - 1.0) / rate) * rate)
}
fn finance_payment(rate: f64, periods: i32, present: f64) -> f64 {
    let a = (1.0 + rate).powf(f64::from(periods));
    // Preserve Finance.pmt expression grouping: present * a can overflow
    // before multiplication by a small rate, even if a regrouped result fits.
    -rate * (present * a + 0.0) / ((1.0 + rate * 0.0) * (a - 1.0))
}
/// Compute the principal component using the pinned payment kernel.
pub fn ppmt(rate: f64, period: i32, periods: i32, present: f64) -> Result<f64> {
    checked(finance_payment(rate, periods, present) - ipmt(rate, period, periods, present)?)
}
/// Documented Excel annuity contract; fractional periods are not truncated.
fn excel_payment_interest(
    rate: f64,
    period: f64,
    periods: f64,
    present: f64,
    future: f64,
    beginning: bool,
) -> Result<(f64, f64)> {
    finite(&[rate, period, periods, present, future])?;
    if period < 1.0 || period > periods {
        return Err(FinancialError::Num);
    }
    let payment = pmt(rate, periods, present, future, beginning)?;
    let interest = if rate == 0.0 || (beginning && period == 1.0) {
        0.0
    } else {
        let interest = fv(rate, period - 1.0, payment, present, beginning)? * rate;
        checked(if beginning {
            interest / (1.0 + rate)
        } else {
            interest
        })?
    };
    Ok((payment, interest))
}
/// Compute the interest component with Excel-documentation period rules.
pub fn ipmt_excel(
    rate: f64,
    period: f64,
    periods: f64,
    present: f64,
    future: f64,
    beginning: bool,
) -> Result<f64> {
    excel_payment_interest(rate, period, periods, present, future, beginning)
        .map(|(_, interest)| interest)
}
/// Compute the principal component with Excel-documentation period rules.
pub fn ppmt_excel(
    rate: f64,
    period: f64,
    periods: f64,
    present: f64,
    future: f64,
    beginning: bool,
) -> Result<f64> {
    let (payment, interest) =
        excel_payment_interest(rate, period, periods, present, future, beginning)?;
    checked(payment - interest)
}
/// Discount periodic cash flows, starting at the first future period.
pub fn npv(rate: f64, flows: &[f64]) -> Result<f64> {
    finite(&[rate])?;
    finite(flows)?;
    let mut denominator = 1.0 + rate;
    let mut value = 0.0;
    for &flow in flows {
        value += flow / denominator;
        denominator *= 1.0 + rate;
    }
    checked(value)
}
fn fraction_convert(number: f64, fraction: f64, to_decimal: bool) -> Result<f64> {
    finite(&[number, fraction])?;
    // Java Double.intValue saturates at the signed integer bounds and truncates.
    let fraction = fraction as i32;
    if fraction < 0 {
        return Err(FinancialError::Num);
    }
    if fraction == 0 {
        return Err(FinancialError::DivZero);
    }
    let scale = 10f64.powi(fraction.to_string().len() as i32);
    let absolute = number.abs();
    let whole = absolute.floor();
    let value = whole
        + if to_decimal {
            (absolute - whole) * scale / f64::from(fraction)
        } else {
            (absolute - whole) * f64::from(fraction) / scale
        };
    checked(if number < 0.0 { -value } else { value })
}
/// Convert a fractional currency quotation to decimal notation.
pub fn dollar_de(number: f64, fraction: f64) -> Result<f64> {
    fraction_convert(number, fraction, true)
}
/// Convert a decimal currency quotation to fractional notation.
pub fn dollar_fr(number: f64, fraction: f64) -> Result<f64> {
    fraction_convert(number, fraction, false)
}

/// Source-compatible MIRR: POI advances the period index for nonzero cash flows only.
pub fn mirr(flows: &[f64], finance_rate: f64, reinvest_rate: f64) -> Result<f64> {
    finite(flows)?;
    finite(&[finance_rate, reinvest_rate])?;
    if flows.iter().all(|&v| v < 0.0) {
        return Ok(-1.0);
    }
    if flows.iter().all(|&v| v > 0.0) {
        return Err(FinancialError::DivZero);
    }
    let years = flows.len() as f64 - 1.0;
    let mut present = 0.0;
    let mut future = 0.0;
    let mut index = 0.0;
    for &flow in flows {
        if flow > 0.0 {
            future += flow * (1.0 + reinvest_rate).powf(years - index);
            index += 1.0;
        } else if flow < 0.0 {
            present += flow / (1.0 + finance_rate).powf(index);
            index += 1.0;
        }
    }
    checked(if future != 0.0 && present != 0.0 {
        (-future / present).powf(1.0 / years) - 1.0
    } else {
        0.0
    })
}

fn irr_value(flows: &[f64], r: f64) -> f64 {
    let y = 1.0 / (1.0 + r);
    let mut p = 1.0;
    let mut value = 0.0;
    for &flow in flows {
        value += flow * p;
        p *= y;
    }
    value
}
fn irr_derivative(flows: &[f64], r: f64) -> f64 {
    let y = 1.0 / (1.0 + r);
    let mut p = y;
    let mut value = 0.0;
    for (k, &flow) in flows.iter().enumerate().skip(1) {
        p *= y;
        value -= k as f64 * flow * p;
    }
    value
}
/// Solve the periodic cash-flow internal rate using the kernel's convergence rules.
pub fn irr(flows: &[f64], guess: f64) -> Result<f64> {
    finite(flows)?;
    finite(&[guess])?;
    if flows.is_empty() {
        return Err(FinancialError::Num);
    }
    let mut x = guess;
    for _ in 0..1000 {
        let factor = 1.0 + x;
        if factor == 0.0 {
            break;
        }
        let mut denominator = factor;
        // SAFETY: The empty-flows guard returns before this first-element access.
        let mut f = flows[0];
        let mut df = 0.0;
        for (k, &v) in flows.iter().enumerate().skip(1) {
            f += v / denominator;
            denominator *= factor;
            df -= k as f64 * v / denominator;
        }
        if df == 0.0 {
            break;
        }
        let next = x - f / df;
        if (next - x).abs() <= 1e-7 {
            if next > -1.0 && next.is_finite() {
                return Ok(next);
            }
            // Source leaves the historical solver immediately on convergence,
            // then invokes the bracketed fallback for an out-of-domain root.
            break;
        }
        x = next;
    }
    let mut grid = Vec::with_capacity(36);
    for e in (2..=9).rev() {
        grid.push(-1.0 + 10f64.powi(-e));
    }
    for i in -9..=10 {
        grid.push(i as f64 / 10.0);
    }
    grid.extend([2.0, 3.0, 5.0, 10.0, 100.0, 1000.0, 10000.0]);
    if guess > -1.0 && guess <= 10000.0 && !grid.contains(&guess) {
        grid.push(guess);
        grid.sort_by(f64::total_cmp);
    }
    let mut previous: Option<(f64, f64)> = None;
    let mut best = None;
    let mut distance = f64::INFINITY;
    for x in grid {
        let f = irr_value(flows, x);
        if f.is_nan() {
            continue;
        }
        if f == 0.0 {
            return Ok(x);
        }
        if let Some((px, pf)) = previous
            && (f < 0.0) != (pf < 0.0)
        {
            let d = if px <= guess && guess <= x {
                0.0
            } else {
                (guess - px).abs().min((guess - x).abs())
            };
            if d < distance {
                distance = d;
                best = Some((px, pf, x));
            }
        }
        previous = Some((x, f));
    }
    let (mut lo, flo, mut hi) = best.ok_or(FinancialError::Num)?;
    if flo > 0.0 {
        std::mem::swap(&mut lo, &mut hi);
    }
    let mut root = if lo.min(hi) < guess && guess < lo.max(hi) {
        guess
    } else {
        0.5 * (lo + hi)
    };
    let mut old_step = (hi - lo).abs();
    let mut step = old_step;
    let mut f = irr_value(flows, root);
    let mut df = irr_derivative(flows, root);
    for _ in 0..1000 {
        let unusable = !f.is_finite()
            || !df.is_finite()
            || df == 0.0
            || ((root - hi) * df - f) * ((root - lo) * df - f) > 0.0
            || (2.0 * f).abs() > (old_step * df).abs();
        old_step = step;
        if unusable {
            step = 0.5 * (hi - lo);
            root = lo + step;
            if root == lo {
                return checked(root);
            }
        } else {
            step = f / df;
            let previous = root;
            root -= step;
            if previous == root {
                return checked(root);
            }
        }
        if step.abs() < 1e-7 {
            return checked(root);
        }
        f = irr_value(flows, root);
        df = irr_derivative(flows, root);
        if f < 0.0 {
            lo = root;
        } else if f > 0.0 {
            hi = root;
        }
    }
    Err(FinancialError::Num)
}
/// POI's pinned RATE Newton quotient, including its precise expression grouping.
pub fn rate(
    periods: f64,
    payment: f64,
    present: f64,
    future: f64,
    payment_type: f64,
    guess: f64,
) -> Result<f64> {
    finite(&[periods, payment, present, future, payment_type, guess])?;
    let mut r = guess;
    for _ in 0..100 {
        let a = (1.0 + r).powf(periods);
        let b = (1.0 + r).powf(periods - 1.0);
        let quotient = (future + a * present + payment * (a - 1.0) * (r * payment_type + 1.0) / r)
            / (periods * b * present
                - payment * (a - 1.0) * (r * payment_type + 1.0)
                    / (r * r
                        + periods * payment * b * (r * payment_type + 1.0) / r
                        + payment * (a - 1.0) * payment_type / r));
        let next = r - quotient;
        if (next - r).abs() < 1e-8 {
            return checked(next);
        }
        r = next;
    }
    Err(FinancialError::Num)
}

/// Currency formatting is an explicit port: DOLLAR follows the user's locale.
/// The caller supplies the resolved locale's separators, grouping and affixes.
#[derive(Debug, Clone, Copy)]
pub struct CurrencyFormat<'a> {
    /// Separator placed between integral and fractional digits.
    pub decimal_separator: &'a str,
    /// Separator placed between integer digit groups.
    pub grouping_separator: &'a str,
    /// Size of the rightmost integer digit group.
    pub primary_group: usize,
    /// Size of subsequent groups to the left.
    pub secondary_group: usize,
    /// Text prepended to nonnegative formatted amounts.
    pub positive_prefix: &'a str,
    /// Text appended to nonnegative formatted amounts.
    pub positive_suffix: &'a str,
    /// Text prepended to negative formatted amounts.
    pub negative_prefix: &'a str,
    /// Text appended to negative formatted amounts.
    pub negative_suffix: &'a str,
}
impl CurrencyFormat<'static> {
    /// US dollar affixes, decimal point, and three-digit grouping.
    pub const US: Self = Self {
        decimal_separator: ".",
        grouping_separator: ",",
        primary_group: 3,
        secondary_group: 3,
        positive_prefix: "$",
        positive_suffix: "",
        negative_prefix: "($",
        negative_suffix: ")",
    };
}
fn decimal_parts(number: f64, places: i32) -> Result<(bool, String, String)> {
    if !number.is_finite() || !(-400..=127).contains(&places) {
        return Err(FinancialError::Num);
    }
    // ExcelArithmetic.toBigDecimal uses exactly NumberToTextConverter.toText.
    // Normalize that source-backed decimal without a second float conversion.
    // TODO(coercion-non-argument): [LOW] Formatting rounds through numeric text before producing output; boundary values can lose significant digits.
    let visible = poirot_coercionv2::compatibility::coercev2_number_to_text(number.abs());
    if visible.len() > 1024 {
        return Err(FinancialError::Num);
    }
    let (mantissa, power) = visible
        .split_once('E')
        .or_else(|| visible.split_once('e'))
        .unwrap_or((&visible, "0"));
    let power: i32 = power.parse().map_err(|_| FinancialError::Num)?;
    if !(-400..=400).contains(&power) {
        return Err(FinancialError::Num);
    }
    let before_dot = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let raw: Vec<u8> = mantissa.bytes().filter(|&c| c != b'.').collect();
    if raw.is_empty() || raw.iter().any(|byte| !byte.is_ascii_digit()) {
        return Err(FinancialError::Num);
    }
    let first = raw.iter().position(|&c| c != b'0').unwrap_or(raw.len() - 1);
    let exponent = power + before_dot - first as i32 - 1;
    // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
    let digits = raw[first..].to_vec();
    let position = exponent + 1 + places;
    let mut scaled = if position <= 0 {
        Vec::new()
    } else {
        // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
        let mut result = digits[..(position as usize).min(digits.len())].to_vec();
        result.resize(position as usize, b'0');
        result
    };
    let round_up =
        // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
        position >= 0 && (position as usize) < digits.len() && digits[position as usize] >= b'5';
    if round_up {
        let mut index = scaled.len();
        loop {
            if index == 0 {
                // SAFETY: Index zero is within or at the end of every vector.
                scaled.insert(0, b'1');
                break;
            }
            index -= 1;
            // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
            if scaled[index] < b'9' {
                // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
                scaled[index] += 1;
                break;
            }
            // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
            scaled[index] = b'0';
        }
    }
    if scaled.is_empty() {
        scaled.push(b'0');
    }
    let nonzero = scaled.iter().any(|&c| c != b'0');
    if places < 0 && nonzero {
        scaled.resize(scaled.len() + (-places) as usize, b'0');
    }
    let (integer, fraction) = if places > 0 {
        let places = places as usize;
        if scaled.len() <= places {
            let mut padded = vec![b'0'; places + 1 - scaled.len()];
            padded.extend(scaled);
            scaled = padded;
        }
        let at = scaled.len() - places;
        (
            // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
            String::from_utf8(scaled[..at].to_vec()).unwrap(),
            // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
            String::from_utf8(scaled[at..].to_vec()).unwrap(),
        )
    } else {
        // SAFETY: raw is checked to be nonempty ASCII digits; prefix/suffix endpoints are clamped or derived from scaled.len(), and carry index is decremented only after a positive check. All constructed bytes remain ASCII digits.
        (String::from_utf8(scaled).unwrap(), String::new())
    };
    Ok((number < 0.0 && nonzero, integer, fraction))
}
fn group_integer(integer: &str, separator: &str, primary: usize, secondary: usize) -> String {
    if primary == 0 || secondary == 0 || !integer.is_ascii() {
        return integer.to_owned();
    }
    let mut splits = Vec::new();
    let mut at = integer.len();
    let mut width = primary;
    while at > width {
        at -= width;
        splits.push(at);
        width = secondary;
    }
    splits.reverse();
    let mut output = String::new();
    let mut previous = 0;
    for split in splits {
        // SAFETY: The entry guard requires ASCII; split positions decrease from integer.len() and are visited in ascending order, so every slice boundary is valid.
        output.push_str(&integer[previous..split]);
        output.push_str(separator);
        previous = split;
    }
    // SAFETY: The entry guard requires ASCII; split positions decrease from integer.len() and are visited in ascending order, so every slice boundary is valid.
    output.push_str(&integer[previous..]);
    output
}
/// Format a number with fixed decimal places and optional grouping.
pub fn fixed(number: f64, places: i32, no_commas: bool) -> Result<String> {
    finite(&[number])?;
    if places > 127 {
        return Err(FinancialError::Value);
    }
    let places = places.max(-400);
    let (negative, integer, fraction) = decimal_parts(number, places)?;
    let integer = if no_commas {
        integer
    } else {
        group_integer(&integer, ",", 3, 3)
    };
    Ok(format!(
        "{}{}{}{}",
        if negative { "-" } else { "" },
        integer,
        if places > 0 { "." } else { "" },
        fraction
    ))
}
/// Format a rounded currency amount with the supplied locale layout.
pub fn dollar(number: f64, places: f64, context: &CurrencyFormat<'_>) -> Result<String> {
    finite(&[number, places])?;
    if places > 127.0 {
        return Err(FinancialError::Value);
    }
    if places < f64::from(i32::MIN) {
        return Err(FinancialError::Num);
    }
    let places = (places as i32).max(-400);
    let (negative, integer, fraction) = decimal_parts(number, places)?;
    let integer = group_integer(
        &integer,
        context.grouping_separator,
        context.primary_group,
        context.secondary_group,
    );
    let (prefix, suffix) = if negative {
        (context.negative_prefix, context.negative_suffix)
    } else {
        (context.positive_prefix, context.positive_suffix)
    };
    Ok(format!(
        "{}{}{}{}{}",
        prefix,
        integer,
        if places > 0 {
            context.decimal_separator
        } else {
            ""
        },
        fraction,
        suffix
    )
    .replace('\u{00a0}', " "))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(actual: Result<f64>, expected: f64) {
        let value = actual.unwrap();
        assert!(
            (value - expected).abs() < 1e-7 * expected.abs().max(1.0),
            "{value} != {expected}"
        );
    }
    #[test]
    fn zero_interest_and_inverse_annuity() {
        near(fv(0.0, 10.0, -20.0, -100.0, false), 300.0);
        near(pv(0.0, 10.0, -20.0, 300.0, false), -100.0);
        near(pmt(0.0, 10.0, 100.0, 100.0, false), -20.0);
        near(nper(0.0, -20.0, 100.0, 100.0, false), 10.0);
        for begin in [false, true] {
            let payment = pmt(0.01, 36.0, 5000.0, 0.0, begin).unwrap();
            near(fv(0.01, 36.0, payment, 5000.0, begin), 0.0);
            near(pv(0.01, 36.0, payment, 0.0, begin), 5000.0);
            near(nper(0.01, payment, 5000.0, 0.0, begin), 36.0);
        }
    }
    #[test]
    fn payment_parts_and_source_zero_rate() {
        near(ipmt(0.01, 1, 12, 1000.0), -10.0);
        near(ppmt(0.01, 1, 12, 1000.0), -78.8487886783);
        assert_eq!(ipmt(0.0, 1, 12, 1000.0), Err(FinancialError::Num));
        assert_eq!(pmt(0.0, 0.0, 100.0, 0.0, false), Err(FinancialError::Num));
        assert_eq!(ipmt(0.1, 1, 10, 1e308), Err(FinancialError::Num));
        assert_eq!(ppmt(0.1, 1, 10, 1e308), Err(FinancialError::Num));
        // Java's per - 1 wraps MIN_VALUE to MAX_VALUE: the power overflows,
        // instead of underflowing toward zero at a negative exponent.
        assert_eq!(ipmt(0.1, i32::MIN, 10, 100.0), Err(FinancialError::Num));
    }
    #[test]
    fn documented_payment_parts_follow_cash_balance_recurrence() {
        // Independent amortization recurrence: interest is minus the balance
        // times rate; end payments then reduce the balance. Beginning payments
        // precede accrual, so their first interest component is zero.
        for rate in [0.0, 0.000001, -0.01, 0.1] {
            for present in [0.0, 1000.0] {
                for future in [0.0, 200.0] {
                    for beginning in [false, true] {
                        let payment = pmt(rate, 5.0, present, future, beginning).unwrap();
                        let mut balance = present;
                        for period in 1..=5 {
                            let interest = if beginning && period == 1 {
                                0.0
                            } else {
                                -balance * rate
                            };
                            near(
                                ipmt_excel(rate, period as f64, 5.0, present, future, beginning),
                                interest,
                            );
                            near(
                                ppmt_excel(rate, period as f64, 5.0, present, future, beginning),
                                payment - interest,
                            );
                            balance += payment - interest;
                        }
                        // Beginning annuities have one final interest accrual.
                        if beginning {
                            balance *= 1.0 + rate;
                        }
                        assert!((balance + future).abs() < 1e-6);
                    }
                }
            }
        }
        for period in [0.0, -1.0, 6.0] {
            assert_eq!(
                ipmt_excel(0.0, period, 5.0, 0.0, 0.0, true),
                Err(FinancialError::Num)
            );
            assert_eq!(
                ppmt_excel(0.1, period, 5.0, 1000.0, 0.0, false),
                Err(FinancialError::Num)
            );
        }
        near(ipmt_excel(0.1, 1.0, 2.0, 0.0, 4.0, false), 0.0);
        near(ipmt_excel(0.01, 1.0, 1.4, 1000.0, 0.0, false), -10.0);
    }
    #[test]
    fn discounted_flows_and_fraction_conversion() {
        near(npv(0.1, &[100.0, 100.0]), 173.55371900826447);
        near(dollar_de(1.02, 16.9), 1.125);
        near(dollar_fr(-1.125, 16.0), -1.02);
        near(dollar_de(3.5, 10.0), 8.0); // POI uses decimal digit length, including powers of ten.
        assert_eq!(dollar_de(1.0, 0.9), Err(FinancialError::DivZero));
        assert_eq!(dollar_fr(1.0, -1.0), Err(FinancialError::Num));
    }
    #[test]
    fn iterative_roots_and_no_root() {
        near(irr(&[-100.0, 110.0], 0.1), 0.1);
        near(irr(&[-100.0, 1.0], 0.1), -0.99);
        near(irr(&[-100.0, 230.0, -132.0], 0.09), 0.1);
        near(irr(&[-100.0, 230.0, -132.0], 0.21), 0.2);
        assert_eq!(irr(&[100.0, 200.0], 0.1), Err(FinancialError::Num));
        assert_eq!(irr(&[], 0.1), Err(FinancialError::Num));
        near(
            rate(10.0, 0.0, -100.0, 200.0, 0.0, 0.1),
            2f64.powf(0.1) - 1.0,
        );
        assert_eq!(rate(1.0, 0.0, 0.0, 0.0, 0.0, 0.1), Err(FinancialError::Num));
    }
    #[test]
    fn mirr_pinned_edge_behavior() {
        near(mirr(&[-100.0, 121.0], 0.1, 0.1), 0.21);
        near(mirr(&[-10.0, -20.0], 0.1, 0.1), -1.0);
        assert_eq!(mirr(&[10.0, 20.0], 0.1, 0.1), Err(FinancialError::DivZero));
        near(mirr(&[0.0, 0.0], 0.1, 0.1), 0.0);
    }
    #[test]
    fn decimal_rounding_and_context() {
        assert_eq!(fixed(2.675, 2, false).unwrap(), "2.68");
        assert_eq!(fixed(0.1 + 0.2, 17, true).unwrap(), "0.30000000000000000");
        assert_eq!(fixed(-1550.0, -2, false).unwrap(), "-1,600");
        assert_eq!(
            dollar(-1550.0, -2.0, &CurrencyFormat::US).unwrap(),
            "($1,600)"
        );
        assert_eq!(fixed(-0.001, 2, false).unwrap(), "0.00");
        assert_eq!(fixed(1.0, 128, false), Err(FinancialError::Value));
        assert_eq!(fixed(1e308, -400, false).unwrap(), "0");
        assert_eq!(fixed(0.0, 127, true).unwrap().len(), 129);
        let eur = CurrencyFormat {
            decimal_separator: ",",
            grouping_separator: ".",
            primary_group: 3,
            secondary_group: 3,
            positive_prefix: "",
            positive_suffix: "\u{a0}€",
            negative_prefix: "-",
            negative_suffix: "\u{a0}€",
        };
        assert_eq!(dollar(1234.56, 2.0, &eur).unwrap(), "1.234,56 €");
    }
}
