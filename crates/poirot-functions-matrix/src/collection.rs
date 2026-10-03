use super::{
    Budget, Cell, CellError, Error, Input, Matrix, MatrixCollection, Result, finite, value,
};

/// Source-specific AreaEval collection: skipped cells compact the numeric
/// sequence, which is then zero-padded into the original rectangle. This is a
/// pinned POI behavior, not ordinary element-wise spreadsheet coercion.
pub(super) fn collect_matrix(
    area: &Matrix<Cell>,
    policy: MatrixCollection,
    budget: Budget,
) -> Result<Matrix<f64>> {
    budget.check(Some(area.values.len()), Some(area.values.len()))?;
    let mut numbers = Vec::with_capacity(area.values.len());
    for cell in &area.values {
        match cell {
            Cell::Number(n) => numbers.push(*n),
            Cell::Error(e) => return Err(Error::Formula(*e)),
            Cell::Blank if policy == MatrixCollection::Determinant => return value(),
            Cell::Blank if policy == MatrixCollection::Transpose => numbers.push(0.0),
            _ => {}
        }
    }
    if numbers.is_empty() || (numbers.len() == 1 && policy != MatrixCollection::Transpose) {
        return value();
    }
    numbers.resize(area.values.len(), 0.0);
    Matrix::new(area.rows, area.cols, numbers)
}

/// TREND rejects any non NumericValueEval (including source error values).
pub(super) fn collect_trend(area: &Matrix<Cell>, budget: Budget) -> Result<Matrix<f64>> {
    budget.check(Some(area.values.len()), Some(area.values.len()))?;
    let mut numbers = Vec::with_capacity(area.values.len());
    for cell in &area.values {
        match cell {
            Cell::Number(n) => numbers.push(*n),
            Cell::Bool(b) => numbers.push(if *b { 1.0 } else { 0.0 }),
            _ => return value(),
        }
    }
    Matrix::new(area.rows, area.cols, numbers)
}
fn product_term(c: &Cell, scalar: bool) -> Result<f64> {
    match c {
        Cell::Number(n) => Ok(*n),
        Cell::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Cell::Error(e) => Err(Error::Formula(*e)),
        Cell::Blank | Cell::Text(_) if !scalar => Ok(0.0),
        _ => value(),
    }
}
/// Multiply corresponding admitted elements and sum their products.
pub(super) fn sumproduct(args: &[Input], budget: Budget) -> Result<f64> {
    let first = args.first().ok_or(Error::Formula(CellError::Value))?;
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
            product *= product_term(c, true)?;
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
                        return Err(Error::Formula(*e));
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
            p *= product_term(
                m.values.get(i).ok_or(Error::Formula(CellError::Value))?,
                false,
            )?;
        }
        sum += p;
    }
    finite(&[sum])?;
    Ok(sum)
}
