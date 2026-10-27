use super::aggregation;
use super::bytes::Bytes;
use super::compile_error::CompileError;
use super::compiled::Compiled;
use super::executable::Executable;
use super::high::High;
use super::preparation::Preparation;
use super::reference_bindings::ReferenceBindings;
use super::reference_kind::ReferenceKind;
use super::sink::Sink;
use super::sizes::Sizes;
use super::template::Template;
use super::writer::Writer;
use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};
use std::sync::Arc;

pub(super) const LIMIT: usize = 100_000;

pub(in crate::eval) const DEPTH: usize = 128;

pub(super) const NONE: u32 = u32::MAX;

/// Compile caller-owned syntax. The result is fully independent of the syntax lifetime.
/// Structural errors are reported before allocating any output buffers.
pub fn compile(expression: &Expr) -> Result<Compiled, CompileError> {
    compile_inner(expression, None, true)
}

/// Bind calls once against exactly the same namespace as the tree binder.
/// Production binding truncates excessive expression depth to a deferred #NUM! atom.
pub fn compile_with_registry(
    expression: &Expr,
    registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
) -> Result<Compiled, CompileError> {
    compile_inner(expression, registry, false)
}

/// Compile an independently supplied argument for the contextual aggregation role.
/// Ordinary compile entry points deliberately do not project root worksheet names.
pub fn compile_aggregation_argument(expression: &Expr) -> Result<Compiled, CompileError> {
    let mut compiled = compile_inner(expression, None, true)?;
    if let Some(plan) = aggregation::lower(expression)? {
        compiled
            .template
            .aggregations
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        // SAFETY: Index zero is always a valid insertion position; one additional slot was reserved locally.
        compiled
            .template
            .aggregations
            .insert(0, aggregation::Entry { start: 0, plan });
    }
    Ok(compiled)
}

pub(super) fn compile_inner(
    expression: &Expr,
    registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
    strict: bool,
) -> Result<Compiled, CompileError> {
    let mut sizes = Sizes::default();
    let mut nodes = 0;
    validate(expression, 0, &mut nodes, strict)?;
    stream(expression, 0, &mut sizes);
    if [
        sizes.code,
        sizes.labels,
        sizes.text,
        sizes.refs,
        sizes.reference_bytes,
    ]
    .iter()
    .any(|n| *n > u32::MAX as usize)
    {
        return Err(CompileError::ResourceLimit);
    }
    let writer = Writer {
        output: Compiled {
            executable: Arc::new(Executable {
                typing: crate::eval::compile::typing::FormulaTyping::default(),
                template: Template {
                    labels: Vec::with_capacity(sizes.labels),
                    aggregations: Vec::new(),
                    code: Vec::with_capacity(sizes.code),
                    operands: Vec::with_capacity(sizes.text),
                    reference_slots: 0,
                },
                calls: Vec::with_capacity(sizes.calls),
                extension_identity: registry.map(|r| r.identity()),
            }),
            references: ReferenceBindings {
                slots: Vec::with_capacity(sizes.refs),
                data: Vec::with_capacity(sizes.reference_bytes),
            },
        },
        error: None,
        pending_labels: [None; DEPTH * 2],
        aggregation_roles: [None; DEPTH],
        registry,
    };
    let mut preparation = Preparation {
        writer,
        inference: crate::eval::compile::typing::InferenceMachine::new(sizes.expressions)?,
        error: None,
    };
    stream(expression, 0, &mut preparation);
    let Preparation {
        mut writer,
        inference,
        error,
    } = preparation;
    if let Some(error) = error.or(writer.error) {
        return Err(error);
    }
    if writer.pending_labels.iter().any(Option::is_some)
        || writer.output.template.labels.len() != sizes.labels
        || writer
            .output
            .template
            .labels
            .iter()
            .any(|target| target.0 == NONE)
        || writer.output.template.code.len() != sizes.code
        || writer.output.template.operands.len() != sizes.text
        || writer.output.references.data.len() != sizes.reference_bytes
    {
        return Err(CompileError::InvalidReference);
    }
    writer.output.typing = inference.finish()?;
    Ok(writer.output)
}

pub(super) fn validate(
    e: &Expr,
    depth: usize,
    nodes: &mut usize,
    strict: bool,
) -> Result<(), CompileError> {
    if depth >= DEPTH && !strict {
        return Ok(());
    }
    *nodes += 1;
    if *nodes > LIMIT || depth >= DEPTH {
        return Err(CompileError::ResourceLimit);
    }
    let invalid_cell = |c: &poirot_parsing::formula::CellReference| {
        c.row >= crate::model::MAX_ROWS || c.column >= crate::model::MAX_COLUMNS
    };
    match e {
        Expr::Unary { operand, .. } => validate(operand, depth + 1, nodes, strict)?,
        Expr::Binary { left, right, .. } => {
            validate(left, depth + 1, nodes, strict)?;
            validate(right, depth + 1, nodes, strict)?;
        }
        Expr::Function { args, .. } => {
            for a in args {
                validate(a, depth + 1, nodes, strict)?;
            }
        }
        Expr::Array(rows) => {
            let width = rows.first().map_or(0, Vec::len);
            if width == 0 || rows.iter().any(|r| r.len() != width) {
                return Err(CompileError::InvalidArray);
            }
            for a in rows.iter().flatten() {
                validate(a, depth + 1, nodes, strict)?;
            }
        }
        Expr::Cell(c) if invalid_cell(c) => return Err(CompileError::InvalidReference),
        Expr::Column { column, .. } if *column >= crate::model::MAX_COLUMNS => {
            return Err(CompileError::InvalidReference);
        }
        Expr::Row { row, .. } if *row >= crate::model::MAX_ROWS => {
            return Err(CompileError::InvalidReference);
        }
        Expr::SheetSpan(s)
            if s.first_sheet.is_empty()
                || s.last_sheet.is_empty()
                || std::iter::once(&s.start)
                    .chain(s.end.iter())
                    .any(|c| invalid_cell(c) || c.sheet.is_some()) =>
        {
            return Err(CompileError::InvalidReference);
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn stream<'ast>(e: &'ast Expr, depth: usize, sink: &mut impl Sink<'ast>) {
    sink.emit(High::Enter(depth, e));
    if depth >= DEPTH {
        sink.emit(High::Truncated);
        sink.emit(High::Leave);
        return;
    }
    match e {
        Expr::Unary { op, operand } => {
            stream(operand, depth + 1, sink);
            sink.emit(High::Unary(*op));
        }
        Expr::Binary { op, left, right } => {
            stream(left, depth + 1, sink);
            stream(right, depth + 1, sink);
            sink.emit(High::Binary(*op));
        }
        Expr::Array(rows) => {
            sink.emit(High::ArrayAllocate(
                rows.len(),
                rows.first().map_or(0, Vec::len),
            ));
            for a in rows.iter().flatten() {
                stream(a, depth + 1, sink);
                sink.emit(High::Scalarize);
            }
            sink.emit(High::Array(rows.len(), rows.first().map_or(0, Vec::len)));
        }
        Expr::Function { name, args } => {
            let done = depth * 2;
            let next = done + 1;
            sink.emit(High::Invoke(name, args.len(), done));
            sink.emit(High::Goto(done));
            for (i, a) in args.iter().enumerate() {
                sink.emit(High::Argument(i, next, depth, a));
                stream(a, depth + 1, sink);
                sink.emit(High::YieldArgument);
                sink.emit(High::Label(next));
            }
            sink.emit(High::Label(done));
        }
        _ => sink.emit(High::Atom(e)),
    }
    sink.emit(High::Leave);
}

pub(super) fn reference(e: &Expr, out: &mut impl Bytes) -> Option<ReferenceKind> {
    Some(match e {
        Expr::Cell(c) => {
            out.cell(c);
            ReferenceKind::Cell
        }
        Expr::Column {
            column,
            absolute,
            sheet,
        } => {
            out.word(*column);
            out.word((*absolute).into());
            out.optional(sheet.as_deref());
            ReferenceKind::Column
        }
        Expr::Row {
            row,
            absolute,
            sheet,
        } => {
            out.word(*row);
            out.word((*absolute).into());
            out.optional(sheet.as_deref());
            ReferenceKind::Row
        }
        Expr::SheetSpan(s) => {
            out.string(&s.first_sheet);
            out.string(&s.last_sheet);
            out.cell(&s.start);
            out.word(s.end.is_some().into());
            if let Some(c) = &s.end {
                out.cell(c);
            }
            ReferenceKind::SheetSpan
        }
        _ => return None,
    })
}

pub(super) fn unary(op: UnaryOp) -> u32 {
    match op {
        UnaryOp::Plus => 0,
        UnaryOp::Minus => 1,
        UnaryOp::Percent => 2,
        UnaryOp::ImplicitIntersection => 3,
        UnaryOp::SpillRange => 4,
    }
}

pub(super) fn binary(op: BinaryOp) -> u32 {
    match op {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Power => 4,
        BinaryOp::Concat => 5,
        BinaryOp::Equal => 6,
        BinaryOp::NotEqual => 7,
        BinaryOp::Less => 8,
        BinaryOp::LessEqual => 9,
        BinaryOp::Greater => 10,
        BinaryOp::GreaterEqual => 11,
        BinaryOp::Range => 12,
        BinaryOp::Union => 13,
        BinaryOp::Intersection => 14,
    }
}

/// Convenience entry point retaining neither the parsed source nor its AST.
pub fn lower_parsed(
    parsed: &poirot_parsing::formula::ParsedFormula,
) -> Result<Compiled, CompileError> {
    compile(parsed.expression())
}
