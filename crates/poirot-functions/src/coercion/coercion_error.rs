use crate::coercion::{CellError, CoercionMismatchError, ExpectedInput, Resource};

/// Conversion failures that distinguish candidate mismatch from terminal errors.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoercionError {
    /// This candidate may be followed by a lower-priority candidate.
    #[error("no matching conversion input")]
    Mismatch(#[source] CoercionMismatchError),
    /// A spreadsheet error is terminal unless explicitly requested as data.
    #[error("conversion returned {0}")]
    Worksheet(CellError),
    /// An operational failure must never select a different overload.
    #[error("conversion resource exhausted: {0:?}")]
    Limit(Resource),
    /// A Unicode-scalar consumer cannot represent isolated UTF-16 surrogates.
    #[error("unicode scalar conversion requires valid UTF-16")]
    UnsupportedText,
}

impl CoercionError {
    /// Describe the input category a candidate requires.
    pub(crate) fn mismatch(expected: ExpectedInput) -> Self {
        Self::Mismatch(CoercionMismatchError::Input {
            argument: None,
            expected,
        })
    }

    /// Attach the selected argument index to a candidate mismatch.
    pub(crate) fn at_argument(self, index: usize) -> Self {
        match self {
            Self::Mismatch(CoercionMismatchError::Input { expected, .. }) => {
                Self::Mismatch(CoercionMismatchError::Input {
                    argument: Some(index),
                    expected,
                })
            }
            error @ (Self::Mismatch(CoercionMismatchError::Arity { .. })
            | Self::Worksheet(_)
            | Self::Limit(_)
            | Self::UnsupportedText) => error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CoercionError;
    use crate::coercion::{CoercionMismatchError, ExpectedInput};
    use std::error::Error;

    #[test]
    fn input_mismatch_preserves_typed_expectation_and_argument() {
        let error = CoercionError::mismatch(ExpectedInput::ExistingNumber).at_argument(2);
        let expected = CoercionMismatchError::Input {
            argument: Some(2),
            expected: ExpectedInput::ExistingNumber,
        };
        assert_eq!(error, CoercionError::Mismatch(expected.clone()));
        assert_eq!(
            error
                .source()
                .and_then(|cause| cause.downcast_ref::<CoercionMismatchError>()),
            Some(&expected)
        );
        fn assert_send_sync_static<T: Send + Sync + 'static>() {}
        assert_send_sync_static::<CoercionError>();
    }
}
