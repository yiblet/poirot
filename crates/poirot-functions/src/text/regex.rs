//! PCRE2-backed spreadsheet text operations. Microsoft specifies the PCRE2 flavor:
//! <https://support.microsoft.com/en-us/excel/functions/regexextract-function>
//! <https://support.microsoft.com/en-us/excel/functions/regexreplace-function>
//! <https://support.microsoft.com/en-us/excel/functions/regextest-function>
//! The implementation and tests are independently authored. Global matching
//! follows PCRE2's documented empty-match iteration rule (see Cursor below); no
//! upstream source or tests were copied. Provenance is in LICENSES/PCRE2.md.
use pcre2::bytes::{CaptureLocations, Regex, RegexBuilder};

/// Case comparison selected by the spreadsheet argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// Match case exactly.
    Sensitive,
    /// Apply PCRE2 Unicode case folding.
    Insensitive,
}
/// Strings selected by REGEXEXTRACT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extraction {
    /// The complete first match.
    First,
    /// Every non-overlapping complete match, in subject order.
    All,
    /// Capturing groups from the first match, in group-number order.
    Groups,
}
/// Native PCRE2 execution guards, separate from caller work estimates.
/// These guards interrupt native backtracking; consumed native work is unavailable.
/// JIT remains disabled so depth and heap guards apply. PCRE2's own ceilings remain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Maximum native match-loop steps per search.
    pub match_steps: u32,
    /// Maximum native backtracking frames per search.
    pub depth: u32,
    /// Maximum native backtracking heap in kibibytes per search.
    pub heap_kib: u32,
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            match_steps: 1_000_000,
            depth: 256,
            heap_kib: 8192,
        }
    }
}
impl ExecutionLimits {
    /// Replace the native match-loop guard; zero is zero allowance.
    pub fn max_match_steps(mut self, maximum: u32) -> Self {
        self.match_steps = maximum;
        self
    }
    /// Replace the native backtracking-frame guard.
    pub fn max_depth(mut self, maximum: u32) -> Self {
        self.depth = maximum;
        self
    }
    /// Replace the native backtracking heap guard in kibibytes.
    pub fn max_heap_kib(mut self, maximum: u32) -> Self {
        self.heap_kib = maximum;
        self
    }
}
/// Caller-controlled admission before native searches and owned allocations.
pub trait Host {
    /// Operational failure propagated unchanged to the caller.
    type Failure;
    /// Native execution guards independent of estimated caller work.
    fn execution_limits(&self) -> ExecutionLimits {
        ExecutionLimits::default()
    }
    /// Admit estimated search work before an uninterruptible native call.
    fn work(&mut self, units: usize) -> Result<(), Self::Failure>;
    /// Admit the byte capacity of a single owned text buffer.
    fn text(&mut self, bytes: usize) -> Result<(), Self::Failure>;
    /// Admit the logical size of a single collection.
    fn values(&mut self, count: usize) -> Result<(), Self::Failure>;
}
/// Formula-domain failures remain distinct from backend and caller failures.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure<E> {
    /// Invalid PCRE2 pattern syntax.
    Pattern,
    /// No matching text, or no groups in capture-group mode.
    NoMatch,
    /// A result exceeds Excel's 32,767 UTF-16-unit text limit.
    TextLimit,
    /// A backend feature cannot preserve the model's text semantics.
    Unsupported(&'static str),
    /// Native execution or compilation failed; this is never a valid no-match.
    Native(i32),
    /// A fallible allocation could not be admitted.
    Resource,
    /// A checked byte or collection size cannot be represented.
    SizeOverflow,
    /// Backend capture geometry did not satisfy the checked adapter state.
    InvalidState,
    /// The caller refused work or allocation.
    Host(E),
}
fn compile_failure<E>(error: pcre2::Error) -> Failure<E> {
    // Numeric codes are the PCRE2 public ABI, verified in pcre2-sys 0.2.10's
    // src/bindings.rs at fd05026e1bf3ad62b3876cf9bd952dc742368462:
    // https://github.com/BurntSushi/rust-pcre2/blob/fd05026e1bf3ad62b3876cf9bd952dc742368462/pcre2-sys/src/bindings.rs
    // Limits, unavailable native capabilities and internal errors are operational.
    let operational = matches!(
        error.code(),
        119 | 133 | 184 | 207 // PARENTHESES/QUERY_BARJX/ECLASS nesting and stack limits
        | 120 | 186 | 188 | 197 | 201 // pattern size/complexity and capture limits
        | 121 // HEAP_FAILED
        | 110 | 123 | 131 | 152 | 153 | 156 | 163 | 170 | 180 | 189 | 190 // INTERNAL_*
        | 132 | 145 | 185 | 196 | 203 // unavailable Unicode, byte escape, script or callout support
        | 135 | 187 | 200 // native lookbehind complexity/length limits
    );
    if matches!(error.kind(), pcre2::ErrorKind::Compile) && !operational {
        Failure::Pattern
    } else {
        Failure::Native(error.code())
    }
}
/// A growing result with a running UTF-16 length, avoiding repeated prefix scans.
#[derive(Default)]
struct TextBuffer {
    text: String,
    units: usize,
}
fn push_text<H: Host>(
    out: &mut TextBuffer,
    text: &str,
    host: &mut H,
) -> Result<(), Failure<H::Failure>> {
    let bytes = out
        .text
        .len()
        .checked_add(text.len())
        .ok_or(Failure::TextLimit)?;
    let units = out.units.saturating_add(text.encode_utf16().count());
    if units > 32767 {
        return Err(Failure::TextLimit);
    }
    host.text(bytes).map_err(Failure::Host)?;
    out.text
        .try_reserve(text.len())
        .map_err(|_| Failure::Resource)?;
    // SAFETY: The local checked byte sum and try_reserve admit this append.
    out.text.push_str(text);
    out.units = units;
    Ok(())
}
fn copy_text<H: Host>(text: &str, host: &mut H) -> Result<String, Failure<H::Failure>> {
    let mut result = TextBuffer::default();
    push_text(&mut result, text, host)?;
    Ok(result.text)
}
fn active_byte_escape(pattern: &str) -> bool {
    let mut chars = pattern.chars().peekable();
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if character != '\\' {
            continue;
        }
        if quoted {
            // In \Q literals, only an actual \E ends quoting. In particular,
            // do not consume a second backslash which may begin that terminator.
            if chars.peek() == Some(&'E') {
                chars.next();
                quoted = false;
            }
        } else {
            match chars.next() {
                Some('Q') => quoted = true,
                Some('C') => return true,
                _ => {}
            }
        }
    }
    false
}
/// A compiled Unicode PCRE2 pattern with bounded native searches.
/// The 8-bit backend does not represent isolated UTF-16 surrogates or Excel
/// code-unit escapes such as \C. Callers must reject malformed UTF-16 before
/// conversion; this type rejects active \C escapes instead of changing meaning.
pub struct Program {
    regex: Regex,
    case: Case,
}
impl Program {
    /// Compile a pattern after caller admission, retaining native resource guards.
    pub fn compile<H: Host>(
        pattern: &str,
        case: Case,
        host: &mut H,
    ) -> Result<Self, Failure<H::Failure>> {
        if pattern.encode_utf16().count() > 32767 {
            return Err(Failure::TextLimit);
        }
        if active_byte_escape(pattern) {
            return Err(Failure::Unsupported(
                "PCRE2 byte escape \\C cannot preserve UTF-16 code-unit semantics",
            ));
        }
        let limits = host.execution_limits();
        let guards = if limits == ExecutionLimits::default() {
            std::borrow::Cow::Borrowed("(*LIMIT_MATCH=1000000)(*LIMIT_DEPTH=256)(*LIMIT_HEAP=8192)")
        } else {
            std::borrow::Cow::Owned(format!(
                "(*LIMIT_MATCH={})(*LIMIT_DEPTH={})(*LIMIT_HEAP={})",
                limits.match_steps, limits.depth, limits.heap_kib
            ))
        };
        let mut source = String::new();
        let bytes = guards
            .len()
            .checked_add(pattern.len())
            .ok_or(Failure::SizeOverflow)?;
        host.text(bytes).map_err(Failure::Host)?;
        host.work(bytes).map_err(Failure::Host)?;
        source
            .try_reserve_exact(bytes)
            .map_err(|_| Failure::Resource)?;
        // SAFETY: The checked local reservation covers both appended strings.
        source.push_str(&guards);
        // SAFETY: guards.len()+pattern.len() was checked and reserved locally above.
        source.push_str(pattern);
        let regex = RegexBuilder::new()
            .utf(true)
            .ucp(true)
            .caseless(case == Case::Insensitive)
            .build(&source)
            .map_err(compile_failure)?;
        host.values(regex.captures_len()).map_err(Failure::Host)?;
        Ok(Self { regex, case })
    }
    /// Test whether any substring matches, preserving all native failures.
    pub fn test<H: Host>(&self, text: &str, host: &mut H) -> Result<bool, Failure<H::Failure>> {
        host.work(text.len().saturating_add(1))
            .map_err(Failure::Host)?;
        self.regex
            .is_match(text.as_bytes())
            .map_err(|e| Failure::Native(e.code()))
    }
    /// Extract the first match, all matches, or the first match's capture groups.
    pub fn extract<H: Host>(
        &self,
        text: &str,
        mode: Extraction,
        host: &mut H,
    ) -> Result<Vec<String>, Failure<H::Failure>> {
        let mut cursor = Cursor::new(self, host)?;
        let mut result = Vec::new();
        while cursor.next(text, host)? {
            let groups = if mode == Extraction::Groups {
                1..self.regex.captures_len()
            } else {
                0..1
            };
            for group in groups {
                let value = cursor.group(text, group)?.unwrap_or("");
                host.values(result.len().saturating_add(1))
                    .map_err(Failure::Host)?;
                result.try_reserve(1).map_err(|_| Failure::Resource)?;
                let value = copy_text(value, host)?;
                // SAFETY: One additional element was reserved immediately above.
                result.push(value);
            }
            if mode != Extraction::All {
                break;
            }
        }
        if result.is_empty() {
            Err(Failure::NoMatch)
        } else {
            Ok(result)
        }
    }
    /// Replace all matches (zero), one from the start (positive), or one from
    /// the end (negative). Replacement recognizes Microsoft's documented $n
    /// captures; $$ emits a literal dollar. Other dollar spellings stay literal.
    pub fn replace<H: Host>(
        &self,
        text: &str,
        replacement: &str,
        occurrence: i64,
        host: &mut H,
    ) -> Result<String, Failure<H::Failure>> {
        let selected = if occurrence < 0 {
            let mut cursor = Cursor::new(self, host)?;
            let mut count = 0u64;
            while cursor.next(text, host)? {
                count = count.saturating_add(1);
            }
            count
                .checked_sub(occurrence.unsigned_abs())
                .map(|n| n.saturating_add(1))
        } else if occurrence == 0 {
            None
        } else {
            Some(occurrence as u64)
        };
        if occurrence < 0 && selected.is_none() {
            return copy_text(text, host);
        }
        let mut cursor = Cursor::new(self, host)?;
        let mut out = TextBuffer::default();
        let mut copied = 0;
        let mut ordinal = 0u64;
        while cursor.next(text, host)? {
            ordinal = ordinal.saturating_add(1);
            if occurrence != 0 && selected != Some(ordinal) {
                continue;
            }
            let (start, end) = cursor.locations.get(0).ok_or(Failure::InvalidState)?;
            push_text(
                &mut out,
                text.get(copied..start)
                    .ok_or(Failure::Unsupported("non-Unicode PCRE2 match boundary"))?,
                host,
            )?;
            expand(replacement, text, &cursor, &mut out, host)?;
            copied = end;
            if occurrence != 0 {
                break;
            }
        }
        push_text(
            &mut out,
            text.get(copied..)
                .ok_or(Failure::Unsupported("non-Unicode PCRE2 match boundary"))?,
            host,
        )?;
        Ok(out.text)
    }
}
/// State for successive regex matches, reusing capture storage between searches.
/// It handles empty-match retries and advances on Unicode boundaries so global extraction or replacement makes progress.
struct Cursor<'a> {
    program: &'a Program,
    locations: CaptureLocations,
    offset: Option<usize>,
    retry_empty: bool,
    retry_regex: Option<Regex>,
}
impl<'a> Cursor<'a> {
    fn new<H: Host>(program: &'a Program, host: &mut H) -> Result<Self, Failure<H::Failure>> {
        host.values(program.regex.captures_len())
            .map_err(Failure::Host)?;
        Ok(Self {
            program,
            locations: program.regex.capture_locations(),
            offset: Some(0),
            retry_empty: false,
            retry_regex: None,
        })
    }
    fn next<H: Host>(&mut self, text: &str, host: &mut H) -> Result<bool, Failure<H::Failure>> {
        let Some(offset) = self.offset else {
            return Ok(false);
        };
        host.work(text.len().saturating_sub(offset).saturating_add(1))
            .map_err(Failure::Host)?;
        let regex = if self.retry_empty {
            if self.retry_regex.is_none() {
                self.retry_regex = Some(retry_regex(self.program, host)?);
            }
            self.retry_regex.as_ref().ok_or(Failure::InvalidState)?
        } else {
            &self.program.regex
        };
        let found = regex
            .captures_read_at(&mut self.locations, text.as_bytes(), offset)
            .map_err(|e| Failure::Native(e.code()))?;
        let Some(found) = found else {
            self.offset = None;
            return Ok(false);
        };
        let start = found.start();
        let end = found.end();
        if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(Failure::Unsupported("non-Unicode PCRE2 match boundary"));
        }
        if self.retry_empty && end <= offset {
            return Err(Failure::InvalidState);
        }
        self.offset = Some(end);
        self.retry_empty = start == end;
        Ok(true)
    }
    fn group<'s, E>(&self, text: &'s str, group: usize) -> Result<Option<&'s str>, Failure<E>> {
        self.locations
            .get(group)
            .map(|(start, end)| {
                text.get(start..end)
                    .ok_or(Failure::Unsupported("non-Unicode PCRE2 capture boundary"))
            })
            .transpose()
    }
}
fn retry_regex<H: Host>(program: &Program, host: &mut H) -> Result<Regex, Failure<H::Failure>> {
    // Iteration rule from PCRE2's documented NOTEMPTY_ATSTART semantics:
    // https://www.pcre.org/current/doc/html/pcre2api.html (match option and
    // "Iterating over all matches") and PCRE2 10.46 pcre2pattern manual:
    // https://github.com/PCRE2Project/pcre2/blob/pcre2-10.46/doc/pcre2pattern.3
    // This independently written code uses the native start directive to retry
    // a nonempty alternative at the previous empty position, then lets PCRE2
    // search later Unicode positions. No upstream implementation was copied.
    // Keeping the original body intact preserves recursion and x-mode comments.
    let source = program.regex.as_str();
    let prefix = "(*NOTEMPTY_ATSTART)";
    let bytes = source
        .len()
        .checked_add(prefix.len())
        .ok_or(Failure::SizeOverflow)?;
    host.text(bytes).map_err(Failure::Host)?;
    host.work(bytes).map_err(Failure::Host)?;
    let mut retry = String::new();
    retry
        .try_reserve_exact(bytes)
        .map_err(|_| Failure::Resource)?;
    // SAFETY: The checked local reservation covers the prefix and original body.
    retry.push_str(prefix);
    // SAFETY: prefix.len()+source.len() was checked and reserved locally above.
    retry.push_str(source);
    RegexBuilder::new()
        .utf(true)
        .ucp(true)
        .caseless(program.case == Case::Insensitive)
        .build(&retry)
        .map_err(compile_failure)
}
fn expand<H: Host>(
    replacement: &str,
    text: &str,
    cursor: &Cursor<'_>,
    out: &mut TextBuffer,
    host: &mut H,
) -> Result<(), Failure<H::Failure>> {
    let mut remaining = replacement;
    while let Some(dollar) = remaining.find('$') {
        push_text(
            out,
            remaining.get(..dollar).ok_or(Failure::InvalidState)?,
            host,
        )?;
        remaining = remaining
            .get(dollar.saturating_add(1)..)
            .ok_or(Failure::InvalidState)?;
        if let Some(rest) = remaining.strip_prefix('$') {
            push_text(out, "$", host)?;
            remaining = rest;
            continue;
        }
        let digits = remaining.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            push_text(out, "$", host)?;
            continue;
        }
        let number = remaining
            .get(..digits)
            .and_then(|s| s.parse::<usize>().ok());
        if let Some(number) = number.filter(|n| *n < cursor.locations.len()) {
            push_text(out, cursor.group(text, number)?.unwrap_or(""), host)?;
        } else {
            push_text(out, "$", host)?;
            push_text(
                out,
                remaining.get(..digits).ok_or(Failure::InvalidState)?,
                host,
            )?;
        }
        remaining = remaining.get(digits..).ok_or(Failure::InvalidState)?;
    }
    push_text(out, remaining, host)
}
#[cfg(test)]
mod tests;
