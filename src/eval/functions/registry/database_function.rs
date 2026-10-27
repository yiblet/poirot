use poirot_functions::database::DatabaseFunction as DatabaseHelper;

/// Database functions aggregating records selected by a criteria table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DatabaseFunction {
    /// DAVERAGE handler.
    Average,
    /// DCOUNT handler.
    Count,
    /// DCOUNTA handler.
    CountA,
    /// DGET handler.
    Get,
    /// DMAX handler.
    Max,
    /// DMIN handler.
    Min,
    /// DPRODUCT handler.
    Product,
    /// DSTDEV handler.
    Stdev,
    /// DSTDEVP handler.
    StdevP,
    /// DSUM handler.
    Sum,
    /// DVAR handler.
    Var,
    /// DVARP handler.
    VarP,
}

impl DatabaseFunction {
    /// Map this builtin identity to its family helper.
    pub(in crate::eval) fn helper(self) -> DatabaseHelper {
        match self {
            Self::Average => DatabaseHelper::Average,
            Self::Count => DatabaseHelper::Count,
            Self::CountA => DatabaseHelper::CountA,
            Self::Get => DatabaseHelper::Get,
            Self::Max => DatabaseHelper::Max,
            Self::Min => DatabaseHelper::Min,
            Self::Product => DatabaseHelper::Product,
            Self::Stdev => DatabaseHelper::Stdev,
            Self::StdevP => DatabaseHelper::StdevP,
            Self::Sum => DatabaseHelper::Sum,
            Self::Var => DatabaseHelper::Var,
            Self::VarP => DatabaseHelper::VarP,
        }
    }
}
