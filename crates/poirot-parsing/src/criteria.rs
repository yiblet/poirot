//! Criteria are a comparison prefix followed by an operand, not formula text.
use nom::{Parser, branch::alt, bytes::complete::tag};

/// Comparison prefix parsed from a criteria operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// No explicit comparison prefix.
    None,
    /// Equality comparison.
    Eq,
    /// Inequality comparison.
    Ne,
    /// Strict less-than comparison.
    Lt,
    /// Less-than-or-equal comparison.
    Le,
    /// Strict greater-than comparison.
    Gt,
    /// Greater-than-or-equal comparison.
    Ge,
}
/// Split a criteria comparison prefix from its operand text.
pub fn parse_comparison(s: &str) -> (Comparison, &str) {
    let result: nom::IResult<&str, &str> = alt((
        tag("<>"),
        tag("<="),
        tag(">="),
        tag("="),
        tag("<"),
        tag(">"),
    ))
    .parse(s);
    match result {
        Ok((rest, op)) => (
            match op {
                "<>" => Comparison::Ne,
                "<=" => Comparison::Le,
                ">=" => Comparison::Ge,
                "=" => Comparison::Eq,
                "<" => Comparison::Lt,
                _ => Comparison::Gt,
            },
            rest,
        ),
        Err(_) => (Comparison::None, s),
    }
}

/// Borrowed criteria syntax; the consumer selects operand interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CriterionText<'a> {
    /// Explicit comparison prefix, or the implicit equality form.
    pub comparison: Comparison,
    /// Operand text, retaining its exact spelling.
    pub operand: &'a str,
}
/// Split a complete criterion's prefix, leaving operand grammar to the consumer.
pub fn parse_criterion(input: &str) -> CriterionText<'_> {
    let (comparison, operand) = parse_comparison(input);
    CriterionText {
        comparison,
        operand,
    }
}

/// Classic spreadsheet error spellings recognized in criteria operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorLiteral {
    Null,
    DivZero,
    Value,
    Ref,
    Name,
    Num,
    Na,
}
/// Borrowed syntax value; candidate matching and argument conversion are separate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operand<'a> {
    Number(f64),
    Boolean(bool),
    Error(ErrorLiteral),
    Text(&'a str),
}
impl<'a> CriterionText<'a> {
    /// Interpret the operand with the classic criteria grammar. Wildcard text
    /// stays borrowed and unevaluated; it is never parsed as a formula.
    pub fn classic_operand(self) -> Operand<'a> {
        let s = self.operand;
        if s.eq_ignore_ascii_case("true") {
            return Operand::Boolean(true);
        }
        if s.eq_ignore_ascii_case("false") {
            return Operand::Boolean(false);
        }
        if let Some(n) = crate::number::parse_criteria_number(s) {
            return Operand::Number(n);
        }
        let error = match s {
            "#NULL!" => ErrorLiteral::Null,
            "#DIV/0!" => ErrorLiteral::DivZero,
            "#VALUE!" => ErrorLiteral::Value,
            "#REF!" => ErrorLiteral::Ref,
            "#NAME?" => ErrorLiteral::Name,
            "#NUM!" => ErrorLiteral::Num,
            "#N/A" => ErrorLiteral::Na,
            _ => return Operand::Text(s),
        };
        Operand::Error(error)
    }
}
