/// Selects the rows of a qualified Excel table reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSection {
    /// Data rows.
    Data,
    /// Header row.
    Headers,
    /// Totals row.
    Totals,
    /// Header, data and totals.
    All,
    /// Data row at the formula's row coordinate.
    CurrentRow,
    /// Adjacent header and data rows.
    HeadersAndData,
    /// Adjacent data and totals rows.
    DataAndTotals,
}

impl TableSection {
    /// Decode a case-insensitive single selector or its compiled pair encoding.
    pub fn parse(text: &str) -> Option<Self> {
        [
            ("#Data", Self::Data),
            ("#Headers", Self::Headers),
            ("#Totals", Self::Totals),
            ("#All", Self::All),
            ("#This Row", Self::CurrentRow),
            ("#Headers,#Data", Self::HeadersAndData),
            ("#Data,#Totals", Self::DataAndTotals),
        ]
        .into_iter()
        .find_map(|(spelling, section)| spelling.eq_ignore_ascii_case(text).then_some(section))
    }

    /// Decode a single bracketed keyword without accepting the compiled pair syntax.
    pub(super) fn keyword(text: &str) -> Option<Self> {
        Self::parse(text)
            .filter(|section| !matches!(section, Self::HeadersAndData | Self::DataAndTotals))
    }

    /// Admit only the ordered selector pairs defined by SpreadsheetML.
    pub(super) fn combine(self, second: Self) -> Option<Self> {
        match (self, second) {
            (Self::Headers, Self::Data) => Some(Self::HeadersAndData),
            (Self::Data, Self::Totals) => Some(Self::DataAndTotals),
            _ => None,
        }
    }

    /// Return the canonical spelling of the keywords, separated by commas for pairs.
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Data => "#Data",
            Self::Headers => "#Headers",
            Self::Totals => "#Totals",
            Self::All => "#All",
            Self::CurrentRow => "#This Row",
            Self::HeadersAndData => "#Headers,#Data",
            Self::DataAndTotals => "#Data,#Totals",
        }
    }
}
