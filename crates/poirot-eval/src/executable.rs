//! Borrowed bytecode execution handles. The tree backend exists only as a test oracle.
use crate::{
    EvaluationFailure,
    bytecode::{Compiled, Opcode},
    execution_observation::ArgumentOrigin,
};

#[cfg(test)]
use crate::bound::Expression;

/// Borrowed expression block in an immutable compiled program.
#[derive(Clone, Copy)]
pub(crate) enum ExecutableRef<'a> {
    /// Handler-bound tree expression.
    #[cfg(test)]
    Tree(&'a Expression),
    /// Expression block in an immutable compiled program.
    Bytecode {
        /// Program owning the block.
        program: &'a Compiled,
        /// Entry instruction index.
        start: usize,
    },
}
#[cfg(test)]
impl<'a> From<&'a Expression> for ExecutableRef<'a> {
    fn from(value: &'a Expression) -> Self {
        Self::Tree(value)
    }
}
impl<'a> ExecutableRef<'a> {
    /// Classify syntax provenance without evaluating the expression.
    pub(crate) fn origin(self) -> ArgumentOrigin {
        match self {
            #[cfg(test)]
            Self::Tree(e) => match e {
                Expression::Missing => ArgumentOrigin::Missing,
                Expression::Number(_)
                | Expression::String(_)
                | Expression::Bool(_)
                | Expression::Error(_) => ArgumentOrigin::Literal,
                Expression::Cell(_)
                | Expression::Row { .. }
                | Expression::Column { .. }
                | Expression::SheetSpan(_) => ArgumentOrigin::Reference,
                _ => ArgumentOrigin::Computed,
            },
            Self::Bytecode { program, start } => match start
                .checked_add(1)
                .and_then(|next| program.template.code.get(next))
                .map(|instruction| instruction.opcode)
            {
                Some(Opcode::Missing) => ArgumentOrigin::Missing,
                Some(Opcode::Number | Opcode::Text | Opcode::Boolean | Opcode::Error) => {
                    ArgumentOrigin::Literal
                }
                Some(Opcode::Reference) => ArgumentOrigin::Reference,
                _ => ArgumentOrigin::Computed,
            },
        }
    }
    /// Whether this handle denotes an omitted argument.
    pub(crate) fn is_missing(self) -> bool {
        self.origin() == ArgumentOrigin::Missing
    }
    /// Inspect literal array dimensions without executing its elements.
    pub(crate) fn array_shape(self) -> Option<(usize, usize)> {
        match self {
            #[cfg(test)]
            Self::Tree(Expression::Array(rows)) => {
                Some((rows.len(), rows.first().map_or(0, Vec::len)))
            }
            Self::Bytecode { program, start } => {
                let allocation = program.template.code.get(start.checked_add(1)?)?;
                (allocation.opcode == Opcode::ArrayAllocate)
                    .then_some((allocation.b as usize, allocation.c as usize))
            }
            #[cfg(test)]
            _ => None,
        }
    }
    /// Visit literal array element positions and their omitted-argument flags.
    pub(crate) fn visit_array_missing(
        self,
        mut visitor: impl FnMut(usize, bool) -> crate::EvalResult<()>,
    ) -> crate::EvalResult<()> {
        match self {
            #[cfg(test)]
            Self::Tree(Expression::Array(rows)) => {
                for (index, expression) in rows.iter().flatten().enumerate() {
                    visitor(index, matches!(expression, Expression::Missing))?;
                }
            }
            Self::Bytecode { program, start } if self.array_shape().is_some() => {
                let mut depth = 0;
                let mut index = 0;
                for (offset, instruction) in program
                    .template
                    .code
                    .get(start..)
                    .ok_or(EvaluationFailure::StalePlan)?
                    .iter()
                    .enumerate()
                {
                    if instruction.opcode == Opcode::Enter {
                        depth += 1;
                        if depth == 2 {
                            visitor(
                                index,
                                program
                                    .template
                                    .code
                                    .get(
                                        start
                                            .checked_add(offset)
                                            .and_then(|index| index.checked_add(1))
                                            .ok_or(EvaluationFailure::StalePlan)?,
                                    )
                                    .ok_or(EvaluationFailure::StalePlan)?
                                    .opcode
                                    == Opcode::Missing,
                            )?;
                            index += 1;
                        }
                    }
                    if instruction.opcode == Opcode::Leave {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    /// Materialize a budgeted omitted-element mask for literal arrays.
    pub(crate) fn array_missing(
        self,
        budget: &mut crate::BudgetMeter,
    ) -> crate::EvalResult<Vec<Vec<bool>>> {
        #[cfg(test)]
        if let Self::Tree(Expression::Array(rows)) = self {
            budget.allocation(rows.iter().map(Vec::len).sum())?;
            return Ok(rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|e| matches!(e, Expression::Missing))
                        .collect()
                })
                .collect());
        }
        if let Some((rows, columns)) = self.array_shape() {
            if columns == 0 {
                return Err(EvaluationFailure::StalePlan);
            }
            budget.allocation(
                rows.checked_mul(columns)
                    .ok_or(EvaluationFailure::StalePlan)?,
            )?;
            let mut result = vec![vec![false; columns]; rows];
            self.visit_array_missing(|index, missing| {
                let entry = result
                    .get_mut(index / columns)
                    .and_then(|row| row.get_mut(index % columns))
                    .ok_or(EvaluationFailure::StalePlan)?;
                *entry = missing;
                Ok(())
            })?;
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
}
/// Deferred argument blocks from a call or independently compiled programs.
#[derive(Clone, Copy)]
pub(crate) enum ArgumentsRef<'a> {
    /// No arguments for scalar host helpers.
    Empty,
    /// Independently compiled arguments supplied through adapter integration APIs.
    Programs(&'a [Compiled]),
    /// Bound-tree operands in source order.
    #[cfg(test)]
    Tree(&'a [Expression]),
    /// Linked bytecode argument descriptors.
    Bytecode {
        /// Program owning the descriptors and blocks.
        program: &'a Compiled,
        /// First argument descriptor index.
        first: usize,
        /// Number of syntactic argument positions.
        count: usize,
    },
}
#[cfg(test)]
impl<'a> From<&'a [Expression]> for ArgumentsRef<'a> {
    fn from(value: &'a [Expression]) -> Self {
        Self::Tree(value)
    }
}
#[cfg(test)]
impl<'a> From<&'a Vec<Expression>> for ArgumentsRef<'a> {
    fn from(value: &'a Vec<Expression>) -> Self {
        Self::Tree(value)
    }
}
#[cfg(test)]
impl<'a, const N: usize> From<&'a [Expression; N]> for ArgumentsRef<'a> {
    fn from(value: &'a [Expression; N]) -> Self {
        Self::Tree(value)
    }
}
impl<'a> ArgumentsRef<'a> {
    /// Count syntactic argument positions, including omissions.
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Programs(v) => v.len(),
            #[cfg(test)]
            Self::Tree(v) => v.len(),
            Self::Bytecode { count, .. } => count,
        }
    }
    /// Whether the call has no argument positions.
    pub(crate) fn is_empty(self) -> bool {
        self.len() == 0
    }
    /// Borrow one deferred argument by zero-based ordinal.
    pub(crate) fn get(self, ordinal: usize) -> Option<ExecutableRef<'a>> {
        match self {
            Self::Empty => None,
            Self::Programs(v) => v
                .get(ordinal)
                .map(|program| ExecutableRef::Bytecode { program, start: 0 }),
            #[cfg(test)]
            Self::Tree(v) => v.get(ordinal).map(ExecutableRef::Tree),
            Self::Bytecode {
                program,
                first,
                count,
            } => {
                if ordinal >= count {
                    return None;
                }
                let mut descriptor = first;
                for _ in 0..ordinal {
                    descriptor = program.template.code.get(descriptor)?.c as usize;
                }
                Some(ExecutableRef::Bytecode {
                    program,
                    start: descriptor.checked_add(1)?,
                })
            }
        }
    }
}
impl<'a> IntoIterator for ArgumentsRef<'a> {
    /// Deferred expression handle for one source argument.
    type Item = ExecutableRef<'a>;
    /// Source-order iterator over deferred argument handles.
    type IntoIter = ArgumentsIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        ArgumentsIter {
            arguments: self,
            index: 0,
        }
    }
}
/// Source-order iterator yielding deferred expression handles.
pub(crate) struct ArgumentsIter<'a> {
    arguments: ArgumentsRef<'a>,
    index: usize,
}
impl<'a> Iterator for ArgumentsIter<'a> {
    /// Deferred expression handle for the next source argument.
    type Item = ExecutableRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.arguments.get(self.index)?;
        self.index += 1;
        Some(value)
    }
}

/// Borrowed decoded reference geometry.
pub(crate) enum ReferenceRef<'a> {
    /// Single-cell coordinates with an optional sheet qualifier.
    Cell {
        /// Zero-based row.
        row: u32,
        /// Zero-based column.
        column: u32,
        /// Optional sheet spelling.
        sheet: Option<&'a str>,
    },
    /// Whole-column reference.
    Column {
        /// Zero-based column.
        column: u32,
        /// Optional sheet spelling.
        sheet: Option<&'a str>,
    },
    /// Whole-row reference.
    Row {
        /// Zero-based row.
        row: u32,
        /// Optional sheet spelling.
        sheet: Option<&'a str>,
    },
    /// Reference across ordered sheet endpoints.
    SheetSpan {
        /// First sheet spelling.
        first_sheet: &'a str,
        /// Last sheet spelling.
        last_sheet: &'a str,
        /// First cell endpoint.
        start: &'a poirot_formula::CellReference,
        /// Optional second cell endpoint.
        end: Option<&'a poirot_formula::CellReference>,
    },
}

impl<'a> From<&'a [Compiled]> for ArgumentsRef<'a> {
    fn from(programs: &'a [Compiled]) -> Self {
        Self::Programs(programs)
    }
}
