/// Text construction, parsing, formatting, and matching handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TextFunction {
    /// NUMBERVALUE handler.
    NumberValue,
    /// PROPER handler.
    Proper,
    /// ARABIC handler.
    Arabic,
    /// ROMAN handler.
    Roman,
    /// TEXTJOIN handler.
    TextJoin,

    /// TEXT handler.
    Text,
    /// LEN handler.
    Len,
    /// LEFT handler.
    Left,
    /// RIGHT handler.
    Right,
    /// MID handler.
    Mid,
    /// LOWER handler.
    Lower,
    /// UPPER handler.
    Upper,
    /// TRIM handler.
    Trim,
    /// CLEAN handler.
    Clean,
    /// CONCATENATE handler.
    Concatenate,
    /// CONCAT handler.
    Concat,
    /// EXACT handler.
    Exact,
    /// FIND handler.
    Find,
    /// SEARCH handler.
    Search,
    /// REPLACE handler.
    Replace,
    /// SUBSTITUTE handler.
    Substitute,
    /// REPT handler.
    Rept,
    /// VALUE handler.
    Value,
    /// CHAR handler.
    Char,
    /// CODE handler.
    Code,
    /// UNICHAR handler.
    UniChar,
    /// UNICODE handler.
    Unicode,
    /// TEXTBEFORE delimiter extraction.
    TextBefore,
    /// TEXTAFTER delimiter extraction.
    TextAfter,
    /// TEXTSPLIT rectangular delimiter splitting.
    TextSplit,
    /// ARRAYTOTEXT aggregate serialization.
    ArrayToText,
    /// VALUETOTEXT elementwise serialization.
    ValueToText,
    /// REGEXTEST Unicode PCRE2 matching.
    RegexTest,
    /// REGEXEXTRACT text or capture extraction.
    RegexExtract,
    /// REGEXREPLACE occurrence-aware replacement.
    RegexReplace,
}
