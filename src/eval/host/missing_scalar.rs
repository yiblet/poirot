use super::missing_policy::MissingPolicy;
use crate::eval::values::FunctionResult;
use crate::model::CellValue;

/// Explicit missing-argument projection. The caller retains omission metadata;
/// this value is only the requested legacy scalar representation.
pub(super) fn coerce_missing_scalar(policy: MissingPolicy) -> FunctionResult<CellValue> {
    use poirot_functions::coercion::compatibility::MissingScalarPolicy;
    let policy = match policy {
        MissingPolicy::Blank => MissingScalarPolicy::Blank,
        #[cfg(test)]
        MissingPolicy::Zero => MissingScalarPolicy::Zero,
        #[cfg(test)]
        MissingPolicy::Reject => MissingScalarPolicy::Reject,
    };
    poirot_functions::coercion::compatibility::coerce_missing_scalar(policy).map_err(Into::into)
}
