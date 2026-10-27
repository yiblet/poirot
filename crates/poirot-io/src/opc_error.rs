/// Invalid XLSX package topology, identity, or relationship metadata.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpcError {
    /// Required entry is absent.
    #[error("missing package part {path:?}")]
    MissingPart {
        /// Absent path.
        path: String,
    },
    /// Relationship target escapes or ambiguously names the package.
    #[error("invalid relationship target {target:?} from {owner:?}")]
    InvalidTarget {
        /// Owning package part.
        owner: String,
        /// Rejected target spelling.
        target: String,
    },
    /// Sheet or part identity is repeated.
    #[error("duplicate package identity {identity:?}")]
    DuplicateIdentity {
        /// Conflicting source identity.
        identity: String,
    },
    /// Relationship identity is repeated in its own part.
    #[error("duplicate relationship {id:?} in {path:?}")]
    DuplicateRelationship {
        /// Relationship part path.
        path: String,
        /// Conflicting source ID.
        id: String,
    },
    /// Selected relationship is absent.
    #[error("missing relationship {id:?}")]
    MissingRelationship {
        /// Missing source identity.
        id: String,
    },
    /// Relationship count violates the cardinality contract.
    #[error("invalid {kind:?} relationship count {found}")]
    RelationshipCount {
        /// Relationship type suffix.
        kind: String,
        /// Whether one relationship is required.
        required: bool,
        /// Actual matching relationships.
        found: usize,
    },
    /// Relationship XML lacks a required attribute.
    #[error("missing relationship attribute {attribute:?} in {path:?}")]
    MissingRelationshipAttribute {
        /// Relationship part.
        path: String,
        /// Absent XML attribute name.
        attribute: String,
    },
    /// Relationship target mode is invalid.
    #[error("invalid target mode for {id:?} in {path:?}")]
    InvalidTargetMode {
        /// Relationship part.
        path: String,
        /// Source relationship ID.
        id: String,
    },
    /// External target is forbidden by policy.
    #[error("external relationship {id:?} from {owner:?}")]
    ExternalRelationship {
        /// Owning package part, if known.
        owner: Option<String>,
        /// Source relationship ID.
        id: String,
    },
    /// Sheet inventory differs from exported sheets.
    #[error("export worksheet inventory mismatch")]
    ExportSheetInventory,
    /// A requested worksheet is absent.
    #[error("missing worksheet {name:?}")]
    MissingSheet {
        /// Source worksheet name.
        name: String,
    },
    /// Sheet metadata lacks a name.
    #[error("missing worksheet name")]
    MissingSheetName,
    /// Sheet metadata has no positive numeric identity.
    #[error("invalid worksheet sheetId")]
    InvalidSheetId,
    /// Sheet metadata lacks a qualified relationship identity.
    #[error("missing relationship ID for worksheet {name:?}")]
    MissingSheetRelationship {
        /// Source worksheet name.
        name: String,
    },
    /// Sheet relationship selects a type unsupported by the sheet contract.
    #[error("invalid sheet relationship type {kind:?} for worksheet {name:?}")]
    InvalidSheetRelationshipType {
        /// Source worksheet name.
        name: String,
        /// Rejected source type URI.
        kind: String,
    },
    /// Required part uses an unsupported XML namespace.
    #[error("invalid namespace in {path:?}")]
    InvalidNamespace {
        /// Source package path.
        path: String,
    },
}
