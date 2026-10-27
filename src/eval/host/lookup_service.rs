//! Checked classic lookup services; caches and reads stay evaluator-owned.
use super::call_host::CallHost;
use super::classic_lookup_kind::ClassicLookupKind;
use crate::eval::adapters::search::execution::{
    LookupTable, SearchAxis, SearchMode, SessionSearch,
};
use crate::eval::values::FunctionResult;
use crate::model::CellValue;

/// Runs classic lookup candidate selection and result reads for one call.
pub(in crate::eval) struct LookupService<'h, 'call, 'workbook> {
    /// The call whose session caches lookup prefixes and records reads.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> LookupService<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's lookups.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Read a selected result vector after classic candidate selection succeeds.
    pub(in crate::eval) fn lookup_result(
        &mut self,
        table: &LookupTable,
        axis: SearchAxis,
        fixed: usize,
        index: usize,
    ) -> FunctionResult<CellValue> {
        let vector = table.vector(axis, fixed)?;
        Ok(SessionSearch::new(self.host.session).lookup_read(table, vector, index)?)
    }

    /// Select a zero-based position in the first row or column with checked reads and prefix reuse.
    /// The caller forces key and table operands before invoking this service.
    pub(in crate::eval) fn lookup_index(
        &mut self,
        table: &LookupTable,
        axis: SearchAxis,
        key: &CellValue,
        mode: SearchMode,
        kind: ClassicLookupKind,
    ) -> FunctionResult<Option<usize>> {
        SessionSearch::new(self.host.session).classic_lookup_index(table, axis, key, mode, kind)
    }
}
