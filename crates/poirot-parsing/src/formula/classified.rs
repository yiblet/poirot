//! Allocation-free lexical classification for formula-template reuse.
//!
//! This is deliberately not a second expression parser. Use a successfully parsed
//! source as the cache exemplar, and require exact normalized equality before reuse.
//! Classification alone does not establish valid syntax, depth, arity, or policy.
//! Cache entries must retain the exemplar policy and validate rebound coordinates
//! against its configured grid bounds before a hit bypasses full parsing.
//! All literal bytes (including whitespace, function spelling and strings) survive.
//! Full-cell tokens are replaced by a Cell placeholder; sheet qualifiers remain literal.
//! Whole-axis pairs retain their exact source bytes and cannot be rebound.
//! Arrays and error literals currently request normal parser fallback. An iterator may yield tokens before reporting unsupported input;
//! consume to completion before publishing a hash or collected bindings.
use crate::formula::references::cell_coordinates;
use crate::formula::tokens::{identifier, identifier_character, number_token, whitespace};
use std::hash::{Hash, Hasher};

/// Coordinate representation shared with the full reference parser.
pub use crate::formula::references::CellCoordinates;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Classification needs the full parser, either for invalid input or syntax
/// outside this fast path. This is not itself a formula parsing diagnostic.
pub enum Unsupported {
    /// Syntax requires the full parser rather than token classification.
    Syntax,
    /// Reference syntax cannot be safely classified by this fast path.
    Reference,
    /// Range endpoints are not a supported bounded cell pair.
    Range,
    /// Sheet qualification requires the full parser.
    QualifiedReference,
    /// Array or error literal requires full syntax parsing.
    ArrayOrError,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Literal source bytes or a cell occurrence whose coordinates can be rebound
/// separately from the shared template.
pub enum TokenKind {
    /// Source bytes that remain unchanged during coordinate rebinding.
    Literal,
    /// Validated finite numeric literal; its source bytes remain significant for execution.
    Number,
    /// Validated escaped text literal; its source bytes remain significant for execution.
    Text,
    /// Complete whole-axis pair whose exact spelling is retained, never rebound.
    FixedReference,
    /// Cell coordinates that may vary between shared templates.
    Cell(CellCoordinates),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Borrowed source slice and its half-open UTF-8 byte span. `raw` preserves
/// original spelling even when `kind` exposes decoded cell coordinates.
pub struct Token<'a> {
    /// Exact source spelling, including whitespace and escaping.
    pub raw: &'a str,
    /// Inclusive byte offset in the classified source.
    pub start: usize,
    /// Exclusive byte offset in the classified source.
    pub end: usize,
    /// Whether this token contains rebindable cell coordinates.
    pub kind: TokenKind,
}
/// Borrowed tokens cover the complete source without gaps. Cell coordinates are
/// occurrence-ordered, zero-based, and retain both absolute anchor flags.
#[derive(Clone)]
pub struct Tokens<'a> {
    source: &'a str,
    position: usize,
    failed: bool,
    last_cell: bool,
    range_cell: bool,
    qualified_cell: bool,
}
impl<'a> Tokens<'a> {
    /// Scan borrowed source without constructing an AST.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            failed: false,
            last_cell: false,
            range_cell: false,
            qualified_cell: false,
        }
    }
    fn read(&mut self) -> Result<Option<Token<'a>>, Unsupported> {
        let start = self.position;
        let tail = self.source.get(start..).ok_or(Unsupported::Syntax)?;
        let Some(first) = tail.chars().next() else {
            return if self.range_cell || self.qualified_cell {
                Err(Unsupported::Range)
            } else {
                Ok(None)
            };
        };
        // Prefix, primitive-axis and ordinary-token handling share this borrowed
        // lexical scan rather than recognizing each identifier three times.
        let word = identifier_character(first)
            .then(|| identifier(tail).ok())
            .flatten();
        let after_word = word.and_then(|(rest, _)| whitespace(rest).ok().map(|(rest, _)| rest));
        if let Some(length) = after_word
            .filter(|rest| rest.starts_with(':'))
            .and_then(|_| fixed_axis_range(tail))
        {
            self.position = self
                .position
                .checked_add(length)
                .ok_or(Unsupported::Syntax)?;
            self.last_cell = false;
            self.range_cell = false;
            self.qualified_cell = false;
            return Ok(Some(Token {
                raw: self
                    .source
                    .get(start..self.position)
                    .ok_or(Unsupported::Syntax)?,
                start,
                end: self.position,
                kind: TokenKind::FixedReference,
            }));
        }
        // Qualifier spelling remains literal. Only bounded cell endpoints qualify
        // for this fast path; names, external workbooks and 3D spans fall back.
        let prefix = if first == '\'' {
            let bytes = tail.as_bytes();
            let mut end = 1;
            loop {
                if end >= bytes.len() {
                    return Err(Unsupported::Syntax);
                }
                // SAFETY: The preceding termination check rejects end at or beyond bytes.len() before this byte access.
                if bytes[end] == b'\'' {
                    end += 1;
                    if bytes.get(end) != Some(&b'\'') {
                        break;
                    }
                }
                end += 1;
            }
            (bytes.get(end) == Some(&b'!')).then_some(end + 1)
        } else {
            word.and_then(|(rest, text)| rest.starts_with('!').then_some(text.len() + 1))
        };
        if let Some(length) = prefix {
            if self.qualified_cell
                || tail
                    .get(..length)
                    .ok_or(Unsupported::Syntax)?
                    .contains([':', '[', ']'])
            {
                return Err(Unsupported::QualifiedReference);
            }
            let (_, text) = identifier(tail.get(length..).ok_or(Unsupported::Syntax)?)
                .map_err(|_| Unsupported::QualifiedReference)?;
            if !matches!(cell_coordinates(text), Ok(Some(_)))
                && fixed_axis_range(tail.get(length..).ok_or(Unsupported::Syntax)?).is_none()
            {
                return Err(Unsupported::QualifiedReference);
            }
            self.position += length;
            self.qualified_cell = true;
            return Ok(Some(Token {
                raw: self
                    .source
                    .get(start..self.position)
                    .ok_or(Unsupported::Syntax)?,
                start,
                end: self.position,
                kind: TokenKind::Literal,
            }));
        }
        let (length, kind, space) = if first.is_whitespace() {
            let (rest, _) = whitespace(tail).map_err(|_| Unsupported::Syntax)?;
            (tail.len() - rest.len(), TokenKind::Literal, true)
        } else if first == '"' {
            // Doubled quotes are escaped quotes, exactly as quoted_token handles them.
            let bytes = tail.as_bytes();
            let mut end = 1;
            loop {
                if end == bytes.len() {
                    return Err(Unsupported::Syntax);
                }
                // SAFETY: The preceding termination check rejects end at or beyond bytes.len() before this byte access.
                if bytes[end] == b'"' {
                    end += 1;
                    if bytes.get(end) != Some(&b'"') {
                        break;
                    }
                }
                end += 1;
            }
            (end, TokenKind::Text, false)
        } else if matches!(first, '\'' | '!') {
            return Err(Unsupported::QualifiedReference);
        } else if matches!(first, '{' | '}' | '#') {
            return Err(Unsupported::ArrayOrError);
        } else if first.is_ascii_digit() || first == '.' {
            let (rest, value) = number_token(tail).map_err(|_| Unsupported::Syntax)?;
            if !value.is_finite() {
                return Err(Unsupported::Syntax);
            }
            (tail.len() - rest.len(), TokenKind::Number, false)
        } else if first.is_alphanumeric() || matches!(first, '$' | '_' | '\\') {
            let (_, text) = word.ok_or(Unsupported::Syntax)?;
            let lookahead = after_word.ok_or(Unsupported::Syntax)?;
            let kind = word_kind(text, lookahead.starts_with('('))?;
            (text.len(), kind, false)
        } else if first == ':' {
            if !self.last_cell || self.range_cell {
                return Err(Unsupported::Range);
            }
            self.range_cell = true;
            self.last_cell = false;
            self.position += 1;
            return Ok(Some(Token {
                raw: self
                    .source
                    .get(start..self.position)
                    .ok_or(Unsupported::Syntax)?,
                start,
                end: self.position,
                kind: TokenKind::Literal,
            }));
        } else if "+-*/^&=<>%,();@".contains(first) {
            (first.len_utf8(), TokenKind::Literal, false)
        } else {
            return Err(Unsupported::Syntax);
        };
        if !space {
            let cell = matches!(kind, TokenKind::Cell(_));
            if (self.range_cell || self.qualified_cell) && !cell {
                return Err(Unsupported::Range);
            }
            self.range_cell = false;
            self.qualified_cell = false;
            self.last_cell = cell;
        }
        self.position += length;
        Ok(Some(Token {
            raw: self
                .source
                .get(start..self.position)
                .ok_or(Unsupported::Syntax)?,
            start,
            end: self.position,
            kind,
        }))
    }
}
/// Decode one leading unqualified cell token, retaining the unvalidated remaining source.
/// The canonical function/cell priority and worksheet coordinate decoder are shared
/// with `Tokens`; callers must validate the complete surrounding formula separately.
pub fn cell_prefix(source: &str) -> Option<(&str, CellCoordinates)> {
    let (rest, text) = identifier(source).ok()?;
    if rest.starts_with('!') {
        return None;
    }
    let (lookahead, _) = whitespace(rest).ok()?;
    match word_kind(text, lookahead.starts_with('(')).ok()? {
        TokenKind::Cell(cell) => Some((rest, cell)),
        _ => None,
    }
}
/// Preserve the parser's function-versus-cell priority in every classification entry point.
fn word_kind(text: &str, call: bool) -> Result<TokenKind, Unsupported> {
    let decoded = cell_coordinates(text);
    // A1(A2) starts as a cell; LOG10(A2) is a known function, while LOG10 alone is a cell.
    if call
        && (!matches!(decoded, Ok(Some(_)))
            || crate::formula::reference_classes::return_class_insensitive(text).is_some())
    {
        if text.contains('$') {
            return Err(Unsupported::Reference);
        }
        Ok(TokenKind::Literal)
    } else {
        match decoded {
            Ok(Some(cell)) => Ok(TokenKind::Cell(cell)),
            Ok(None) if !text.contains('$') => Ok(TokenKind::Literal),
            _ => Err(Unsupported::Reference),
        }
    }
}
/// Recognize only complete primitive axis pairs using the parser's own decoder.
fn fixed_axis_range(source: &str) -> Option<usize> {
    let (rest, first) = identifier(source).ok()?;
    let (rest, _) = whitespace(rest).ok()?;
    let rest = rest.strip_prefix(':')?;
    let (rest, _) = whitespace(rest).ok()?;
    let (rest, last) = identifier(rest).ok()?;
    use crate::formula::Expr;
    let a = super::references::partial_reference(first, None)?;
    let b = super::references::partial_reference(last, None)?;
    if matches!(
        (a, b),
        (Expr::Column { .. }, Expr::Column { .. }) | (Expr::Row { .. }, Expr::Row { .. })
    ) {
        source.len().checked_sub(rest.len())
    } else {
        None
    }
}
impl<'a> Iterator for Tokens<'a> {
    /// Classified source token or an explicit full-parser fallback reason.
    type Item = Result<Token<'a>, Unsupported>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        match self.read() {
            Ok(Some(token)) => Some(Ok(token)),
            Ok(None) => None,
            Err(error) => {
                self.failed = true;
                Some(Err(error))
            }
        }
    }
}

/// Hash is only a bucket selector; `normalized_equal` must confirm a cache hit.
/// The hash algorithm is process/library-local, not a persistent file format.
pub fn normalized_hash(source: &str) -> Result<u64, Unsupported> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for token in Tokens::new(source) {
        let token = token?;
        match token.kind {
            TokenKind::Literal
            | TokenKind::Number
            | TokenKind::Text
            | TokenKind::FixedReference => {
                0u8.hash(&mut hasher);
                token.raw.hash(&mut hasher);
            }
            TokenKind::Cell(_) => 1u8.hash(&mut hasher),
        }
    }
    Ok(hasher.finish())
}
/// Exact normalized token comparison, including complete tails on both sides.
/// Reference anchor flags/coordinates become per-instance bindings, not key bytes.
pub fn normalized_equal(left: &str, right: &str) -> Result<bool, Unsupported> {
    let mut left = Tokens::new(left);
    let mut right = Tokens::new(right);
    let mut equal = true;
    loop {
        let a = left.next().transpose()?;
        let b = right.next().transpose()?;
        match (a, b) {
            (None, None) => return Ok(equal),
            (Some(a), Some(b)) => {
                equal &= match (a.kind, b.kind) {
                    (TokenKind::Cell(_), TokenKind::Cell(_)) => true,
                    (TokenKind::Literal, TokenKind::Literal)
                    | (TokenKind::Number, TokenKind::Number)
                    | (TokenKind::Text, TokenKind::Text)
                    | (TokenKind::FixedReference, TokenKind::FixedReference) => a.raw == b.raw,
                    _ => false,
                }
            }
            _ => equal = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{Expr, parse_formula};
    fn ast_cells(e: &Expr, out: &mut Vec<CellCoordinates>) {
        match e {
            Expr::Cell(c) => out.push(CellCoordinates {
                row: c.row,
                column: c.column,
                row_absolute: c.row_absolute,
                column_absolute: c.column_absolute,
            }),
            Expr::Unary { operand, .. } => ast_cells(operand, out),
            Expr::Binary { left, right, .. } => {
                ast_cells(left, out);
                ast_cells(right, out);
            }
            Expr::Function { args, .. } => {
                for a in args {
                    ast_cells(a, out);
                }
            }
            _ => {}
        }
    }
    #[test]
    fn classified_cells_agree_with_parser() {
        for source in [
            "=SUM(A1,$B2,C$3,$D$4)",
            "LOG10(A1)",
            "log10 (A1)",
            "LOG10+A1",
            "A1 (A2)",
            "A1:B2",
            "SUM(A1 : B2)",
            "SUM(A1,C2:D3)",
            "VLOOKUP(A1,Sheet1!$B$2:C3,2,FALSE)",
            "SUM('O''Brien'!A1:B2,Sheet2!C3:Sheet3!D4)",
            "unknown(A1)",
            "A1+\"A2\"",
            "SUM(\"escaped\"\"A1\",A2)",
            "A01+$$B$$002",
            "Z9+XFD1048576",
            "TRUE+A1",
            "A1_name+A2",
            "A1\\suffix+A2",
        ] {
            let expression = parse_formula(source).unwrap_or_else(|e| panic!("{source}: {e}"));
            let mut expected = Vec::new();
            ast_cells(&expression, &mut expected);
            let mut reconstructed = String::new();
            let actual: Vec<_> = Tokens::new(source)
                .map(|t| {
                    let t = t.unwrap();
                    assert_eq!(&source[t.start..t.end], t.raw);
                    reconstructed.push_str(t.raw);
                    t.kind
                })
                .filter_map(|k| match k {
                    TokenKind::Cell(c) => Some(c),
                    _ => None,
                })
                .collect();
            assert_eq!(reconstructed, source);
            assert_eq!(actual, expected, "{source}");
        }
    }
    #[test]
    fn classified_function_cell_priority() {
        for source in ["A1(A2)", "A1 (A2)"] {
            let tokens: Vec<_> = Tokens::new(source).collect::<Result<_, _>>().unwrap();
            assert!(matches!(tokens[0].kind, TokenKind::Cell(_)));
        }
        assert!(parse_formula("A1(A2)").is_err());
        assert!(parse_formula("A1 (A2)").is_ok());
        for source in ["LOG10(A1)", "log10 (A1)", "XFE1(A1)"] {
            assert!(parse_formula(source).is_ok());
            assert_eq!(
                Tokens::new(source).next().unwrap().unwrap().kind,
                TokenKind::Literal
            );
        }
        assert!(matches!(
            Tokens::new("LOG10").next().unwrap().unwrap().kind,
            TokenKind::Cell(_)
        ));
    }
    #[test]
    fn classified_normalization_preserves_every_literal() {
        assert_eq!(normalized_equal("SUM(A1,$B$2)", "SUM(Z9,C3)"), Ok(true));
        assert_eq!(
            normalized_hash("SUM(A1,$B$2)"),
            normalized_hash("SUM(Z9,C3)")
        );
        for other in [
            "sum(A1,$B$2)",
            "SUM(A1, $B$2)",
            "SUM(A1,$B$2)+1",
            "SUM(A1,\"B2\")",
            "SUM(A1,B2_name)",
        ] {
            assert_eq!(
                normalized_equal("SUM(A1,$B$2)", other),
                Ok(false),
                "{other}"
            );
        }
        assert_eq!(normalized_equal("Sheet1!A1:B2", "Sheet1!C3:D4"), Ok(true));
        assert_eq!(normalized_equal("Sheet1!A1:B2", "Sheet2!C3:D4"), Ok(false));
        assert_eq!(normalized_equal("LOG10(A1)", "A1(A2)"), Ok(false));
        assert_eq!(normalized_equal("A1 (A2)", "A3(A4)"), Ok(false));
        assert_eq!(normalized_equal("SUM(A1)", "SUM(B2)garbage"), Ok(false));
    }
    #[test]
    fn leading_cell_decoder_agrees_with_canonical_tokens() {
        for source in [
            "A1",
            "$B$2+C3",
            "A$1:$C3",
            "A1 (A2)",
            "A1(A2)",
            "LOG10",
            "log10 + 1",
            "XFD1048576",
            "$$A$$01",
            "A1#",
            "A1[unsupported]",
        ] {
            let token = Tokens::new(source).next().unwrap().unwrap();
            let TokenKind::Cell(expected) = token.kind else {
                panic!("expected cell: {source}");
            };
            let (rest, actual) = cell_prefix(source).unwrap();
            assert_eq!(actual, expected, "{source}");
            assert_eq!(source.len() - rest.len(), token.end);
        }
        for source in [
            "",
            " A1",
            "1",
            "1:2",
            "A:A",
            "'O''Brien'!A1",
            "Sheet1!A1",
            "A1!B2",
            "123!A1",
            "LOG10(A1)",
            "log10 (A1)",
            "XFE1",
            "A1048577",
            "A0",
            "A1_name",
            "A1\\suffix",
            "Å1",
            "A1.5",
            "\"A1\"",
        ] {
            assert!(cell_prefix(source).is_none(), "{source}");
            assert!(
                !matches!(Tokens::new(source).next(), Some(Ok(token)) if matches!(token.kind, TokenKind::Cell(_))),
                "{source}"
            );
        }
        let (_, cell) = cell_prefix("$B$3").unwrap();
        assert_eq!(
            (
                cell.row,
                cell.column,
                cell.row_absolute,
                cell.column_absolute
            ),
            (2, 1, true, true)
        );
        let (_, cell) = cell_prefix("XFD1048576").unwrap();
        assert_eq!((cell.row, cell.column), (1048575, 16383));
    }
    #[test]
    fn fixed_axis_pairs_and_literal_kinds_preserve_execution_identity() {
        for source in [
            "A:B",
            "$A:$B",
            "1:2",
            "$1:$2",
            "SUM('O''Brien'!$A : $C)",
            "SUM(S!$1 : $4)",
        ] {
            assert!(parse_formula(source).is_ok(), "{source}");
            let tokens: Vec<_> = Tokens::new(source).collect::<Result<_, _>>().unwrap();
            assert!(tokens.iter().any(|t| t.kind == TokenKind::FixedReference));
            assert_eq!(tokens.iter().map(|t| t.raw).collect::<String>(), source);
            assert_eq!(normalized_equal(source, source), Ok(true));
        }
        for (left, right) in [
            ("SUM(A:B)", "SUM(C:D)"),
            ("SUM(1:2)", "SUM(3:4)"),
            ("SUM(A1,1)", "SUM(A2,2)"),
            ("SUM(A1,\"a\")", "SUM(A2,\"b\")"),
        ] {
            assert_eq!(normalized_equal(left, right), Ok(false));
        }
        let tokens = Tokens::new("SUM(123!A1,1.2e3,\"a\"\"b\")")
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            tokens
                .iter()
                .any(|t| t.raw == "123!" && t.kind == TokenKind::Literal)
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.raw == "1.2e3" && t.kind == TokenKind::Number)
        );
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Text));
        for source in ["A:1", "1:A", "'S'!A:A1", "A:XFE", "0:2", "1:1048577"] {
            assert!(normalized_hash(source).is_err(), "{source}");
        }
    }
    #[test]
    fn classified_unsupported_requests_parser_fallback() {
        for source in [
            "'Sheet1:Sheet3'!A1",
            "Sheet1!Name",
            "'Quoted sheet'!Name",
            "Sheet1!A1!B2",
            "'unterminated!A1",
            "Sheet1!A1:",
            "Sheet1:Sheet3!A1",
            "A1:OFFSET(B2,0,0)",
            "A1:Z",
            "{1,2}",
            "#REF!",
            "A1[tail]",
            "A1+\"unterminated",
            "XFE1",
            "A1048577",
            "A1:",
        ] {
            assert!(normalized_hash(source).is_err(), "{source}");
        }
        for tail in ["[bad]", "!A1", "\"unterminated", "#BAD", ":"] {
            let candidate = format!("SUM(B2){tail}");
            assert!(
                normalized_equal("SUM(A1)", &candidate).is_err(),
                "{candidate}"
            );
        }
        // The front stage never claims to validate syntax. Safe cache hits require
        // a successful exemplar parse and exact normalized token equality.
        assert!(normalized_hash("SUM(A1").is_ok());
        assert!(parse_formula("SUM(A1").is_err());
        assert_eq!(normalized_equal("SUM(A1)", "SUM(B2"), Ok(false));
    }
}
