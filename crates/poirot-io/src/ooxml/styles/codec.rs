//! Shared checked lexical mappings for ordinary SpreadsheetML style records.
use super::super::xml::attr;
use crate::IoError;
use poirot_model::*;
use quick_xml::events::BytesStart;

/// Exact OOXML token mapping for a supported named choice.
pub(super) trait Token: Sized {
    /// Decode an exact supported OOXML token.
    fn parse(value: &str) -> Result<Self, IoError>;
    /// Return the canonical OOXML spelling.
    fn token(&self) -> &'static str;
}
macro_rules! tokens {
    ($kind:ident { $($variant:ident => $name:literal),* $(,)? }) => {
        impl Token for $kind {
            fn parse(value: &str) -> Result<Self, IoError> {
                match value { $($name => Ok(Self::$variant),)* _ => Err(invalid(value)) }
            }
            fn token(&self) -> &'static str {
                match self { $(Self::$variant => $name,)* }
            }
        }
    };
}
tokens! { PatternType { None => "none", Solid => "solid", MediumGray => "mediumGray", DarkGray => "darkGray", LightGray => "lightGray", DarkHorizontal => "darkHorizontal", DarkVertical => "darkVertical", DarkDown => "darkDown", DarkUp => "darkUp", DarkGrid => "darkGrid", DarkTrellis => "darkTrellis", LightHorizontal => "lightHorizontal", LightVertical => "lightVertical", LightDown => "lightDown", LightUp => "lightUp", LightGrid => "lightGrid", LightTrellis => "lightTrellis", Gray125 => "gray125", Gray0625 => "gray0625" } }
tokens! { BorderLine { Hair => "hair", Thin => "thin", Medium => "medium", Thick => "thick", Dotted => "dotted", Dashed => "dashed", DashDot => "dashDot", DashDotDot => "dashDotDot", Double => "double", MediumDashed => "mediumDashed", MediumDashDot => "mediumDashDot", MediumDashDotDot => "mediumDashDotDot", SlantDashDot => "slantDashDot" } }
tokens! { HorizontalAlignment { General => "general", Left => "left", Center => "center", Right => "right", Fill => "fill", Justify => "justify", CenterContinuous => "centerContinuous", Distributed => "distributed" } }
tokens! { VerticalAlignment { Top => "top", Center => "center", Bottom => "bottom", Justify => "justify", Distributed => "distributed" } }
tokens! { FontScheme { None => "none", Major => "major", Minor => "minor" } }
tokens! { Underline { Single => "single", Double => "double" } }
tokens! { FontFamily { NotApplicable => "0", Roman => "1", Swiss => "2", Modern => "3", Script => "4", Decorative => "5" } }
tokens! { ReadingOrder { Context => "0", LeftToRight => "1", RightToLeft => "2" } }
tokens! { ThemeColor { Light1 => "0", Dark1 => "1", Light2 => "2", Dark2 => "3", Accent1 => "4", Accent2 => "5", Accent3 => "6", Accent4 => "7", Accent5 => "8", Accent6 => "9", Hyperlink => "10", FollowedHyperlink => "11" } }

/// Reject a malformed supported style value.
pub(super) fn invalid(value: impl std::fmt::Display) -> IoError {
    IoError::Read(format!("invalid SpreadsheetML style value: {value}"))
}
/// Parse a present XML boolean, retaining the element's specified default.
pub(super) fn boolean(e: &BytesStart<'_>, name: &str, default: bool) -> Result<bool, IoError> {
    match attr(e, name)?.as_deref() {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        Some(v) => Err(invalid(v)),
    }
}
/// Parse an optional typed attribute without truncation.
pub(super) fn number<T: std::str::FromStr>(
    e: &BytesStart<'_>,
    name: &str,
) -> Result<Option<T>, IoError> {
    attr(e, name)?
        .map(|v| v.parse().map_err(|_| invalid(v)))
        .transpose()
}
/// Parse an optional enumerated attribute.
pub(super) fn choice<T: Token>(e: &BytesStart<'_>, name: &str) -> Result<Option<T>, IoError> {
    attr(e, name)?.map(|v| T::parse(&v)).transpose()
}
/// Decode exact eight-digit ARGB bytes.
pub(super) fn argb(value: &str) -> Result<[u8; 4], IoError> {
    if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid(value));
    }
    Ok(u32::from_str_radix(value, 16)
        .map_err(|_| invalid(value))?
        .to_be_bytes())
}
/// Decode one color identity, refusing ambiguous source attributes.
pub(super) fn color(e: &BytesStart<'_>) -> Result<StyleColor, IoError> {
    let mut sources = Vec::new();
    if let Some(v) = attr(e, "rgb")? {
        sources.push(ColorSource::Argb(argb(&v)?));
    }
    if let Some(v) = choice(e, "theme")? {
        sources.push(ColorSource::Theme(v));
    }
    if let Some(v) = number(e, "indexed")? {
        sources.push(ColorSource::Indexed(IndexedColor::new(v)?));
    }
    if attr(e, "auto")?.is_some() && boolean(e, "auto", false)? {
        sources.push(ColorSource::Automatic);
    }
    if sources.len() != 1 {
        return Err(invalid("color requires one source"));
    }
    Ok(StyleColor {
        source: sources.pop().ok_or_else(|| invalid("missing color"))?,
        tint: number(e, "tint")?.map(Tint::new).transpose()?,
    })
}

/// Reject source strings that XML 1.0 cannot represent.
pub(super) fn valid(value: &str) -> Result<(), IoError> {
    if value.chars().any(|c| {
        !matches!(c, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
    }) {
        return Err(IoError::Unsupported("style string contains invalid XML character".into()));
    }
    Ok(())
}
