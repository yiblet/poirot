use super::preparation_policy::PreparationPolicy;

/// Recognizes formulas that differ only in point-cell references and are proven not to spill.
/// Exact literal gaps and checked coordinates let discovery reuse that classification
/// without parsing each similar formula again.
#[derive(Default)]
pub(super) struct ScalarMatcher {
    pub(super) source: String,
    pub(super) cells: Vec<(usize, usize)>,
}

impl ScalarMatcher {
    /// Maximum source retained by the one-pattern locality optimization.
    pub(super) const MAX_SOURCE_BYTES: usize = 16 * 1024;

    /// Maximum variable point spans retained by the locality optimization.
    pub(super) const MAX_CELLS: usize = 64;

    /// Reuse storage for spans already validated by the canonical key pass.
    pub(super) fn update(&mut self, source: &str, cells: &[(usize, usize)]) -> bool {
        if source.len() > Self::MAX_SOURCE_BYTES || cells.len() > Self::MAX_CELLS {
            return false;
        }
        // Reserve both buffers before changing either value. A refusal leaves the
        // previous source and spans usable, even if the first reserve grew capacity.
        if self
            .source
            .try_reserve(source.len().saturating_sub(self.source.len()))
            .is_err()
            || self
                .cells
                .try_reserve(cells.len().saturating_sub(self.cells.len()))
                .is_err()
        {
            return false;
        }
        self.source.clear();
        // SAFETY: Reservation above admitted source.len() total bytes before clear.
        self.source.push_str(source);
        self.cells.clear();
        // SAFETY: Reservation above admitted cells.len() total spans before clear.
        self.cells.extend_from_slice(cells);
        true
    }

    /// Compare exact literal gaps and validate varying cells with the shared canonical decoder.
    pub(super) fn matches(&self, source: &str, policy: &PreparationPolicy) -> bool {
        let mut tail = source;
        let mut previous = 0;
        for &(start, end) in &self.cells {
            let Some(literal) = self.source.get(previous..start) else {
                return false;
            };
            let Some(rest) = tail.strip_prefix(literal) else {
                return false;
            };
            let Some((rest, cell)) = poirot_parsing::formula::classified::cell_prefix(rest) else {
                return false;
            };
            if cell.row >= policy.parse.max_rows || cell.column >= policy.parse.max_columns {
                return false;
            }
            tail = rest;
            previous = end;
        }
        self.source
            .get(previous..)
            .is_some_and(|literal| tail == literal)
    }
}
