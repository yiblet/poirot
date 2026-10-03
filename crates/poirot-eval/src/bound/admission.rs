//! Pure structural admission of caller-provided syntax.
//! This checks resource budgets, array shape and grid bounds, never function eligibility.
use poirot_formula::Expr as Syntax;

/// Structural failure while admitting caller-provided expression syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    /// Syntax exceeds supported node count or depth.
    ResourceLimit,
    /// Array syntax is empty or not rectangular.
    InvalidArray,
    /// Reference coordinates exceed worksheet bounds.
    InvalidReference,
}
impl std::fmt::Display for BindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ResourceLimit => "formula binding resource limit exceeded",
            Self::InvalidArray => "array expressions must be nonempty and rectangular",
            Self::InvalidReference => "reference exceeds spreadsheet grid bounds",
        })
    }
}
impl std::error::Error for BindingError {}
/// Validate node count, depth, array shape, and reference bounds before binding.
pub(super) fn validate_size(syntax: &Syntax) -> Result<(), BindingError> {
    let mut pending = vec![(syntax, 0usize)];
    let mut nodes = 0;
    while let Some((node, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 100_000 || depth >= 128 {
            return Err(BindingError::ResourceLimit);
        }
        match node {
            Syntax::Unary { operand, .. } => pending.push((operand, depth + 1)),
            Syntax::Binary { left, right, .. } => {
                pending.push((left, depth + 1));
                pending.push((right, depth + 1));
            }
            Syntax::Function { args, .. } => {
                if args.len() > 100_000 - nodes {
                    return Err(BindingError::ResourceLimit);
                }
                pending.extend(args.iter().map(|a| (a, depth + 1)));
            }
            Syntax::Array(rows) => {
                let width = rows.first().map_or(0, Vec::len);
                if width == 0 || rows.iter().any(|row| row.len() != width) {
                    return Err(BindingError::InvalidArray);
                }
                if rows
                    .iter()
                    .try_fold(0usize, |n, r| n.checked_add(r.len()))
                    .is_none_or(|n| n > 100_000 - nodes)
                {
                    return Err(BindingError::ResourceLimit);
                }
                pending.extend(rows.iter().flatten().map(|a| (a, depth + 1)));
            }
            Syntax::SheetSpan(span)
                if std::iter::once(&span.start)
                    .chain(span.end.iter())
                    .any(|c| {
                        c.row >= poirot_model::MAX_ROWS
                            || c.column >= poirot_model::MAX_COLUMNS
                            || c.sheet.is_some()
                    })
                    || span.first_sheet.is_empty()
                    || span.last_sheet.is_empty() =>
            {
                return Err(BindingError::InvalidReference);
            }
            Syntax::Cell(reference)
                if reference.row >= poirot_model::MAX_ROWS
                    || reference.column >= poirot_model::MAX_COLUMNS =>
            {
                return Err(BindingError::InvalidReference);
            }
            Syntax::Column { column, .. } if *column >= poirot_model::MAX_COLUMNS => {
                return Err(BindingError::InvalidReference);
            }
            Syntax::Row { row, .. } if *row >= poirot_model::MAX_ROWS => {
                return Err(BindingError::InvalidReference);
            }
            _ => {}
        }
    }
    Ok(())
}

impl From<crate::bytecode::CompileError> for BindingError {
    fn from(error: crate::bytecode::CompileError) -> Self {
        match error {
            crate::bytecode::CompileError::ResourceLimit => Self::ResourceLimit,
            crate::bytecode::CompileError::InvalidArray => Self::InvalidArray,
            crate::bytecode::CompileError::InvalidReference => Self::InvalidReference,
        }
    }
}
