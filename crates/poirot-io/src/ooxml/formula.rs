//! AST-aware storage spelling for modern functions and spill-range references.
use crate::IoError;
use poirot_parsing::formula::{Expr, FormulaPrinter, UnaryOp, parse_formula};

/// Encode modern function names and postfix spill syntax for XLSX storage.
/// Format evidence: rust_xlsxwriter 0.101.0 Formula documentation, section
/// "The Spilled Range Operator #" and its future-function table:
/// https://docs.rs/rust_xlsxwriter/0.101.0/rust_xlsxwriter/struct.Formula.html
/// This is writer-author format documentation, not Microsoft normative proof.
/// No upstream implementation or tests were copied or adapted.
pub(crate) fn stored_formula(source: &str) -> Result<String, IoError> {
    let Ok(mut expression) = parse_formula(source) else {
        // Existing export accepts formulas beyond Poirot's executable grammar.
        return Ok(source.trim_start_matches('=').into());
    };
    if !rewrite(&mut expression)? {
        return Ok(source.trim_start_matches('=').into());
    }
    FormulaPrinter::print(&expression).map_err(|error| IoError::Unsupported(error.to_string()))
}
/// Assign XLSX storage names independently of the evaluator's support catalog.
/// MS-XLSX §2.2.3 lists the future-function spellings, retrieved 2026-10-21:
/// https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf
/// ARRAYTOTEXT, VALUETOTEXT and XMATCH supplement that table from the public
/// rust_xlsxwriter 0.101.0 Formula documentation's future-function table:
/// https://docs.rs/rust_xlsxwriter/0.101.0/rust_xlsxwriter/struct.Formula.html
/// Only format names were used; no upstream implementation or tests reused.
/// REGEXTEST/EXTRACT/REPLACE and TRIMRANGE postdate those tables and use `_xlfn.`
/// future-function convention; native Excel storage verification is still pending.
/// Storage support does not make an unsupported function executable.
fn modern_stored_name(name: &str) -> Option<&'static str> {
    const NAMES: &[&str] = &[
        "_xlfn._xlws.FILTER",
        "_xlfn._xlws.SORT",
        "_xlfn._xlws.PY",
        "_xlfn.ACOT",
        "_xlfn.ACOTH",
        "_xlfn.AGGREGATE",
        "_xlfn.ARABIC",
        "_xlfn.BASE",
        "_xlfn.BETA.DIST",
        "_xlfn.BETA.INV",
        "_xlfn.BINOM.DIST",
        "_xlfn.BINOM.DIST.RANGE",
        "_xlfn.BINOM.INV",
        "_xlfn.BITAND",
        "_xlfn.BITLSHIFT",
        "_xlfn.BITOR",
        "_xlfn.BITRSHIFT",
        "_xlfn.BITXOR",
        "_xlfn.BYCOL",
        "_xlfn.BYROW",
        "_xlfn.CEILING.MATH",
        "_xlfn.CEILING.PRECISE",
        "_xlfn.CHISQ.DIST",
        "_xlfn.CHISQ.DIST.RT",
        "_xlfn.CHISQ.INV",
        "_xlfn.CHISQ.INV.RT",
        "_xlfn.CHISQ.TEST",
        "_xlfn.CHOOSECOLS",
        "_xlfn.CHOOSEROWS",
        "_xlfn.COMBINA",
        "_xlfn.CONCAT",
        "_xlfn.CONFIDENCE.NORM",
        "_xlfn.CONFIDENCE.T",
        "_xlfn.COT",
        "_xlfn.COTH",
        "_xlfn.COVARIANCE.P",
        "_xlfn.COVARIANCE.S",
        "_xlfn.CSC",
        "_xlfn.CSCH",
        "_xlfn.DAYS",
        "_xlfn.DECIMAL",
        "_xlfn.DROP",
        "_xlfn.ERF.PRECISE",
        "_xlfn.ERFC.PRECISE",
        "_xlfn.EXPAND",
        "_xlfn.EXPON.DIST",
        "_xlfn.F.DIST",
        "_xlfn.F.DIST.RT",
        "_xlfn.F.INV",
        "_xlfn.F.INV.RT",
        "_xlfn.F.TEST",
        "_xlfn.FIELDVALUE",
        "_xlfn.FILTERXML",
        "_xlfn.FLOOR.MATH",
        "_xlfn.FLOOR.PRECISE",
        "_xlfn.FORECAST.ETS",
        "_xlfn.FORECAST.ETS.CONFINT",
        "_xlfn.FORECAST.ETS.SEASONALITY",
        "_xlfn.FORECAST.ETS.STAT",
        "_xlfn.FORECAST.LINEAR",
        "_xlfn.FORMULATEXT",
        "_xlfn.GAMMA",
        "_xlfn.GAMMA.DIST",
        "_xlfn.GAMMA.INV",
        "_xlfn.GAMMALN.PRECISE",
        "_xlfn.GAUSS",
        "_xlfn.HSTACK",
        "_xlfn.HYPGEOM.DIST",
        "_xlfn.IFNA",
        "_xlfn.IFS",
        "_xlfn.IMCOSH",
        "_xlfn.IMCOT",
        "_xlfn.IMCSC",
        "_xlfn.IMCSCH",
        "_xlfn.IMSEC",
        "_xlfn.IMSECH",
        "_xlfn.IMSINH",
        "_xlfn.IMTAN",
        "_xlfn.ISFORMULA",
        "_xlfn.ISOMITTED",
        "_xlfn.ISOWEEKNUM",
        "_xlfn.LAMBDA",
        "_xlfn.LET",
        "_xlfn.LOGNORM.DIST",
        "_xlfn.LOGNORM.INV",
        "_xlfn.MAKEARRAY",
        "_xlfn.MAP",
        "_xlfn.MAXIFS",
        "_xlfn.MINIFS",
        "_xlfn.MODE.MULT",
        "_xlfn.MODE.SNGL",
        "_xlfn.MUNIT",
        "_xlfn.NEGBINOM.DIST",
        "_xlfn.NORM.DIST",
        "_xlfn.NORM.INV",
        "_xlfn.NORM.S.DIST",
        "_xlfn.NORM.S.INV",
        "_xlfn.NUMBERVALUE",
        "_xlfn.PDURATION",
        "_xlfn.PERCENTILE.EXC",
        "_xlfn.PERCENTILE.INC",
        "_xlfn.PERCENTRANK.EXC",
        "_xlfn.PERCENTRANK.INC",
        "_xlfn.PERMUTATIONA",
        "_xlfn.PHI",
        "_xlfn.POISSON.DIST",
        "_xlfn.PQSOURCE",
        "_xlfn.PYTHON_STR",
        "_xlfn.PYTHON_TYPE",
        "_xlfn.PYTHON_TYPENAME",
        "_xlfn.QUARTILE.EXC",
        "_xlfn.QUARTILE.INC",
        "_xlfn.QUERYSTRING",
        "_xlfn.RANDARRAY",
        "_xlfn.RANK.AVG",
        "_xlfn.RANK.EQ",
        "_xlfn.REDUCE",
        "_xlfn.REGEXEXTRACT",
        "_xlfn.REGEXREPLACE",
        "_xlfn.GROUPBY",
        "_xlfn.TRIMRANGE",
        "_xlfn.PERCENTOF",
        "_xlfn.REGEXTEST",
        "_xlfn.RRI",
        "_xlfn.SCAN",
        "_xlfn.SEC",
        "_xlfn.SECH",
        "_xlfn.SEQUENCE",
        "_xlfn.SHEET",
        "_xlfn.SHEETS",
        "_xlfn.SKEW.P",
        "_xlfn.SORTBY",
        "_xlfn.STDEV.P",
        "_xlfn.STDEV.S",
        "_xlfn.SWITCH",
        "_xlfn.T.DIST",
        "_xlfn.T.DIST.2T",
        "_xlfn.T.DIST.RT",
        "_xlfn.T.INV",
        "_xlfn.T.INV.2T",
        "_xlfn.T.TEST",
        "_xlfn.TAKE",
        "_xlfn.TEXTAFTER",
        "_xlfn.TEXTBEFORE",
        "_xlfn.TEXTJOIN",
        "_xlfn.TEXTSPLIT",
        "_xlfn.TOCOL",
        "_xlfn.TOROW",
        "_xlfn.UNICHAR",
        "_xlfn.UNICODE",
        "_xlfn.UNIQUE",
        "_xlfn.VAR.P",
        "_xlfn.VAR.S",
        "_xlfn.VSTACK",
        "_xlfn.WEBSERVICE",
        "_xlfn.WEIBULL.DIST",
        "_xlfn.WRAPCOLS",
        "_xlfn.WRAPROWS",
        "_xlfn.XLOOKUP",
        "_xlfn.XOR",
        "_xlfn.Z.TEST",
        "_xlfn.ARRAYTOTEXT",
        "_xlfn.VALUETOTEXT",
        "_xlfn.XMATCH",
    ];
    NAMES.iter().copied().find(|stored| {
        stored
            .strip_prefix("_xlfn.")
            .map(|suffix| suffix.strip_prefix("_xlws.").unwrap_or(suffix))
            .is_some_and(|canonical| name.eq_ignore_ascii_case(canonical))
    })
}
/// Rewrite storage-only syntax using a fallibly grown stack of disjoint mutable nodes.
fn rewrite(expression: &mut Expr) -> Result<bool, IoError> {
    let allocation = |_| IoError::Unsupported("spill formula export allocation failed".into());
    let mut pending = Vec::new();
    pending.try_reserve(1).map_err(allocation)?;
    // SAFETY: try_reserve(1) just succeeded on the empty stack.
    pending.push(expression);
    let mut changed = false;
    while let Some(expression) = pending.pop() {
        if let Expr::Function { name, .. } = expression
            && let Some(stored) = modern_stored_name(name.as_str())
        {
            *name = stored.into();
            changed = true;
        }
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
