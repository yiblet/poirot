use crate::matrix::{Budget, MatrixError, Result, selection_work};

/// Store the record positions that belong to one pivot axis group.
/// Positions stay in source order so that reductions such as CONCAT keep that order.
#[derive(Debug)]
pub struct PivotMembers {
    ordinals: Vec<usize>,
}

impl PivotMembers {
    /// Sort a group's record positions, rejecting duplicates and positions >= `records`.
    /// The budget bounds the stored members and estimated sorting work.
    pub fn new(ordinals: &[usize], records: usize, budget: Budget) -> Result<Self> {
        budget.check(
            Some(ordinals.len()),
            selection_work(ordinals.len(), ordinals.len(), 1),
        )?;
        if ordinals.iter().any(|&ordinal| ordinal >= records) {
            return Err(MatrixError::Formula(crate::scalar::CellError::Value));
        }
        let mut ordered = Vec::new();
        ordered
            .try_reserve_exact(ordinals.len())
            .map_err(|_| MatrixError::AllocationFailure)?;
        // SAFETY: Capacity for this unchanged slice was reserved locally.
        ordered.extend_from_slice(ordinals);
        // SAFETY: usize's fixed total order cannot panic.
        ordered.sort_unstable();
        if ordered.windows(2).any(|pair| pair.first() == pair.get(1)) {
            return Err(MatrixError::Formula(crate::scalar::CellError::Value));
        }
        Ok(Self { ordinals: ordered })
    }

    /// Borrow this group's record positions in source order.
    pub fn ordinals(&self) -> &[usize] {
        &self.ordinals
    }

    /// Select records belonging to both axis groups, preserving original source order.
    pub fn intersection(&self, other: &Self, budget: Budget) -> Result<Vec<usize>> {
        let work = self
            .ordinals
            .len()
            .checked_add(other.ordinals.len())
            .ok_or(MatrixError::SizeOverflow)?;
        let capacity = self.ordinals.len().min(other.ordinals.len());
        budget.check(Some(capacity), Some(work))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(capacity)
            .map_err(|_| MatrixError::AllocationFailure)?;
        let mut left = self.ordinals.iter().peekable();
        let mut right = other.ordinals.iter().peekable();
        while let (Some(&a), Some(&b)) = (left.peek(), right.peek()) {
            match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    left.next();
                }
                std::cmp::Ordering::Greater => {
                    right.next();
                }
                std::cmp::Ordering::Equal => {
                    // SAFETY: Each iteration consumes one distinct member of both inputs, so at most capacity members are emitted.
                    output.push(*a);
                    left.next();
                    right.next();
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::PivotMembers;
    use crate::matrix::{Budget, MatrixError};

    const BUDGET: Budget = Budget {
        max_cells: 100,
        max_work: 1000,
    };

    #[test]
    fn asymmetric_groups_intersect_in_source_order() {
        let rows = PivotMembers::new(&[5, 0, 3, 1], 6, BUDGET).unwrap();
        let columns = PivotMembers::new(&[4, 3, 0], 6, BUDGET).unwrap();
        assert_eq!(rows.intersection(&columns, BUDGET).unwrap(), [0, 3]);
        assert_eq!(rows.ordinals(), [0, 1, 3, 5]);
        let empty = PivotMembers::new(&[], 6, BUDGET).unwrap();
        assert!(empty.intersection(&rows, BUDGET).unwrap().is_empty());
    }

    #[test]
    fn malformed_members_and_budgets_are_rejected() {
        assert!(PivotMembers::new(&[0, 0], 2, BUDGET).is_err());
        assert!(PivotMembers::new(&[2], 2, BUDGET).is_err());
        let rows = PivotMembers::new(&[0, 1], 2, BUDGET).unwrap();
        assert!(matches!(
            rows.intersection(
                &rows,
                Budget {
                    max_cells: 1,
                    max_work: 100
                }
            ),
            Err(MatrixError::BudgetExceeded)
        ));
        assert!(
            rows.intersection(
                &rows,
                Budget {
                    max_cells: 100,
                    max_work: 3
                }
            )
            .is_err()
        );
    }
}
