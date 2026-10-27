use super::criteria_function::CriteriaFunction;
use super::database_function::DatabaseFunction;
use super::date_function::DateFunction;
use super::engineering_function::EngineeringFunction;
use super::financial_function::FinancialFunction;
use super::function_descriptor::FunctionDescriptor;
use super::information_function::InformationFunction;
use super::logical_function::LogicalFunction;
use super::math_function::MathFunction;
use super::matrix_function::MatrixFunction;
use super::reference_function::ReferenceFunction;
use super::search_function::SearchFunction;
use super::statistics_function::StatisticsFunction;
use super::text_function::TextFunction;

/// Typed builtin handler identity. Multiple advertised spellings can select one handler;
/// this identity is independent of the spelling retained for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FunctionId {
    /// Cash flow and monetary operations.
    Financial(FinancialFunction),
    /// Engineering and complex number operations.
    Engineering(EngineeringFunction),
    /// Matrix and array operations.
    Matrix(MatrixFunction),
    /// Conditional aggregation operations.
    Criteria(CriteriaFunction),
    /// Criteria-table database operations.
    Database(DatabaseFunction),
    /// Date and time operations.
    Date(DateFunction),
    /// Logical tests and conditional execution.
    Logical(LogicalFunction),
    /// Aggregation and statistical operations.
    Statistics(StatisticsFunction),
    /// Reference construction and selection.
    Reference(ReferenceFunction),
    /// Lookup and position matching.
    Search(SearchFunction),
    /// Value inspection and type conversion.
    Information(InformationFunction),
    /// Mathematical and random number operations.
    Math(MathFunction),
    /// Text manipulation and formatting.
    Text(TextFunction),
}

impl FunctionId {
    // Internal execution slots, not serialized identities. Separate family bands
    // avoid renumbering other families when one family gains a handler.
    pub(super) const FAMILY_SLOTS: usize = 256;

    /// Capacity of the internal per-handler execution counter array.
    pub(in crate::eval) const SLOT_COUNT: usize = 13 * Self::FAMILY_SLOTS;

    /// Map a typed handler to its non-serialized execution counter slot.
    pub(in crate::eval) fn index(self) -> usize {
        let (family, member) = match self {
            Self::Financial(member) => (0, member as usize),
            Self::Engineering(member) => (1, member as usize),
            Self::Matrix(member) => (2, member as usize),
            Self::Criteria(member) => (3, member as usize),
            Self::Database(member) => (4, member as usize),
            Self::Date(member) => (5, member as usize),
            Self::Logical(member) => (6, member as usize),
            Self::Statistics(member) => (7, member as usize),
            Self::Reference(member) => (8, member as usize),
            Self::Search(member) => (9, member as usize),
            Self::Information(member) => (10, member as usize),
            Self::Math(member) => (11, member as usize),
            Self::Text(member) => (12, member as usize),
        };
        if member >= Self::FAMILY_SLOTS {
            return Self::SLOT_COUNT;
        }
        family * Self::FAMILY_SLOTS + member
    }
}

impl FunctionId {
    /// Return canonical registration metadata with exhaustive typed dispatch.
    pub(in crate::eval) fn descriptor(self) -> &'static FunctionDescriptor {
        super::catalog::descriptor(self)
    }

    /// Canonical registry name of this typed builtin handler.
    pub fn canonical_name(self) -> &'static str {
        self.descriptor().name
    }
}
