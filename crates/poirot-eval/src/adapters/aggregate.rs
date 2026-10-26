//! Shared one-shot source traversal for aggregate admission policies.
use crate::host::{CallHost, HostOperand, ScalarOperand};
use crate::*;
use std::{collections::TryReserveError, ops::ControlFlow};

/// A typed accumulator and admission rule over sparse aggregate members.
pub(crate) trait AggregatePolicy: Sized {
    /// Reject an operand or record state before any of its members are read.
    fn before(&mut self, _host: &CallHost<'_, '_>, _operand: &HostOperand) -> EvalResult<()> {
        Ok(())
    }

    /// Reserve storage for a complete eager batch or one ordered member.
    fn reserve(&mut self, additional: usize) -> Result<(), TryReserveError>;

    /// Admit one member; Break stops only an ordered sheet span.
    fn consume(&mut self, member: ScalarOperand, selected: bool) -> ControlFlow<()>;

    /// Finish one operand after all required reads, publishing semantic failures.
    fn after(&mut self, host: &mut CallHost<'_, '_>) -> EvalResult<Option<Value>>;

    /// Convert the completed accumulator into its worksheet result.
    fn finish(self) -> EvalResult<Value>;
}

/// Evaluate each argument once, folding eager operands fully and spans in order.
pub(crate) fn execute<P: AggregatePolicy>(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    arguments: ArgumentsRef<'_>,
    mut policy: P,
) -> EvalResult<Value> {
    let mut host = CallHost::new_inheriting(session, sheet, address, arguments);
    for index in 0..host.len() {
        let operand = match semantic(host.aggregate_argument_once(index))? {
            Ok(operand) => operand,
            Err(error) => return Ok(err(error)),
        };
        policy.before(&host, &operand)?;
        let selected = matches!(operand.value(), Value::SelectedScalar(_, _));
        if matches!(operand.value(), Value::SheetSpan(_)) {
            policy = host.fold_ordered_sheet_span(operand, policy, |policy, member| {
                policy.reserve(1).map_err(|_| EvaluationFailure::Resource {
                    kind: ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: Some(EvaluationTarget { sheet, address }),
                    detail: "aggregate member allocation failed".into(),
                })?;
                Ok(policy.consume(member, selected))
            })?;
        } else {
            policy = host.fold_aggregate_operand(
                operand,
                policy,
                |policy, additional| policy.reserve(additional),
                |policy, member| {
                    let _ = policy.consume(member, selected);
                    Ok(())
                },
            )?;
        }
        if let Some(value) = policy.after(&mut host)? {
            return Ok(value);
        }
    }
    policy.finish()
}
