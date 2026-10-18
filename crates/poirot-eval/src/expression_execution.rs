//! Expression admission, context observation, and lazy typed dispatch.
use super::*;
use crate::executable::ReferenceRef;

/// The consuming operation selects reference expansion independently of worksheet publication.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ExpressionDemand {
    /// Ordinary argument and worksheet formulas retain reference implicit intersection.
    #[default]
    Scalar,
    /// Array consumers retain rectangular operator inputs.
    Array,
}

impl<'a> Session<'a> {
    /// Evaluate a deferred operand under its consumer's demand, restoring it on failure too.
    pub(super) fn expr_with_demand<'expression>(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: impl Into<ExecutableRef<'expression>>,
        demand: ExpressionDemand,
    ) -> EvalResult<Value> {
        let previous = std::mem::replace(&mut self.expression_demand, demand);
        let result = self.expr(sheet, at, expression);
        self.expression_demand = previous;
        result
    }

    /// Execute a deferred bytecode block (or the unit-test-only tree oracle).
    pub(super) fn expr<'expression>(
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
                    self.bytecode_expr(sheet, at, program)
                } else {
                    self.execute_bytecode(sheet, at, program, start)
                }
            }
        }
    }
    #[cfg(test)]
    fn tree_expr(&mut self, sheet: usize, at: CellAddress, expression: &Expr) -> EvalResult<Value> {
        self.budget.charge(WorkKind::Expression, 1)?;
        self.budget.enter_depth()?;
        let result = self.observed_expression(sheet, at, expression);
        self.budget.leave_depth();
        result
    }
    /// Inspect the builtin selected by a bound expression and active profile policy.
    #[cfg(test)]
    pub(super) fn builtin_invocation<'expr>(
        &self,
        expression: &'expr Expr,
    ) -> Option<(FunctionId, &'expr bound::Arguments)> {
        consumed_calls::builtin_call(expression).or_else(|| {
            if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
                && let Expr::UnresolvedFunction(arguments) = expression
            {
                registry::resolve_excel_function(&poirot_formula::FunctionName::from(
                    arguments.resolved_spelling(),
                ))
                .map(|handler| (handler, arguments))
            } else {
                None
            }
        })
    }
    /// Admit and execute one bound-tree node without repeating depth accounting.
    #[cfg(test)]
    pub(super) fn inner(
        &mut self,
        s: usize,
        at: CellAddress,
        expression: &Expr,
    ) -> EvalResult<Value> {
        let invocation = self.builtin_invocation(expression);
        self.admit_resolved_call(
            call_admission::ResolvedCall {
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
                read_builtin_policy: matches!(expression, Expr::UnresolvedFunction(_)),
            },
            call_admission::AdmissionOrder::ObserverBeforePolicy,
        )?;
        if let Some((function, arguments)) = invocation {
            return adapters::execute(
                self,
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
            Expr::Number(value) => Ok(self.numeric_policy.coerce_numeric_literal(*value)?.into()),
            Expr::String(text) => {
                self.budget.text(text.len())?;
                Ok(Text(text.clone()).into())
            }
            Expr::Bool(value) => Ok(Boolean(*value).into()),
            Expr::Error(error) => Ok(Error(error.parse().unwrap_or(VALUE_ERROR)).into()),
            // TODO(coercion-non-argument): [MEDIUM] Legacy expression evaluation loses omission identity when it represents missing syntax as blank.
            Expr::Missing => Ok(coerce_legacy_missing_blank().into()),
            Expr::SheetSpan(span) => self.reference_atom(
                s,
                at,
                ReferenceRef::SheetSpan {
                    first_sheet: &span.first_sheet,
                    last_sheet: &span.last_sheet,
                    start: &span.start,
                    end: span.end.as_ref(),
                },
            ),
            Expr::Cell(cell) => self.reference_atom(
                s,
                at,
                ReferenceRef::Cell {
                    row: cell.row,
                    column: cell.column,
                    sheet: cell.sheet.as_deref(),
                },
            ),
            Expr::Column { column, sheet, .. } => self.reference_atom(
                s,
                at,
                ReferenceRef::Column {
                    column: *column,
                    sheet: sheet.as_deref(),
                },
            ),
            Expr::Row { row, sheet, .. } => self.reference_atom(
                s,
                at,
                ReferenceRef::Row {
                    row: *row,
                    sheet: sheet.as_deref(),
                },
            ),
            Expr::Name(name) => self.defined_name(s, s, at, name),
            Expr::QualifiedName { sheet, name } => match self.sheet(s, &Some(sheet.clone()))? {
                Some(scope) => self.defined_name(s, scope, at, name),
                None => Ok(err(Ref)),
            },
            Expr::Array(rows) => {
                let count = rows.iter().map(Vec::len).sum();
                self.budget.allocation(count)?;
                let mut result = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut values = Vec::with_capacity(row.len());
                    for expression in row {
                        let value = self.expr(s, at, expression)?;
                        values.push(self.scalar(at, value)?);
                    }
                    result.push(values);
                }
                Ok(Value::Array(ArrayView::anchored(
                    result,
                    Some(CellAddress { row: 0, col: 0 }),
                )))
            }
            Expr::Unary { op, operand } => {
                let value = self.expr(s, at, operand.as_ref())?;
                self.unary_value(s, at, *op, value)
            }
            Expr::Binary { op, left, right } => {
                let left = self.expr(s, at, left.as_ref())?;
                let right = self.expr(s, at, right.as_ref())?;
                self.binary_values(s, at, *op, left, right)
            }
            Expr::Date(_)
            | Expr::Logical(_)
            | Expr::Statistics(_)
            | Expr::Reference(_)
            | Expr::Search(_)
            | Expr::Information(_)
            | Expr::Math(_)
            | Expr::Text(_)
            | Expr::Financial(_)
            | Expr::Engineering(_)
            | Expr::Matrix(_)
            | Expr::Criteria(_)
            | Expr::Database(_) => {
                unreachable!("builtin calls dispatch through the composed adapter")
            }
            Expr::Extension { binding, arguments } => {
                extensions::execute(self, s, at, binding, (&arguments.expressions).into())
            }
            Expr::UnresolvedFunction(arguments) => {
                self.read_lazy(|| DependencyRead::OpaqueProvider(arguments.source_name.clone()));
                Ok(err(Name))
            }
        }
    }
    /// Explicit arrays retain shape; references expand only for an array consumer or dynamic execution.
    pub(super) fn retains_array(&self, value: &Value) -> bool {
        matches!(value, Value::Array(_))
            || (self.expands_operator_references() && array_operator::is_array_operand(value))
    }

    fn expands_operator_references(&self) -> bool {
        self.expression_demand == ExpressionDemand::Array
            || self.array_result_policy == ArrayResultPolicy::DynamicSpill
    }

    /// Apply a unary operation to an already evaluated value channel.
    pub(super) fn unary_value(
        &mut self,
        s: usize,
        at: CellAddress,
        operation: UnaryOp,
        value: Value,
    ) -> EvalResult<Value> {
        let op = &operation;
        if *op != UnaryOp::ImplicitIntersection && self.retains_array(&value) {
            return self.array_unary(s, at, operation, value);
        }
        let value = self.scalar(at, value)?;
        // Selection above materializes references while preserving the scalar type.
        // The pinned POI numeric contract retains its text-only plus exception.
        if *op == UnaryOp::ImplicitIntersection
            || (*op == UnaryOp::Plus && self.numeric_policy == NumericOutcomePolicy::LegacyFinite)
        {
            return Ok(value.into());
        }
        if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209 {
            if *op == UnaryOp::Plus && value.text_ref().is_some() {
                return Ok(value.into());
            }
            return Ok(
                // TODO(coercion-non-argument): [LOW] Unary numeric coercion can erase blank or text identity if applied before the operator consumes it.
                match semantic(self.coercev2_operator_number(&value))? {
                    Ok(n) => match op {
                        // SAFETY: ImplicitIntersection returns before either numeric unary match in this function.
                        UnaryOp::ImplicitIntersection => unreachable!("selected above"),
                        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                        UnaryOp::Plus => self.numeric_policy.coerce_numeric_outcome(n),
                        UnaryOp::Minus => {
                            self.numeric_policy
                                // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                                .coerce_numeric_outcome(if n == 0.0 { 0.0 } else { -n })
                        }
                        UnaryOp::Percent => {
                            self.numeric_policy
                                // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                                .coerce_numeric_outcome(if n == 0.0 { 0.0 } else { n / 100.0 })
                        }
                    }
                    .into(),
                    Err(error) => err(error),
                },
            );
        }
        Ok(
            // TODO(coercion-non-argument): [LOW] Unary numeric coercion must stay at the operator; applying it during operand acquisition would affect selectors.
            match semantic(self.coercev2_scalar_number(&value))? {
                Ok(number) => finite(match op {
                    // SAFETY: ImplicitIntersection returns before either numeric unary match in this function.
                    UnaryOp::ImplicitIntersection => unreachable!("selected above"),
                    UnaryOp::Plus => number,
                    UnaryOp::Minus => -number,
                    UnaryOp::Percent => number / 100.,
                })
                .into(),
                Err(error) => err(error),
            },
        )
    }
    /// Apply a binary operation while preserving reference and array semantics.
    pub(super) fn binary_values(
        &mut self,
        s: usize,
        at: CellAddress,
        operation: BinaryOp,
        left: Value,
        right: Value,
    ) -> EvalResult<Value> {
        let op = &operation;
        if matches!(
            op,
            BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection
        ) {
            return self.references(*op, left, right);
        }
        if self.retains_array(&left) || self.retains_array(&right) {
            // Literal arrays retain shape in legacy formulas, but do not turn an
            // ordinary reference operand into an array-formula request.
            let left = self.operator_reference_operand(at, left)?;
            let right = self.operator_reference_operand(at, right)?;
            return self.array_binary(s, at, operation, left, right);
        }
        let left = self.scalar(at, left)?;
        let right = self.scalar(at, right)?;
        if *op == BinaryOp::Concat {
            return function_value(self.text_concat_operator(left, right));
        }
        if matches!(
            op,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            for value in [&left, &right] {
                if let Text(text) = value {
                    self.budget.text(
                        text.chars()
                            .flat_map(char::to_lowercase)
                            .map(char::len_utf8)
                            .sum(),
                    )?;
                }
            }
        }
        self.budget.charge(WorkKind::ScalarOperator, 1)?;
        function_value(self.binary(*op, left, right))
    }

    fn operator_reference_operand(&mut self, at: CellAddress, value: Value) -> EvalResult<Value> {
        if !self.expands_operator_references()
            && matches!(
                value,
                Value::Refs(_) | Value::SignedRefs(_) | Value::SheetSpan(_)
            )
        {
            self.scalar(at, value).map(Value::Scalar)
        } else {
            Ok(value)
        }
    }

    /// Resolve borrowed reference geometry against workbook sheets into a value channel.
    pub(super) fn reference_atom(
        &mut self,
        s: usize,
        at: CellAddress,
        reference: ReferenceRef<'_>,
    ) -> EvalResult<Value> {
        match reference {
            ReferenceRef::SheetSpan {
                first_sheet,
                last_sheet,
                start,
                end,
            } => {
                self.read_lazy(|| DependencyRead::SheetLayout);
                self.budget
                    .text(first_sheet.len().saturating_add(last_sheet.len()))?;
                let first = self.sheet_named(s, Some(first_sheet))?;
                let last = self.sheet_named(s, Some(last_sheet))?;
                let (first, last) = match (first, last) {
                    (Some(first), Some(last)) => (first, last),
                    // HSSF's missing first local endpoint makes getExternalSheet
                    // return null. OperationEvaluationContext then selects sheet 0
                    // alone, discarding the surviving last endpoint. The retained
                    // native probe observes SUM(#REF!A1) evaluating its first sheet.
                    (None, Some(_)) => (0, 0),
                    (first, None) => {
                        return Err(EvaluationFailure::UnsupportedCapability {
                            capability: CapabilityId::FunctionAdapter,
                            source: Some(EvaluationTarget { sheet: s, address: at }),
                            detail: if first.is_some() {
                                "pinned HSSF missing last sheet endpoint throws java.lang.IllegalArgumentException during formula authoring"
                            } else {
                                "both sheet-span endpoints unresolved; native classification unavailable"
                            }.into(),
                        });
                    }
                };
                if last < first {
                    return Err(EvaluationFailure::UnsupportedCapability {
                        capability: CapabilityId::FunctionAdapter,
                        source: Some(EvaluationTarget {
                            sheet: s,
                            address: at,
                        }),
                        detail: "pinned HSSF reversed sheet endpoints throw java.lang.NegativeArraySizeException during evaluation"
                            .into(),
                    });
                }
                let count = last - first + 1;
                self.budget.allocation(count)?;
                let is_area = end.is_some();
                let end = end.unwrap_or(start);
                let mut areas = Vec::with_capacity(count);
                for sheet in first..=last {
                    self.budget.charge(WorkKind::ReferencePair, 1)?;
                    areas.push(Area {
                        kind: if is_area {
                            AreaKind::Area
                        } else {
                            AreaKind::Cell
                        },
                        sheet,
                        r0: start.row.min(end.row),
                        r1: start.row.max(end.row),
                        c0: start.column.min(end.column),
                        c1: start.column.max(end.column),
                    });
                }
                Ok(Value::SheetSpan(areas))
            }
            ReferenceRef::Cell { row, column, sheet } => Ok(match self.sheet_named(s, sheet)? {
                Some(sheet) => {
                    self.budget.allocation(1)?;
                    Value::Refs(vec![Area {
                        kind: AreaKind::Cell,
                        sheet,
                        r0: row,
                        r1: row,
                        c0: column,
                        c1: column,
                    }])
                }
                None => err(Ref),
            }),
            ReferenceRef::Column { column, sheet } => Ok(match self.sheet_named(s, sheet)? {
                Some(sheet) => {
                    self.budget.allocation(1)?;
                    Value::Refs(vec![Area {
                        kind: AreaKind::Area,
                        sheet,
                        r0: 0,
                        r1: MAX_ROWS - 1,
                        c0: column,
                        c1: column,
                    }])
                }
                None => err(Ref),
            }),
            ReferenceRef::Row { row, sheet } => Ok(match self.sheet_named(s, sheet)? {
                Some(sheet) => {
                    self.budget.allocation(1)?;
                    Value::Refs(vec![Area {
                        kind: AreaKind::Area,
                        sheet,
                        r0: row,
                        r1: row,
                        c0: 0,
                        c1: MAX_COLUMNS - 1,
                    }])
                }
                None => err(Ref),
            }),
        }
    }
}
