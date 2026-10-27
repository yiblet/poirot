/// Failure of native FODS extraction.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    /// Odf text exceeds bounded input size.
    #[error("ODF text exceeds bounded input size")]
    TextLimit,
    /// Odf spaces exceed bounded input size.
    #[error("ODF spaces exceed bounded input size")]
    SpaceLimit,
    /// Odf year out of bounds.
    #[error("ODF year out of bounds")]
    OdfYearOutOfBounds,
    /// Materialized fods cells exceed bounded input size.
    #[error("materialized FODS cells exceed bounded input size")]
    CellLimit,
    /// Nonempty fods cell exceeds worksheet bounds.
    #[error("nonempty FODS cell exceeds worksheet bounds")]
    CellBounds,
    /// Matrix span exceeds worksheet/materialization bounds.
    #[error("matrix span exceeds worksheet/materialization bounds")]
    MatrixBounds,
    /// Nonempty fods row exceeds worksheet bounds.
    #[error("nonempty FODS row exceeds worksheet bounds")]
    RowBounds,
    /// Enormous nonempty repeated row.
    #[error("enormous nonempty repeated row")]
    RepeatedRowLimit,
    /// Fods fixture exceeds bounded source size.
    #[error("FODS fixture exceeds bounded source size")]
    SourceLimit,
    /// Fods row padding overflows coordinates.
    #[error("FODS row padding overflows coordinates")]
    RowOverflow,
    /// Fods column padding overflows coordinates.
    #[error("FODS column padding overflows coordinates")]
    ColumnOverflow,
    /// Missing matrix origin.
    #[error("missing matrix origin")]
    MissingMatrixOrigin,
    /// Missing matrix cell.
    #[error("missing matrix cell")]
    MissingMatrixCell,
    /// Fods workbook materialization exceeds bounded size.
    #[error("FODS workbook materialization exceeds bounded size")]
    WorkbookCellLimit,
    /// Source target formula has no original expression.
    #[error("source target formula has no original expression")]
    SourceTargetFormulaHasNoOriginalExpression,
    /// Source correct formula has no original expression.
    #[error("source Correct formula has no original expression")]
    SourceCorrectFormulaHasNoOriginalExpression,
    /// Fods is not utf-8.
    #[error("FODS is not UTF-8")]
    Utf8(#[from] std::str::Utf8Error),
    /// Invalid fods xml.
    #[error("invalid FODS XML")]
    Xml(#[from] roxmltree::Error),
    /// Read.
    #[error("could not read FODS fixture {path:?}")]
    Read {
        /// Fixture path.
        path: std::path::PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// Fixture.
    #[error("could not extract FODS fixture {path:?}")]
    Fixture {
        /// Fixture path.
        path: std::path::PathBuf,
        /// Extraction failure.
        #[source]
        source: Box<ExtractionError>,
    },
    /// Missing attribute on element.
    #[error("missing {attribute} on {element}")]
    MissingAttribute {
        /// Required XML attribute name.
        attribute: String,
        /// XML element name.
        element: String,
    },
    /// Invalid positive attribute: value.
    #[error("invalid positive {attribute}: {value}")]
    PositiveAttribute {
        /// XML attribute name.
        attribute: String,
        /// Rejected attribute value.
        value: String,
    },
    /// Finite number.
    #[error("invalid finite ODF number {value:?}")]
    FiniteNumber {
        /// Rejected numeric input.
        value: String,
    },
    /// Date.
    #[error("invalid ODF date {value:?}")]
    Date {
        /// Rejected date input.
        value: String,
    },
    /// An integer-valued ODF attribute cannot be parsed.
    #[error("invalid ODF integer {value:?}")]
    Integer {
        /// Rejected attribute input.
        value: String,
        /// Integer parser failure.
        #[source]
        source: std::num::ParseIntError,
    },
    /// A number-valued ODF attribute cannot be parsed.
    #[error("invalid ODF number {value:?}")]
    Number {
        /// Rejected attribute input.
        value: String,
        /// Floating-point parser failure.
        #[source]
        source: std::num::ParseFloatError,
    },
}

#[cfg(test)]
mod tests {
    use super::ExtractionError;
    use std::error::Error;

    #[test]
    fn native_extraction_preserves_dependency_causes() {
        let error = crate::fods::xml::extract_bytes(&[255], "invalid.fods").unwrap_err();
        assert!(matches!(error, ExtractionError::Utf8(_)));
        assert!(error.source().unwrap().is::<std::str::Utf8Error>());
        let error = crate::fods::xml::extract_bytes(b"<", "invalid.fods").unwrap_err();
        assert!(matches!(error, ExtractionError::Xml(_)));
        assert!(error.source().unwrap().is::<roxmltree::Error>());
    }

    #[test]
    fn malformed_attribute_retains_input_and_parser_cause() {
        let xml = br#"<document xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><table:table table:name="Bad"><table:table-row table:number-rows-repeated="invalid"/></table:table></document>"#;
        let error = crate::fods::xml::extract_bytes(xml, "invalid.fods").unwrap_err();
        let ExtractionError::Integer { value, source } = error else {
            panic!("wrong error category")
        };
        assert_eq!(value, "invalid");
        assert_eq!(source.kind(), &std::num::IntErrorKind::InvalidDigit);
    }
}
