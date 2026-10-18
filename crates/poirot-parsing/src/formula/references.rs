//! Pure reference decoding, range coercion and array constant checks.
use crate::formula::{BinaryOp, CellReference, Expr, ParseError, UnaryOp};

/// Parse a rectangular constant and return the remaining input.
pub(super) fn array_constant(expr: &Expr) -> bool {
    match expr {
        Expr::Number(_) | Expr::String(_) | Expr::Bool(_) | Expr::Error(_) => true,
        Expr::Unary {
            op: UnaryOp::Plus | UnaryOp::Minus,
            operand,
        } => matches!(operand.as_ref(), Expr::Number(_)),
        _ => false,
    }
}
/// Decode a column spelling into a zero-based worksheet coordinate.
pub(super) fn column(text: &str) -> Option<u32> {
    let mut value = 0u32;
    if text.is_empty() {
        return None;
    }
    for c in text.bytes() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        value = value
            .checked_mul(26)?
            .checked_add(u32::from(c.to_ascii_uppercase() - b'A') + 1)?;
    }
    (value <= 16384).then(|| value - 1)
}
/// Recognize a cell or whole-axis reference before composing ranges.
pub(super) fn partial_reference(text: &str, sheet: Option<String>) -> Option<Expr> {
    let absolute = text.starts_with('$');
    let text = text.trim_start_matches('$');
    if let Some(column) = column(text) {
        Some(Expr::Column {
            column,
            absolute,
            sheet,
        })
    } else {
        let row = text.parse::<u32>().ok()?;
        (1..=1048576).contains(&row).then(|| Expr::Row {
            row: row - 1,
            absolute,
            sheet,
        })
    }
}
/// Expand legal whole-axis pairs without changing the retained formula source.
pub(super) fn sheet_span_bounds(
    start: Expr,
    end: Option<Expr>,
) -> Result<(CellReference, Option<CellReference>), String> {
    let cell = |row, column, row_absolute, column_absolute| CellReference {
        row,
        column,
        row_absolute,
        column_absolute,
        sheet: None,
    };
    match (start, end) {
        (Expr::Cell(start), None) => Ok((start, None)),
        (Expr::Cell(start), Some(Expr::Cell(end))) => Ok((start, Some(end))),
        (
            Expr::Column {
                column: first,
                absolute: a,
                ..
            },
            Some(Expr::Column {
                column: last,
                absolute: b,
                ..
            }),
        ) => Ok((cell(0, first, true, a), Some(cell(1048575, last, true, b)))),
        (
            Expr::Row {
                row: first,
                absolute: a,
                ..
            },
            Some(Expr::Row {
                row: last,
                absolute: b,
                ..
            }),
        ) => Ok((cell(first, 0, a, true), Some(cell(last, 16383, b, true)))),
        _ => Err("sheet spans require cells or paired whole-axis references".into()),
    }
}
/// Primitive cell decoding shared with the no-AST classifier. No allocation on errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellCoordinates {
    /// Zero-based decoded row coordinate.
    pub row: u32,
    /// Zero-based decoded column coordinate.
    pub column: u32,
    /// Whether the row has a dollar-sign anchor.
    pub row_absolute: bool,
    /// Whether the column has a dollar-sign anchor.
    pub column_absolute: bool,
}
/// Decode an A1 token, retaining absolute-axis flags.
pub(super) fn cell_coordinates(text: &str) -> Result<Option<CellCoordinates>, ()> {
    let column_absolute = text.starts_with('$');
    let rest = text.trim_start_matches('$');
    let split = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    if split == 0 || split == rest.len() {
        return Ok(None);
    }
    // SAFETY: split is produced by str::find on a character predicate, or rest.len(), both valid UTF-8 boundaries.
    let row_text = &rest[split..];
    let row_absolute = row_text.starts_with('$');
    let digits = row_text.trim_start_matches('$');
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    // SAFETY: split is produced by str::find on a character predicate, or rest.len(), both valid UTF-8 boundaries.
    let col = column(&rest[..split]);
    let row = digits
        .parse::<u32>()
        .ok()
        .filter(|r| (1..=1048576).contains(r));
    match (col, row) {
        (Some(column), Some(row)) => Ok(Some(CellCoordinates {
            row: row - 1,
            column,
            row_absolute,
            column_absolute,
        })),
        _ => Err(()),
    }
}

/// Parse an optionally sheet-qualified A1 reference.
pub(super) fn cell_reference(
    text: &str,
    sheet: Option<String>,
) -> Result<Option<CellReference>, ParseError> {
    cell_coordinates(text)
        .map(|cell| {
            cell.map(|c| CellReference {
                row: c.row,
                column: c.column,
                row_absolute: c.row_absolute,
                column_absolute: c.column_absolute,
                sheet,
            })
        })
        .map_err(|()| ParseError {
            position: 0,
            message: "cell reference exceeds Excel grid".into(),
        })
}
/// Decode a compatible primitive axis pair before admitting generic operands.
/// POI gives a first column-shaped token lexical priority: A:OFFSET(...) is invalid,
/// while A1:Z retains Z as a defined-name operand. No workbook lookup occurs here.
pub(super) fn range_pair(left: Expr, right: Expr) -> Result<(Expr, Expr), String> {
    let axis = |expr: &Expr| match expr {
        Expr::Name(name) => partial_reference(name, None),
        Expr::QualifiedName { sheet, name } => partial_reference(name, Some(sheet.clone())),
        Expr::Column { .. } | Expr::Row { .. } => Some(expr.clone()),
        Expr::Number(n) if n.fract() == 0.0 && (1.0..=1048576.0).contains(n) => Some(Expr::Row {
            row: *n as u32 - 1,
            absolute: false,
            sheet: None,
        }),
        _ => None,
    };
    match (axis(&left), axis(&right)) {
        (Some(a @ Expr::Column { .. }), Some(mut b @ Expr::Column { .. }))
        | (Some(a @ Expr::Row { .. }), Some(mut b @ Expr::Row { .. })) => {
            inherit_sheet(&a, &mut b);
            return Ok((a, b));
        }
        (Some(Expr::Column { .. } | Expr::Row { .. }), _) => {
            return Err("incompatible primitive range endpoints".into());
        }
        _ => {}
    }
    if !range_operand(&left) || !range_operand(&right) {
        return Err("invalid range endpoint".into());
    }
    let mut right = right;
    inherit_sheet(&left, &mut right);
    Ok((left, right))
}
fn range_operand(expr: &Expr) -> bool {
    match expr {
        Expr::Cell(_) | Expr::Name(_) | Expr::QualifiedName { .. } | Expr::SheetSpan(_) => true,
        Expr::Error(error) => error == "#REF!",
        Expr::Function { name, .. } => {
            crate::formula::reference_classes::return_class(name)
                != Some(crate::formula::reference_classes::ReturnClass::Array)
        }
        Expr::Binary {
            op: BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection,
            ..
        } => true,
        _ => false,
    }
}
/// Propagate an endpoint's sheet qualifier across its range when omitted.
pub(super) fn inherit_sheet(left: &Expr, right: &mut Expr) {
    let sheet = match left {
        Expr::Cell(c) => &c.sheet,
        Expr::Column { sheet, .. } | Expr::Row { sheet, .. } => sheet,
        _ => return,
    };
    let target = match right {
        Expr::Cell(c) => &mut c.sheet,
        Expr::Column { sheet, .. } | Expr::Row { sheet, .. } => sheet,
        _ => return,
    };
    if target.is_none() {
        target.clone_from(sheet);
    }
}
