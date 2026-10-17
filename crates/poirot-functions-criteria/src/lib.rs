//! Conditional aggregation domain, based on pinned POI Countif, Sumif and Baseifs.
//! Adapters resolve references, implicit intersection and array criteria before planning.
use nom::{Parser, branch::alt, bytes::complete::tag};
use std::cmp::Ordering;
/// Subtotal aggregation with hidden-row and nested-subtotal provenance.
pub mod subtotal;
#[cfg(test)]
mod tests;

/// Spreadsheet error vocabulary used by criteria matching and aggregation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Empty reference intersection (`#NULL!`).
    Null,
    /// Zero denominator (`#DIV/0!`).
    DivZero,
    /// Invalid criterion or operand (`#VALUE!`).
    Value,
    /// Invalid reference (`#REF!`).
    Ref,
    /// Unresolved name (`#NAME?`).
    Name,
    /// Invalid numeric result (`#NUM!`).
    Num,
    /// Unavailable value (`#N/A`).
    Na,
}
/// Borrowed cell projection used to match criteria without workbook ownership.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Blank criterion candidate.
    Blank,
    /// Numeric criterion candidate.
    Number(f64),
    /// Text candidate consumed by comparison or wildcard matching.
    Text(String),
    /// Boolean criterion candidate.
    Bool(bool),
    /// Spreadsheet error retained for family-specific handling.
    Error(Error),
}
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
impl Comparison {
    fn test(self, c: Ordering) -> bool {
        match self {
            Self::None | Self::Eq => c == Ordering::Equal,
            Self::Ne => c != Ordering::Equal,
            Self::Lt => c == Ordering::Less,
            Self::Le => c != Ordering::Greater,
            Self::Gt => c == Ordering::Greater,
            Self::Ge => c != Ordering::Less,
        }
    }
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
/// Parse numeric criterion text using the family's supported grammar.
pub fn coerce_criteria_number_text(s: &str) -> Option<f64> {
    let s = s.trim_matches(|c: char| c <= '\u{20}');
    let parsed: nom::IResult<&str, f64> = nom::number::complete::double(s);
    match parsed {
        Ok(("", n)) if n.is_finite() => Some(n),
        _ => None,
    }
}
fn parse_error(s: &str) -> Option<Error> {
    Some(match s {
        "#NULL!" => Error::Null,
        "#DIV/0!" => Error::DivZero,
        "#VALUE!" => Error::Value,
        "#REF!" => Error::Ref,
        "#NAME?" => Error::Name,
        "#NUM!" => Error::Num,
        "#N/A" => Error::Na,
        _ => return None,
    })
}

/// Parsed conditional-aggregation comparison and operand, such as `>=5` or a text pattern.
#[derive(Clone, Debug)]
pub struct Criterion {
    /// Comparison selected by the criterion's prefix.
    pub comparison: Comparison,
    /// Right-hand operand used when testing candidate cells.
    pub value: Cell,
}
impl Criterion {
    /// Compile a cell criterion into comparison and wildcard state.
    pub fn parse(value: &Cell) -> Self {
        if let Cell::Text(text) = value {
            let (comparison, s) = parse_comparison(text);
            let value = if s.eq_ignore_ascii_case("true") {
                Cell::Bool(true)
            } else if s.eq_ignore_ascii_case("false") {
                Cell::Bool(false)
            } else if let Some(n) = coerce_criteria_number_text(s) {
                Cell::Number(n)
            } else if let Some(e) = parse_error(s) {
                Cell::Error(e)
            } else {
                Cell::Text(s.into())
            };
            Self { comparison, value }
        } else {
            Self {
                comparison: Comparison::None,
                value: value.clone(),
            }
        }
    }
    /// Test a projected cell against the compiled criterion.
    pub fn matches(&self, cell: &Cell) -> bool {
        let op = self.comparison;
        match (&self.value, cell) {
            (Cell::Blank, _) => false,
            (Cell::Number(a), Cell::Number(b)) => op.test(b.total_cmp(a)),
            (Cell::Number(a), Cell::Text(b)) => match op {
                Comparison::None | Comparison::Eq => coerce_criteria_number_text(b) == Some(*a),
                Comparison::Ne => true,
                _ => false,
            },
            (Cell::Bool(a), Cell::Bool(b)) => op.test(b.cmp(a)),
            (Cell::Error(a), Cell::Error(b)) => op.test(error_code(*b).cmp(&error_code(*a))),
            (Cell::Text(a), Cell::Blank) => match op {
                Comparison::None | Comparison::Eq => a.is_empty(),
                Comparison::Ne => !a.is_empty(),
                _ => false,
            },
            (Cell::Text(a), Cell::Text(b)) => {
                if a.is_empty() && b.is_empty() {
                    return matches!(op, Comparison::None | Comparison::Ne);
                }
                if matches!(op, Comparison::None | Comparison::Eq | Comparison::Ne)
                    && has_wildcards(a)
                {
                    return op.test(if wildcard_match(a, b) {
                        Ordering::Equal
                    } else {
                        Ordering::Less
                    });
                }
                op.test(b.to_lowercase().cmp(&a.to_lowercase()))
            }
            _ => op == Comparison::Ne,
        }
    }
}
fn error_code(e: Error) -> u8 {
    match e {
        Error::Null => 0,
        Error::DivZero => 7,
        Error::Value => 15,
        Error::Ref => 23,
        Error::Name => 29,
        Error::Num => 36,
        Error::Na => 42,
    }
}

/// Compiled wildcard atom used while matching text criteria.
#[derive(Clone, Copy)]
enum Token {
    Literal(char),
    One,
    Many,
}
fn tokens(pattern: &str) -> Vec<Token> {
    let mut chars = pattern.chars().peekable();
    let mut out = Vec::new();
    while let Some(c) = chars.next() {
        out.push(match c {
            '*' => Token::Many,
            '?' => Token::One,
            '~' if chars.peek().is_some_and(|c| matches!(c, '*' | '?' | '~')) => {
                // SAFETY: The guard peeked Some on this iterator, with no intervening advancement.
                Token::Literal(chars.next().unwrap())
            }
            c => Token::Literal(c),
        });
    }
    out
}
/// Recognize unescaped wildcard syntax in criteria text.
pub fn has_wildcards(pattern: &str) -> bool {
    pattern.contains(['*', '?']) || pattern.contains("~~")
}
/// Full, ASCII-case-insensitive POI wildcard match. Iterative star backtracking, no recursion.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = tokens(pattern);
    let t: Vec<char> = text.chars().collect();
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len()
            // SAFETY: The loop guards bound i and j; retry starts at j and never exceeds j before advancing both together.
            && match p[i] {
                // SAFETY: The loop guards bound i and j; retry starts at j and never exceeds j before advancing both together.
                Token::Literal(c) => c.eq_ignore_ascii_case(&t[j]),
                // SAFETY: The loop guards bound i and j; retry starts at j and never exceeds j before advancing both together.
                Token::One => !matches!(t[j], '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'),
                Token::Many => false,
            }
        {
            i += 1;
            j += 1;
        // SAFETY: The loop guards bound i and j; retry starts at j and never exceeds j before advancing both together.
        } else if i < p.len() && matches!(p[i], Token::Many) {
            star = Some(i);
            i += 1;
            retry = j;
        } else if let Some(s) = star {
            // SAFETY: The loop guards bound i and j; retry starts at j and never exceeds j before advancing both together.
            if matches!(t[retry], '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}') {
                return false;
            }
            retry += 1;
            j = retry;
            i = s + 1;
        } else {
            return false;
        }
    }
    // SAFETY: The loop guards bound i and j; retry starts at j and never exceeds j before advancing both together.
    while i < p.len() && matches!(p[i], Token::Many) {
        i += 1;
    }
    i == p.len()
}

/// Aggregation applied to rows selected by compiled criteria.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateKind {
    /// Count matching entries.
    Count,
    /// Sum matching numeric entries.
    Sum,
    /// Average entries admitted by the aggregate's coercion rules.
    Average,
    /// Average matching entries with AVERAGEIF blank handling.
    AverageIf,
    /// Select the maximum matching numeric entry.
    Max,
    /// Select the minimum matching numeric entry.
    Min,
}
/// Borrowed rectangular cells in row-major order. Shape must match the slice length;
/// [`CriteriaPlan::new`] validates all ranges before execution.
#[derive(Clone, Copy, Debug)]
pub struct Range<'a> {
    /// Logical number of rows in the projected range.
    pub rows: usize,
    /// Logical number of columns in the projected range.
    pub columns: usize,
    /// Row-major projected cells, including logical blanks.
    pub cells: &'a [Cell],
}
impl Range<'_> {
    fn valid(&self) -> bool {
        self.rows.checked_mul(self.columns) == Some(self.cells.len())
            && self.rows > 0
            && self.columns > 0
    }
}
/// Validated matching ranges, criteria, and optional aggregation target.
/// Planning fixes dimensions and coercion rules; execution visits cells in source order.
pub struct CriteriaPlan<'a> {
    kind: AggregateKind,
    ranges: Vec<Range<'a>>,
    criteria: Vec<Criterion>,
    target: Option<Range<'a>>,
}
impl<'a> CriteriaPlan<'a> {
    /// Single-IF target must already be resized from its reference anchor by the adapter.
    pub fn new(
        kind: AggregateKind,
        ranges: Vec<Range<'a>>,
        criteria: Vec<Criterion>,
        target: Option<Range<'a>>,
        ifs: bool,
    ) -> Result<Self, Error> {
        if ranges.is_empty()
            || ranges.len() != criteria.len()
            || (!ifs && ranges.len() != 1)
            || (kind != AggregateKind::Count && target.is_none())
        {
            return Err(Error::Value);
        }
        // SAFETY: The guard above rejects an empty ranges vector before selecting its first element.
        let shape = (ranges[0].rows, ranges[0].columns);
        if ranges
            .iter()
            .chain(target.iter())
            .any(|r| !r.valid() || (r.rows, r.columns) != shape)
        {
            return Err(Error::Value);
        }
        if ifs || kind == AggregateKind::AverageIf {
            for c in &criteria {
                if let Cell::Error(e) = c.value {
                    return Err(e);
                }
            }
        }
        Ok(Self {
            kind,
            ranges,
            criteria,
            target,
        })
    }
    /// Dense visits; adapter must additionally charge criterion bytes and wildcard work.
    pub fn cell_visits(&self) -> Option<usize> {
        self.ranges
            .first()?
            .cells
            .len()
            .checked_mul(self.ranges.len() + usize::from(self.target.is_some()))
    }
    /// Execute the prevalidated criteria selection and aggregate its admitted values.
    pub fn evaluate(&self) -> Result<f64, Error> {
        let first = self.ranges.first().ok_or(Error::Value)?;
        if self
            .ranges
            .iter()
            .any(|range| range.cells.len() != first.cells.len())
        {
            return Err(Error::Value);
        }
        let (mut sum, mut count, mut extremum) = (0.0, 0usize, None::<f64>);
        for i in 0..first.cells.len() {
            if !self
                .ranges
                .iter()
                .zip(&self.criteria)
                // SAFETY: The local shape check gives every range first.cells.len() entries, and i ranges strictly below that length.
                .all(|(r, c)| c.matches(&r.cells[i]))
            {
                continue;
            }
            if self.kind == AggregateKind::Count {
                count += 1;
                continue;
            }
            if self.kind == AggregateKind::AverageIf
                // SAFETY: The local shape check gives every range first.cells.len() entries, and i ranges strictly below that length.
                && let Cell::Error(e) = first.cells[i]
            {
                return Err(e);
            }
            let cell = self
                .target
                .as_ref()
                .and_then(|target| target.cells.get(i))
                .ok_or(Error::Value)?;
            if let Cell::Error(e) = cell
                && self.kind != AggregateKind::AverageIf
            {
                return Err(*e);
            }
            if let Cell::Number(n) = cell {
                sum += n;
                count += 1;
            }
            // POI MAXIFS/MINIFS explicitly treat matched nonnumeric values as zero.
            let n = if let Cell::Number(n) = cell { *n } else { 0.0 };
            extremum = Some(match extremum {
                None => n,
                Some(a) if self.kind == AggregateKind::Min => a.min(n),
                Some(a) => a.max(n),
            });
        }
        let n = match self.kind {
            AggregateKind::Count => count as f64,
            AggregateKind::Sum => sum,
            AggregateKind::Average | AggregateKind::AverageIf if count == 0 => {
                return Err(Error::DivZero);
            }
            AggregateKind::Average | AggregateKind::AverageIf => sum / count as f64,
            AggregateKind::Max | AggregateKind::Min => extremum.unwrap_or(0.0),
        };
        if n.is_finite() {
            Ok(n)
        } else {
            Err(Error::Num)
        }
    }
}
