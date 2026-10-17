//! Lazy financial execution over engine-owned references and validated arrays.
/// Provenance used by financial collection coercion.
pub use poirot_functions_core::Origin;
/// Scalar representation accepted by financial helpers.
pub use poirot_functions_core::Scalar;
use poirot_functions_financial as domain;
use poirot_model::CellError;

mod coercionv2;
mod collections;
mod formatting;
#[cfg(test)]
mod tests;

#[cfg(test)]
use coercionv2::coercev2_financial_decimal_text;
use coercionv2::{
    coercev2_financial_argument_number, coercev2_financial_floor_i32,
    coercev2_financial_optional_number, coercev2_financial_scalar_number,
    coercev2_financial_text_number, resolve_operands, work,
};
use collections::coercev2_financial_cashflows;

/// Cash flow, loan, and monetary formatting operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinancialFunction {
    /// Future value.
    Fv,
    /// Present value.
    Pv,
    /// Periodic payment.
    Pmt,
    /// Number of payment periods.
    Nper,
    /// Interest portion of a payment.
    Ipmt,
    /// Principal portion of a payment.
    Ppmt,
    /// Interest rate per period.
    Rate,
    /// Net present value of cash flows.
    Npv,
    /// Internal rate of return.
    Irr,
    /// Modified internal rate of return.
    Mirr,
    /// Convert fractional dollar notation to decimal.
    DollarDe,
    /// Convert decimal dollars to fractional notation.
    DollarFr,
    /// Format a fixed number of decimal places.
    Fixed,
    /// Format currency using explicit host settings.
    Dollar,
}
/// Financial adapter failures preserving the host's operational errors.
pub type Failure<O> = crate::adapters::failure::AdapterFailure<O, &'static str>;
/// Numeric calculation or rendered monetary text.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Numeric financial result.
    Number(f64),
    /// Rendered currency or fixed-decimal result.
    Text(String),
}

/// Owned user-locale descriptor: no leaked or assumed static locale strings.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CurrencyContext {
    /// Separator between integer and fractional digits.
    pub decimal_separator: String,
    /// Separator inserted between integer digit groups.
    pub grouping_separator: String,
    /// Size of the rightmost integer digit group.
    pub primary_group: usize,
    /// Size of subsequent integer digit groups.
    pub secondary_group: usize,
    /// Affix preceding nonnegative amounts.
    pub positive_prefix: String,
    /// Affix following nonnegative amounts.
    pub positive_suffix: String,
    /// Affix preceding negative amounts.
    pub negative_prefix: String,
    /// Affix following negative amounts.
    pub negative_suffix: String,
}
impl CurrencyContext {
    /// Borrow these owned settings as the financial helper's formatting descriptor.
    pub fn as_format(&self) -> domain::CurrencyFormat<'_> {
        domain::CurrencyFormat {
            decimal_separator: &self.decimal_separator,
            grouping_separator: &self.grouping_separator,
            primary_group: self.primary_group,
            secondary_group: self.secondary_group,
            positive_prefix: &self.positive_prefix,
            positive_suffix: &self.positive_suffix,
            negative_prefix: &self.negative_prefix,
            negative_suffix: &self.negative_suffix,
        }
    }
}
impl From<domain::CurrencyFormat<'_>> for CurrencyContext {
    fn from(f: domain::CurrencyFormat<'_>) -> Self {
        Self {
            decimal_separator: f.decimal_separator.into(),
            grouping_separator: f.grouping_separator.into(),
            primary_group: f.primary_group,
            secondary_group: f.secondary_group,
            positive_prefix: f.positive_prefix.into(),
            positive_suffix: f.positive_suffix.into(),
            negative_prefix: f.negative_prefix.into(),
            negative_suffix: f.negative_suffix.into(),
        }
    }
}
/// Shape inspection must not evaluate cells. Indexed access is lazy, sheet-first
/// and row-major; direct scalars have length one. Invalid rectangular arrays are
/// rejected by the engine before exposing a length. All port faults stay typed.
/// Returned owned Text must be checked through check_text BEFORE cloning or
/// allocating in the engine. Collection lengths are checked without cell reads;
/// scalar resolution uses OperandResolver caller-coordinate selection, never
/// eagerly traverses a reference. RefEval sheet selection is source-specific.
pub trait EvaluationPort {
    /// Deferred engine-owned operand handle.
    type Argument;
    /// Host operational failure type.
    type Operational;
    /// Resolve the selected scalar without traversing the whole reference.
    fn resolve_scalar(
        &mut self,
        arg: &Self::Argument,
    ) -> Result<Scalar, Failure<Self::Operational>>;
    /// Inspect collection length without evaluating its members.
    fn collection_len(&mut self, arg: &Self::Argument)
    -> Result<usize, Failure<Self::Operational>>;
    /// Read one collection member in sheet-first, row-major order.
    fn collection_cell(
        &mut self,
        arg: &Self::Argument,
        index: usize,
    ) -> Result<(Scalar, Origin), Failure<Self::Operational>>;
    /// Admit work before performing it.
    fn charge_work(&mut self, units: u64) -> Result<(), Self::Operational>;
    /// Check collection capacity before allocation.
    fn check_collection(&mut self, elements: usize) -> Result<(), Self::Operational>;
    /// Check text capacity before copying or allocating.
    fn check_text(&mut self, bytes: usize) -> Result<(), Self::Operational>;
    /// Engine Excel numeric grammar/date-time fallback, after a text budget check.
    fn coercev2_text_number(&mut self, text: &str) -> Result<f64, Failure<Self::Operational>>;
    /// Apply the host's explicit numeric coercion for an omitted argument.
    fn coercev2_missing_number(&mut self) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            "MissingArgEval numeric coercion",
        ))
    }
    /// Locale discovery is explicit: a non-US context must never default to US.
    fn currency_format(&mut self) -> Result<CurrencyContext, Failure<Self::Operational>>;
}
fn domain_error<O>(e: domain::FinancialError) -> Failure<O> {
    Failure::Spreadsheet(match e {
        domain::FinancialError::Value => CellError::Value,
        domain::FinancialError::Num => CellError::Num,
        domain::FinancialError::DivZero => CellError::Div0,
    })
}
/// Select the documented Excel contract without changing the pinned entry point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticsPolicy {
    /// Pinned Apache POI behavior.
    PinnedPoi,
    /// Documented Excel behavior.
    ExcelDocumentation,
}
impl FinancialFunction {
    /// Check pinned argument-count rules without resolving operands.
    pub const fn accepts(self, n: usize) -> bool {
        use FinancialFunction::*;
        match self {
            Fv | Pv | Pmt | Nper => n >= 3 && n <= 5,
            Ipmt => n == 4,
            Ppmt => n >= 4,
            Rate => n >= 3,
            Npv => n >= 2,
            Irr => n == 1 || n == 2,
            Mirr => n <= 3,
            DollarDe | DollarFr => n == 2,
            Fixed => n >= 1 && n <= 3,
            Dollar => n == 1 || n == 2,
        }
    }
    /// Check argument-count rules under the selected compatibility policy.
    pub const fn accepts_with_policy(self, n: usize, policy: SemanticsPolicy) -> bool {
        if matches!(policy, SemanticsPolicy::ExcelDocumentation)
            && matches!(self, Self::Ipmt | Self::Ppmt)
        {
            n >= 4 && n <= 6
        } else {
            self.accepts(n)
        }
    }
}
/// Reject arity before resolution, and never resolve ignored PPMT/RATE extras.
pub fn evaluate<P: EvaluationPort>(
    p: &mut P,
    f: FinancialFunction,
    args: &[P::Argument],
) -> Result<Output, Failure<P::Operational>> {
    evaluate_with_policy(p, f, args, SemanticsPolicy::PinnedPoi)
}
/// Execute a financial operation under explicitly selected compatibility rules.
pub fn evaluate_with_policy<P: EvaluationPort>(
    p: &mut P,
    f: FinancialFunction,
    args: &[P::Argument],
    policy: SemanticsPolicy,
) -> Result<Output, Failure<P::Operational>> {
    use FinancialFunction::*;
    if !f.accepts_with_policy(args.len(), policy) {
        return Err(CellError::Value.into());
    }
    work(p, 1)?;
    let result = match f {
        Npv => {
            let r = coercev2_financial_argument_number(p, args.first().ok_or(CellError::Value)?)?;
            let v = coercev2_financial_cashflows(p, args.get(1..).ok_or(CellError::Value)?)?;
            work(p, v.len() as u64)?;
            domain::npv(r, &v)
        }
        Irr => {
            let v = coercev2_financial_cashflows(p, args.get(..1).ok_or(CellError::Value)?)?;
            let g = if args.len() > 1 {
                coercev2_financial_argument_number(p, args.get(1).ok_or(CellError::Value)?)?
            } else {
                0.1
            };
            if !v.is_empty() {
                // Rust kernel: <=1000*n historical visits, <=36*n grid
                // visits, <=2*n initial value/derivative visits and
                // <=2000*n safeguarded visits. Reserve a further 4096 units
                // for loop control, grid construction and sorting (<=36
                // entries). Storage is checked before the kernel allocates.
                p.check_collection(36).map_err(Failure::Operational)?;
                work(
                    p,
                    (v.len() as u64).saturating_mul(3038).saturating_add(4096),
                )?;
            }
            domain::irr(&v, g)
        }
        Mirr => {
            let mut v = coercev2_financial_cashflows(p, args)?;
            if v.len() < 2 {
                return Err(CellError::Value.into());
            }
            let r = v.pop().ok_or(CellError::Value)?;
            let q = v.pop().ok_or(CellError::Value)?;
            work(p, v.len() as u64)?;
            domain::mirr(&v, q, r)
        }
        DollarDe | DollarFr => {
            let n = coercev2_financial_text_number(p, args.first().ok_or(CellError::Value)?)?;
            let d = coercev2_financial_text_number(p, args.get(1).ok_or(CellError::Value)?)?;
            if f == DollarDe {
                domain::dollar_de(n, d)
            } else {
                domain::dollar_fr(n, d)
            }
        }
        Fixed | Dollar => return formatting::evaluate(p, f, args),
        Ipmt | Ppmt if policy == SemanticsPolicy::ExcelDocumentation => {
            // Preserve financial resolve-before-coerce ordering, including fv/type.
            let mut values = resolve_operands(p, args)?.into_iter();
            let mut numbers = [0.0; 6];
            for number in &mut numbers {
                *number = poirot_coercionv2::compatibility::coercev2_financial_payment_number(
                    values.next(),
                    |value| coercev2_financial_scalar_number(p, value),
                )?;
            }
            let [rate, period, periods, present, future, timing] = numbers;
            let beginning =
                poirot_coercionv2::compatibility::coercev2_financial_payment_timing(timing)?;
            if f == Ipmt {
                domain::ipmt_excel(rate, period, periods, present, future, beginning)
            } else {
                domain::ppmt_excel(rate, period, periods, present, future, beginning)
            }
        }
        Ipmt | Ppmt => {
            let mut values =
                resolve_operands(p, args.get(..4).ok_or(CellError::Value)?)?.into_iter();
            let r = coercev2_financial_scalar_number(p, values.next().ok_or(CellError::Value)?)?;
            let t = coercev2_financial_floor_i32(coercev2_financial_scalar_number(
                p,
                values.next().ok_or(CellError::Value)?,
            )?)?;
            let n = coercev2_financial_floor_i32(coercev2_financial_scalar_number(
                p,
                values.next().ok_or(CellError::Value)?,
            )?)?;
            let pv = coercev2_financial_scalar_number(p, values.next().ok_or(CellError::Value)?)?;
            if f == Ipmt {
                domain::ipmt(r, t, n, pv)
            } else {
                domain::ppmt(r, t, n, pv)
            }
        }
        Rate => {
            let count = args.len().min(6);
            let resolved = resolve_operands(p, args.get(..count).ok_or(CellError::Value)?)?;
            let mut numbers = [0.0, 0.0, 0.0, 0.0, 0.0, 0.1];
            for (i, v) in resolved.into_iter().enumerate() {
                *numbers.get_mut(i).ok_or(CellError::Value)? =
                    coercev2_financial_scalar_number(p, v)?;
            }
            work(p, 100)?;
            domain::rate(
                numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
            )
        }
        Fv | Pv | Pmt | Nper => {
            let r = coercev2_financial_argument_number(p, args.first().ok_or(CellError::Value)?)?;
            let n = coercev2_financial_argument_number(p, args.get(1).ok_or(CellError::Value)?)?;
            let a = coercev2_financial_argument_number(p, args.get(2).ok_or(CellError::Value)?)?;
            let b = coercev2_financial_optional_number(p, args, 3, 0.0)?;
            let t = coercev2_financial_optional_number(p, args, 4, 0.0)? != 0.0;
            match f {
                Fv => domain::fv(r, n, a, b, t),
                Pv => domain::pv(r, n, a, b, t),
                Pmt => domain::pmt(r, n, a, b, t),
                Nper => domain::nper(r, n, a, b, t),
                _ => return Err(CellError::Value.into()),
            }
        }
    };
    result.map(Output::Number).map_err(domain_error)
}
