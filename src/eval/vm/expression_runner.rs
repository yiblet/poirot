//! Expression admission, context observation, and lazy typed dispatch.

#[cfg(test)]
use super::admission_order::AdmissionOrder;
use super::bytecode_vm::Vm;
#[cfg(test)]
use super::call_admission::CallAdmission;
use super::executable_ref::ExecutableRef;
#[cfg(test)]
use super::operators::Operators;
#[cfg(test)]
use super::reference_atoms::ReferenceAtoms;
#[cfg(test)]
use super::reference_ref::ReferenceRef;
#[cfg(test)]
use super::resolved_call::ResolvedCall;
#[cfg(test)]
use crate::eval::compile::bound::Expression;
#[cfg(test)]
use crate::eval::config::BuiltinSemanticsPolicy;
#[cfg(test)]
use crate::eval::coordinates::EvaluationTarget;
#[cfg(test)]
use crate::eval::functions::registry::FunctionId;
use crate::eval::limits::EvalResult;
#[cfg(test)]
use crate::eval::limits::WorkKind;
#[cfg(test)]
use crate::eval::observe::ConsumedBuiltinInvocation;
#[cfg(test)]
use crate::eval::ports::DependencyRead;
#[cfg(test)]
use crate::eval::session::ObservationRecorder;
use crate::eval::session::{ExpressionDemand, Session};
#[cfg(test)]
use crate::eval::session::{NameResolver, ReadRecorder, ResultProjection};
use crate::eval::values::Value;
#[cfg(test)]
use crate::eval::values::numeric_outcome;
#[cfg(test)]
use crate::eval::values::{ArrayView, VALUE_ERROR, convert_legacy_missing_blank, err};
#[cfg(test)]
use crate::eval::{adapters, compile, functions, host, observe};
use crate::model::CellAddress;
#[cfg(test)]
use crate::model::CellError::{Name, Ref};
#[cfg(test)]
use crate::model::CellValue::{Boolean, Error, Text};

/// Executes deferred expression blocks, dispatching bytecode to the VM.
pub(in crate::eval) struct ExpressionRunner<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ExpressionRunner<'s, 'a> {
    /// Borrow the session that expression execution reads and charges.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Evaluate a deferred operand under its consumer's demand, restoring it on failure too.
    pub(in crate::eval) fn expr_with_demand<'expression>(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: impl Into<ExecutableRef<'expression>>,
        demand: ExpressionDemand,
    ) -> EvalResult<Value> {
        let previous = std::mem::replace(&mut self.session.state.expression_demand, demand);
        let result = self.expr(sheet, at, expression);
        self.session.state.expression_demand = previous;
        result
    }

    /// Execute a deferred bytecode block (or the unit-test-only tree oracle).
    pub(in crate::eval) fn expr<'expression>(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: impl Into<ExecutableRef<'expression>>,
    ) -> EvalResult<Value> {
        match expression.into() {
            #[cfg(test)]
            ExecutableRef::Tree(expression) => self.tree_expr(sheet, at, expression),
            ExecutableRef::Bytecode { program, start } => {
                if start == 0 {
                    Vm::new(self.session).run(sheet, at, program)
                } else {
                    Vm::new(self.session).execute(sheet, at, program, start)
                }
            }
        }
    }

    /// Execute one bound-tree node under depth accounting (unit-test oracle).
    #[cfg(test)]
    fn tree_expr(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: &Expression,
    ) -> EvalResult<Value> {
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::Expression, 1)?;
        self.session.state.runtime.budget.enter_depth()?;
        let result =
            ObservationRecorder::new(self.session).observed_expression(sheet, at, expression);
        self.session.state.runtime.budget.leave_depth();
        result
    }

    /// Inspect the builtin selected by a bound expression and active profile policy.
    #[cfg(test)]
    pub(in crate::eval) fn builtin_invocation<'expr>(
        &self,
        expression: &'expr Expression,
    ) -> Option<(FunctionId, &'expr compile::bound::Arguments)> {
        observe::builtin_call(expression).or_else(|| {
            if self.session.policies.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
                && let Expression::UnresolvedFunction(arguments) = expression
            {
                functions::registry::resolve_excel_function(
                    &poirot_parsing::formula::FunctionName::from(arguments.resolved_spelling()),
                )
                .map(|handler| (handler, arguments))
            } else {
                None
            }
        })
    }

    /// Admit and execute one bound-tree node without repeating depth accounting.
    #[cfg(test)]
    pub(in crate::eval) fn inner(
        &mut self,
        s: usize,
        at: CellAddress,
        expression: &Expression,
    ) -> EvalResult<Value> {
        let invocation = self.builtin_invocation(expression);
        CallAdmission::new(self.session).admit(
            ResolvedCall {
                invocation: invocation.map(|(handler, arguments)| ConsumedBuiltinInvocation {
                    target: EvaluationTarget {
                        sheet: s,
                        address: at,
                    },
                    handler,
                    parsed_spelling: arguments.source_name.as_str(),
                    resolved_spelling: arguments.resolved_spelling(),
                    argument_count: arguments.expressions.len(),
                }),
                read_builtin_policy: matches!(expression, Expression::UnresolvedFunction(_)),
            },
            AdmissionOrder::ObserverBeforePolicy,
        )?;
        if let Some((function, arguments)) = invocation {
            return adapters::execute(
                self.session,
                EvaluationTarget {
                    sheet: s,
                    address: at,
                },
                function,
                (&arguments.expressions).into(),
            );
        }
        match expression {
            // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
            Expression::Number(value) => {
                Ok(numeric_outcome::convert_numeric_literal(*value)?.into())
            }
            Expression::String(text) => {
                self.session.state.runtime.budget.text(text.len())?;
                Ok(Text(text.clone()).into())
            }
            Expression::Bool(value) => Ok(Boolean(*value).into()),
            Expression::Error(error) => Ok(Error(error.parse().unwrap_or(VALUE_ERROR)).into()),
            // TODO(coercion-non-argument): [MEDIUM] Legacy expression evaluation loses omission identity when it represents missing syntax as blank.
            Expression::Missing => Ok(convert_legacy_missing_blank().into()),
            Expression::SheetSpan(span) => ReferenceAtoms::new(self.session).resolve(
                s,
                at,
                ReferenceRef::SheetSpan {
                    first_sheet: &span.first_sheet,
                    last_sheet: &span.last_sheet,
                    start: &span.start,
                    end: span.end.as_ref(),
                },
            ),
            Expression::Cell(cell) => ReferenceAtoms::new(self.session).resolve(
                s,
                at,
                ReferenceRef::Cell {
                    row: cell.row,
                    column: cell.column,
                    sheet: cell.sheet.as_deref(),
                },
            ),
            Expression::Column { column, sheet, .. } => ReferenceAtoms::new(self.session).resolve(
                s,
                at,
                ReferenceRef::Column {
                    column: *column,
                    sheet: sheet.as_deref(),
                },
            ),
            Expression::Row { row, sheet, .. } => ReferenceAtoms::new(self.session).resolve(
                s,
                at,
                ReferenceRef::Row {
                    row: *row,
                    sheet: sheet.as_deref(),
                },
            ),
            Expression::Name(name) => NameResolver::new(self.session).defined_name(s, s, at, name),
            Expression::QualifiedName { sheet, name } => {
                match NameResolver::new(self.session).sheet(s, &Some(sheet.clone()))? {
                    Some(scope) => NameResolver::new(self.session).defined_name(s, scope, at, name),
                    None => Ok(err(Ref)),
                }
            }
            Expression::Array(rows) => {
                let count = rows.iter().map(Vec::len).sum();
                self.session.state.runtime.budget.allocation(count)?;
                let mut result = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut values = Vec::with_capacity(row.len());
                    for expression in row {
                        let value = self.expr(s, at, expression)?;
                        values.push(
                            ResultProjection::new(self.session).select_legacy_scalar(at, value)?,
                        );
                    }
                    result.push(values);
                }
                Ok(Value::Array(ArrayView::anchored(
                    result,
                    Some(CellAddress { row: 0, col: 0 }),
                )))
            }
            Expression::Unary { op, operand } => {
                let value = self.expr(s, at, operand.as_ref())?;
                Operators::new(self.session).unary_value(s, at, *op, value)
            }
            Expression::Binary { op, left, right } => {
                let left = self.expr(s, at, left.as_ref())?;
                let right = self.expr(s, at, right.as_ref())?;
                Operators::new(self.session).binary_values(s, at, *op, left, right)
            }
            Expression::Date(_)
            | Expression::Logical(_)
            | Expression::Statistics(_)
            | Expression::Reference(_)
            | Expression::Search(_)
            | Expression::Information(_)
            | Expression::Math(_)
            | Expression::Text(_)
            | Expression::Financial(_)
            | Expression::Engineering(_)
            | Expression::Matrix(_)
            | Expression::Criteria(_)
            | Expression::Database(_) => {
                unreachable!("builtin calls dispatch through the composed adapter")
            }
            Expression::Extension { binding, arguments } => host::execute_extension(
                self.session,
                s,
                at,
                binding,
                (&arguments.expressions).into(),
            ),
            Expression::UnresolvedFunction(arguments) => {
                ReadRecorder::new(self.session)
                    .read_lazy(|| DependencyRead::OpaqueProvider(arguments.source_name.clone()));
                Ok(err(Name))
            }
        }
    }
}
