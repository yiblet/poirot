/// File or package location whose resource claim is being admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoOperation {
    /// Bytes supplied directly by the caller.
    InputBuffer,
    /// Filesystem source.
    File {
        /// Preserved filesystem path.
        path: std::path::PathBuf,
    },
    /// Complete XLSX package bytes.
    XlsxPackage,
    /// Package central-directory inventory.
    XlsxCatalog,
    /// One expanded package part.
    Entry {
        /// Source package path.
        path: String,
    },
}
