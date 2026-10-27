//! Cell presentation and display formatting.
//!
//! ```
//! use poirot::{CellValue, styles::{compile_format, render_value, FormatCompilePolicy, FormatRenderContext}};
//! let plan = compile_format("0.00", FormatCompilePolicy::default())?;
//! let display = render_value(&CellValue::Number(12.5), &plan, FormatRenderContext::default())?;
//! assert_eq!(display.text, "12.50");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
pub use crate::model::{
    Alignment, BorderLine, BorderSide, Borders, CellStyle, ColorSource, FontFamily, FontScheme,
    FontSize, FontStyle, HorizontalAlignment, IndexedColor, PatternFill, PatternType, ReadingOrder,
    RotationDegrees, SheetPresentation, SheetVisibility, StyleColor, StyleContext, TextRotation,
    ThemeColor, Tint, Underline, VerticalAlignment, VisualStyle,
};
pub use poirot_functions::format::{
    FormatCompileError, FormatCompilePolicy, FormatError, FormatErrorKind, FormatLocale,
    FormatPlan, FormatRenderContext, FormattedValue, compile_format, render_value,
};
