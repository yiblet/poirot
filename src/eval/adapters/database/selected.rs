use super::dispatch::database_failure;
use crate::eval::adapters::elementwise::rows::Reducer;
use crate::eval::config::BuiltinSemanticsPolicy;
use crate::eval::host::{CallHost, PolicyReads};
use crate::eval::values::FunctionResult;
use crate::model::CellError::Num;
use crate::model::CellValue;
use crate::model::CellValue::{Blank, Error};
use poirot_functions::database::{DatabaseFunction, FinalizationPolicy, aggregate_selected};

/// Accumulates database payloads accepted by conditional row selection for domain aggregation.
/// DGET can stop at a second nonblank match instead of reading further records.
pub(super) struct Selected {
    pub(super) function: DatabaseFunction,
    pub(super) values: Vec<CellValue>,
    pub(super) unique: bool,
}

impl Reducer for Selected {
    fn push(
        &mut self,
        host: &mut CallHost<'_, '_>,
        value: CellValue,
    ) -> FunctionResult<Option<CellValue>> {
        if self.function == DatabaseFunction::Get && value != Blank {
            if self.unique {
                return Ok(Some(Error(Num)));
            }
            self.unique = true;
        }
        host.check_values(self.values.len().saturating_add(1))?;
        self.values
            .try_reserve(1)
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: one additional slot was reserved immediately above.
        self.values.push(value);
        Ok(None)
    }

    fn finish(self, host: &mut CallHost<'_, '_>) -> FunctionResult<CellValue> {
        let Self {
            function, values, ..
        } = self;
        // Finalization visits each selected value once, or twice for variance.
        let passes = if matches!(
            function,
            DatabaseFunction::Stdev
                | DatabaseFunction::StdevP
                | DatabaseFunction::Var
                | DatabaseFunction::VarP
        ) {
            2
        } else {
            1
        };
        host.charge_work((values.len() as u64).saturating_mul(passes))?;
        let policy = match PolicyReads::new(host).builtin_policy_snapshot() {
            BuiltinSemanticsPolicy::PinnedPoi => FinalizationPolicy::PinnedPoi,
            BuiltinSemanticsPolicy::ExcelDocumentation => FinalizationPolicy::ExcelDocumentation,
        };
        aggregate_selected(function, &values, policy).map_err(|error| database_failure(host, error))
    }
}
