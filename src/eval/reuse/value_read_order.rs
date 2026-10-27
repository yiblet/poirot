use crate::model::query::RangeMembership;

/// Ordered actual value reads; the common single span needs no heap allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) enum ValueReadOrder {
    /// No point value was read.
    #[default]
    Empty,
    /// One consecutive traversal or one cell.
    One(RangeMembership),
    /// Nonconsecutive traversals retained in execution order.
    Many(Vec<RangeMembership>),
    /// Optional evidence could not be allocated; normal execution remains available.
    Unavailable,
}

impl ValueReadOrder {
    pub(super) fn last_mut(&mut self) -> Option<&mut RangeMembership> {
        match self {
            Self::One(span) => Some(span),
            Self::Many(spans) => spans.last_mut(),
            _ => None,
        }
    }

    pub(super) fn push(&mut self, span: RangeMembership) {
        match self {
            Self::Empty => *self = Self::One(span),
            Self::One(first) => {
                let mut spans = Vec::new();
                if spans.try_reserve(2).is_err() {
                    *self = Self::Unavailable;
                    return;
                }
                // SAFETY: try_reserve(2) succeeded above, providing capacity for both spans.
                spans.push(*first);
                // SAFETY: Exactly one of the two reserved slots has been filled.
                spans.push(span);
                *self = Self::Many(spans);
            }
            Self::Many(spans) => {
                if spans.try_reserve(1).is_err() {
                    *self = Self::Unavailable;
                    return;
                }
                // SAFETY: try_reserve(1) succeeded immediately above.
                spans.push(span);
            }
            Self::Unavailable => {}
        }
    }

    /// Visit the retained spans without expanding their coordinates.
    pub fn iter(&self) -> std::slice::Iter<'_, RangeMembership> {
        match self {
            Self::One(span) => std::slice::from_ref(span).iter(),
            Self::Many(spans) => spans.iter(),
            _ => [].iter(),
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::One(_) => 1,
            Self::Many(spans) => spans.len(),
            _ => 0,
        }
    }
}
