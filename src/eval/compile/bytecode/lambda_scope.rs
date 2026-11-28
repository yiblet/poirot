use super::compile_error::CompileError;
use super::lexical_scope::LexicalScope;
use poirot_parsing::formula::{Expr, LambdaForm, LetForm};

/// Frame layout for a lambda body with immutable captures followed by parameters.
pub(super) struct LambdaScope<'ast, 'registry> {
    /// Slots to copy from the enclosing frame.
    pub(super) captures: Vec<usize>,
    /// Local name-to-slot mapping inside the lambda body.
    pub(super) body: LexicalScope<'ast, 'registry>,
}

impl<'ast, 'registry> LambdaScope<'ast, 'registry> {
    /// Compile only names free in the body, including names used by nested closures.
    pub(super) fn new(
        form: &LambdaForm<'ast>,
        outer: &LexicalScope<'ast, 'registry>,
        remaining: usize,
    ) -> Result<Self, CompileError> {
        let mut captures = Vec::new();
        let mut local = LexicalScope::new(outer.registry);
        if outer.next_slot() != 0 {
            for parameter in form.parameters() {
                local.bind(parameter)?;
            }
            collect(form.body, remaining, &mut local, outer, &mut captures)?;
            local.finish_scope(0);
        }
        for slot in &captures {
            local.bind(outer.name(*slot).ok_or(CompileError::InvalidReference)?)?;
        }
        for parameter in form.parameters() {
            local.bind(parameter)?;
        }
        Ok(Self {
            captures,
            body: local,
        })
    }
}

/// Find visible outer declarations while respecting lexical shadowing.
fn collect<'ast, 'registry>(
    expression: &'ast Expr,
    remaining: usize,
    inner: &mut LexicalScope<'ast, 'registry>,
    outer: &LexicalScope<'ast, 'registry>,
    captures: &mut Vec<usize>,
) -> Result<(), CompileError> {
    let Some(remaining) = remaining.checked_sub(1) else {
        return Ok(());
    };
    match expression {
        Expr::Name(name) => {
            if inner.slot(name).is_none()
                && let Some(slot) = outer.slot(name)
            {
                capture(captures, slot)?;
            }
        }
        Expr::Function { name, args } if LetForm::matches(name) => {
            if let Some(form) = LetForm::parse(args) {
                let base = inner.next_slot();
                for (name, initializer) in form.bindings() {
                    collect(initializer, remaining, inner, outer, captures)?;
                    inner.bind(name)?;
                }
                collect(form.result, remaining, inner, outer, captures)?;
                inner.finish_scope(base);
            }
        }
        Expr::Function { name, args } if LambdaForm::matches(name) => {
            if let Some(form) = LambdaForm::parse(args) {
                let base = inner.next_slot();
                for parameter in form.parameters() {
                    inner.bind(parameter)?;
                }
                collect(form.body, remaining, inner, outer, captures)?;
                inner.finish_scope(base);
            }
        }
        Expr::Function { name, args } => {
            if inner.slot(name).is_none()
                && let Some(slot) = outer.callable_slot(name)
            {
                capture(captures, slot)?;
            }
            for operand in args {
                collect(operand, remaining, inner, outer, captures)?;
            }
        }
        Expr::Invoke { callee, args } => {
            collect(callee, remaining, inner, outer, captures)?;
            for operand in args {
                collect(operand, remaining, inner, outer, captures)?;
            }
        }
        Expr::Array(rows) => {
            for operand in rows.iter().flatten() {
                collect(operand, remaining, inner, outer, captures)?;
            }
        }
        Expr::Unary { operand, .. } => collect(operand, remaining, inner, outer, captures)?,
        Expr::Binary { left, right, .. } => {
            collect(left, remaining, inner, outer, captures)?;
            collect(right, remaining, inner, outer, captures)?;
        }
        _ => {}
    }
    Ok(())
}

/// Retain each free declaration once, in its first-use order.
fn capture(captures: &mut Vec<usize>, slot: usize) -> Result<(), CompileError> {
    if !captures.contains(&slot) {
        captures
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        // SAFETY: The successful reservation admits the additional slot.
        captures.push(slot);
    }
    Ok(())
}
