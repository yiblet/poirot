/// Failure of authored JSON scenario operations.
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    /// Numbers must be finite.
    #[error("numbers must be finite")]
    NonfiniteNumber,
    /// Unsupported schema_version; expected 1.
    #[error("unsupported schema_version; expected 1")]
    UnsupportedSchema,
    /// Suite group must be nonempty when provided.
    #[error("suite group must be nonempty when provided")]
    EmptyGroup,
    /// A suite needs worksheets and scenarios.
    #[error("a suite needs worksheets and scenarios")]
    EmptySuite,
    /// Scenario names must be nonempty and unique within a suite.
    #[error("scenario names must be nonempty and unique within a suite")]
    InvalidScenarioName,
    /// Tolerance must be finite and nonnegative.
    #[error("tolerance must be finite and nonnegative")]
    InvalidTolerance,
    /// Tolerance is only valid for numeric expectations.
    #[error("tolerance is only valid for numeric expectations")]
    NonnumericTolerance,
    /// Known gaps need a reason.
    #[error("known gaps need a reason")]
    MissingGapReason,
    /// Known gap actual must differ from expected.
    #[error("known gap actual must differ from expected")]
    RedundantGap,
    /// Output api returned no result.
    #[error("output API returned no result")]
    MissingOutput,
    /// Scenario file or report i/o failed.
    #[error("scenario file or report I/O failed")]
    Io(#[from] std::io::Error),
    /// Scenario json decoding or encoding failed.
    #[error("scenario JSON decoding or encoding failed")]
    Json(#[from] serde_json::Error),
    /// Scenario input is not utf-8.
    #[error("scenario input is not UTF-8")]
    Utf8(#[from] std::str::Utf8Error),
    /// Scenario workbook mutation failed.
    #[error("scenario workbook mutation failed")]
    Model(#[from] poirot::model::ModelError),
    /// Scenario evaluation setup or mutation failed.
    #[error("scenario evaluation setup or mutation failed")]
    Overlay(#[from] poirot::calculation::OverlayError),
    /// Scenario calculation failed.
    #[error("scenario calculation failed")]
    Evaluation(#[from] poirot::calculation::EvaluationError),
    /// Scenario number-format compilation failed.
    #[error("scenario number-format compilation failed")]
    FormatCompile(#[from] poirot::styles::FormatCompileError),
    /// Scenario value formatting failed.
    #[error("scenario value formatting failed")]
    Format(#[from] poirot::styles::FormatError),
    /// Specification operation failed.
    #[error("specification operation failed")]
    Specification(#[from] super::specs::SpecificationError),
    /// Action.
    #[error("scenario action {index} failed")]
    Action {
        /// Zero-based action index.
        index: usize,
        /// Underlying action failure.
        #[source]
        source: Box<JsonError>,
    },
    /// Missing evidence.
    #[error("scenario {scenario:?} needs expectation evidence")]
    MissingEvidence {
        /// Scenario identity.
        scenario: String,
    },
    /// Missing check.
    #[error("scenario {scenario:?} needs at least one check")]
    MissingCheck {
        /// Scenario identity.
        scenario: String,
    },
    /// Duplicate initial cell.
    #[error("duplicate initial cell {cell:?}")]
    DuplicateInitialCell {
        /// Rejected cell reference.
        cell: String,
    },
    /// A scenario cell reference cannot resolve to a workbook target.
    #[error("scenario cell reference is invalid")]
    Target(#[from] poirot::eval::overlay::OverlayTargetError),
}

#[cfg(test)]
mod tests {
    use super::JsonError;
    use std::error::Error;

    #[test]
    fn action_keeps_index_and_original_model_cause() {
        let error = JsonError::Action {
            index: 7,
            source: Box::new(JsonError::Model(
                poirot::model::ModelError::InvalidWorksheetIndex { index: 3 },
            )),
        };
        let JsonError::Action { index, source } = &error else {
            panic!("wrong error category")
        };
        assert_eq!(*index, 7);
        let cause = source
            .source()
            .unwrap()
            .downcast_ref::<poirot::model::ModelError>()
            .unwrap();
        assert!(matches!(
            cause,
            poirot::model::ModelError::InvalidWorksheetIndex { index: 3 }
        ));
        assert_eq!(error.to_string(), "scenario action 7 failed");
    }

    #[test]
    fn public_errors_can_cross_worker_threads() {
        fn send_sync_static<T: Send + Sync + 'static>() {}
        send_sync_static::<JsonError>();
        send_sync_static::<crate::RunnerError>();
        send_sync_static::<crate::fods::FodsError>();
        send_sync_static::<crate::json::specs::SpecificationError>();
        send_sync_static::<crate::fods::xml::ExtractionError>();
    }
}
