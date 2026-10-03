use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Invalid value representation, coordinates, or model metadata.
/// Formula errors are stored as [`CellError`] values instead.
pub struct ValueError(
    #[doc = "Model validation diagnostic carried across crate boundaries."] pub String,
);
impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ValueError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
/// A spreadsheet error value such as `#DIV/0!` or `#N/A`.
/// These values may be handled by formulas such as IFERROR; resource exhaustion
/// and unsupported execution capabilities are separate operational failures.
pub enum CellError {
    /// `#NULL!`: reference intersection has no result.
    Null,
    /// `#DIV/0!`: division by zero or an empty numeric denominator.
    Div0,
    /// `#VALUE!`: operand type or shape is invalid.
    Value,
    /// `#REF!`: reference is invalid.
    Ref,
    /// `#NAME?`: name or function is unresolved.
    Name,
    /// `#NUM!`: numeric domain or convergence failure.
    Num,
    /// `#N/A`: requested value is unavailable.
    NA,
    /// `#GETTING_DATA`: external data is pending.
    GettingData,
    /// `#SPILL!`: dynamic result cannot occupy its footprint.
    Spill,
    /// `#CALC!`: array calculation cannot produce a valid result.
    Calc,
}
impl fmt::Display for CellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Null => "#NULL!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::NA => "#N/A",
            Self::GettingData => "#GETTING_DATA",
            Self::Spill => "#SPILL!",
            Self::Calc => "#CALC!",
        })
    }
}
impl FromStr for CellError {
    /// Validation diagnostic for an unrecognized spreadsheet error spelling.
    type Err = ValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "#NULL!" => Ok(Self::Null),
            "#DIV/0!" => Ok(Self::Div0),
            "#VALUE!" => Ok(Self::Value),
            "#REF!" => Ok(Self::Ref),
            "#NAME?" => Ok(Self::Name),
            "#NUM!" => Ok(Self::Num),
            "#N/A" => Ok(Self::NA),
            "#GETTING_DATA" => Ok(Self::GettingData),
            "#SPILL!" => Ok(Self::Spill),
            "#CALC!" => Ok(Self::Calc),
            _ => Err(ValueError(format!("unknown Excel error: {s}"))),
        }
    }
}
