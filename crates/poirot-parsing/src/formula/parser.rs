//! Stateful Pratt grammar and authenticated invocation collection.
use super::references::{array_constant, cell_reference, partial_reference, range_pair};
use super::tokens::{identifier, number_token, quoted_token, sheet_span_prefix, whitespace};
use crate::formula::{
    BinaryOp, Expr, FormulaParsePolicy, FormulaSpan, FunctionInvocation, ParseError, ParsedFormula,
    SheetSpanReference, UnaryOp,
};
use nom::{Parser as NomParser, character::complete::char as token_char};

/// Parse using explicit tightening limits, retaining reusable source diagnostics.
pub fn parse_formula_with_policy(
    input: &str,
    policy: FormulaParsePolicy,
) -> Result<ParsedFormula, ParseError> {
    let defaults = FormulaParsePolicy::default();
    if policy.max_expressions == 0
        || policy.max_depth == 0
        || policy.max_rows == 0
        || policy.max_columns == 0
        || policy.max_expressions > defaults.max_expressions
        || policy.max_depth > defaults.max_depth
        || policy.max_rows > defaults.max_rows
        || policy.max_columns > defaults.max_columns
    {
        return Err(ParseError {
            position: 0,
            message: "parse policy must tighten canonical bounds".into(),
        });
    }
    let mut parser = Parser {
        input,
        pos: 0,
        depth: 0,
        expressions: 0,
        policy,
        spans: Vec::new(),
        function_invocations: Vec::new(),
    };
    parser.space();
    parser.take('=');
    let result = parser.expression(0, true)?;
    parser.space();
    if parser.pos != input.len() {
        return parser.fail("unexpected trailing input");
    }
    if policy.max_rows != defaults.max_rows || policy.max_columns != defaults.max_columns {
        let mut pending = vec![&result];
        while let Some(expr) = pending.pop() {
            let exceeds = match expr {
                Expr::Cell(c) => c.row >= policy.max_rows || c.column >= policy.max_columns,
                Expr::SheetSpan(span) => std::iter::once(&span.start)
                    .chain(span.end.iter())
                    .any(|c| c.row >= policy.max_rows || c.column >= policy.max_columns),
                Expr::Column { column, .. } => *column >= policy.max_columns,
                Expr::Row { row, .. } => *row >= policy.max_rows,
                Expr::Unary { operand, .. } => {
                    pending.push(operand);
                    false
                }
                Expr::Binary { left, right, .. } => {
                    pending.extend([left.as_ref(), right.as_ref()]);
                    false
                }
                Expr::Function { args, .. } => {
                    pending.extend(args);
                    false
                }
                Expr::Array(rows) => {
                    pending.extend(rows.iter().flatten());
                    false
                }
                _ => false,
            };
            if exceeds {
                return Err(ParseError {
                    position: 0,
                    message: "reference exceeds configured grid policy".into(),
                });
            }
        }
    }
    parser
        .function_invocations
        .sort_by_key(|call| call.name_span.start);
    Ok(ParsedFormula {
        source: input.into(),
        expression: result,
        spans: parser.spans,
        function_invocations: parser.function_invocations,
        policy,
    })
}
/// Parse an expression, optionally beginning with `=`. Positions are UTF-8 byte offsets.
pub fn parse_formula(input: &str) -> Result<Expr, ParseError> {
    parse_formula_with_policy(input, FormulaParsePolicy::default())
        .map(ParsedFormula::into_expression)
}
/// Stateful syntax cursor carrying parser limits, source spans, and nesting counters.
struct Parser<'a> {
    input: &'a str,
    pos: usize,
    depth: usize,
    expressions: usize,
    policy: FormulaParsePolicy,
    spans: Vec<FormulaSpan>,
    function_invocations: Vec<FunctionInvocation>,
}
impl Parser<'_> {
    fn fail<T>(&self, message: &str) -> Result<T, ParseError> {
        Err(ParseError {
            position: self.pos,
            message: message.into(),
        })
    }
    fn peek(&self) -> Option<char> {
        self.input.get(self.pos..)?.chars().next()
    }
    fn tail(&self) -> Result<&str, ParseError> {
        self.input.get(self.pos..).ok_or_else(|| ParseError {
            position: self.pos,
            message: "invalid UTF-8 parser cursor".into(),
        })
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn take(&mut self, c: char) -> bool {
        let Some(input) = self.input.get(self.pos..) else {
            return false;
        };
        match token_char::<_, nom::error::Error<_>>(c).parse(input) {
            Ok((rest, _)) => {
                self.pos = self.input.len() - rest.len();
                true
            }
            Err(_) => false,
        }
    }
    fn space(&mut self) -> bool {
        let start = self.pos;
        let Some(input) = self.input.get(self.pos..) else {
            return false;
        };
        let Ok((rest, _)) = whitespace(input) else {
            return false;
        };
        self.pos = self.input.len() - rest.len();
        self.pos != start
    }
    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.space();
        if self.take(c) {
            Ok(())
        } else {
            self.fail(&format!("expected '{c}'"))
        }
    }
    fn admit_expression(&mut self) -> Result<(), ParseError> {
        // This is a resource bound, independent of Excel's language restrictions.
        if self.expressions >= self.policy.max_expressions {
            return self.fail("expression complexity limit exceeded");
        }
        self.expressions += 1;
        Ok(())
    }
    fn expression(&mut self, min: u8, union: bool) -> Result<Expr, ParseError> {
        self.expression_context(min, union, false)
    }
    fn expression_context(
        &mut self,
        min: u8,
        union: bool,
        range_rhs: bool,
    ) -> Result<Expr, ParseError> {
        self.admit_expression()?;
        if self.depth >= self.policy.max_depth {
            return self.fail("expression nesting limit exceeded");
        }
        let start = self.pos + self.tail()?.len() - self.tail()?.trim_start().len();
        self.depth += 1;
        let invocation_checkpoint = self.function_invocations.len();
        let result = self.expression_inner(min, union, range_rhs);
        self.depth -= 1;
        if result.is_ok() {
            self.spans.push(FormulaSpan {
                start,
                end: self.pos,
            });
        } else {
            self.function_invocations.truncate(invocation_checkpoint);
        }
        result
    }
    fn expression_inner(
        &mut self,
        min: u8,
        union: bool,
        range_rhs: bool,
    ) -> Result<Expr, ParseError> {
        self.space();
        let mut left = if self.take('+') {
            Expr::Unary {
                op: UnaryOp::Plus,
                operand: Box::new(self.expression(8, union)?),
            }
        } else if self.take('-') {
            Expr::Unary {
                op: UnaryOp::Minus,
                operand: Box::new(self.expression(8, union)?),
            }
        } else if self.take('@') {
            Expr::Unary {
                op: UnaryOp::ImplicitIntersection,
                // Include range operands, but leave arithmetic to the outer expression.
                operand: Box::new(self.expression(8, union)?),
            }
        } else {
            self.atom(range_rhs)?
        };
        loop {
            let operator_start = self.pos;
            let whitespace = self.space();
            if self.peek() == Some('%') && min <= 8 {
                self.bump();
                left = Expr::Unary {
                    op: UnaryOp::Percent,
                    operand: Box::new(left),
                };
                continue;
            }
            let tail = self.tail()?;
            let operator = [
                ("<=", BinaryOp::LessEqual, BinaryOp::LessEqual.precedence()),
                (
                    ">=",
                    BinaryOp::GreaterEqual,
                    BinaryOp::GreaterEqual.precedence(),
                ),
                ("<>", BinaryOp::NotEqual, BinaryOp::NotEqual.precedence()),
                ("=", BinaryOp::Equal, BinaryOp::Equal.precedence()),
                ("<", BinaryOp::Less, BinaryOp::Less.precedence()),
                (">", BinaryOp::Greater, BinaryOp::Greater.precedence()),
                ("&", BinaryOp::Concat, BinaryOp::Concat.precedence()),
                ("+", BinaryOp::Add, BinaryOp::Add.precedence()),
                ("-", BinaryOp::Subtract, BinaryOp::Subtract.precedence()),
                ("*", BinaryOp::Multiply, BinaryOp::Multiply.precedence()),
                ("/", BinaryOp::Divide, BinaryOp::Divide.precedence()),
                ("^", BinaryOp::Power, BinaryOp::Power.precedence()),
                (":", BinaryOp::Range, BinaryOp::Range.precedence()),
                (",", BinaryOp::Union, BinaryOp::Union.precedence()),
            ]
            .into_iter()
            .find(|(text, op, _)| tail.starts_with(text) && (*op != BinaryOp::Union || union));
            let (op, precedence, length) = if let Some((text, op, precedence)) = operator {
                (op, precedence, text.len())
            } else if whitespace
                && self.peek().is_some_and(|c| {
                    c.is_alphabetic() || matches!(c, '(' | '$' | '\'' | '_' | '\\')
                })
            {
                (
                    BinaryOp::Intersection,
                    BinaryOp::Intersection.precedence(),
                    0,
                )
            } else {
                self.pos = operator_start;
                break;
            };
            if precedence < min {
                self.pos = operator_start;
                break;
            }
            self.pos += length;
            let mut right =
                self.expression_context(precedence + 1, union, op == BinaryOp::Range)?;
            if op == BinaryOp::Range {
                (left, right) = range_pair(left, right).map_err(|message| ParseError {
                    position: self.pos,
                    message,
                })?;
            }
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    fn quoted(&mut self, quote: char) -> Result<String, ParseError> {
        self.space();
        match quoted_token(self.tail()?, quote) {
            Ok((rest, value)) => {
                self.pos = self.input.len() - rest.len();
                Ok(value)
            }
            Err(_) => {
                self.pos = self.input.len();
                self.fail("unterminated quoted text")
            }
        }
    }
    fn atom(&mut self, range_rhs: bool) -> Result<Expr, ParseError> {
        self.space();
        // A direct colon RHS starts with a cell in A1:B2:Sheet!C3.
        // Bare B2:Sheet!C3 remains a 3D atom; quoting or parentheses also
        // retain that interpretation. Decide before consuming any prefix.
        if let Ok((rest, (first_sheet, last_sheet))) = sheet_span_prefix(self.tail()?)
            && (!range_rhs
                || self.peek() == Some('\'')
                || !matches!(cell_reference(&first_sheet, None), Ok(Some(_))))
        {
            self.pos = self.input.len() - rest.len();
            if [&first_sheet, &last_sheet]
                .iter()
                .any(|name| name.contains(['[', ']']))
            {
                return self.fail("external workbook sheet spans are unsupported");
            }
            let start = self.span_endpoint()?;
            let end = if self.take(':') {
                Some(self.span_endpoint()?)
            } else {
                None
            };
            let (start, end) =
                super::references::sheet_span_bounds(start, end).map_err(|message| ParseError {
                    position: self.pos,
                    message,
                })?;
            return Ok(Expr::SheetSpan(SheetSpanReference {
                first_sheet,
                last_sheet,
                start,
                end,
            }));
        }
        if self.peek() == Some('"') {
            return self.quoted('"').map(Expr::String);
        }
        if self.take('(') {
            let expression = self.expression(0, true)?;
            self.expect(')')?;
            return Ok(expression);
        }
        if self.take('{') {
            return self.array();
        }
        if self.take('#') {
            let start = self.pos - 1;
            for error in [
                "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#NULL!", "#N/A", "#SPILL!",
                "#CALC!",
            ] {
                if self
                    .input
                    .get(start..)
                    .ok_or_else(|| ParseError {
                        position: start,
                        message: "invalid error literal cursor".into(),
                    })?
                    .to_ascii_uppercase()
                    .starts_with(error)
                {
                    self.pos = start + error.len();
                    return Ok(Expr::Error(error.into()));
                }
            }
            return self.fail("unknown error literal");
        }
        if self.peek() == Some('\'') {
            let sheet = self.quoted('\'')?;
            self.expect('!')?;
            return self.reference(Some(sheet));
        }
        let start = self.pos;
        if self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            let (rest, number) = number_token(self.input.get(start..).ok_or_else(|| {
                ParseError {
                    position: start,
                    message: "invalid UTF-8 parser cursor".into(),
                }
            })?)
            .map_err(|error| {
                let offset = match error {
                    nom::Err::Error(e) | nom::Err::Failure(e) => self.input.len() - e.input.len(),
                    nom::Err::Incomplete(_) => start,
                };
                ParseError {
                    position: offset,
                    message: "invalid number".into(),
                }
            })?;
            self.pos = self.input.len() - rest.len();
            if !number.is_finite() {
                return self.fail("number is not finite");
            }
            return Ok(Expr::Number(number));
        }
        self.reference(None)
    }
    fn reference(&mut self, sheet: Option<String>) -> Result<Expr, ParseError> {
        let start = self.pos;
        let (rest, text) = identifier(self.input.get(start..).ok_or_else(|| ParseError {
            position: start,
            message: "invalid UTF-8 parser cursor".into(),
        })?)
        .map_err(|_| ParseError {
            position: start,
            message: "expected expression".into(),
        })?;
        let text = text.to_owned();
        self.pos = self.input.len() - rest.len();
        if self.take('!') {
            if sheet.is_some() {
                return self.fail("repeated sheet qualifier");
            }
            return self.reference(Some(text));
        }
        let saved = self.pos;
        self.space();
        // POI FormulaParser::isValidCellReference gives in-grid cells priority
        // over unknown function spellings. Only registered built-ins (LOG10)
        // use '(' lookahead to override that priority. Retain whitespace for
        // the outer intersection grammar, e.g. A1 (A2).
        if self.peek() == Some('(')
            && let Ok(Some(cell)) = cell_reference(&text, sheet.clone())
            && crate::formula::reference_classes::return_class_insensitive(&text).is_none()
        {
            self.pos = saved;
            return Ok(Expr::Cell(cell));
        }
        if self.take('(') {
            if sheet.is_some() || text.contains('$') {
                return self.fail("invalid function name");
            }
            let mut args = Vec::new();
            self.space();
            if !self.take(')') {
                loop {
                    self.space();
                    if matches!(self.peek(), Some(',') | Some(')')) {
                        self.admit_expression()?;
                        args.push(Expr::Missing);
                    } else {
                        args.push(self.expression(1, false)?);
                    }
                    self.space();
                    if self.take(')') {
                        break;
                    }
                    self.expect(',')?;
                }
            }
            // No successful function branch rolls back its position. Failed outer
            // expressions discard these entries at their metadata checkpoint.
            self.function_invocations.push(FunctionInvocation {
                name_span: FormulaSpan { start, end: saved },
                expression_span: FormulaSpan {
                    start,
                    end: self.pos,
                },
                argument_count: args.len(),
            });
            return Ok(Expr::Function {
                // ATP lookup strips only the exact lowercase `_xlfn.` prefix.
                // Keep that source spelling available to the namespace adapter.
                name: if text
                    .get(..6)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("_xlfn."))
                {
                    text
                } else {
                    text.to_uppercase()
                }
                .into(),
                args,
            });
        }
        self.pos = saved;
        if let Some(cell) = cell_reference(&text, sheet.clone()).map_err(|mut error| {
            error.position = start;
            error
        })? {
            return Ok(Expr::Cell(cell));
        }
        if text.contains('$') {
            return partial_reference(&text, sheet).ok_or(ParseError {
                position: start,
                message: "invalid absolute reference".into(),
            });
        }
        if let Some(sheet) = sheet {
            // A bare column-shaped identifier can also be a defined name.
            // Only range grammar turns it into a whole-column endpoint.
            if text.bytes().all(|c| c.is_ascii_digit()) {
                return partial_reference(&text, Some(sheet)).ok_or(ParseError {
                    position: start,
                    message: "invalid sheet reference".into(),
                });
            }
            return Ok(Expr::QualifiedName { sheet, name: text });
        }
        match text.to_ascii_uppercase().as_str() {
            "TRUE" => Ok(Expr::Bool(true)),
            "FALSE" => Ok(Expr::Bool(false)),
            _ => Ok(Expr::Name(text)),
        }
    }
    fn span_endpoint(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        let (rest, text) = identifier(self.input.get(start..).ok_or_else(|| ParseError {
            position: start,
            message: "invalid UTF-8 parser cursor".into(),
        })?)
        .map_err(|_| ParseError {
            position: start,
            message: "expected bounded sheet-span cell".into(),
        })?;
        self.pos = self.input.len() - rest.len();
        if let Some(cell) = cell_reference(text, None)? {
            return Ok(Expr::Cell(cell));
        }
        partial_reference(text, None).ok_or(ParseError {
            position: start,
            message: "sheet spans require bounded cell references".into(),
        })
    }
    fn array(&mut self) -> Result<Expr, ParseError> {
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row = Vec::new();
        loop {
            let value = self.expression(1, false)?;
            if !array_constant(&value) {
                return self.fail("array elements must be constants");
            }
            row.push(value);
            self.space();
            if self.take(',') {
                continue;
            }
            // SAFETY: The short-circuit condition checks rows is nonempty before reading its first row.
            if !rows.is_empty() && rows[0].len() != row.len() {
                return self.fail("array rows have different lengths");
            }
            rows.push(row);
            row = Vec::new();
            if self.take('}') {
                break;
            }
            self.expect(';')?;
        }
        Ok(Expr::Array(rows))
    }
}

#[cfg(test)]
#[path = "function_invocation_tests.rs"]
mod function_invocation_tests;
#[cfg(test)]
#[path = "missing_argument_policy_tests.rs"]
mod missing_argument_policy_tests;
