//! AST-aware storage spelling for the modern spill-range reference operator.
use crate::IoError;
use poirot_parsing::formula::{Expr, FormulaPrinter, UnaryOp, parse_formula};

/// Encode postfix spill syntax using the stored future-function spelling.
/// Format evidence: rust_xlsxwriter 0.101.0 Formula documentation, section
/// "The Spilled Range Operator #" and its future-function table:
/// https://docs.rs/rust_xlsxwriter/0.101.0/rust_xlsxwriter/struct.Formula.html
/// This is writer-author format documentation, not Microsoft normative proof.
/// No upstream implementation or tests were copied or adapted.
pub(crate) fn stored_formula(source: &str) -> Result<String, IoError> {
    if !source.contains('#') {
        return Ok(source.trim_start_matches('=').into());
    }
    let Ok(mut expression) = parse_formula(source) else {
        // Existing export accepts formulas beyond Poirot's executable grammar.
        return Ok(source.trim_start_matches('=').into());
    };
    if !rewrite(&mut expression)? {
        return Ok(source.trim_start_matches('=').into());
    }
    FormulaPrinter::print(&expression).map_err(|error| IoError::Unsupported(error.to_string()))
}
/// Rewrite only spill operators using a fallibly grown stack of disjoint mutable nodes.
fn rewrite(expression: &mut Expr) -> Result<bool, IoError> {
    let allocation = |_| IoError::Unsupported("spill formula export allocation failed".into());
    let mut pending = Vec::new();
    pending.try_reserve(1).map_err(allocation)?;
    // SAFETY: try_reserve(1) just succeeded on the empty stack.
    pending.push(expression);
    let mut changed = false;
    while let Some(expression) = pending.pop() {
        if matches!(
            expression,
            Expr::Unary {
                op: UnaryOp::SpillRange,
                ..
            }
        ) {
            let mut args = Vec::new();
            args.try_reserve_exact(1).map_err(allocation)?;
            let previous = std::mem::replace(expression, Expr::Missing);
            match previous {
                Expr::Unary { operand, .. } => {
                    // SAFETY: try_reserve_exact(1) succeeded on this empty vector;
                    // no argument was inserted between that reservation and this push.
                    args.push(*operand);
                    *expression = Expr::Function {
                        name: "_xlfn.ANCHORARRAY".into(),
                        args,
                    };
                    changed = true;
                }
                other => *expression = other,
            }
        }
        let children = match expression {
            Expr::Unary { .. } => 1,
            Expr::Binary { .. } => 2,
            Expr::Function { args, .. } => args.len(),
            Expr::Array(rows) => rows
                .iter()
                .try_fold(0usize, |count, row| count.checked_add(row.len()))
                .ok_or_else(|| IoError::Unsupported("spill formula export size overflow".into()))?,
            _ => 0,
        };
        pending.try_reserve(children).map_err(allocation)?;
        match expression {
            Expr::Unary { operand, .. } => {
                // SAFETY: try_reserve(children) just reserved this unary node's one child.
                pending.push(operand.as_mut());
            }
            Expr::Binary { left, right, .. } => {
                // SAFETY: try_reserve(children) just reserved this binary node's two children.
                pending.extend([left.as_mut(), right.as_mut()]);
            }
            Expr::Function { args, .. } => {
                // SAFETY: children was args.len(); no arguments or stack entries have
                // changed since the successful reservation of that many slots.
                pending.extend(args.iter_mut());
            }
            Expr::Array(rows) => {
                // SAFETY: children was the checked sum of these unchanged row lengths;
                // the stack has not changed since reserving that many slots.
                pending.extend(rows.iter_mut().flatten());
            }
            _ => {}
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_spill_syntax_is_rewritten_and_literals_are_preserved() {
        assert_eq!(
            stored_formula("SUM('Quoted sheet'!A1#)+LEN(\"A1#\")").unwrap(),
            "SUM(_xlfn.ANCHORARRAY('Quoted sheet'!A1))+LEN(\"A1#\")"
        );
        assert_eq!(stored_formula("=\"A1#\"").unwrap(), "\"A1#\"");
        assert_eq!(
            stored_formula("SUM(A1#,(B1#))").unwrap(),
            "SUM(_xlfn.ANCHORARRAY(A1),_xlfn.ANCHORARRAY(B1))"
        );
    }
    #[test]
    fn flat_postfix_chains_reach_the_existing_bounded_printer_without_recursive_rewrite() {
        // Pratt parsing does not recurse for a postfix chain. The mutable stack
        // visits it safely, and the existing printer rejects its excessive depth.
        let formula = format!("A1{}", "#".repeat(512));
        assert!(parse_formula(&formula).is_ok());
        assert!(
            matches!(stored_formula(&formula), Err(IoError::Unsupported(detail)) if detail.contains("printer complexity limit exceeded"))
        );
    }
    #[test]
    fn deep_ordinary_formula_with_hash_literal_is_preserved_without_printing() {
        let formula = format!("{}LEN(\"#\")", "1+".repeat(512));
        assert!(parse_formula(&formula).is_ok());
        assert_eq!(stored_formula(&formula).unwrap(), formula);
    }
}
