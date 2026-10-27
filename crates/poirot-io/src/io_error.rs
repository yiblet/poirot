use crate::{
    ExportLimitError, IoQuotaError, IoResource, ModelError, NumericFidelityError, PreflightError,
    StyleDecodeError, WorkbookFeatureError, WorkbookReadError, XmlCodecError,
};

/// Failure to read or write a workbook under the selected file contract.
#[derive(Debug, thiserror::Error)]
pub enum IoError {
    /// Actual claim rejected by caller quota.
    #[error("workbook import quota exceeded")]
    Quota(#[source] IoQuotaError),
    /// Expanded entry byte sum cannot fit its representation.
    #[error("expanded XLSX entry total overflows")]
    SizeOverflow,
    /// Allocation of workbook storage failed.
    #[error("workbook allocation failed")]
    Allocation(#[source] crate::AllocationError),
    /// Filesystem read or write failed.
    #[error("workbook file IO failed")]
    File(#[source] std::io::Error),
    /// Checked source structure cannot be decoded.
    #[error("workbook import failed")]
    Read(#[source] WorkbookReadError),
    /// Dependency failed to decode an XLSX workbook.
    #[error("XLSX backend import failed")]
    Xlsx(#[source] calamine::XlsxError),
    /// Backend failed to encode XLSX.
    #[error("XLSX export failed")]
    Write(#[from] rust_xlsxwriter::XlsxError),
    /// Source values cannot be represented in the model.
    #[error("workbook model rejected input")]
    Model(#[from] ModelError),
    /// XML or package codec failed.
    #[error("OOXML codec failed")]
    Xml(#[source] XmlCodecError),
    /// Package topology violates its contract.
    #[error("OPC validation failed")]
    Opc(#[source] crate::opc::OpcError),
    /// Compound file admission failed.
    #[error("workbook preflight failed")]
    BiffPreflight(#[from] PreflightError),
    /// Export exceeds its configured limit or representation.
    #[error("export resource limit exceeded")]
    Limit(#[source] ExportLimitError),
    /// Requested workbook feature cannot be represented.
    #[error("unsupported workbook feature")]
    Unsupported(#[source] WorkbookFeatureError),
    /// Number violates its file fidelity contract.
    #[error("numeric fidelity policy rejected value")]
    NumericFidelity(#[source] NumericFidelityError),
    /// Source style metadata violates its contract.
    #[error("invalid SpreadsheetML styles")]
    Style(#[source] StyleDecodeError),
    /// Formula printer failed to produce export spelling.
    #[error("cannot print export formula")]
    Formula(#[source] poirot_parsing::formula::ParseError),
}
impl IoError {
    /// Retain the actual claim rejected by an import quota.
    pub(crate) fn quota(
        resource: IoResource,
        limit: u64,
        consumed: u64,
        requested: u64,
        operation: crate::IoOperation,
    ) -> Self {
        Self::Quota(IoQuotaError {
            resource,
            limit,
            consumed,
            requested,
            operation,
        })
    }
}
impl From<std::io::Error> for IoError {
    fn from(source: std::io::Error) -> Self {
        if source.kind() == std::io::ErrorKind::OutOfMemory {
            Self::Allocation(crate::AllocationError::FileBuffer { source })
        } else {
            Self::File(source)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IoError;
    use crate::test_support::Workbook;
    use crate::{IoOperation, IoResource, ModelError, ReadOptions, XmlCodecError};
    use std::error::Error;

    #[test]
    fn import_quota_retains_actual_claim_and_typed_location() {
        let error = crate::read_workbook_bytes_with_options::<Workbook>(
            b"abcd",
            ReadOptions::default().max_input_bytes(3),
        )
        .unwrap_err();
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<crate::IoQuotaError>()
            .unwrap();
        assert_eq!(cause.resource, IoResource::InputBytes);
        assert_eq!((cause.limit, cause.consumed, cause.requested), (3, 0, 4));
        assert_eq!(cause.operation, IoOperation::InputBuffer);
        assert!(matches!(error, IoError::Quota(_)));
    }

    #[test]
    fn file_allocation_classification_preserves_original_io_cause() {
        let error = IoError::from(std::io::Error::from(std::io::ErrorKind::OutOfMemory));
        let allocation = error
            .source()
            .unwrap()
            .downcast_ref::<crate::AllocationError>()
            .unwrap();
        let io = allocation
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(io.kind(), std::io::ErrorKind::OutOfMemory);
        assert!(matches!(
            error,
            IoError::Allocation(crate::AllocationError::FileBuffer { .. })
        ));
        let error = IoError::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(matches!(error, IoError::File(_)));
    }

    #[test]
    fn corrupt_zip_retains_dependency_error_in_source_chain() {
        let error = crate::read_workbook_bytes::<Workbook>(b"PK\x03\x04").unwrap_err();
        let codec = error
            .source()
            .unwrap()
            .downcast_ref::<XmlCodecError>()
            .unwrap();
        assert!(codec.source().unwrap().is::<zip::result::ZipError>());
        assert!(matches!(error, IoError::Xml(XmlCodecError::Zip(_))));
    }

    #[test]
    fn scalar_model_error_retains_rejected_input_and_cause() {
        let scalar = "#INVALID!".parse::<crate::CellError>().unwrap_err();
        let error = IoError::from(ModelError::from(scalar));
        let model = error
            .source()
            .unwrap()
            .downcast_ref::<ModelError>()
            .unwrap();
        let scalar = model
            .source()
            .unwrap()
            .downcast_ref::<poirot_functions::scalar::ValueError>()
            .unwrap();
        assert!(
            matches!(scalar, poirot_functions::scalar::ValueError::UnknownCellError {input} if input == "#INVALID!")
        );
    }

    #[test]
    fn invalid_model_values_retain_typed_evidence() {
        assert!(
            matches!(crate::FontSize::new(-1.0), Err(ModelError::InvalidFontSize {value}) if value == -1.0)
        );
        assert!(
            matches!(crate::Tint::new(2.0), Err(ModelError::InvalidTint {value}) if value == 2.0)
        );
        assert!(matches!(
            crate::IndexedColor::new(66),
            Err(ModelError::InvalidIndexedColor { value: 66 })
        ));
        assert!(matches!(
            crate::RotationDegrees::new(91),
            Err(ModelError::InvalidRotation { value: 91 })
        ));
        assert!(matches!(
            crate::CellAddress::new(crate::MAX_ROWS, 2),
            Err(ModelError::InvalidCellCoordinates {
                row: crate::MAX_ROWS,
                column: 2
            })
        ));
        assert!(
            matches!("!".parse::<crate::CellAddress>(), Err(ModelError::InvalidCellAddress {input}) if input == "!")
        );
        assert!(matches!(
            crate::Cell::with_formula("", crate::CellValue::Blank),
            Err(ModelError::EmptyFormula)
        ));
    }

    #[test]
    fn source_format_and_numeric_failures_stay_typed() {
        assert!(matches!(
            crate::read_workbook_bytes::<Workbook>(b"unknown"),
            Err(IoError::Unsupported(
                crate::WorkbookFeatureError::UnsupportedFileSignature
            ))
        ));
        let location = crate::NumericLocation {
            sheet: "Data".into(),
            address: crate::CellAddress::new(1, 2).unwrap(),
            formula_cache: true,
        };
        let error = crate::FiniteNumericRepresentationPolicy::Binary64Finite
            .check(
                f64::INFINITY,
                crate::NumericFileFormat::Xls,
                location.clone(),
            )
            .unwrap_err();
        assert_eq!(error.location, location);
        assert_eq!(error.bits.bits(), f64::INFINITY.to_bits());
        assert_eq!(error.reason, crate::NumericFidelityReason::Nonfinite);
        let error = IoError::NumericFidelity(error);
        assert!(error.source().unwrap().is::<crate::NumericFidelityError>());
    }

    #[test]
    fn public_errors_can_cross_threads() {
        fn thread_error<T: Error + Send + Sync + 'static>() {}
        thread_error::<IoError>();
        thread_error::<ModelError>();
        thread_error::<crate::PreflightError>();
        thread_error::<crate::opc::OpcError>();
        thread_error::<crate::identifiers::IdentifierError>();
        thread_error::<crate::NumericFidelityError>();
    }
}
