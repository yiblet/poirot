//! Scalar and array-aware unary and binary operator semantics.

use super::array_operators::ArrayOperators;
use super::reference_operators::ReferenceOperators;
use crate::eval::config::BuiltinSemanticsPolicy;
use crate::eval::limits::{EvalResult, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::{Coercer, ReadRecorder, ResultProjection, Session};
use crate::eval::spill::SpillDemand;
use crate::eval::values::core_projection::semantic;
use crate::eval::values::{
    FunctionResult, VALUE_ERROR, Value, core_projection, err, function_value, numeric_outcome,
    source_arithmetic,
};
use crate::eval::vm::arguments_ref::ArgumentsRef;
use crate::eval::{adapters, host};
use crate::model::CellError::{Div0, Num};
use crate::model::CellValue::{Boolean, Error};
use crate::model::{CellAddress, CellValue};
use poirot_parsing::formula::{BinaryOp, UnaryOp};

/// Applies formula operators to evaluated operands within one session.
pub(in crate::eval) struct Operators<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> Operators<'s, 'a> {
    /// Borrow the session whose reads, policies and budget operators use.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Apply a unary operation to an already evaluated value channel.
    pub(in crate::eval) fn unary_value(
        &mut self,
        s: usize,
        at: CellAddress,
        operation: UnaryOp,
        value: Value,
    ) -> EvalResult<Value> {
        if operation == UnaryOp::SpillRange {
            return SpillDemand::new(self.session).reference(value);
        }
        let op = &operation;
        if *op != UnaryOp::ImplicitIntersection
            && self.session.state.expression_demand.retains_array(&value)
        {
            return ArrayOperators::new(self.session).array_unary(s, at, operation, value);
        }
        if *op == UnaryOp::ImplicitIntersection {
            return adapters::reference::execution::intersect(self.session, s, at, value);
        }
        let value = ResultProjection::new(self.session).select_legacy_scalar(at, value)?;
        // Scalar numeric consumers retain their existing coercion contract.

        if *op == UnaryOp::Plus {
            ReadRecorder::new(self.session).read(DependencyRead::BuiltinPolicy);
        }
        if *op == UnaryOp::Plus
            && (self.session.policies.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
                || value.text_ref().is_some())
        {
            return Ok(value.into());
        }
        Ok(
            // TODO(coercion-non-argument): [LOW] Unary numeric coercion can erase blank or text identity if applied before the operator consumes it.
            match semantic(Coercer::new(self.session).coerce_operator_number(&value))? {
                Ok(n) => match op {
                    // SAFETY: Both reference/selection operators return before numeric coercion.
                    UnaryOp::ImplicitIntersection | UnaryOp::SpillRange => {
                        unreachable!("selected above")
                    }
                    // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                    UnaryOp::Plus => numeric_outcome::convert_numeric_outcome(n),
                    UnaryOp::Minus => {
                        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                        numeric_outcome::convert_numeric_outcome(if n == 0.0 { 0.0 } else { -n })
                    }
                    UnaryOp::Percent => {
                        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                        numeric_outcome::convert_numeric_outcome(if n == 0.0 {
                            0.0
                        } else {
                            n / 100.0
                        })
                    }
                }
                .into(),
                Err(error) => err(error),
            },
        )
    }

    /// Apply a binary operation while preserving reference and array semantics.
    pub(in crate::eval) fn binary_values(
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
            return ReferenceOperators::new(self.session).references(*op, left, right);
        }
        if self.session.state.expression_demand.retains_array(&left)
            || self.session.state.expression_demand.retains_array(&right)
        {
            // Literal arrays retain shape in legacy formulas, but do not turn an
            // ordinary reference operand into an array-formula request.
            let left = self.operator_reference_operand(at, left)?;
            let right = self.operator_reference_operand(at, right)?;
            return ArrayOperators::new(self.session).array_binary(s, at, operation, left, right);
        }
        let left = ResultProjection::new(self.session).select_legacy_scalar(at, left)?;
        let right = ResultProjection::new(self.session).select_legacy_scalar(at, right)?;
        self.scalar_binary_values(operation, left, right)
    }

    /// Apply a scalar operator after operand shape dispatch has completed.
    pub(in crate::eval) fn scalar_binary_values(
        &mut self,
        operation: BinaryOp,
        left: CellValue,
        right: CellValue,
    ) -> EvalResult<Value> {
        let op = &operation;
        if *op == BinaryOp::Concat {
            return function_value(self.text_concat_operator(left, right));
        }

        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::ScalarOperator, 1)?;
        function_value(self.binary(*op, left, right))
    }

    /// Intersect a reference operand unless the consumer requested array expansion.
    fn operator_reference_operand(&mut self, at: CellAddress, value: Value) -> EvalResult<Value> {
        if !self
            .session
            .state
            .expression_demand
            .expands_operator_references()
            && matches!(
                value,
                Value::Refs(_) | Value::SignedRefs(_) | Value::SheetSpan(_)
            )
        {
            ResultProjection::new(self.session)
                .select_legacy_scalar(at, value)
                .map(Value::Scalar)
        } else {
            Ok(value)
        }
    }

    /// Execute a binary expression with reference operators and lazy operand projection.
    pub(in crate::eval) fn binary(
        &mut self,
        op: BinaryOp,
        l: CellValue,
        r: CellValue,
    ) -> FunctionResult<CellValue> {
        if let Error(e) = l {
            return Err(e.into());
        }
        if let Error(e) = r {
            return Err(e.into());
        }
        if op == BinaryOp::Concat {
            return self.text_concat_operator(l, r);
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
            let c =
                core_projection::source_compare(&l, &r, &mut self.session.state.runtime.budget)?;
            return Ok(Boolean(match op {
                BinaryOp::Equal => c.is_eq(),
                BinaryOp::NotEqual => !c.is_eq(),
                BinaryOp::Less => c.is_lt(),
                BinaryOp::LessEqual => !c.is_gt(),
                BinaryOp::Greater => c.is_gt(),
                _ => !c.is_lt(),
            }));
        }
        let l = Coercer::new(self.session).coerce_operator_number(&l)?;
        let r = Coercer::new(self.session).coerce_operator_number(&r)?;
        let result = match op {
            BinaryOp::Add => source_arithmetic::add(l, r, &mut self.session.state.runtime.budget)?,
            BinaryOp::Subtract => {
                source_arithmetic::subtract(l, r, &mut self.session.state.runtime.budget)?
            }
            BinaryOp::Multiply => l * r,
            BinaryOp::Divide => {
                if r == 0. {
                    return Err(Div0.into());
                }
                l / r
            }
            BinaryOp::Power => {
                if l == 0. && r == 0. {
                    return Err(Num.into());
                }
                if l == 0. && r < 0. {
                    return Err(Div0.into());
                }
                l.powf(r)
            }
            _ => return Err(VALUE_ERROR.into()),
        };
        let result = if op != BinaryOp::Subtract && result == 0.0 {
            0.0
        } else {
            result
        };
        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
        Ok(numeric_outcome::convert_checked_numeric_result(result))
    }

    /// Evaluate the concatenation operator through checked text services.
    pub(in crate::eval) fn text_concat_operator(
        &mut self,
        left: CellValue,
        right: CellValue,
    ) -> FunctionResult<CellValue> {
        let mut host = host::CallHost::new(
            self.session,
            0,
            CellAddress { row: 0, col: 0 },
            ArgumentsRef::Empty,
        );
        adapters::text::execution::concat_operator(&mut host, left, right)
    }
}
