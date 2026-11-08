/// Regex syntax and matching semantics selected when compiling this library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegexBackend {
    /// Excel's PCRE2-compatible syntax and empty-match retry behavior.
    Pcre2,
    /// Rust regex syntax and non-overlapping Unicode iteration.
    Rust,
}
