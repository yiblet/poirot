use super::preparation_policy::PreparationPolicy;
use poirot_parsing::formula::classified::{CellCoordinates, TokenKind, Tokens};

/// Scratch for one source's exact fixed token key and ordered point coordinates.
#[derive(Clone, Default)]
pub(super) struct TemplateLexemes {
    pub(super) key: Vec<u8>,
    pub(super) cells: Vec<CellCoordinates>,
    pub(super) point_range: bool,
}

impl TemplateLexemes {
    /// Bound optional key and coordinate scratch independently of the source parser.
    pub(super) const MAX_SOURCE_BYTES: usize = 16 * 1024;

    /// Build the entire key before attempting reuse, retaining each fixed token byte and kind.
    pub(super) fn scan(&mut self, source: &str, policy: &PreparationPolicy) -> Option<()> {
        self.key.clear();
        self.cells.clear();
        self.point_range = false;
        if source.len() > Self::MAX_SOURCE_BYTES {
            return None;
        }
        for token in Tokens::new(source) {
            let token = token.ok()?;
            let tag = match token.kind {
                TokenKind::Cell(cell) => {
                    if self.cells.len() >= policy.parse.max_expressions
                        || cell.row >= policy.parse.max_rows
                        || cell.column >= policy.parse.max_columns
                    {
                        return None;
                    }
                    self.cells.try_reserve(1).ok()?;
                    self.key.try_reserve(1).ok()?;
                    // SAFETY: Each reservation admitted one additional item in its buffer.
                    self.cells.push(cell);
                    self.key.push(0);
                    continue;
                }
                TokenKind::Literal => 1u8,
                TokenKind::Number => 2,
                TokenKind::Text => 3,
                TokenKind::FixedReference => 4,
            };
            self.point_range |= token.raw == ":";
            self.fixed(tag, token.raw)?;
        }
        (!self.cells.is_empty()).then_some(())
    }

    /// Encode the token kind and byte length, retaining a short header for ordinary spellings.
    fn fixed(&mut self, tag: u8, raw: &str) -> Option<()> {
        const MAX_INLINE: usize = 31;
        let length = raw.len();
        let wide = length >= MAX_INLINE;
        self.key
            .try_reserve(length.checked_add(if wide {
                1 + std::mem::size_of::<usize>()
            } else {
                1
            })?)
            .ok()?;
        // SAFETY: The reservation covers the header, optional length word, and complete token.
        self.key.push((tag << 5) | length.min(MAX_INLINE) as u8);
        if wide {
            self.key.extend_from_slice(&length.to_le_bytes());
        }
        self.key.extend_from_slice(raw.as_bytes());
        Some(())
    }
}
