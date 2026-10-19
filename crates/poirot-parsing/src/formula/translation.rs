//! Bounded copy translations derived from POI FormulaShifter row/column copy.
//! Deleted-reference tokens and endpoint reordering are explicit unsupported cases.
use crate::formula::{Expr, FormulaParsePolicy, FormulaPrinter, ParseError, ParsedFormula};

/// Zero-based formula location; sheet qualification belongs to references in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormulaLocation {
    /// Zero-based source or destination row.
    pub row: u32,
    /// Zero-based source or destination column.
    pub column: u32,
}
/// How copy translation handles references outside the supported grid.
/// Currently such translations are rejected rather than rewritten as `#REF!` tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranslationBoundaryPolicy {
    /// Reject translations that cannot preserve supported reference semantics.
    #[default]
    RejectUnsupported,
}
/// Parser limits and out-of-grid handling for copying a formula between locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TranslationPolicy {
    /// Limits used to validate source and translated formulas.
    pub parse: FormulaParsePolicy,
    /// Behavior when a reference crosses a supported translation boundary.
    pub boundary: TranslationBoundaryPolicy,
}
/// A parsed formula with relative references shifted by a source-to-target displacement.
/// Absolute axes stay anchored. Planning produces replacement text without editing
/// a workbook or moving cells.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslationPlan {
    original: ParsedFormula,
    source: FormulaLocation,
    target: FormulaLocation,
    policy: TranslationPolicy,
    expression: Expr,
    replacement: String,
}
impl TranslationPlan {
    /// Parse and validate a formula copy before exposing its replacement text.
    pub fn new(
        text: &str,
        source: FormulaLocation,
        target: FormulaLocation,
        policy: TranslationPolicy,
    ) -> Result<Self, ParseError> {
        let original = crate::formula::parse_formula_with_policy(text, policy.parse)?;
        for location in [source, target] {
            if location.row >= policy.parse.max_rows || location.column >= policy.parse.max_columns
            {
                return Err(failure("formula location exceeds translation grid"));
            }
        }
        let mut expression = original.expression().clone();
        translate(
            &mut expression,
            i64::from(target.row) - i64::from(source.row),
            i64::from(target.column) - i64::from(source.column),
            policy,
        )?;
        let replacement = if expression == *original.expression() {
            text.to_owned()
        } else {
            FormulaPrinter::print(&expression)?
        };
        Ok(Self {
            original,
            source,
            target,
            policy,
            expression,
            replacement,
        })
    }
    /// Borrow the exact source checked by this translation plan.
    pub fn original(&self) -> &ParsedFormula {
        &self.original
    }
    /// Return the cell location the formula is copied from.
    pub fn source(&self) -> FormulaLocation {
        self.source
    }
    /// Return the destination used to translate relative coordinates.
    pub fn target(&self) -> FormulaLocation {
        self.target
    }
    /// Borrow the parser and boundary policy used for translation.
    pub fn policy(&self) -> TranslationPolicy {
        self.policy
    }
    /// Borrow the translated syntax.
    pub fn expression(&self) -> &Expr {
        &self.expression
    }
    /// Borrow the formula text to write at the destination.
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
    /// Whether translated source differs from the original.
    pub fn changed(&self) -> bool {
        self.expression != *self.original.expression()
    }
    /// Reject application if the formula source changed after planning.
    pub fn verify_source(&self, text: &str) -> Result<(), ParseError> {
        if text != self.original.source() {
            Err(failure("stale formula translation source"))
        } else {
            Ok(())
        }
    }
}
fn failure(message: &str) -> ParseError {
    ParseError {
        kind: super::ParseErrorKind::Syntax,
        position: 0,
        message: message.into(),
    }
}
fn axis(value: &mut u32, absolute: bool, delta: i64, bound: u32) -> Result<(), ParseError> {
    let translated = i64::from(*value) + if absolute { 0 } else { delta };
    if translated < 0 || translated >= i64::from(bound) {
        return Err(failure(
            "unsupported deleted reference: translation crosses grid boundary",
        ));
    }
    *value = translated as u32;
    Ok(())
}
fn translate(
    expr: &mut Expr,
    rows: i64,
    columns: i64,
    policy: TranslationPolicy,
) -> Result<(), ParseError> {
    match expr {
        Expr::SheetSpan(span) => {
            for c in std::iter::once(&mut span.start).chain(span.end.iter_mut()) {
                axis(&mut c.row, c.row_absolute, rows, policy.parse.max_rows)?;
                axis(
                    &mut c.column,
                    c.column_absolute,
                    columns,
                    policy.parse.max_columns,
                )?;
            }
        }
        Expr::QualifiedName { .. } => {}
        Expr::Cell(c) => {
            axis(&mut c.row, c.row_absolute, rows, policy.parse.max_rows)?;
            axis(
                &mut c.column,
                c.column_absolute,
                columns,
                policy.parse.max_columns,
            )?;
        }
        Expr::Row { row, absolute, .. } => axis(row, *absolute, rows, policy.parse.max_rows)?,
        Expr::Column {
            column, absolute, ..
        } => axis(column, *absolute, columns, policy.parse.max_columns)?,
        Expr::Unary { operand, .. } => translate(operand, rows, columns, policy)?,
        Expr::Binary { op, left, right } => {
            translate(left, rows, columns, policy)?;
            translate(right, rows, columns, policy)?;
            if *op == crate::formula::BinaryOp::Range {
                let inverted = match (left.as_ref(), right.as_ref()) {
                    (Expr::Cell(a), Expr::Cell(b)) => a.row > b.row || a.column > b.column,
                    (Expr::Row { row: a, .. }, Expr::Row { row: b, .. }) => a > b,
                    (Expr::Column { column: a, .. }, Expr::Column { column: b, .. }) => a > b,
                    _ => false,
                };
                if inverted {
                    return Err(failure("unsupported area endpoint reordering"));
                }
            }
        }
        Expr::Function { args, .. } => {
            for arg in args {
                translate(arg, rows, columns, policy)?;
            }
        }
        Expr::Array(_) => {} // Canonical grammar allows constant arrays only.
        _ => {}
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn plan(text: &str, row: u32, column: u32) -> Result<TranslationPlan, ParseError> {
        TranslationPlan::new(
            text,
            FormulaLocation { row: 0, column: 0 },
            FormulaLocation { row, column },
            TranslationPolicy::default(),
        )
    }
    #[test]
    fn mixed_anchors_and_ast_structure() {
        for (source, expected) in [
            ("A1+$B2+C$3+$D$4", "D3+$B4+F$3+$D$4"),
            (
                "SUM('O''Brien Data'!A1:B2,Name_1,LEN(\"A1\"))",
                "SUM('O''Brien Data'!D3:E4,Name_1,LEN(\"A1\"))",
            ),
            ("A1:B5 B2:C6,D1", "D3:E7 E4:F8,G3"),
            ("A:C+$1:3", "D:F+$1:5"),
        ] {
            let p = plan(source, 2, 3).unwrap();
            assert_eq!(
                p.expression(),
                &crate::formula::parse_formula(expected).unwrap()
            );
            assert_eq!(
                crate::formula::parse_formula(p.replacement()).unwrap(),
                *p.expression()
            );
            assert_eq!(p.original().source(), source);
        }
    }
    #[test]
    fn span_copy_preserves_namespace_and_moves_mixed_anchors() {
        let p = plan("SUM(First:Last!A1:$B$2)+Data!Local", 1, 1).unwrap();
        assert_eq!(
            p.expression(),
            &crate::formula::parse_formula("SUM(First:Last!B2:$B$2)+Data!Local").unwrap()
        );
    }
    #[test]
    fn bounded_failures_and_exact_identity() {
        assert!(plan("XFD1", 0, 1).is_err());
        assert!(plan("A1048576", 1, 0).is_err());
        assert!(plan("A1:$A$2", 0, 2).is_err());
        let p = plan(" $A$1 ", 2, 3).unwrap();
        assert_eq!(p.replacement(), " $A$1 ");
        assert!(p.verify_source("$A$1").is_err());
        assert!(p.verify_source(" $A$1 ").is_ok());
        assert!(
            TranslationPlan::new(
                "A1",
                FormulaLocation { row: 1, column: 1 },
                FormulaLocation { row: 0, column: 0 },
                TranslationPolicy::default()
            )
            .is_err()
        );
    }
}
