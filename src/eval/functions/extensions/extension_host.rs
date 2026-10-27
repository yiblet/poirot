use super::extension_operand_description::ExtensionOperandDescription;
use super::extension_result::ExtensionResult;
use super::extension_scalar::ExtensionScalar;
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;

/// Reads are deferred and budgeted. Errors remain spreadsheet values for providers to inspect.
pub trait ExtensionHost {
    /// Number of syntactic arguments, including omitted positions.
    fn argument_count(&self) -> usize;

    /// Current formula cell. Requires the context capability.
    fn target(&mut self) -> ExtensionResult<EvaluationTarget>;

    /// Captured clock, random seed, and localization. Requires the context capability.
    fn context(&mut self) -> ExtensionResult<EvaluationInputs>;

    /// Resolve an argument expression and inspect its shape without reading its members.
    /// Argument indices are zero-based; required capabilities are checked before access.
    fn describe(&mut self, index: usize) -> ExtensionResult<ExtensionOperandDescription>;

    /// Select one scalar using the caller-cell intersection rules. Omitted arguments
    /// return a blank value with `missing = true`; cell errors remain inspectable data.
    fn scalar(&mut self, index: usize) -> ExtensionResult<ExtensionScalar>;

    /// Read a zero-based row/column offset within an argument array or reference.
    /// Only the requested cell is read; operational limits remain in force.
    fn read_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> ExtensionResult<ExtensionScalar>;

    /// Charge provider-owned work before performing it. Exceeding a limit aborts the run.
    fn charge_work(&mut self, units: u64) -> ExtensionResult<()>;
}
