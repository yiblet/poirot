//! Complete FIXED/DOLLAR coercion, admission, and locale formatting workflow.
use super::coercion::{boolean, int, number, work};
use super::{CellError, EvaluationPort, Failure, FinancialFunction, Output, domain, domain_error};

/// Render FIXED or DOLLAR after admitting output storage and explicit currency inputs.
pub(super) fn evaluate<P: EvaluationPort>(
    p: &mut P,
    f: FinancialFunction,
    args: &[P::Argument],
) -> Result<Output, Failure<P::Operational>> {
    use FinancialFunction::{Dollar, Fixed};
    let n = number(p, args.first().ok_or(CellError::Value)?)?;
    let places = if args.len() > 1 {
        number(p, args.get(1).ok_or(CellError::Value)?)?
    } else {
        2.0
    };
    let fixed_places = if f == Fixed { Some(int(places)?) } else { None };
    if (f == Fixed && fixed_places.ok_or(CellError::Value)? > 127)
        || (f == Dollar && places > 127.0)
    {
        return Err(CellError::Value.into());
    }
    // Worst-case finite IEEE decimal: 309 integer digits + 127 places;
    // fixed workspace buffers also cover scale -400 before allocation.
    p.check_collection(1024).map_err(Failure::Operational)?;
    work(p, 4096)?;
    let output = if f == Fixed {
        let commas = if args.len() > 2 {
            boolean(p, args.get(2).ok_or(CellError::Value)?)?
        } else {
            false
        };
        p.check_text(1024).map_err(Failure::Operational)?;
        domain::fixed(n, fixed_places.ok_or(CellError::Value)?, commas).map_err(domain_error)?
    } else {
        let context = p.currency_format()?;
        let bytes = 1024usize
            .saturating_mul(context.grouping_separator.len().max(1))
            .saturating_add(context.decimal_separator.len())
            .saturating_add(context.positive_prefix.len())
            .saturating_add(context.negative_prefix.len())
            .saturating_add(context.positive_suffix.len())
            .saturating_add(context.negative_suffix.len());
        p.check_text(bytes).map_err(Failure::Operational)?;
        work(p, bytes as u64)?;
        domain::dollar(n, places, &context.as_format()).map_err(domain_error)?
    };
    Ok(Output::Text(output))
}
