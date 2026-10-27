use super::call_target::CallTarget;

/// Resolved function identity and spelling metadata for one bytecode call.
/// The VM uses it to select a handler while argument expressions remain deferred
/// in instruction blocks.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Call {
    /// Operand-buffer byte offset of the source function spelling.
    pub source_offset: u32,
    /// Exact lookup spelling when it differs from the source spelling.
    pub resolved_name: Option<String>,
    /// Builtin, extension, or deferred unresolved call target.
    pub target: CallTarget,
}
