//! Spreadsheet regex operations use Rust regex by default. The opt-in pcre2
//! feature selects PCRE2 syntax and iteration semantics. Microsoft specifies PCRE2:
//! <https://support.microsoft.com/en-us/excel/functions/regexextract-function>
//! <https://support.microsoft.com/en-us/excel/functions/regexreplace-function>
//! <https://support.microsoft.com/en-us/excel/functions/regextest-function>
//! The implementation and tests are independently authored. Global matching
//! follows the selected engine's empty-match iteration rule (see Cursor below); no
//! upstream source or tests were copied.
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
/// Backend execution guards, separate from caller work estimates.
/// PCRE2 guards interrupt backtracking; consumed backend work is unavailable.
/// PCRE2 JIT remains disabled so depth and heap guards apply. Rust regex uses
/// heap_kib as its compiled-program size limit; match_steps and depth do not apply.
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
/// Caller-controlled admission before backend searches and owned allocations.
pub trait Host {
    /// Operational failure propagated unchanged to the caller.
    type Error;
    /// Backend execution guards independent of estimated caller work.
    fn execution_limits(&self) -> ExecutionLimits {
        ExecutionLimits::default()
    }
    /// Admit estimated search work before an uninterruptible native call.
    fn work(&mut self, units: usize) -> Result<(), Self::Error>;
    /// Admit the byte capacity of a single owned text buffer.
    fn text(&mut self, bytes: usize) -> Result<(), Self::Error>;
    /// Admit the logical size of a single collection.
    fn values(&mut self, count: usize) -> Result<(), Self::Error>;
}

use super::regex_backend::RegexBackend;
use super::regex_error::RegexError;
#[cfg(feature = "pcre2")]
use pcre2::bytes::{CaptureLocations, Regex, RegexBuilder};

#[cfg(feature = "pcre2")]
fn compile_failure<E>(error: pcre2::Error) -> RegexError<E> {
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
        RegexError::Pattern(error.into())
    } else {
        RegexError::Native(error.into())
    }
}
#[cfg(not(feature = "pcre2"))]
use ::regex::{Regex, RegexBuilder};

#[cfg(not(feature = "pcre2"))]
fn compile_failure<E>(error: ::regex::Error) -> RegexError<E> {
    match error {
        error @ ::regex::Error::Syntax(_) => RegexError::Pattern(error.into()),
        error @ ::regex::Error::CompiledTooBig(_) => RegexError::Native(error.into()),
        error => RegexError::Native(error.into()),
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
) -> Result<(), RegexError<H::Error>> {
    let bytes = out
        .text
        .len()
        .checked_add(text.len())
        .ok_or(RegexError::TextLimit)?;
    let units = out.units.saturating_add(text.encode_utf16().count());
    if units > 32767 {
        return Err(RegexError::TextLimit);
    }
    host.text(bytes).map_err(RegexError::Host)?;
    out.text
        .try_reserve(text.len())
        .map_err(|_| RegexError::Resource)?;
    // SAFETY: The local checked byte sum and try_reserve admit this append.
    out.text.push_str(text);
    out.units = units;
    Ok(())
}
fn copy_text<H: Host>(text: &str, host: &mut H) -> Result<String, RegexError<H::Error>> {
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
/// A compiled Unicode pattern with caller admission and backend execution guards.
/// The 8-bit backend does not represent isolated UTF-16 surrogates or Excel
/// code-unit escapes such as \C. Callers must reject malformed UTF-16 before
/// conversion; this type rejects active \C escapes instead of changing meaning.
pub struct Program {
    regex: Regex,
    #[cfg(feature = "pcre2")]
    case: Case,
}
impl Program {
    /// Syntax and matching semantics of patterns compiled by this build.
    pub const BACKEND: RegexBackend = if cfg!(feature = "pcre2") {
        RegexBackend::Pcre2
    } else {
        RegexBackend::Rust
    };

    /// Compile a pattern after caller admission, retaining backend resource guards.
    pub fn compile<H: Host>(
        pattern: &str,
        case: Case,
        host: &mut H,
    ) -> Result<Self, RegexError<H::Error>> {
        if pattern.encode_utf16().count() > 32767 {
            return Err(RegexError::TextLimit);
        }
        if active_byte_escape(pattern) {
            return Err(RegexError::ByteEscape);
        }
        let limits = host.execution_limits();
        #[cfg(feature = "pcre2")]
        let guards = if limits == ExecutionLimits::default() {
            std::borrow::Cow::Borrowed("(*LIMIT_MATCH=1000000)(*LIMIT_DEPTH=256)(*LIMIT_HEAP=8192)")
        } else {
            std::borrow::Cow::Owned(format!(
                "(*LIMIT_MATCH={})(*LIMIT_DEPTH={})(*LIMIT_HEAP={})",
                limits.match_steps, limits.depth, limits.heap_kib
            ))
        };
        #[cfg(not(feature = "pcre2"))]
        let guards = std::borrow::Cow::Borrowed("");
        let mut source = String::new();
        let bytes = guards
            .len()
            .checked_add(pattern.len())
            .ok_or(RegexError::SizeOverflow)?;
        host.text(bytes).map_err(RegexError::Host)?;
        host.work(bytes).map_err(RegexError::Host)?;
        source
            .try_reserve_exact(bytes)
            .map_err(|_| RegexError::Resource)?;
        // SAFETY: The checked local reservation covers both appended strings.
        source.push_str(&guards);
        // SAFETY: guards.len()+pattern.len() was checked and reserved locally above.
        source.push_str(pattern);
        #[cfg(feature = "pcre2")]
        let regex = RegexBuilder::new()
            .utf(true)
            .ucp(true)
            .caseless(case == Case::Insensitive)
            .build(&source)
            .map_err(compile_failure)?;
        #[cfg(not(feature = "pcre2"))]
        let regex = {
            let bytes = usize::try_from(limits.heap_kib)
                .map_err(|_| RegexError::SizeOverflow)?
                .checked_mul(1024)
                .ok_or(RegexError::SizeOverflow)?;
            RegexBuilder::new(&source)
                .case_insensitive(case == Case::Insensitive)
                .size_limit(bytes)
                .build()
                .map_err(compile_failure)?
        };
        host.values(regex.captures_len())
            .map_err(RegexError::Host)?;
        Ok(Self {
            regex,
            #[cfg(feature = "pcre2")]
            case,
        })
    }
    /// Test whether any substring matches, preserving all backend failures.
    pub fn test<H: Host>(&self, text: &str, host: &mut H) -> Result<bool, RegexError<H::Error>> {
        host.work(text.len().saturating_add(1))
            .map_err(RegexError::Host)?;
        #[cfg(feature = "pcre2")]
        {
            self.regex
                .is_match(text.as_bytes())
                .map_err(|error| RegexError::Native(error.into()))
        }
        #[cfg(not(feature = "pcre2"))]
        {
            Ok(self.regex.is_match(text))
        }
    }
    /// Extract the first match, all matches, or the first match's capture groups.
    pub fn extract<H: Host>(
        &self,
        text: &str,
        mode: Extraction,
        host: &mut H,
    ) -> Result<Vec<String>, RegexError<H::Error>> {
        let mut cursor = Cursor::new(self, text, host)?;
        let mut result = Vec::new();
        while cursor.next(host)? {
            let groups = if mode == Extraction::Groups {
                1..self.regex.captures_len()
            } else {
                0..1
            };
            for group in groups {
                let value = cursor.group(group)?.unwrap_or("");
                host.values(result.len().saturating_add(1))
                    .map_err(RegexError::Host)?;
                result.try_reserve(1).map_err(|_| RegexError::Resource)?;
                let value = copy_text(value, host)?;
                // SAFETY: One additional element was reserved immediately above.
                result.push(value);
            }
            if mode != Extraction::All {
                break;
            }
        }
        if result.is_empty() {
            Err(RegexError::NoMatch)
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
    ) -> Result<String, RegexError<H::Error>> {
        let selected = if occurrence < 0 {
            let mut cursor = Cursor::new(self, text, host)?;
            let mut count = 0u64;
            while cursor.next(host)? {
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
        let mut cursor = Cursor::new(self, text, host)?;
        let mut out = TextBuffer::default();
        let mut copied = 0;
        let mut ordinal = 0u64;
        while cursor.next(host)? {
            ordinal = ordinal.saturating_add(1);
            if occurrence != 0 && selected != Some(ordinal) {
                continue;
            }
            let (start, end) = cursor.range(0).ok_or(RegexError::InvalidState)?;
            push_text(
                &mut out,
                text.get(copied..start).ok_or(RegexError::MatchBoundary)?,
                host,
            )?;
            expand(replacement, &cursor, &mut out, host)?;
            copied = end;
            if occurrence != 0 {
                break;
            }
        }
        push_text(
            &mut out,
            text.get(copied..).ok_or(RegexError::MatchBoundary)?,
            host,
        )?;
        Ok(out.text)
    }
}
/// State for successive regex matches, reusing capture storage between searches.
/// It handles empty-match retries and advances on Unicode boundaries so global extraction or replacement makes progress.
struct Cursor<'a, 's> {
    program: &'a Program,
    text: &'s str,
    #[cfg(feature = "pcre2")]
    locations: CaptureLocations,
    #[cfg(not(feature = "pcre2"))]
    matches: ::regex::CaptureMatches<'a, 's>,
    #[cfg(not(feature = "pcre2"))]
    captures: Option<::regex::Captures<'s>>,
    offset: Option<usize>,
    #[cfg(feature = "pcre2")]
    retry_empty: bool,
    #[cfg(feature = "pcre2")]
    retry_regex: Option<Regex>,
}
impl<'a, 's> Cursor<'a, 's> {
    fn new<H: Host>(
        program: &'a Program,
        text: &'s str,
        host: &mut H,
    ) -> Result<Self, RegexError<H::Error>> {
        host.values(program.regex.captures_len())
            .map_err(RegexError::Host)?;
        Ok(Self {
            program,
            text,
            #[cfg(feature = "pcre2")]
            locations: program.regex.capture_locations(),
            #[cfg(not(feature = "pcre2"))]
            matches: program.regex.captures_iter(text),
            #[cfg(not(feature = "pcre2"))]
            captures: None,
            offset: Some(0),
            #[cfg(feature = "pcre2")]
            retry_empty: false,
            #[cfg(feature = "pcre2")]
            retry_regex: None,
        })
    }
    fn next<H: Host>(&mut self, host: &mut H) -> Result<bool, RegexError<H::Error>> {
        let Some(offset) = self.offset else {
            return Ok(false);
        };
        host.work(self.text.len().saturating_sub(offset).saturating_add(1))
            .map_err(RegexError::Host)?;
        #[cfg(feature = "pcre2")]
        let found = {
            let regex = if self.retry_empty {
                if self.retry_regex.is_none() {
                    self.retry_regex = Some(retry_regex(self.program, host)?);
                }
                self.retry_regex.as_ref().ok_or(RegexError::InvalidState)?
            } else {
                &self.program.regex
            };
            regex
                .captures_read_at(&mut self.locations, self.text.as_bytes(), offset)
                .map_err(|error| RegexError::Native(error.into()))?
                .map(|found| (found.start(), found.end()))
        };
        #[cfg(not(feature = "pcre2"))]
        let found = {
            self.captures = self.matches.next();
            self.captures
                .as_ref()
                .and_then(|captures| captures.get(0))
                .map(|found| (found.start(), found.end()))
        };
        let Some((start, end)) = found else {
            self.offset = None;
            return Ok(false);
        };
        if start > end || !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
            return Err(RegexError::MatchBoundary);
        }
        #[cfg(feature = "pcre2")]
        {
            if self.retry_empty && end <= offset {
                return Err(RegexError::InvalidState);
            }
            self.retry_empty = start == end;
        }
        self.offset = Some(end);
        Ok(true)
    }
    fn range(&self, group: usize) -> Option<(usize, usize)> {
        #[cfg(feature = "pcre2")]
        {
            self.locations.get(group)
        }
        #[cfg(not(feature = "pcre2"))]
        {
            self.captures
                .as_ref()?
                .get(group)
                .map(|found| (found.start(), found.end()))
        }
    }
    fn group<E>(&self, group: usize) -> Result<Option<&'s str>, RegexError<E>> {
        self.range(group)
            .map(|(start, end)| self.text.get(start..end).ok_or(RegexError::CaptureBoundary))
            .transpose()
    }
}
#[cfg(feature = "pcre2")]
fn retry_regex<H: Host>(program: &Program, host: &mut H) -> Result<Regex, RegexError<H::Error>> {
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
        .ok_or(RegexError::SizeOverflow)?;
    host.text(bytes).map_err(RegexError::Host)?;
    host.work(bytes).map_err(RegexError::Host)?;
    let mut retry = String::new();
    retry
        .try_reserve_exact(bytes)
        .map_err(|_| RegexError::Resource)?;
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
    cursor: &Cursor<'_, '_>,
    out: &mut TextBuffer,
    host: &mut H,
) -> Result<(), RegexError<H::Error>> {
    let mut remaining = replacement;
    while let Some(dollar) = remaining.find('$') {
        push_text(
            out,
            remaining.get(..dollar).ok_or(RegexError::InvalidState)?,
            host,
        )?;
        remaining = remaining
            .get(dollar.saturating_add(1)..)
            .ok_or(RegexError::InvalidState)?;
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
        if let Some(number) = number.filter(|n| *n < cursor.program.regex.captures_len()) {
            push_text(out, cursor.group(number)?.unwrap_or(""), host)?;
        } else {
            push_text(out, "$", host)?;
            push_text(
                out,
                remaining.get(..digits).ok_or(RegexError::InvalidState)?,
                host,
            )?;
        }
        remaining = remaining.get(digits..).ok_or(RegexError::InvalidState)?;
    }
    push_text(out, remaining, host)
}
#[cfg(all(test, feature = "pcre2"))]
mod tests;

#[cfg(all(test, not(feature = "pcre2")))]
#[path = "regex/rust_tests.rs"]
mod rust_tests;
