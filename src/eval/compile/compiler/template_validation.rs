//! Structural validation of restored bytecode templates and reference bindings.

use super::plan_snapshot_error::PlanSnapshotError;
use super::snapshot_reader::SnapshotReader;
use crate::eval::compile::bytecode::{
    Call, CallTarget, InstructionIndex, Label, Opcode, ReferenceBindings, ReferenceKind, Template,
};

/// Validate instruction structure, operand bounds, and call descriptor indices.
pub(super) fn validate_template(t: &Template, calls: usize) -> Result<(), PlanSnapshotError> {
    // Loading quotas belong to SnapshotLimits. This validator checks only encoded
    // instruction/index structure; no independent workbook-size quota is hidden here.
    if t.code.is_empty() {
        return Err(PlanSnapshotError::SnapshotEmptyTemplate);
    }
    let mut previous = None;
    if t.aggregations.len() > t.code.len() {
        return Err(PlanSnapshotError::SnapshotAggregationCount);
    }
    for entry in &t.aggregations {
        if !entry.plan.valid()
            || previous.is_some_and(|p| entry.start <= p)
            || t.code
                .get(entry.start)
                .is_none_or(|i| i.opcode != Opcode::Enter)
        {
            return Err(PlanSnapshotError::SnapshotAggregationPlan);
        }
        previous = Some(entry.start);
    }
    if t.labels.len() > t.code.len() || t.labels.iter().any(|target| target.index() >= t.code.len())
    {
        return Err(PlanSnapshotError::SnapshotLabelTable);
    }
    let target = |n: u32| {
        t.resolve_label(Label(n))
            .map(InstructionIndex::index)
            .ok_or(PlanSnapshotError::SnapshotJumpLabel)
    };
    for (ip, i) in t.code.iter().enumerate() {
        if i.flags != 0 || i.reserved != 0 {
            return Err(PlanSnapshotError::SnapshotInstructionFlags);
        }
        match i.opcode {
            Opcode::Text | Opcode::Error | Opcode::Name | Opcode::QualifiedName => {
                let end = (i.a as usize)
                    .checked_add(i.b as usize)
                    .ok_or(PlanSnapshotError::SnapshotOperandOverflow)?;
                let bytes = t
                    .operands
                    .get(i.a as usize..end)
                    .ok_or(PlanSnapshotError::SnapshotOperandBounds)?;
                if i.opcode == Opcode::QualifiedName {
                    let mut reader = SnapshotReader::new(bytes);
                    reader.string()?;
                    std::str::from_utf8(reader.remaining())
                        .map_err(|_| PlanSnapshotError::SnapshotQualifiedNameUTF8)?;
                } else {
                    std::str::from_utf8(bytes)
                        .map_err(|_| PlanSnapshotError::SnapshotUTF8Operand)?;
                }
            }
            Opcode::Reference if i.a >= t.reference_slots => {
                return Err(PlanSnapshotError::SnapshotReferenceSlot);
            }
            Opcode::Invoke | Opcode::Conditional => {
                if i.a as usize >= calls {
                    return Err(PlanSnapshotError::SnapshotCallIndexArity);
                }
                let continuation = target(i.c)?;
                if continuation <= ip {
                    return Err(PlanSnapshotError::SnapshotBackwardsInvocation);
                }
                let jump = t
                    .code
                    .get(ip + 1)
                    .ok_or(PlanSnapshotError::SnapshotInvokeJump)?;
                if jump.opcode != Opcode::Jump || jump.c != i.c {
                    return Err(PlanSnapshotError::SnapshotInvokeContinuation);
                }
                let mut descriptor = ip + 2;
                for ordinal in 0..i.b {
                    let argument = t
                        .code
                        .get(descriptor)
                        .ok_or(PlanSnapshotError::SnapshotArgumentDescriptor)?;
                    let next = target(argument.c)?;
                    if argument.opcode != Opcode::Argument
                        || argument.a != ordinal
                        || next <= descriptor
                        || next > continuation
                    {
                        return Err(PlanSnapshotError::SnapshotArgumentChain);
                    }
                    descriptor = next;
                }
                if descriptor != continuation {
                    return Err(PlanSnapshotError::SnapshotArgumentCount);
                }
            }
            Opcode::Argument | Opcode::Jump => {
                let continuation = target(i.c)?;
                if continuation <= ip {
                    return Err(PlanSnapshotError::SnapshotBackwardsJump);
                }
            }
            Opcode::Unary if i.a > 4 => return Err(PlanSnapshotError::SnapshotUnary),
            Opcode::Binary if i.a > 14 => return Err(PlanSnapshotError::SnapshotBinary),
            _ => {}
        }
    }
    Ok(())
}

/// Require each contextual reducer to belong to exactly one supported call slot;
/// standalone root projections have no owning call.
pub(super) fn validate_aggregation_roles(
    template: &Template,
    calls: &[Call],
) -> Result<(), PlanSnapshotError> {
    use crate::eval::functions::registry::{FunctionId, MatrixFunction};

    if template.aggregations.is_empty() {
        return Ok(());
    }
    let mut seen = Vec::new();
    seen.try_reserve_exact(template.aggregations.len())
        .map_err(PlanSnapshotError::AggregationReservation)?;
    // SAFETY: The successful reservation admits exactly this many bools into
    // the empty vector; no insertion has consumed that capacity.
    seen.resize(template.aggregations.len(), false);
    let mut matched = 0;
    // A root projection is compiled directly, without an owning call.
    if template
        .aggregations
        .first()
        .is_some_and(|entry| entry.start == 0)
        && let Some(root) = seen.first_mut()
    {
        *root = true;
        matched += 1;
    }
    for (ip, instruction) in template.code.iter().enumerate() {
        if !instruction.opcode.is_call() {
            continue;
        }
        let role = match calls.get(instruction.a as usize).map(|call| &call.target) {
            Some(CallTarget::Builtin(FunctionId::Matrix(MatrixFunction::Groupby))) => Some(2),
            Some(CallTarget::Builtin(FunctionId::Matrix(MatrixFunction::Pivotby))) => Some(3),
            _ => None,
        };
        let mut descriptor = ip.saturating_add(2);
        for ordinal in 0..instruction.b {
            let argument = template
                .code
                .get(descriptor)
                .ok_or(PlanSnapshotError::SnapshotArgumentDescriptor)?;
            if let Some(start) = descriptor.checked_add(1)
                && let Ok(index) = template
                    .aggregations
                    .binary_search_by_key(&start, |entry| entry.start)
            {
                let visited = seen
                    .get_mut(index)
                    .ok_or(PlanSnapshotError::SnapshotAggregationCount)?;
                if *visited || role != Some(ordinal) || argument.opcode != Opcode::Argument {
                    return Err(PlanSnapshotError::SnapshotAggregationArgumentRole);
                }
                *visited = true;
                matched += 1;
            }
            descriptor = template
                .resolve_label(Label(argument.c))
                .ok_or(PlanSnapshotError::SnapshotJumpLabel)?
                .index();
        }
    }
    if matched != template.aggregations.len() {
        return Err(PlanSnapshotError::SnapshotAggregationArgumentRole);
    }
    Ok(())
}

/// Validate concrete reference encodings against a template's slot requirements.
pub(super) fn validate_references(
    r: &ReferenceBindings,
    slots: u32,
) -> Result<(), PlanSnapshotError> {
    if r.slots.len() != slots as usize {
        return Err(PlanSnapshotError::SnapshotReferenceCountBytes);
    }
    for s in &r.slots {
        let end = (s.offset as usize)
            .checked_add(s.length as usize)
            .ok_or(PlanSnapshotError::SnapshotReferenceOverflow)?;
        let mut reader = SnapshotReader::new(
            r.data
                .get(s.offset as usize..end)
                .ok_or(PlanSnapshotError::SnapshotReferenceBounds)?,
        );
        match s.kind {
            ReferenceKind::Cell => reader.cell()?,
            ReferenceKind::Row | ReferenceKind::Column => {
                let bound = if s.kind == ReferenceKind::Row {
                    crate::model::MAX_ROWS
                } else {
                    crate::model::MAX_COLUMNS
                };
                if reader.word()? >= bound {
                    return Err(PlanSnapshotError::SnapshotReferenceIndex);
                }
                reader.boolean()?;
                reader.optional()?;
            }
            ReferenceKind::SheetSpan => {
                reader.string()?;
                reader.string()?;
                reader.cell()?;
                if reader.boolean()? {
                    reader.cell()?;
                }
            }
        }
        if !reader.remaining().is_empty() {
            return Err(PlanSnapshotError::SnapshotReferenceTrailingBytes);
        }
    }
    Ok(())
}
