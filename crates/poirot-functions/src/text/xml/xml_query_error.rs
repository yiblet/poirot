/// Distinguish invalid XML/XPath from resource limits and unsupported features.
#[derive(Debug, thiserror::Error)]
pub enum XmlQueryError<E> {
    /// The XML or admitted XPath is malformed, or selects no nodes.
    #[error("invalid XML selection")]
    Invalid,
    /// The XPath argument exceeds this backend's 1,024 UTF-16 code unit limit.
    #[error("XPath exceeds Excel's 1024-character limit")]
    XPathLength,
    /// The input or estimated work exceeds one of the backend's resource limits.
    #[error("XML selection exceeds the bounded backend's resource domain")]
    ResourceLimit,
    /// XPath uses an expression that this backend does not support.
    #[error("XPath needs unsupported expression constructs")]
    UnsupportedXPath,
    /// Input contains a DOCTYPE literal, even inside a comment or CDATA section.
    #[error("XML document type declarations are unsupported")]
    DocumentType,
    /// The XML document binds the same prefix to different namespace URIs.
    #[error("XML namespace prefixes are rebound within the document")]
    NamespaceRebinding,
    /// A selected value exceeds Excel's text limit.
    #[error("selected XML text exceeds Excel's text limit")]
    TextLimit,
    /// The allocator refused an output collection or string.
    #[error("could not allocate XML selection output")]
    Allocation,
    /// The caller refused a resource request.
    #[error("XML selection host refused admission")]
    Host(#[source] E),
}
