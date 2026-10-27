use super::xml::model::FormulaAttribute;

/// Original source position inspected to prove a rejected declaration unused.
#[derive(Debug, Clone)]
pub(super) enum InspectionLocation {
    /// A native formula XML attribute.
    Attribute {
        /// Attribute kind.
        attribute: FormulaAttribute,
        /// Source-ordered attribute position.
        index: usize,
    },
    /// A worksheet cell's original source.
    Cell {
        /// Worksheet name.
        sheet: String,
        /// Zero-based row.
        row: u32,
        /// Zero-based column.
        col: u32,
    },
    /// A defined-name expression.
    Definition {
        /// Defined-name spelling.
        name: String,
    },
    /// A defined-name relative base.
    DefinitionBase {
        /// Defined-name spelling.
        name: String,
    },
    /// A source case's actual expression.
    Actual {
        /// Source case identity.
        case: String,
    },
    /// A source case's Correct predicate.
    Correct {
        /// Source case identity.
        case: String,
    },
}

impl std::fmt::Display for InspectionLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Attribute { attribute, index } => {
                write!(f, "native XML {attribute:?} attribute {index}")
            }
            Self::Cell { sheet, row, col } => write!(f, "sheet {sheet:?} row {row} column {col}"),
            Self::Definition { name } => write!(f, "definition {name:?}"),
            Self::DefinitionBase { name } => write!(f, "definition {name:?} base"),
            Self::Actual { case } => write!(f, "case {case} actual"),
            Self::Correct { case } => write!(f, "case {case} Correct"),
        }
    }
}
