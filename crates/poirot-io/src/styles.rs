//! Shared visual presentation independent of file formats and calculation.
use crate::{CellStyle, ModelError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Positive finite font size in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct FontSize(u64);
impl FontSize {
    /// Validate a positive finite point size.
    pub fn new(value: f64) -> Result<Self, ModelError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(ModelError("invalid FontSize".into()));
        }
        Ok(Self(value.to_bits()))
    }
    /// Return the validated value.
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}
impl TryFrom<f64> for FontSize {
    type Error = ModelError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<FontSize> for f64 {
    fn from(value: FontSize) -> Self {
        value.get()
    }
}

/// Finite color tint between minus one and one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Tint(u64);
impl Tint {
    /// Validate a numeric presentation value, normalizing signed zero.
    pub fn new(value: f64) -> Result<Self, ModelError> {
        if !value.is_finite() || !((-1.0..=1.0).contains(&value)) {
            return Err(ModelError("invalid Tint".into()));
        }
        Ok(Self(if value == 0.0 {
            0.0f64.to_bits()
        } else {
            value.to_bits()
        }))
    }
    /// Return the validated value.
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}
impl TryFrom<f64> for Tint {
    type Error = ModelError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<Tint> for f64 {
    fn from(value: Tint) -> Self {
        value.get()
    }
}

/// Workbook theme color slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeColor {
    Light1,
    Dark1,
    Light2,
    Dark2,
    Accent1,
    Accent2,
    Accent3,
    Accent4,
    Accent5,
    Accent6,
    Hyperlink,
    FollowedHyperlink,
}

/// Font underline decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Underline {
    Single,
    Double,
}

/// Theme font family selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontScheme {
    None,
    Major,
    Minor,
}

/// Generic font family classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontFamily {
    NotApplicable,
    Roman,
    Swiss,
    Modern,
    Script,
    Decorative,
}

/// Cell fill pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternType {
    None,
    Solid,
    MediumGray,
    DarkGray,
    LightGray,
    DarkHorizontal,
    DarkVertical,
    DarkDown,
    DarkUp,
    DarkGrid,
    DarkTrellis,
    LightHorizontal,
    LightVertical,
    LightDown,
    LightUp,
    LightGrid,
    LightTrellis,
    Gray125,
    Gray0625,
}

/// Border line appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BorderLine {
    Hair,
    Thin,
    Medium,
    Thick,
    Dotted,
    Dashed,
    DashDot,
    DashDotDot,
    Double,
    MediumDashed,
    MediumDashDot,
    MediumDashDotDot,
    SlantDashDot,
}

/// Horizontal placement within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HorizontalAlignment {
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterContinuous,
    Distributed,
}

/// Vertical placement within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
    Justify,
    Distributed,
}

/// Direction used to order cell text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReadingOrder {
    Context,
    LeftToRight,
    RightToLeft,
}

/// Validated indexed-palette entry, including automatic foreground/background slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct IndexedColor(u8);
impl IndexedColor {
    /// Validate an Excel palette index from zero through sixty-five.
    pub fn new(value: u8) -> Result<Self, ModelError> {
        if value > 65 {
            return Err(ModelError("invalid indexed color".into()));
        }
        Ok(Self(value))
    }
    /// Return the palette index.
    pub fn get(self) -> u8 {
        self.0
    }
}
impl TryFrom<u8> for IndexedColor {
    type Error = ModelError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<IndexedColor> for u8 {
    fn from(value: IndexedColor) -> Self {
        value.get()
    }
}
/// Color source identity; theme and palette references are never resolved to RGB here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSource {
    /// Alpha, red, green and blue bytes in source order.
    Argb([u8; 4]),
    /// Theme slot in the workbook style context.
    Theme(ThemeColor),
    /// Indexed palette reference.
    Indexed(IndexedColor),
    /// Application-selected automatic color.
    Automatic,
}
/// A cell color whose RGB, theme, or indexed source identity survives import and export.
/// Tint remains separate so adapters can preserve the source choice rather than flattening it to RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StyleColor {
    pub source: ColorSource,
    pub tint: Option<Tint>,
}
/// Cell text rotation, including vertically stacked characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextRotation {
    /// Validated angle from minus ninety through ninety degrees.
    Degrees(RotationDegrees),
    /// Characters stacked vertically.
    Stacked,
}
/// Validated cell-text rotation from minus 90 through 90 degrees.
/// Stacked text uses a separate `TextRotation` variant rather than an out-of-range angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "i16", into = "i16")]
pub struct RotationDegrees(i16);
impl RotationDegrees {
    /// Validate a rotation angle in degrees.
    pub fn new(value: i16) -> Result<Self, ModelError> {
        if !(-90..=90).contains(&value) {
            return Err(ModelError("invalid rotation".into()));
        }
        Ok(Self(value))
    }
    /// Return the angle in degrees.
    pub fn get(self) -> i16 {
        self.0
    }
}
impl TryFrom<i16> for RotationDegrees {
    type Error = ModelError;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<RotationDegrees> for i16 {
    fn from(value: RotationDegrees) -> Self {
        value.get()
    }
}
/// Common font presentation; absent fields retain the application default.
/// See [`VisualStyle`] for applying a bold font to a cell without changing its value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FontStyle {
    /// Font family name shared across distinct source style records.
    pub name: Option<Arc<str>>,
    pub size: Option<FontSize>,
    pub bold: bool,
    pub italic: bool,
    pub underline: Option<Underline>,
    pub strike: bool,
    pub color: Option<StyleColor>,
    pub scheme: Option<FontScheme>,
    pub family: Option<FontFamily>,
    pub charset: Option<u8>,
}
/// Cell-fill pattern retaining foreground and background color identities separately.
/// Keeping both colors preserves patterned fills instead of reducing them to a single resolved color.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternFill {
    pub pattern: PatternType,
    pub foreground: Option<StyleColor>,
    pub background: Option<StyleColor>,
}
/// Line and color choices for one cell-border edge, preserved independently.
/// A retained color does not imply a visible line when the line style is absent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BorderSide {
    pub line: Option<BorderLine>,
    pub color: Option<StyleColor>,
}
/// Per-edge border presentation allowing each side to retain its own line and color.
/// An absent side stays distinct from a present side with unspecified styling.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Borders {
    pub left: Option<BorderSide>,
    pub right: Option<BorderSide>,
    pub top: Option<BorderSide>,
    pub bottom: Option<BorderSide>,
}
/// Cell-text placement and layout settings retained as presentation metadata.
/// Adapters preserve these choices independently of the cell's value and number format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Alignment {
    pub horizontal: Option<HorizontalAlignment>,
    pub vertical: Option<VerticalAlignment>,
    pub wrap_text: bool,
    pub shrink_to_fit: bool,
    pub rotation: Option<TextRotation>,
    pub indent: u16,
    pub reading_order: Option<ReadingOrder>,
}
/// Immutable visual payload shared by cells; equality compares presentation values.
///
/// ```rust
/// use poirot_io::{Cell, CellStyle, CellValue, FontStyle, VisualStyle};
/// use std::sync::Arc;
/// let visual = Arc::new(VisualStyle {
///     font: Some(FontStyle { bold: true, ..Default::default() }),
///     ..Default::default()
/// });
/// let stored = Cell {
///     value: CellValue::Number(12.5),
///     style: CellStyle { number_format: Some("0.00".into()), visual: Some(visual.clone()) },
///     ..Default::default()
/// };
/// assert_eq!(stored.value, CellValue::Number(12.5));
/// assert_eq!(stored.style.number_format.as_deref(), Some("0.00"));
/// assert_eq!(stored.style.visual.as_deref(), Some(visual.as_ref()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct VisualStyle {
    pub font: Option<FontStyle>,
    pub fill: Option<PatternFill>,
    pub borders: Option<Borders>,
    pub alignment: Option<Alignment>,
}
impl VisualStyle {
    /// Count logical retained text bytes, independently of Arc sharing.
    pub fn text_bytes(&self) -> usize {
        self.font
            .as_ref()
            .and_then(|font| font.name.as_ref())
            .map_or(0, |name| name.len())
    }
}
/// Bounded workbook theme bytes and custom indexed colors used by source identities.
/// Theme XML validity is checked by file adapters, not this format-independent model.
///
/// ```rust
/// use poirot_io::{CellStyle, StyleContext};
/// let context = StyleContext::new(None, vec![[255, 0, 0, 255]])?
///     .with_default_style(CellStyle { number_format: Some("0.00".into()), ..Default::default() });
/// assert_eq!(context.default_style().number_format.as_deref(), Some("0.00"));
/// assert_eq!(context.indexed_palette(), &[[255, 0, 0, 255]]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "StyleContextWire", into = "StyleContextWire")]
pub struct StyleContext {
    theme: Option<Arc<[u8]>>,
    indexed_palette: Arc<[[u8; 4]]>,
    default_style: CellStyle,
}
/// Serde transport validated before constructing retained workbook context.
#[derive(Serialize, Deserialize)]
struct StyleContextWire(Option<ThemeBytes>, Arc<[[u8; 4]]>, CellStyle);
/// Byte-string transport keeps theme payloads covered by native decoded-byte quotas.
struct ThemeBytes(Arc<[u8]>);
impl Serialize for ThemeBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde_bytes::Bytes::new(&self.0).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ThemeBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = serde_bytes::ByteBuf::deserialize(deserializer)?;
        Ok(Self(Arc::from(bytes.into_vec())))
    }
}
impl StyleContext {
    /// Retain theme data and at most sixty-four custom palette entries.
    pub fn new(theme: Option<Vec<u8>>, indexed_palette: Vec<[u8; 4]>) -> Result<Self, ModelError> {
        Self::try_from(StyleContextWire(
            theme.map(|bytes| ThemeBytes(Arc::from(bytes))),
            Arc::from(indexed_palette),
            CellStyle::default(),
        ))
    }
    /// Set the default style used for unstored cells by file consumers.
    pub fn with_default_style(mut self, style: CellStyle) -> Self {
        self.default_style = style;
        self
    }
    /// Borrow the workbook default cell presentation without expanding absent cells.
    pub fn default_style(&self) -> &CellStyle {
        &self.default_style
    }
    /// Borrow preserved theme metadata bytes.
    pub fn theme(&self) -> Option<&[u8]> {
        self.theme.as_deref()
    }
    /// Borrow custom indexed colors in palette order.
    pub fn indexed_palette(&self) -> &[[u8; 4]] {
        &self.indexed_palette
    }
    /// Count retained metadata bytes for logical resource budgets.
    pub fn retained_bytes(&self) -> Option<usize> {
        self.theme
            .as_ref()
            .map_or(0, |bytes| bytes.len())
            .checked_add(self.indexed_palette.len().checked_mul(4)?)?
            .checked_add(self.default_style.text_bytes()?)
    }
}
impl TryFrom<StyleContextWire> for StyleContext {
    type Error = ModelError;
    fn try_from(value: StyleContextWire) -> Result<Self, Self::Error> {
        if value.1.len() > 64 {
            return Err(ModelError("style context limit".into()));
        }
        Ok(Self {
            theme: value.0.map(|bytes| bytes.0),
            indexed_palette: value.1,
            default_style: value.2,
        })
    }
}
impl From<StyleContext> for StyleContextWire {
    fn from(value: StyleContext) -> Self {
        Self(
            value.theme.map(ThemeBytes),
            value.indexed_palette,
            value.default_style,
        )
    }
}
