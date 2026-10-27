use poirot_functions::lookup::{Function, Search};

/// Lookup and position matching handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SearchFunction {
    /// LOOKUP handler.
    Lookup,
    /// XLOOKUP handler.
    XLookup,
    /// XMATCH handler.
    XMatch,

    /// MATCH handler.
    Match,
    /// VLOOKUP handler.
    VLookup,
    /// HLOOKUP handler.
    HLookup,
}

impl SearchFunction {
    /// Wrap the family helper, or return `None` for evaluator-owned handlers.
    pub(in crate::eval) fn helper(self) -> Option<Function> {
        match self {
            Self::Lookup => Some(Function::Search(Search::Lookup)),
            Self::XLookup => Some(Function::Search(Search::XLookup)),
            Self::XMatch => Some(Function::Search(Search::XMatch)),
            Self::Match | Self::VLookup | Self::HLookup => None,
        }
    }
}
