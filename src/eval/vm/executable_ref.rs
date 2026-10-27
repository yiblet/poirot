//! Borrowed bytecode execution handles. The tree backend exists only as a test oracle.

#[cfg(test)]
use crate::eval::compile::bound::Expression;
use crate::eval::compile::bytecode::{Compiled, Opcode};
use crate::eval::limits::EvaluationError;
use crate::eval::observe::ArgumentOrigin;

/// Borrowed expression block in an immutable compiled program.
#[derive(Clone, Copy)]
pub(in crate::eval) enum ExecutableRef<'a> {
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
    /// Borrow a contextual aggregation projection without executing a worksheet operand.
    pub(in crate::eval) fn aggregation(
        self,
    ) -> Result<
        Option<std::borrow::Cow<'a, crate::eval::compile::bytecode::aggregation::Plan>>,
        crate::eval::compile::bytecode::CompileError,
    > {
        match self {
            #[cfg(test)]
            Self::Tree(expression) => {
                crate::eval::compile::bytecode::aggregation::lower_tree(expression)
                    .map(|plan| plan.map(std::borrow::Cow::Owned))
            }
            Self::Bytecode { program, start } => Ok(program
                .template
                .aggregations
                .binary_search_by_key(&start, |e| e.start)
                .ok()
                .and_then(|i| program.template.aggregations.get(i))
                .map(|entry| std::borrow::Cow::Borrowed(&entry.plan))),
        }
    }

    /// Classify syntax provenance without evaluating the expression.
    pub(in crate::eval) fn origin(self) -> ArgumentOrigin {
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
    pub(in crate::eval) fn is_missing(self) -> bool {
        self.origin() == ArgumentOrigin::Missing
    }

    /// Inspect literal array dimensions without executing its elements.
    pub(in crate::eval) fn array_shape(self) -> Option<(usize, usize)> {
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
    pub(in crate::eval) fn visit_array_missing(
        self,
        mut visitor: impl FnMut(usize, bool) -> crate::eval::limits::EvalResult<()>,
    ) -> crate::eval::limits::EvalResult<()> {
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
                    .ok_or(EvaluationError::StalePlan)?
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
                                            .ok_or(EvaluationError::StalePlan)?,
                                    )
                                    .ok_or(EvaluationError::StalePlan)?
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
    pub(in crate::eval) fn array_missing(
        self,
        budget: &mut crate::eval::limits::BudgetMeter,
    ) -> crate::eval::limits::EvalResult<Vec<Vec<bool>>> {
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
                return Err(EvaluationError::StalePlan);
            }
            budget.allocation(
                rows.checked_mul(columns)
                    .ok_or(EvaluationError::StalePlan)?,
            )?;
            let mut result = vec![vec![false; columns]; rows];
            self.visit_array_missing(|index, missing| {
                let entry = result
                    .get_mut(index / columns)
                    .and_then(|row| row.get_mut(index % columns))
                    .ok_or(EvaluationError::StalePlan)?;
                *entry = missing;
                Ok(())
            })?;
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }
}
