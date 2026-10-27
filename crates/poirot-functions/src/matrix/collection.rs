use super::{
    Budget, Cell, CellError, Input, Matrix, MatrixCollection, MatrixError, Result, finite, value,
};

/// Admit a numeric matrix without changing coordinates under `SquareNumeric`. Other
/// policies preserve pinned POI compaction and padding of skipped members.
pub(super) fn coerce_matrix_collection(
    area: &Matrix<Cell>,
    policy: MatrixCollection,
    budget: Budget,
) -> Result<Matrix<f64>> {
    budget.check(Some(area.values.len()), Some(area.values.len()))?;
    let mut numbers = Vec::with_capacity(area.values.len());
    let mut admission =
        crate::coercion::collections::MatrixAdmission::new(policy, area.rows, area.cols);
    for cell in &area.values {
        if let Some(number) = admission
            .coerce_matrix_member(cell.into())
            .map_err(MatrixError::Formula)?
        {
            numbers.push(number);
        }
    }
    admission.coerce_finish().map_err(MatrixError::Formula)?;
    if policy != MatrixCollection::SquareNumeric {
        numbers.resize(area.values.len(), 0.0);
    }
    Matrix::new(area.rows, area.cols, numbers)
}

/// TREND rejects any non NumericValueEval (including source error values).
pub(super) fn coerce_trend_collection(area: &Matrix<Cell>, budget: Budget) -> Result<Matrix<f64>> {
    budget.check(Some(area.values.len()), Some(area.values.len()))?;
    let mut numbers = Vec::with_capacity(area.values.len());
    for cell in &area.values {
        numbers.push(
            crate::coercion::collections::coerce_trend_member(cell.into())
                .map_err(MatrixError::Formula)?,
        );
    }
    Matrix::new(area.rows, area.cols, numbers)
}
fn coerce_sumproduct_term(c: &Cell, scalar: bool) -> Result<f64> {
    crate::coercion::collections::coerce_sumproduct_member(c.into(), scalar)
        .map_err(MatrixError::Formula)
}
/// Multiply corresponding admitted elements and sum their products.
pub(super) fn sumproduct(args: &[Input], budget: Budget) -> Result<f64> {
    let first = args.first().ok_or(MatrixError::Formula(CellError::Value))?;
    let scalar = match first {
        Input::Scalar(Cell::Number(_) | Cell::Bool(_)) | Input::Reference(_) => true,
        Input::Scalar(_) => return value(),
        Input::Area(m) => m.values.len() == 1,
    };
    if scalar {
        budget.check(Some(0), Some(args.len()))?;
        let mut product = 1.0;
        for arg in args {
            let c = match arg {
                Input::Scalar(c) | Input::Reference(c) => c,
                // SAFETY: This match arm explicitly requires m.values.len() == 1.
                Input::Area(m) if m.values.len() == 1 => &m.values[0],
                _ => return value(),
            };
            product *= coerce_sumproduct_term(c, true)?;
        }
        finite(&[product])?;
        return Ok(product);
    }
    let Input::Area(first) = first else {
        return value();
    };
    budget.check(Some(0), first.values.len().checked_mul(args.len()))?;
    // POI casts all arguments to areas before checking dimensions.
    if args
        .iter()
        .any(|a| matches!(a, Input::Scalar(_) | Input::Reference(_)))
    {
        return value();
    }
    if args
        .iter()
        .any(|a| matches!(a,Input::Area(m) if m.rows!=first.rows || m.cols!=first.cols))
    {
        let scan = args.iter().try_fold(0usize, |acc, a| match a {
            Input::Area(m) => acc.checked_add(m.values.len()),
            _ => Some(acc),
        });
        budget.check(Some(0), scan)?;
        for arg in args.iter().skip(1) {
            if let Input::Area(m) = arg {
                for cell in &m.values {
                    if let Cell::Error(e) = cell {
                        return Err(MatrixError::Formula(*e));
                    }
                }
            }
        }
        return value();
    }
    let mut sum = 0.0;
    for i in 0..first.values.len() {
        let mut p = 1.0;
        for arg in args {
            let Input::Area(m) = arg else { return value() };
            p *= coerce_sumproduct_term(
                m.values
                    .get(i)
                    .ok_or(MatrixError::Formula(CellError::Value))?,
                false,
            )?;
        }
        sum += p;
    }
    finite(&[sum])?;
    Ok(sum)
}
