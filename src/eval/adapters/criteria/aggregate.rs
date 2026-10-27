//! AGGREGATE filters workbook cells before calling the independent numeric kernel.
use crate::eval::adapters::elementwise::{
    self, Broadcast, FixedScalar, Kernel, MemberContext, NumericParameter, WholeCollection,
};
use crate::eval::host::{CallHost, OperandReader, SubtotalReader};
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value};
use crate::model::CellValue;
use poirot_functions::statistics::{AggregateFunction, AggregateInput};

/// Validates AGGREGATE controls and collects values without hiding operational failures.
pub(super) struct AggregateExecutor<'h, 'call, 'workbook> {
    /// Deferred arguments and workbook reads checked against the evaluation budget.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> AggregateExecutor<'h, 'call, 'workbook> {
    /// Borrow the host for one AGGREGATE call.
    pub(super) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Collect a sample using the selector and exclusion options, then calculate its result.
    pub(super) fn evaluate(&mut self) -> FunctionResult<Value> {
        // Microsoft documents the reference/array forms and options here:
        // https://support.microsoft.com/en-us/excel/functions/aggregate-function
        // Reference-only selectors 1–13 and rejection of 3-D references:
        // https://learn.microsoft.com/en-us/office/vba/api/excel.worksheetfunction.aggregate
        if self.host.len() < 3 {
            return Err(VALUE_ERROR.into());
        }
        let selector = self.control(0)?.trunc();
        if !(1.0..=19.0).contains(&selector) {
            return Err(VALUE_ERROR.into());
        }
        let function = AggregateFunction::from_number(selector as u8)?;
        let options = if FixedScalar::at(1).read(self.host)?.missing {
            0.0
        } else {
            self.control(1)?.trunc()
        };
        if !(0.0..=7.0).contains(&options) {
            return Err(VALUE_ERROR.into());
        }
        let options = options as u8;
        let array_form = selector >= 14.0;
        if array_form && self.host.len() != 4 {
            return Err(VALUE_ERROR.into());
        }
        let end = if array_form { 3 } else { self.host.len() };
        let mut numbers = Vec::new();
        let mut nonempty_count = 0usize;
        for argument in 2..end {
            SubtotalReader::new(self.host).aggregate_source(argument, !array_form)?;
            let len = OperandReader::new(self.host).collection_len(argument)?;
            self.host.check_values(len)?;
            self.host.charge_work(len as u64)?;
            for offset in 0..len {
                let Some(member) = SubtotalReader::new(self.host).aggregate_cell(
                    argument,
                    offset,
                    options % 2 == 1,
                    options < 4,
                )?
                else {
                    continue;
                };
                if matches!(member.value, CellValue::Error(_)) && options % 4 >= 2 {
                    continue;
                }
                match member.value {
                    CellValue::Blank => continue,
                    CellValue::Number(number) => {
                        self.host
                            .check_values_capacity(numbers.len().saturating_add(1))?;
                        self.host.check_values(1)?;
                        numbers
                            .try_reserve(1)
                            .map_err(|_| self.host.allocation_failure())?;
                        // SAFETY: capacity for one additional number was reserved above.
                        numbers.push(number);
                    }
                    CellValue::Error(error)
                        if !matches!(
                            function,
                            AggregateFunction::Count | AggregateFunction::CountA
                        ) =>
                    {
                        return Err(error.into());
                    }
                    _ => {}
                }
                nonempty_count = nonempty_count.checked_add(1).ok_or(VALUE_ERROR)?;
            }
        }
        if array_form {
            let address = self.host.target().address;
            return elementwise::execute(
                self.host,
                address,
                AggregateRanks {
                    function,
                    input: AggregateInput {
                        numbers: &numbers,
                        nonempty_count,
                    },
                },
            );
        }
        // The independent kernel may copy and sort the numeric sample. Check its
        // temporary storage and O(n log n) work against the budget before calculating.
        self.host.check_values(numbers.len().saturating_mul(3))?;
        self.host.charge_work(
            (numbers.len() as u64)
                .saturating_mul(16 + u64::from(usize::BITS - numbers.len().leading_zeros())),
        )?;
        let result = function.calculate(
            AggregateInput {
                numbers: &numbers,
                nonempty_count,
            },
            None,
        )?;
        Ok(CellValue::Number(result).into())
    }

    /// Coerce one scalar numeric control without swallowing source failures.
    fn control(&mut self, argument: usize) -> FunctionResult<f64> {
        FixedScalar::at(argument).number(self.host, NumericParameter::required())
    }
}

/// Reuses one collected sample for each broadcast rank or percentile argument.
struct AggregateRanks<'a> {
    /// Operation applied to each rank or percentile parameter.
    function: AggregateFunction,
    /// Sample collected once before any rank is computed.
    input: AggregateInput<'a>,
}

impl Kernel for AggregateRanks<'_> {
    type Signature = (FixedScalar, FixedScalar, WholeCollection, Broadcast);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let k = arguments.3.number(context, None)?;
        let len = self.input.numbers.len();
        context.host.check_values(len.saturating_mul(3))?;
        context.host.charge_work(
            (len as u64).saturating_mul(16 + u64::from(usize::BITS - len.leading_zeros())),
        )?;
        Ok(CellValue::Number(
            self.function.calculate(self.input, Some(k))?,
        ))
    }
}
