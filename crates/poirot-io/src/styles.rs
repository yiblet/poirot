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
            return Err(ModelError::InvalidFontSize { value });
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

/// Finite color tint between minus one (black) and one (white), with zero unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Tint(u64);
impl Tint {
    /// Validate a numeric presentation value, normalizing signed zero.
    pub fn new(value: f64) -> Result<Self, ModelError> {
        if !value.is_finite() || !((-1.0..=1.0).contains(&value)) {
            return Err(ModelError::InvalidTint { value });
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
    /// Primary light theme slot.
    Light1,
    /// Primary dark theme slot.
    Dark1,
    /// Secondary light theme slot.
    Light2,
    /// Secondary dark theme slot.
    Dark2,
    /// Theme accent slot 1.
    Accent1,
    /// Theme accent slot 2.
    Accent2,
    /// Theme accent slot 3.
    Accent3,
    /// Theme accent slot 4.
    Accent4,
    /// Theme accent slot 5.
    Accent5,
    /// Theme accent slot 6.
    Accent6,
    /// Theme color for unvisited hyperlinks.
    Hyperlink,
    /// Theme color for visited hyperlinks.
    FollowedHyperlink,
}

/// Font underline decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Underline {
    /// One underline beneath the text.
    Single,
    /// Two underlines beneath the text.
    Double,
}

/// Theme font family selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontScheme {
    /// Explicitly select no theme font scheme.
    None,
    /// Use the theme heading font.
    Major,
    /// Use the theme body font.
    Minor,
}

/// Generic font family classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontFamily {
    /// No generic family classification.
    NotApplicable,
    /// Proportional serif family.
    Roman,
    /// Proportional sans-serif family.
    Swiss,
    /// Fixed-width family.
    Modern,
    /// Handwriting or script family.
    Script,
    /// Decorative display family.
    Decorative,
}

/// Cell fill pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternType {
    /// No fill pattern.
    None,
    /// Solid foreground color.
    Solid,
    /// Medium-density gray stipple.
    MediumGray,
    /// Dense gray stipple.
    DarkGray,
    /// Sparse gray stipple.
    LightGray,
    /// Dense horizontal stripes.
    DarkHorizontal,
    /// Dense vertical stripes.
    DarkVertical,
    /// Dense downward diagonal stripes.
    DarkDown,
    /// Dense upward diagonal stripes.
    DarkUp,
    /// Dense horizontal and vertical crosshatch.
    DarkGrid,
    /// Dense diagonal crosshatch.
    DarkTrellis,
    /// Sparse horizontal stripes.
    LightHorizontal,
    /// Sparse vertical stripes.
    LightVertical,
    /// Sparse downward diagonal stripes.
    LightDown,
    /// Sparse upward diagonal stripes.
    LightUp,
    /// Sparse horizontal and vertical crosshatch.
    LightGrid,
    /// Sparse diagonal crosshatch.
    LightTrellis,
    /// Stipple covering 12.5 percent of the background.
    Gray125,
    /// Stipple covering 6.25 percent of the background.
    Gray0625,
}

/// Border line appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BorderLine {
    /// Hairline stroke.
    Hair,
    /// Thin continuous stroke.
    Thin,
    /// Medium continuous stroke.
    Medium,
    /// Thick continuous stroke.
    Thick,
    /// Dotted stroke.
    Dotted,
    /// Dashed stroke.
    Dashed,
    /// Alternating dash and dot stroke.
    DashDot,
    /// Alternating dash and two dots.
    DashDotDot,
    /// Two parallel strokes.
    Double,
    /// Medium-weight dashed stroke.
    MediumDashed,
    /// Medium-weight alternating dash and dot.
    MediumDashDot,
    /// Medium-weight alternating dash and two dots.
    MediumDashDotDot,
    /// Slanted dash-and-dot stroke.
    SlantDashDot,
}

/// Horizontal placement within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HorizontalAlignment {
    /// Application-selected alignment based on cell content.
    General,
    /// Align to the left edge.
    Left,
    /// Center within the cell.
    Center,
    /// Align to the right edge.
    Right,
    /// Repeat the cell text to fill its width.
    Fill,
    /// Justify text between the left and right edges.
    Justify,
    /// Center across adjacent cells without merging their storage.
    CenterContinuous,
    /// Distribute text spacing across the cell width.
    Distributed,
}

/// Vertical placement within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerticalAlignment {
    /// Align to the top edge.
    Top,
    /// Center between the top and bottom edges.
    Center,
    /// Align to the bottom edge.
    Bottom,
    /// Justify lines between the top and bottom edges.
    Justify,
    /// Distribute lines across the cell height.
    Distributed,
}

/// Direction used to order cell text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReadingOrder {
    /// Infer text direction from its content.
    Context,
    /// Order text from left to right.
    LeftToRight,
    /// Order text from right to left.
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
            return Err(ModelError::InvalidIndexedColor { value });
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
    /// Original ARGB, theme, palette, or automatic color identity.
    pub source: ColorSource,
    /// Optional source tint: negative darkens toward black, positive lightens toward white; absent means no tint attribute.
    pub tint: Option<Tint>,
}
/// Cell text rotation, including vertically stacked characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextRotation {
    /// Validated angle from minus ninety through ninety degrees; positive angles rotate upward.
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
            return Err(ModelError::InvalidRotation { value });
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
/// Common font presentation; absent optional fields retain the application default.
/// Boolean flags default to false and underline defaults to absent.
/// See [`VisualStyle`] for applying a bold font to a cell without changing its value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FontStyle {
    /// Font family name shared across distinct source style records.
    pub name: Option<Arc<str>>,
    /// Optional positive font size in points; absent leaves the application default.
    pub size: Option<FontSize>,
    /// Use a bold face; false is the default.
    pub bold: bool,
    /// Use an italic face; false is the default.
    pub italic: bool,
    /// Underline decoration; absent means no underline.
    pub underline: Option<Underline>,
    /// Draw a strike-through line; false is the default.
    pub strike: bool,
    /// Optional text color retaining its source identity.
    pub color: Option<StyleColor>,
    /// Optional theme font selection; absent differs from explicit `FontScheme::None`.
    pub scheme: Option<FontScheme>,
    /// Optional generic family classification, separate from the concrete font name.
    pub family: Option<FontFamily>,
    /// Optional source character-set code (the OOXML font charset byte), not a Rust text encoding.
    pub charset: Option<u8>,
}
/// Cell-fill pattern retaining foreground and background color identities separately.
/// Keeping both colors preserves patterned fills instead of reducing them to a single resolved color.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternFill {
    /// Pattern drawn using the foreground and background colors.
    pub pattern: PatternType,
    /// Optional pattern ink color; supplies the visible color for a solid fill.
    pub foreground: Option<StyleColor>,
    /// Optional color behind the pattern; retained separately even for solid fills.
    pub background: Option<StyleColor>,
}
/// Line and color choices for one cell-border edge, preserved independently.
/// A retained color does not imply a visible line when the line style is absent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BorderSide {
    /// Optional stroke appearance; absent means no visible border line.
    pub line: Option<BorderLine>,
    /// Optional source color, retained even when no line is specified.
    pub color: Option<StyleColor>,
}
/// Per-edge border presentation allowing each side to retain its own line and color.
/// An absent side stays distinct from a present side with unspecified styling.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Borders {
    /// Optional left cell edge; absent differs from an explicit edge with no line.
    pub left: Option<BorderSide>,
    /// Optional right cell edge; absent differs from an explicit edge with no line.
    pub right: Option<BorderSide>,
    /// Optional top cell edge; absent differs from an explicit edge with no line.
    pub top: Option<BorderSide>,
    /// Optional bottom cell edge; absent differs from an explicit edge with no line.
    pub bottom: Option<BorderSide>,
}
/// Cell-text placement and layout settings retained as presentation metadata.
/// Adapters preserve these choices independently of the cell's value and number format.
/// Optional settings default to absent; boolean flags are false and indent is zero.
/// These records describe presentation; Poirot does not lay out or render text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Alignment {
    /// Optional horizontal placement; absent leaves the application default.
    pub horizontal: Option<HorizontalAlignment>,
    /// Optional vertical placement; absent leaves the application default.
    pub vertical: Option<VerticalAlignment>,
    /// Wrap text within the cell width; false is the default.
    pub wrap_text: bool,
    /// Allow the application to reduce text size to fit; false is the default.
    pub shrink_to_fit: bool,
    /// Optional angle or stacked characters; absent leaves rotation unspecified.
    pub rotation: Option<TextRotation>,
    /// Indent level in spreadsheet character-based units; zero is the default. The model retains any `u16` value.
    pub indent: u16,
    /// Optional text direction; absent differs from explicit content-based `ReadingOrder::Context`.
    pub reading_order: Option<ReadingOrder>,
}
/// Immutable visual payload shared by cells; equality compares presentation values.
/// All components are absent by default; this is a complete retained payload, not
/// a patch that merges each field with an earlier style.
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
    /// Optional font component; absent supplies no font override.
    pub font: Option<FontStyle>,
    /// Optional fill component; absent differs from an explicit `PatternType::None` fill.
    pub fill: Option<PatternFill>,
    /// Optional border component; absent supplies no border override.
    pub borders: Option<Borders>,
    /// Optional alignment component; absent supplies no layout override.
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
    /// Borrow custom indexed colors in palette order, each as alpha, red, green, blue bytes.
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
            return Err(ModelError::StyleContextLimit);
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
