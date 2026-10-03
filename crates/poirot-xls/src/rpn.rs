use super::*;
use poirot_formula::{BinaryOp, Expr, GroupingContext, UnaryOp, needs_parentheses};
/// Lower supported formula syntax into BIFF RPN with the supplied source context.
#[cfg(test)]
pub(super) fn compile(
    w: &Workbook,
    s: usize,
    e: &Expr,
    class: u8,
    budget: usize,
) -> Result<Vec<u8>, XlsError> {
    let mut addins = addins::AddInPlan::default();
    addins.visit(e)?;
    compile_with_addins(w, s, e, class, budget, &addins)
}
/// Lower formula syntax using preplanned external add-in indexes.
pub(super) fn compile_with_addins(
    w: &Workbook,
    s: usize,
    e: &Expr,
    class: u8,
    budget: usize,
    addins: &addins::AddInPlan,
) -> Result<Vec<u8>, XlsError> {
    let mut out = Vec::new();
    emit(w, s, e, class, budget, addins, &mut out)?;
    Ok(out)
}

fn emit(
    w: &Workbook,
    s: usize,
    e: &Expr,
    class: u8,
    budget: usize,
    addins: &addins::AddInPlan,
    out: &mut Vec<u8>,
) -> Result<(), XlsError> {
    match e {
        Expr::Number(n) => {
            if !n.is_finite() {
                return Err(fail("nonfinite formula number"));
            }
            if n.fract() == 0. && (0.0..=65535.).contains(n) {
                out.push(0x1e);
                short(out, *n as u16)
            } else {
                out.push(0x1f);
                out.extend(n.to_le_bytes())
            }
        }
        Expr::String(t) => {
            if t.encode_utf16().count() > 255 {
                return Err(fail("formula string literal exceeds BIFF8 255-unit limit"));
            }
            let units = unicode(t)?;
            let wide = units.iter().any(|u| *u > 255);
            out.extend([0x17, units.len() as u8, u8::from(wide)]);
            for u in units {
                if wide {
                    short(out, u)
                } else {
                    out.push(u as u8)
                }
            }
        }
        Expr::Bool(b) => out.extend([0x1d, u8::from(*b)]),
        Expr::Error(e) => out.extend([
            0x1c,
            error_code(e.parse().map_err(|_| fail("unknown formula error"))?)?,
        ]),
        Expr::Missing => out.push(0x16),
        Expr::SheetSpan(_) => {
            return Err(XlsError::Unsupported(
                "3D sheet-span BIFF8 reference token encoding is not independently verified".into(),
            ));
        }
        Expr::Cell(c) => {
            if c.row >= 65536 || c.column >= 256 {
                return Err(fail("formula reference exceeds XLS grid"));
            }
            if let Some(name) = &c.sheet {
                out.push(0x3a + class);
                short(out, sheet(w, name)? as u16)
            } else {
                out.push(0x24 + class)
            }
            short(out, c.row as u16);
            short(
                out,
                column(c.column as u16, c.row_absolute, c.column_absolute),
            );
        }
        Expr::Column { .. } | Expr::Row { .. } => {
            return Err(fail("whole-column/row endpoint must occur within a range"));
        }
        Expr::QualifiedName { sheet, name } => {
            // BIFF NamePtg can select a NAME record, but the current writer has
            // no NameX/local external-name table to preserve explicit scope.
            // Do not silently erase the qualifier or invent an external link.
            return Err(fail(&format!(
                "XLS sheet-qualified defined name is unsupported: {sheet}!{name}"
            )));
        }
        Expr::Name(n) => {
            let resolved = w
                .try_defined_name(n, s)
                .map_err(|error| fail(&error.to_string()))?
                .ok_or_else(|| fail(&format!("undefined name {n}")))?;
            let index = w
                .named_ranges
                .iter()
                .position(|existing| std::ptr::eq(existing, resolved))
                .ok_or_else(|| fail("resolved name is missing from workbook"))?;
            out.push(0x23 + class);
            long(out, index as u32 + 1)
        }
        Expr::Unary {
            op: UnaryOp::ImplicitIntersection,
            ..
        } => {
            return Err(XlsError::Unsupported(
                "explicit @ implicit intersection has no supported BIFF8 encoding".into(),
            ));
        }
        Expr::Unary { op, operand } => {
            emit(w, s, operand, 0x20, budget, addins, out)?;
            if needs_parentheses(operand, GroupingContext::UnaryOperand(*op)) {
                out.push(0x15)
            }
            out.push(match op {
                // SAFETY: The unary branch rejects ImplicitIntersection before selecting its emitted opcode.
                UnaryOp::ImplicitIntersection => unreachable!("rejected above"),
                UnaryOp::Plus => 0x12,
                UnaryOp::Minus => 0x13,
                UnaryOp::Percent => 0x14,
            })
        }
        Expr::Binary {
            op: BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection,
            ..
        } => {
            let plan = reference_plan::ReferencePlan::new(w, e)?;
            reference_plan::emit_plan(w, s, &plan, class, budget, addins, out)?;
        }
        Expr::Binary { op, left, right } => {
            let child_class = 0x20;
            child(w, s, left, (*op, false, child_class), budget, addins, out)?;
            child(w, s, right, (*op, true, child_class), budget, addins, out)?;
            out.push(match op {
                BinaryOp::Add => 3,
                BinaryOp::Subtract => 4,
                BinaryOp::Multiply => 5,
                BinaryOp::Divide => 6,
                BinaryOp::Power => 7,
                BinaryOp::Concat => 8,
                BinaryOp::Less => 9,
                BinaryOp::LessEqual => 10,
                BinaryOp::Equal => 11,
                BinaryOp::GreaterEqual => 12,
                BinaryOp::Greater => 13,
                BinaryOp::NotEqual => 14,
                BinaryOp::Intersection => 15,
                BinaryOp::Union => 16,
                _ => return Err(fail("unsupported binary operator")),
            })
        }
        Expr::Function { name, args } => {
            if let Some(index) = addins.index(name) {
                if w.named_ranges.iter().any(|named| {
                    (named.sheet_scope.is_none() || named.sheet_scope == Some(s))
                        && named.name.eq_ignore_ascii_case(name)
                }) {
                    return Err(XlsError::Unsupported(format!(
                        "defined name {name} shadows add-in function; function-defined names unsupported"
                    )));
                }
                // POI external calls include their NameX identity as argument zero;
                // all parameters of function index 255 have value operand class.
                out.push(0x59);
                short(out, w.sheets().len() as u16);
                short(out, index as u16 + 1);
                short(out, 0);
                check_budget(out, budget)?;
                for arg in args {
                    emit(w, s, arg, 0x20, budget, addins, out)?;
                    if needs_parentheses(arg, GroupingContext::FunctionArgument) {
                        out.push(0x15);
                    }
                }
                out.extend([0x42, (args.len() + 1) as u8, 0xff, 0]);
                return check_budget(out, budget);
            }
            let metadata = functions::metadata(name)?.ok_or_else(|| {
                XlsError::Unsupported(format!("function {name} is not a legacy BIFF8 built-in"))
            })?;
            if args.len() < metadata.min_args || args.len() > metadata.max_args {
                return Err(fail(&format!(
                    "{name} argument count cannot be represented by supported legacy metadata"
                )));
            }
            for (i, arg) in args.iter().enumerate() {
                let expected = metadata.parameter(i)?;
                if expected == functions::OperandClass::Array
                    && !reference_plan::array_reference(arg)?
                {
                    return Err(XlsError::Unsupported(format!(
                        "array-operand function {name} unsupported"
                    )));
                }
                emit(w, s, arg, expected.bits(), budget, addins, out)?;
                if needs_parentheses(arg, GroupingContext::FunctionArgument) {
                    out.push(0x15);
                }
            }
            let result = match metadata.result {
                functions::OperandClass::Reference => {
                    // Pinned OperandClassTransformer keeps a reference-returning
                    // function in CLASS_REF when an array-class parameter requests it.
                    if class == 0x40 { 0 } else { class }
                }
                functions::OperandClass::Value => metadata.result.bits(),
                functions::OperandClass::Array => {
                    return Err(XlsError::Unsupported(format!(
                        "array-result function {name} unsupported"
                    )));
                }
            };
            if metadata.min_args == metadata.max_args {
                out.push(0x21 + result);
                short(out, metadata.id)
            } else {
                out.extend([0x22 + result, args.len() as u8]);
                short(out, metadata.id)
            }
        }
        Expr::Array(_) => {
            return Err(XlsError::Unsupported(
                "array constants not yet supported in BIFF8 formula output".into(),
            ));
        }
    }
    check_budget(out, budget)
}
fn check_budget(out: &[u8], budget: usize) -> Result<(), XlsError> {
    if out.len() > budget {
        return Err(XlsError::ResourceLimit(format!(
            "formula token bytes exceeds export limit {budget}"
        )));
    }
    Ok(())
}
fn child(
    w: &Workbook,
    s: usize,
    e: &Expr,
    position: (BinaryOp, bool, u8),
    budget: usize,
    addins: &addins::AddInPlan,
    out: &mut Vec<u8>,
) -> Result<(), XlsError> {
    let (parent, right, class) = position;
    emit(w, s, e, class, budget, addins, out)?;
    if needs_parentheses(e, GroupingContext::BinaryChild { parent, right }) {
        out.push(0x15)
    }
    check_budget(out, budget)
}
fn column(col: u16, row_abs: bool, col_abs: bool) -> u16 {
    col | if row_abs { 0 } else { 0x8000 } | if col_abs { 0 } else { 0x4000 }
}
fn sheet(w: &Workbook, name: &str) -> Result<usize, XlsError> {
    w.try_sheet_index_by_name(name)
        .map_err(|error| fail(&error.to_string()))?
        .ok_or_else(|| fail(&format!("unknown formula worksheet {name}")))
}
#[path = "functions.rs"]
mod functions;
mod reference_plan;

#[cfg(test)]
mod identifier_tests {
    use super::*;

    fn tokens(w: &Workbook, sheet: usize, formula: &str) -> Result<Vec<u8>, XlsError> {
        compile(
            w,
            sheet,
            &poirot_formula::parse_formula(formula).unwrap(),
            0x20,
            1024,
        )
    }

    #[test]
    fn explicit_implicit_intersection_is_not_encoded_as_an_arithmetic_token() {
        let mut w = Workbook::new();
        w.add_sheet("Sheet1").unwrap();
        for formula in ["@42", "@A1:A3", "SUM(@A1:A3)"] {
            assert!(
                matches!(tokens(&w, 0, formula), Err(XlsError::Unsupported(message)) if message.contains("explicit @")),
                "{formula}"
            );
        }
    }

    #[test]
    fn compiler_resolves_sheet_aliases_and_scoped_name_identity() {
        let mut w = Workbook::new();
        w.add_sheet("Ä Data").unwrap();
        w.add_sheet("Other").unwrap();
        w.define_name("ÉRate", "1", None).unwrap();
        w.define_name("érate", "2", Some(0)).unwrap();
        assert_eq!(
            tokens(&w, 1, "'ä dATA'!B1").unwrap(),
            tokens(&w, 1, "'Ä Data'!B1").unwrap()
        );
        // BIFF NAME tokens carry the resolved vector index, local before global.
        assert_eq!(tokens(&w, 0, "ÉRATE").unwrap(), vec![0x43, 2, 0, 0, 0]);
        assert_eq!(tokens(&w, 1, "érate").unwrap(), vec![0x43, 1, 0, 0, 0]);
    }

    #[test]
    fn compiler_reports_unresolved_sheet_and_local_name_mapping() {
        let mut w = Workbook::new();
        w.import_sheet("Σ").unwrap();
        w.named_ranges.push(poirot_model::NamedRange {
            name: "Σ".into(),
            formula: "1".into(),
            sheet_scope: Some(0),
        });
        w.named_ranges.push(poirot_model::NamedRange {
            name: "σ".into(),
            formula: "2".into(),
            sheet_scope: None,
        });
        for formula in ["'σ'!B1", "σ"] {
            assert!(
                tokens(&w, 0, formula)
                    .unwrap_err()
                    .to_string()
                    .contains("unsupported identifier comparison")
            );
        }
        assert_eq!(w.named_ranges[0].name, "Σ");
    }
}

#[cfg(test)]
#[path = "rpn/reference_tests.rs"]
mod reference_tests;
