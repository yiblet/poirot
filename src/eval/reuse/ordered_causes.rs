use super::key::Key;
use super::validity::Validity;
use crate::model::CellAddress;
use crate::model::query::RangeMembership;

/// Dirty causes in consumed order, with lazy allocation-free single-span traversal.
pub(in crate::eval) enum OrderedCauses {
    /// One dirty input needs no temporary collection.
    One(Option<(Key, u64)>),
    /// A single ordered read span can yield dirty coordinates on demand.
    Span {
        span: RangeMembership,
        next: Option<CellAddress>,
        remaining: usize,
    },
    /// Irregular traces require sorting by the consumed read order.
    Many(std::vec::IntoIter<(Key, u64)>),
}

impl OrderedCauses {
    /// Find the next cause using a short cache borrow, released before recursive evaluation.
    pub fn next(
        &mut self,
        validity: &Validity,
        budget: &mut crate::eval::limits::BudgetMeter,
    ) -> crate::eval::limits::EvalResult<Option<(Key, u64)>> {
        match self {
            Self::One(cause) => Ok(cause.take()),
            Self::Many(causes) => Ok(causes.next()),
            Self::Span {
                span,
                next,
                remaining,
            } => {
                if *remaining == 0 {
                    return Ok(None);
                }
                let Validity::Causes(causes) = validity else {
                    return Err(crate::eval::limits::EvaluationFailure::StalePlan);
                };
                while let Some(address) = next.take() {
                    budget.charge(crate::eval::limits::WorkKind::Comparison, 1)?;
                    if address != span.last {
                        *next = if span.first.col == span.last.col {
                            address.row.checked_add(1).map(|row| CellAddress {
                                row,
                                col: address.col,
                            })
                        } else {
                            address.col.checked_add(1).map(|col| CellAddress {
                                row: address.row,
                                col,
                            })
                        };
                    }
                    let key = (span.sheet, address);
                    if let Some(&version) = causes.get(&key) {
                        *remaining = remaining.saturating_sub(1);
                        return Ok(Some((key, version)));
                    }
                }
                Ok(None)
            }
        }
    }
}
