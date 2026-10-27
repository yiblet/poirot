/// Failure in an XML, ZIP, or UTF-8 codec used by package execution.
#[derive(Debug, thiserror::Error)]
pub enum XmlCodecError {
    /// XML parser rejected input.
    #[error("XML parser failed")]
    Parse(#[from] quick_xml::Error),
    /// XML attribute parser rejected input.
    #[error("XML attribute parser failed")]
    Attribute(#[from] quick_xml::events::attributes::AttrError),
    /// XML entity decoder rejected input.
    #[error("XML entity decoder failed")]
    Escape(#[from] quick_xml::escape::EscapeError),
    /// ZIP reader or writer rejected the package.
    #[error("ZIP codec failed")]
    Zip(#[from] zip::result::ZipError),
    /// XML writer failed.
    #[error("XML writer IO failed")]
    Write(#[from] std::io::Error),
    /// XML attribute contains malformed character references.
    #[error("XML attribute normalization failed")]
    Normalize(#[from] quick_xml::encoding::EncodingError),
    /// Generated package lacks a required part.
    #[error("missing generated part {path:?}")]
    MissingGeneratedPart {
        /// Missing package path.
        path: String,
    },
    /// SpreadsheetML text is truncated.
    #[error("truncated SpreadsheetML text")]
    TruncatedText,
    /// Cell text is truncated.
    #[error("truncated text cell")]
    TruncatedCell,
    /// Integer-valued XML attribute cannot be decoded.
    #[error("invalid XML integer attribute")]
    Integer(#[from] std::num::ParseIntError),
}
