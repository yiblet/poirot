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
    let mut local_depth = 0u32;
    let mut deepest_scope = 0u32;
    // Each deferred body starts with captures and parameters, then restores its parent depth.
    let mut lambda_scopes = Vec::new();
    for (ip, i) in t.code.iter().enumerate() {
        if let Some(&(end, parent_depth, seed)) = lambda_scopes.last()
            && ip == end
        {
            if local_depth != seed {
                return Err(PlanSnapshotError::SnapshotLocalScope);
            }
            local_depth = parent_depth;
            lambda_scopes.pop();
        }
        let scope_end = lambda_scopes
            .last()
            .map_or(t.code.len(), |&(end, _, _)| end);
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
            Opcode::BindLocal => {
                if i.a != local_depth || i.a >= t.local_slots || i.b != 0 || i.c != 0 {
                    return Err(PlanSnapshotError::SnapshotLocalScope);
                }
                local_depth = local_depth
                    .checked_add(1)
                    .ok_or(PlanSnapshotError::SnapshotLocalScope)?;
                deepest_scope = deepest_scope.max(local_depth);
            }
            Opcode::ReadLocal | Opcode::ClearLocals => {
                if i.a >= local_depth || i.b != 0 || i.c != 0 {
                    return Err(PlanSnapshotError::SnapshotLocalScope);
                }
                if i.opcode == Opcode::ClearLocals {
                    if lambda_scopes.last().is_some_and(|&(_, _, seed)| i.a < seed) {
                        return Err(PlanSnapshotError::SnapshotLocalScope);
                    }
                    local_depth = i.a;
                }
            }
            Opcode::Reference if i.a >= t.reference_slots => {
                return Err(PlanSnapshotError::SnapshotReferenceSlot);
            }
            Opcode::Lambda => {
                let end = target(i.c)?;
                if end <= ip.saturating_add(2)
                    || end >= scope_end
                    || i.b > 253
                    || t.code
                        .get(ip + 1)
                        .is_none_or(|op| op.opcode != Opcode::Enter)
                    || t.code
                        .get(end - 1)
                        .is_none_or(|op| op.opcode != Opcode::Leave)
                {
                    return Err(PlanSnapshotError::SnapshotLambdaDescriptor);
                }
                let mut reader = SnapshotReader::new(
                    t.operands
                        .get(i.a as usize..)
                        .ok_or(PlanSnapshotError::SnapshotOperandBounds)?,
                );
                let captures = reader.word()?;
                let seed = captures
                    .checked_add(i.b)
                    .filter(|seed| *seed <= t.local_slots && captures <= local_depth)
                    .ok_or(PlanSnapshotError::SnapshotLambdaDescriptor)?;
                for _ in 0..captures {
                    if reader.word()? >= local_depth {
                        return Err(PlanSnapshotError::SnapshotLambdaDescriptor);
                    }
                }
                if reader.word()? > i.b {
                    return Err(PlanSnapshotError::SnapshotLambdaDescriptor);
                }
                lambda_scopes
                    .try_reserve(1)
                    .map_err(PlanSnapshotError::LambdaScopeReservation)?;
                // SAFETY: The reservation admits this frame.
                lambda_scopes.push((end, local_depth, seed));
                local_depth = seed;
                deepest_scope = deepest_scope.max(seed);
            }
            Opcode::Invoke | Opcode::Conditional | Opcode::InvokeValue => {
                if (i.opcode.is_call() && i.a as usize >= calls)
                    || (i.opcode == Opcode::InvokeValue && i.a != 0)
                {
                    return Err(PlanSnapshotError::SnapshotCallIndexArity);
                }
                let continuation = target(i.c)?;
                if continuation <= ip || continuation >= scope_end {
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
                if continuation <= ip || continuation >= scope_end {
                    return Err(PlanSnapshotError::SnapshotBackwardsJump);
                }
            }
            Opcode::Unary if i.a > 4 => return Err(PlanSnapshotError::SnapshotUnary),
            Opcode::Binary if i.a > 14 => return Err(PlanSnapshotError::SnapshotBinary),
            _ => {}
        }
    }
    if !lambda_scopes.is_empty() || local_depth != 0 || deepest_scope != t.local_slots {
        return Err(PlanSnapshotError::SnapshotLocalScope);
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
            ReferenceKind::Table => {
                if (reader.boolean()? && reader.string()?.is_empty())
                    || poirot_parsing::formula::TableSection::parse(reader.string()?).is_none()
                {
                    return Err(PlanSnapshotError::SnapshotReferenceIndex);
                }
                let columns = reader.word()?;
                if columns > 2 {
                    return Err(PlanSnapshotError::SnapshotReferenceIndex);
                }
                for _ in 0..columns {
                    if reader.string()?.is_empty() {
                        return Err(PlanSnapshotError::SnapshotReferenceIndex);
                    }
                }
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
