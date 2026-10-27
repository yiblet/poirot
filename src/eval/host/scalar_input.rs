use super::call_host::CallHost;
use super::missing_policy::MissingPolicy;
use super::scalar_operand::ScalarOperand;
use crate::eval::values::{FunctionFailure, FunctionResult};

/// Presents one deferred argument through the function-core scalar input interface.
/// Forcing it uses the host's memoized evaluation and preserves the chosen omission policy.
pub(super) struct ScalarInput<'input, 'call, 'workbook> {
    pub(super) host: &'input mut CallHost<'call, 'workbook>,
    pub(super) index: usize,
    pub(super) missing: MissingPolicy,
}

impl poirot_functions::scalar::Input for ScalarInput<'_, '_, '_> {
    /// Selected scalar operand retaining its argument provenance.
    type Value = ScalarOperand;

    /// Family failure channel preserving spreadsheet versus operational errors.
    type Error = FunctionFailure;

    fn force(&mut self) -> FunctionResult<ScalarOperand> {
        self.host.scalar_data(self.index, self.missing)
    }
}
