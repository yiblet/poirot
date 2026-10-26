//! Declarative admission for loan, dollar-conversion, and monetary-format kernels.
use crate::adapters::{
    elementwise::{
        self, Broadcast, Kernel, MemberContext, NumericErrorAdmission, NumericParameter as P,
        NumericSignature, Variadic,
    },
    failure::operational_only,
};
use crate::host::CallHost;
use crate::*;
use poirot_functions_financial as domain;

/// A financial scalar operation whose numeric inputs follow its declared defaults.
struct ScalarFinance(FinancialFunction);
impl Kernel for ScalarFinance {
    type Signature = Variadic<Broadcast>;
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use FinancialFunction::*;
        let parameters: &[P] = match self.0 {
            Fv | Pv | Pmt | Nper => &[
                P::required(),
                P::required(),
                P::required(),
                P::constant(0.0),
                P::constant(0.0),
            ],
            Ipmt | Ppmt => &[P::constant(0.0); 6],
            Rate => &[
                P::required(),
                P::required(),
                P::required(),
                P::constant(0.0),
                P::constant(0.0),
                P::constant(0.1),
            ],
            DollarDe | DollarFr => &[P::decimal(); 2],
            Fixed => &[P::required(), P::constant(2.0), P::boolean_default(false)],
            Dollar => &[P::required(), P::constant(2.0)],
            _ => return Err(VALUE_ERROR.into()),
        };
        let values = NumericSignature::new(parameters)
            .with_error_admission(NumericErrorAdmission::BeforeCoercion)
            .numbers(context, arguments)?;
        let n = |index| values.get(index);
        let host = &mut *context.host;
        let number = match self.0 {
            Fv => domain::fv(n(0)?, n(1)?, n(2)?, n(3)?, n(4)? != 0.0),
            Pv => domain::pv(n(0)?, n(1)?, n(2)?, n(3)?, n(4)? != 0.0),
            Pmt => domain::pmt(n(0)?, n(1)?, n(2)?, n(3)?, n(4)? != 0.0),
            Nper => domain::nper(n(0)?, n(1)?, n(2)?, n(3)?, n(4)? != 0.0),
            Ipmt | Ppmt => {
                let timing = n(5)?;
                if timing != 0.0 && timing != 1.0 {
                    return Err(Num.into());
                }
                if self.0 == Ipmt {
                    domain::ipmt_excel(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, timing == 1.0)
                } else {
                    domain::ppmt_excel(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, timing == 1.0)
                }
            }
            Rate => {
                host.charge_work(100)?;
                domain::rate(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?)
            }
            DollarDe => domain::dollar_de(n(0)?, n(1)?),
            DollarFr => domain::dollar_fr(n(0)?, n(1)?),
            Fixed | Dollar => {
                let places = n(1)?;
                if !places.is_finite()
                    || places.floor() < f64::from(i32::MIN)
                    || places.floor() > 127.0
                {
                    return Err(VALUE_ERROR.into());
                }
                host.check_values(1024)?;
                host.charge_work(4096)?;
                let output = if self.0 == Fixed {
                    host.check_text(1024)?;
                    domain::fixed(n(0)?, places.floor() as i32, n(2)? != 0.0)
                } else {
                    let currency = host.currency_context()?;
                    let bytes = 1024usize
                        .saturating_mul(currency.grouping_separator.len().max(1))
                        .saturating_add(currency.decimal_separator.len())
                        .saturating_add(currency.positive_prefix.len())
                        .saturating_add(currency.negative_prefix.len())
                        .saturating_add(currency.positive_suffix.len())
                        .saturating_add(currency.negative_suffix.len());
                    host.check_text(bytes)?;
                    host.charge_work(bytes as u64)?;
                    domain::dollar(n(0)?, places, &currency.as_format())
                }
                .map_err(super::modern::spreadsheet)?;
                return Ok(Text(output));
            }
            _ => return Err(VALUE_ERROR.into()),
        }
        .map_err(super::modern::spreadsheet)?;
        Ok(Number(number))
    }
}
/// Execute scalar finance through the shared typed kernel runtime.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: FinancialFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Financial(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    match elementwise::execute(&mut host, address, ScalarFinance(function)) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
