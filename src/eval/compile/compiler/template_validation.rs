//! Structural validation of restored bytecode templates and reference bindings.

use super::snapshot_reader::SnapshotReader;
use crate::eval::compile::bytecode::{
    InstructionIndex, Label, Opcode, ReferenceBindings, ReferenceKind, Template,
};

/// Validate instruction structure, operand bounds, and call descriptor indices.
pub(super) fn validate_template(t: &Template, calls: usize) -> Result<(), String> {
    // Loading quotas belong to SnapshotLimits. This validator checks only encoded
    // instruction/index structure; no independent workbook-size quota is hidden here.
    if t.code.is_empty() {
        return Err("snapshot empty template".into());
    }
    let mut previous = None;
    if t.aggregations.len() > t.code.len() {
        return Err("snapshot aggregation count".into());
    }
    for entry in &t.aggregations {
        if !entry.plan.valid()
            || previous.is_some_and(|p| entry.start <= p)
            || t.code
                .get(entry.start)
                .is_none_or(|i| i.opcode != Opcode::Enter)
        {
            return Err("snapshot aggregation plan".into());
        }
        if entry.start != 0
            && t.code
                .get(entry.start - 1)
                .is_none_or(|i| i.opcode != Opcode::Argument || i.a != 2)
        {
            return Err("snapshot aggregation argument role".into());
        }
        previous = Some(entry.start);
    }
    if t.labels.len() > t.code.len() || t.labels.iter().any(|target| target.index() >= t.code.len())
    {
        return Err("snapshot label table".into());
    }
    let target = |n: u32| {
        t.resolve_label(Label(n))
            .map(InstructionIndex::index)
            .ok_or_else(|| "snapshot jump label".to_owned())
    };
    for (ip, i) in t.code.iter().enumerate() {
        if i.flags != 0 || i.reserved != 0 {
            return Err("snapshot instruction flags".into());
        }
        match i.opcode {
            Opcode::Text | Opcode::Error | Opcode::Name | Opcode::QualifiedName => {
                let end = (i.a as usize)
                    .checked_add(i.b as usize)
                    .ok_or("snapshot operand overflow")?;
                let bytes = t
                    .operands
                    .get(i.a as usize..end)
                    .ok_or("snapshot operand bounds")?;
                if i.opcode == Opcode::QualifiedName {
                    let mut reader = SnapshotReader::new(bytes);
                    reader.string()?;
                    std::str::from_utf8(reader.remaining())
                        .map_err(|_| "snapshot qualified name UTF8")?;
                } else {
                    std::str::from_utf8(bytes).map_err(|_| "snapshot UTF8 operand")?;
                }
            }
            Opcode::Reference if i.a >= t.reference_slots => {
                return Err("snapshot reference slot".into());
            }
            Opcode::Invoke | Opcode::Conditional => {
                if i.a as usize >= calls {
                    return Err("snapshot call index/arity".into());
                }
                let continuation = target(i.c)?;
                if continuation <= ip {
                    return Err("snapshot backwards invocation".into());
                }
                let jump = t.code.get(ip + 1).ok_or("snapshot invoke jump")?;
                if jump.opcode != Opcode::Jump || jump.c != i.c {
                    return Err("snapshot invoke continuation".into());
                }
                let mut descriptor = ip + 2;
                for ordinal in 0..i.b {
                    let argument = t
                        .code
                        .get(descriptor)
                        .ok_or("snapshot argument descriptor")?;
                    let next = target(argument.c)?;
                    if argument.opcode != Opcode::Argument
                        || argument.a != ordinal
                        || next <= descriptor
                        || next > continuation
                    {
                        return Err("snapshot argument chain".into());
                    }
                    descriptor = next;
                }
                if descriptor != continuation {
                    return Err("snapshot argument count".into());
                }
            }
            Opcode::Argument | Opcode::Jump => {
                let continuation = target(i.c)?;
                if continuation <= ip {
                    return Err("snapshot backwards jump".into());
                }
            }
            Opcode::Unary if i.a > 4 => return Err("snapshot unary".into()),
            Opcode::Binary if i.a > 14 => return Err("snapshot binary".into()),
            _ => {}
        }
    }
    Ok(())
}

/// Validate concrete reference encodings against a template's slot requirements.
pub(super) fn validate_references(r: &ReferenceBindings, slots: u32) -> Result<(), String> {
    if r.slots.len() != slots as usize {
        return Err("snapshot reference count/bytes".into());
    }
    for s in &r.slots {
        let end = (s.offset as usize)
            .checked_add(s.length as usize)
            .ok_or("snapshot reference overflow")?;
        let mut reader = SnapshotReader::new(
            r.data
                .get(s.offset as usize..end)
                .ok_or("snapshot reference bounds")?,
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
                    return Err("snapshot reference index".into());
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
            return Err("snapshot reference trailing bytes".into());
        }
    }
    Ok(())
}
