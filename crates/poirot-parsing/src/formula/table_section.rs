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
}

impl TableSection {
    /// Decode a case-insensitive selector spelling.
    pub fn parse(text: &str) -> Option<Self> {
        [
            ("#Data", Self::Data),
            ("#Headers", Self::Headers),
            ("#Totals", Self::Totals),
            ("#All", Self::All),
            ("#This Row", Self::CurrentRow),
        ]
        .into_iter()
        .find_map(|(spelling, section)| spelling.eq_ignore_ascii_case(text).then_some(section))
    }

    /// Return canonical formula spelling.
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Data => "#Data",
            Self::Headers => "#Headers",
            Self::Totals => "#Totals",
            Self::All => "#All",
            Self::CurrentRow => "#This Row",
        }
    }
}
