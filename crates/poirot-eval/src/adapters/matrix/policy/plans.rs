//! Immutable checked admission plans; application remains an ordered port effect.
use super::*;

/// Checked element and work bounds admitted before invoking a matrix algorithm.
pub(super) struct ResourcePlan {
    cells: Option<usize>,
    work: Option<usize>,
}

impl ResourcePlan {
    /// Bound frequency output and bin-search work.
    pub(super) fn frequency(data: usize, bins: usize) -> Self {
        Self {
            cells: bins.checked_add(1),
            work: data
                .checked_mul(usize::BITS as usize + 1)
                .and_then(|n| n.checked_add(bins)),
        }
    }

    /// Bound matrix multiplication output and multiply-add work.
    pub(super) fn multiply(a: &domain::Matrix<f64>, b: &domain::Matrix<f64>) -> Self {
        let cells = a.rows().checked_mul(b.cols());
        Self {
            cells,
            work: cells.and_then(|n| n.checked_mul(a.cols())),
        }
    }

    /// Bound transpose or determinant/inverse scratch storage and work.
    pub(super) fn unary(a: &domain::Matrix<f64>, transpose: bool) -> Self {
        let n = a.rows();
        let cells = a.values().len();
        Self {
            cells: cells.checked_mul(3).and_then(|c| c.checked_add(n)),
            work: if transpose {
                Some(cells)
            } else {
                n.checked_mul(n)
                    .and_then(|n2| n2.checked_mul(n))
                    .and_then(|n3| n3.checked_mul(3))
            },
        }
    }

    /// Bound regression scratch storage and prediction work from operand dimensions.
    pub(super) fn trend(
        y: &domain::Matrix<f64>,
        x: Option<&domain::Matrix<f64>>,
        nx: Option<&domain::Matrix<f64>>,
        constant: bool,
    ) -> Self {
        let n = y.values().len();
        let predictors = x.map_or(1, |a| {
            if y.rows() == 1 && a.cols() > 1 {
                a.rows()
            } else if y.rows() > 1 && y.cols() > 1 {
                1
            } else {
                a.cols()
            }
        });
        let q = predictors.checked_add(usize::from(constant));
        let predictions = nx.map_or(n, |a| {
            if predictors == 1 {
                a.values().len()
            } else {
                a.rows()
            }
        });
        let cells = q.and_then(|q| {
            n.checked_mul(q)?
                .checked_mul(2)?
                .checked_add(q)?
                .checked_add(n)?
                .checked_add(predictions.checked_mul(predictors)?)?
                .checked_add(predictions)
        });
        let work = q.and_then(|q| {
            n.checked_mul(q)?
                .checked_mul(q)?
                .checked_mul(4)?
                .checked_add(predictions.checked_mul(predictors)?)
        });
        Self { cells, work }
    }

    /// Reject overflow or host capacity exhaustion before algorithm execution.
    pub(super) fn admit<P: EvaluationPort>(
        &self,
        p: &mut P,
    ) -> Result<(), Failure<P::Operational>> {
        preflight(p, self.cells, self.work)
    }
}
