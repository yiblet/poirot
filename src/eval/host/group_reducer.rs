use crate::eval::values::CallableBody;
use poirot_functions::statistics::BuiltinAggregator;
use std::sync::Arc;

/// One admitted built-in grouping operation or a worksheet callback.
pub(in crate::eval) enum GroupReducer {
    Builtin(BuiltinAggregator),
    Callback(Arc<dyn CallableBody>),
    RelativeCallback(Arc<dyn CallableBody>),
}

impl GroupReducer {
    /// Return a label for a published aggregation result.
    pub(in crate::eval) fn name(&self) -> &'static str {
        match self {
            Self::Builtin(operation) => operation.name(),
            Self::Callback(_) | Self::RelativeCallback(_) => "LAMBDA",
        }
    }
}
