//! Pure canonical syntax/grouping rules shared with external encoders.
use super::{BinaryOp, Expr, ParseError, ParseErrorKind, UnaryOp, parse_formula};
impl BinaryOp {
    /// Return operator binding strength for parsing-compatible printing.
    pub fn precedence(self) -> u8 {
        match self {
            Self::Union => 0,
            Self::Intersection => 1,
            Self::Equal
            | Self::NotEqual
            | Self::Less
            | Self::LessEqual
            | Self::Greater
            | Self::GreaterEqual => 2,
            Self::Concat => 3,
            Self::Add | Self::Subtract => 4,
            Self::Multiply | Self::Divide => 5,
            Self::Power => 6,
            Self::Range => 9,
        }
    }
    fn symbol(self) -> &'static str {
        match self {
            Self::Union => ",",
            Self::Intersection => " ",
            Self::Range => ":",
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Power => "^",
            Self::Concat => "&",
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }
}
/// Position of an expression when deciding whether canonical printing needs parentheses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingContext {
    /// Child expression being printed beneath a binary operator.
    BinaryChild {
        /// Parent operator whose precedence controls grouping.
        parent: BinaryOp,
        /// Whether this is the right child, where associativity can require parentheses.
        right: bool,
    },
    /// Expression consumed by a prefix or postfix operator.
    UnaryOperand(UnaryOp),
    /// Argument position where reference-union commas need grouping.
    FunctionArgument,
}
/// Preserve semantic tree structure under the current left-associative grammar.
pub fn needs_parentheses(expr: &Expr, context: GroupingContext) -> bool {
    match context {
        GroupingContext::BinaryChild { parent, right } => match expr {
            Expr::Binary { op, .. } => {
                op.precedence() < parent.precedence()
                    || (right && op.precedence() == parent.precedence())
            }
            Expr::Unary { op, .. } => {
                (if *op == UnaryOp::SpillRange { 10 } else { 8 }) < parent.precedence()
            }
            _ => false,
        },
        GroupingContext::FunctionArgument => matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::Union,
                ..
            }
        ),
        GroupingContext::UnaryOperand(unary) => {
            let strength = if unary == UnaryOp::SpillRange { 10 } else { 8 };
            matches!(expr, Expr::Binary {op,..} if op.precedence()<strength)
                || (matches!(unary, UnaryOp::Percent | UnaryOp::SpillRange)
                    && matches!(
                        expr,
                        Expr::Unary {
                            op: UnaryOp::Plus | UnaryOp::Minus | UnaryOp::ImplicitIntersection,
                            ..
                        }
                    ))
        }
    }
}
/// Fallible printer for semantic ASTs representable by the current canonical grammar.
pub struct FormulaPrinter;
impl FormulaPrinter {
    /// Render syntax as a formula while preserving operator grouping.
    pub fn print(expression: &Expr) -> Result<String, ParseError> {
        let mut output = String::new();
        write_expr(expression, &mut output, 0, &mut 0)?;
        if parse_formula(&output)? != *expression {
            return Err(ParseError {
                kind: super::ParseErrorKind::Syntax,
                position: 0,
                message: "AST cannot be preserved by canonical syntax".into(),
            });
        }
        Ok(output)
    }
}
fn failure(message: &str) -> ParseError {
    ParseError {
        kind: super::ParseErrorKind::Syntax,
        position: 0,
        message: message.into(),
    }
}
fn grouped(
    expr: &Expr,
    context: GroupingContext,
    out: &mut String,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), ParseError> {
    let parentheses = needs_parentheses(expr, context);
    if parentheses {
        out.push('(');
    }
    write_expr(expr, out, depth, nodes)?;
    if parentheses {
        out.push(')');
    }
    Ok(())
}
fn qualifier(sheet: &Option<String>, out: &mut String) {
    if let Some(sheet) = sheet {
        out.push('\'');
        out.push_str(&sheet.replace('\'', "''"));
        out.push_str("'!");
    }
}
fn column_name(column: u32) -> Result<String, ParseError> {
    if column >= 16384 {
        return Err(failure("column exceeds canonical grid"));
    }
    let mut n = column + 1;
    let mut name = Vec::new();
    while n > 0 {
        n -= 1;
        name.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    Ok(name.into_iter().rev().collect())
}
fn write_expr(
    expr: &Expr,
    out: &mut String,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), ParseError> {
    *nodes += 1;
    if depth >= 128 || *nodes > 1024 {
        return Err(ParseError {
            kind: ParseErrorKind::ResourceLimit,
            position: 0,
            message: "printer complexity limit exceeded".into(),
        });
    }
    match expr {
        Expr::Number(n) => {
            if !n.is_finite() {
                return Err(failure("nonfinite formula number"));
            }
            out.push_str(&n.to_string());
        }
        Expr::String(s) => {
            out.push('"');
            out.push_str(&s.replace('"', "\"\""));
            out.push('"');
        }
        Expr::Bool(b) => out.push_str(if *b { "TRUE" } else { "FALSE" }),
        Expr::Error(e) | Expr::Name(e) => out.push_str(e),
        Expr::QualifiedName { sheet, name } => {
            qualifier(&Some(sheet.clone()), out);
            out.push_str(name);
        }
        Expr::Missing => {}
        Expr::SheetSpan(span) => {
            if span.first_sheet.is_empty()
                || span.last_sheet.is_empty()
                || span.first_sheet.contains(':')
                || span.last_sheet.contains(':')
                || span.start.sheet.is_some()
                || span.end.as_ref().is_some_and(|c| c.sheet.is_some())
            {
                return Err(failure("invalid bounded sheet-span reference"));
            }
            out.push('\'');
            out.push_str(&span.first_sheet.replace('\'', "''"));
            out.push(':');
            out.push_str(&span.last_sheet.replace('\'', "''"));
            out.push_str("'!");
            write_expr(&Expr::Cell(span.start.clone()), out, depth + 1, nodes)?;
            if let Some(end) = &span.end {
                out.push(':');
                write_expr(&Expr::Cell(end.clone()), out, depth + 1, nodes)?;
            }
        }
        Expr::Cell(c) => {
            qualifier(&c.sheet, out);
            if c.column_absolute {
                out.push('$');
            }
            out.push_str(&column_name(c.column)?);
            if c.row_absolute {
                out.push('$');
            }
            out.push_str(
                &(c.row
                    .checked_add(1)
                    .ok_or_else(|| failure("row overflow"))?)
                .to_string(),
            );
        }
        Expr::Column {
            column,
            absolute,
            sheet,
        } => {
            qualifier(sheet, out);
            if *absolute {
                out.push('$');
            }
            out.push_str(&column_name(*column)?);
        }
        Expr::Row {
            row,
            absolute,
            sheet,
        } => {
            qualifier(sheet, out);
            if *absolute {
                out.push('$');
            }
            out.push_str(&(row.checked_add(1).ok_or_else(|| failure("row overflow"))?).to_string());
        }
        Expr::Unary { op, operand } => {
            if !matches!(op, UnaryOp::Percent | UnaryOp::SpillRange) {
                out.push(match op {
                    UnaryOp::Plus => '+',
                    UnaryOp::Minus => '-',
                    UnaryOp::ImplicitIntersection => '@',
                    // SAFETY: The enclosing condition excludes both postfix operators.
                    UnaryOp::Percent | UnaryOp::SpillRange => unreachable!(),
                });
            }
            grouped(
                operand,
                GroupingContext::UnaryOperand(*op),
                out,
                depth + 1,
                nodes,
            )?;
            match op {
                UnaryOp::Percent => out.push('%'),
                UnaryOp::SpillRange => out.push('#'),
                _ => {}
            }
        }
        Expr::Binary { op, left, right } => {
            grouped(
                left,
                GroupingContext::BinaryChild {
                    parent: *op,
                    right: false,
                },
                out,
                depth + 1,
                nodes,
            )?;
            out.push_str(op.symbol());
            grouped(
                right,
                GroupingContext::BinaryChild {
                    parent: *op,
                    right: true,
                },
                out,
                depth + 1,
                nodes,
            )?;
        }
        Expr::Function { name, args } => {
            out.push_str(name);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                grouped(
                    arg,
                    GroupingContext::FunctionArgument,
                    out,
                    depth + 1,
                    nodes,
                )?;
            }
            out.push(')');
        }
        Expr::Array(rows) => {
            out.push('{');
            for (r, row) in rows.iter().enumerate() {
                if r > 0 {
                    out.push(';');
                }
                for (c, value) in row.iter().enumerate() {
                    if c > 0 {
                        out.push(',');
                    }
                    write_expr(value, out, depth + 1, nodes)?;
                }
            }
            out.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{FormulaParsePolicy, parse_formula_with_policy};
    #[test]
    fn canonical_printer_preserves_semantic_grouping_and_current_precedence() {
        for text in [
            "(1+2)*3",
            "1-(2-3)",
            "-(2^2)",
            "2^(3^2)",
            "(2^3)^2",
            "(-1)%",
            "-1%",
            "2^200%",
            "SUM((A1,B1),C1)",
            "IF(TRUE,(A1,A2),3)",
            "A1:A3 A2:A4",
            "SUM(A1:A3 A2:A4,1)",
            "F(,1,)",
            "'Other 😀 Sheet'!$A$1:$B2",
            "{1,\"héllo\";TRUE,-2}",
            "A:A",
            "$1:$2",
        ] {
            let parsed = parse_formula(text).unwrap();
            let printed = FormulaPrinter::print(&parsed).unwrap();
            assert_eq!(
                parse_formula(&printed).unwrap(),
                parsed,
                "source={text}, canonical={printed}"
            );
        }
        assert_eq!(
            FormulaPrinter::print(&parse_formula("SUM((A1,B1),C1)").unwrap()).unwrap(),
            "SUM((A1,B1),C1)"
        );
        // Range endpoints currently exclude unary expressions; no grammar expansion.
        for source in ["(-A1):B1", "(A1%):B1"] {
            assert!(parse_formula(source).is_err());
        }
        let unary = Expr::Unary {
            op: UnaryOp::Minus,
            operand: Box::new(Expr::Number(1.)),
        };
        assert!(needs_parentheses(
            &unary,
            GroupingContext::BinaryChild {
                parent: BinaryOp::Range,
                right: false
            }
        ));
        assert!(FormulaPrinter::print(&Expr::Number(f64::INFINITY)).is_err());
        assert!(FormulaPrinter::print(&Expr::Name("TRUE".into())).is_err());
    }
    #[test]
    fn parsed_plans_retain_original_source_and_utf8_expression_ranges() {
        let source = String::from(" = SUM(\"😀\",1+2) ");
        let parsed = parse_formula_with_policy(&source, FormulaParsePolicy::default()).unwrap();
        drop(source);
        assert_eq!(parsed.source(), " = SUM(\"😀\",1+2) ");
        assert_eq!(
            parsed.expression(),
            &parse_formula(parsed.source()).unwrap()
        );
        assert_eq!(parsed.clone(), parsed);
        assert!(parsed.expression_spans().len() >= 4);
        for span in parsed.expression_spans() {
            assert!(span.start <= span.end);
            assert!(parsed.source().get(span.start..span.end).is_some());
        }
        assert!(
            parsed
                .expression_spans()
                .iter()
                .any(|span| &parsed.source()[span.start..span.end] == "\"😀\"")
        );
        let printed = FormulaPrinter::print(parsed.expression()).unwrap();
        assert_eq!(parse_formula(&printed).unwrap(), *parsed.expression());
    }
    #[test]
    fn explicit_parse_policy_tightens_resources_and_grid_without_changing_default_errors() {
        for (source, position) in [("\"😀\"+)", 7), ("1+)", 2), ("=XFE1", 1)] {
            assert_eq!(parse_formula(source).unwrap_err().position, position);
            assert_eq!(
                parse_formula_with_policy(source, FormulaParsePolicy::default())
                    .unwrap_err()
                    .position,
                position
            );
        }
        let tiny = FormulaParsePolicy {
            max_expressions: 2,
            ..FormulaParsePolicy::default()
        };
        assert!(
            parse_formula_with_policy("1+2+3", tiny)
                .unwrap_err()
                .message
                .contains("complexity")
        );
        let tiny = FormulaParsePolicy {
            max_depth: 2,
            ..FormulaParsePolicy::default()
        };
        assert!(
            parse_formula_with_policy("((1))", tiny)
                .unwrap_err()
                .message
                .contains("nesting")
        );
        let xls = FormulaParsePolicy {
            max_rows: 65536,
            max_columns: 256,
            ..FormulaParsePolicy::default()
        };
        for source in ["A65537", "IW1", "$IW:$IW", "65537:65538"] {
            assert!(parse_formula(source).is_ok());
            assert!(
                parse_formula_with_policy(source, xls)
                    .unwrap_err()
                    .message
                    .contains("configured grid")
            );
        }
        assert!(parse_formula_with_policy("IV65536", xls).is_ok());
        for policy in [
            FormulaParsePolicy {
                max_depth: 129,
                ..xls
            },
            FormulaParsePolicy { max_rows: 0, ..xls },
        ] {
            assert!(parse_formula_with_policy("1", policy).is_err());
        }
    }
}
