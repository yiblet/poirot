//! Indexed source style records; component IDs describe the direct cell XF.
use super::codec::{Token, argb, boolean, choice, color, number, parse_boolean, parse_number};
use super::component_ids::ComponentIds;
use super::metadata_xml::MetadataXml;
use crate::IoError;
use crate::ooxml::xml::{attr, xml};
use crate::{
    Alignment, BorderLine, BorderSide, Borders, CellStyle, FontSize, FontStyle, ImportDiagnostic,
    ImportDiagnosticKind, PatternFill, PatternType, RotationDegrees, StyleContext, TextRotation,
    Underline, VisualStyle,
};
use quick_xml::events::{BytesStart, Event, attributes::Attribute};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

/// Styles resolved from OOXML component tables, paired with their workbook color context.
/// The cell-style vector preserves XF order so worksheet style indices select the
/// correct shared presentation.
pub(crate) struct ImportedStyles {
    pub(crate) cells: Vec<CellStyle>,
    pub(crate) context: StyleContext,
}
/// Unresolved OOXML cell-format record holding indices into the style component tables.
/// References are checked before constructing shared visual styles, allowing invalid
/// style metadata to fail without inventing presentation values.
#[derive(Default)]
struct Xf {
    font: usize,
    fill: usize,
    border: usize,
    number: u32,
    parent: Option<usize>,
    alignment: Option<Alignment>,
}
/// Append a source record with fallible vector growth, ignoring declared counts.
fn push<T>(records: &mut Vec<T>, value: T) -> Result<(), IoError> {
    records
        .try_reserve(1)
        .map_err(|source| IoError::Allocation(crate::AllocationError::StyleTable(source)))?;
    records.push(value);
    Ok(())
}
/// Visit direct children, letting each visitor consume its own subtree.
fn children(
    reader: &mut MetadataXml<'_>,
    mut visit: impl FnMut(&mut MetadataXml<'_>, &BytesStart<'_>) -> Result<(), IoError>,
) -> Result<(), IoError> {
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                if reader.in_namespace() {
                    visit(reader, &e)?;
                } else {
                    reader.skip_subtree()?;
                }
            }
            Event::End(_) => return Ok(()),
            Event::Eof => {
                return Err(IoError::Style(
                    crate::StyleDecodeError::TruncatedStyleRecord,
                ));
            }
            _ => {}
        }
    }
}
/// Skip an unrepresented record and retain one warning per element family.
fn skip(
    reader: &mut MetadataXml<'_>,
    e: &BytesStart<'_>,
    losses: &mut BTreeSet<String>,
) -> Result<(), IoError> {
    losses.insert(e.local_name().as_ref().to_string());
    reader.skip_subtree()?;
    Ok(())
}
/// Validate authored or imported DrawingML themes before retaining or packaging bytes.
pub(crate) fn validate_theme(bytes: &[u8]) -> Result<(), IoError> {
    MetadataXml::new(
        bytes,
        "theme",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
        false,
    )
    .validate()
}
/// Decode one ordinary font record without resolving theme or indexed colors.
fn font(reader: &mut MetadataXml<'_>, losses: &mut BTreeSet<String>) -> Result<FontStyle, IoError> {
    let mut f = FontStyle::default();
    children(reader, |r, e| {
        match e.local_name().as_ref() {
            "name" => {
                f.name = Some(Arc::from(attr(e, "val")?.ok_or({
                    IoError::Style(crate::StyleDecodeError::MissingFontName)
                })?))
            }
            "sz" => {
                f.size = Some(FontSize::new(number(e, "val")?.ok_or({
                    IoError::Style(crate::StyleDecodeError::MissingFontSize)
                })?)?)
            }
            "b" => f.bold = boolean(e, "val", true)?,
            "i" => f.italic = boolean(e, "val", true)?,
            "strike" => f.strike = boolean(e, "val", true)?,
            "u" => {
                f.underline = match attr(e, "val")?.as_deref() {
                    None => Some(Underline::Single),
                    Some("none") => None,
                    Some("singleAccounting" | "doubleAccounting") => {
                        losses.insert("accounting underline".into());
                        None
                    }
                    Some(v) => Some(Underline::parse(v)?),
                }
            }
            "color" => f.color = Some(color(e)?),
            "scheme" => {
                f.scheme = Some(
                    choice(e, "val")?
                        .ok_or(IoError::Style(crate::StyleDecodeError::MissingFontScheme))?,
                )
            }
            "family" => {
                f.family = Some(
                    choice(e, "val")?
                        .ok_or(IoError::Style(crate::StyleDecodeError::MissingFontFamily))?,
                )
            }
            "charset" => {
                f.charset = Some(
                    number(e, "val")?
                        .ok_or(IoError::Style(crate::StyleDecodeError::MissingFontCharset))?,
                )
            }
            _ => return skip(r, e, losses),
        }
        r.skip_subtree()?;
        Ok(())
    })?;
    Ok(f)
}
/// Decode a pattern fill, reporting gradients rather than claiming preservation.
fn fill(
    reader: &mut MetadataXml<'_>,
    losses: &mut BTreeSet<String>,
) -> Result<Option<PatternFill>, IoError> {
    let mut result = None;
    children(reader, |r, e| {
        if e.local_name().as_ref() != "patternFill" {
            return skip(r, e, losses);
        }
        let mut fill = PatternFill {
            pattern: choice(e, "patternType")?.unwrap_or(PatternType::None),
            foreground: None,
            background: None,
        };
        children(r, |r, e| {
            match e.local_name().as_ref() {
                "fgColor" => fill.foreground = Some(color(e)?),
                "bgColor" => fill.background = Some(color(e)?),
                _ => return skip(r, e, losses),
            }
            r.skip_subtree()?;
            Ok(())
        })?;
        result = Some(fill);
        Ok(())
    })?;
    Ok(result)
}
/// Decode four independent edges; omitted edge records remain omitted.
fn borders(
    reader: &mut MetadataXml<'_>,
    start: &BytesStart<'_>,
    losses: &mut BTreeSet<String>,
) -> Result<Borders, IoError> {
    let mut b = Borders::default();
    let diagonal_up = boolean(start, "diagonalUp", false)?;
    let diagonal_down = boolean(start, "diagonalDown", false)?;
    if diagonal_up || diagonal_down {
        losses.insert("diagonal border".into());
    }
    if attr(start, "outline")?.is_some() {
        boolean(start, "outline", true)?;
        losses.insert("border outline".into());
    }
    children(reader, |r, e| {
        let side = match e.local_name().as_ref() {
            "left" => &mut b.left,
            "right" => &mut b.right,
            "top" => &mut b.top,
            "bottom" => &mut b.bottom,
            _ => return skip(r, e, losses),
        };
        let mut s = BorderSide {
            line: match attr(e, "style")?.as_deref() {
                None | Some("none") => None,
                Some(v) => Some(BorderLine::parse(v)?),
            },
            color: None,
        };
        children(r, |r, e| {
            if e.local_name().as_ref() != "color" {
                return skip(r, e, losses);
            }
            s.color = Some(color(e)?);
            r.skip_subtree()?;
            Ok(())
        })?;
        *side = Some(s);
        Ok(())
    })?;
    Ok(b)
}
/// Decode the supported alignment attributes, checking their numeric domains.
fn alignment(e: &BytesStart<'_>, losses: &mut BTreeSet<String>) -> Result<Alignment, IoError> {
    for a in e.attributes() {
        let a = a.map_err(xml)?;
        if !matches!(
            a.key.as_ref(),
            "horizontal"
                | "vertical"
                | "wrapText"
                | "shrinkToFit"
                | "textRotation"
                | "indent"
                | "readingOrder"
        ) {
            losses.insert("alignment attributes".into());
        }
    }
    let rotation = number::<u16>(e, "textRotation")?
        .map(|v| match v {
            0..=90 => Ok(TextRotation::Degrees(RotationDegrees::new(v as i16)?)),
            91..=180 => Ok(TextRotation::Degrees(RotationDegrees::new(90 - v as i16)?)),
            255 => Ok(TextRotation::Stacked),
            _ => Err(IoError::Style(crate::StyleDecodeError::TextRotation)),
        })
        .transpose()?;
    let indent = number(e, "indent")?.unwrap_or(0);
    Ok(Alignment {
        horizontal: choice(e, "horizontal")?,
        vertical: choice(e, "vertical")?,
        wrap_text: boolean(e, "wrapText", false)?,
        shrink_to_fit: boolean(e, "shrinkToFit", false)?,
        rotation,
        indent,
        reading_order: choice(e, "readingOrder")?,
    })
}
/// Find the selected attributes without opening a new XML iterator for each field.
fn attributes<'a, const N: usize>(
    event: &'a BytesStart<'_>,
    names: [&str; N],
) -> Result<[Option<Attribute<'a>>; N], IoError> {
    let mut found: [Option<Attribute<'a>>; N] = [const { None }; N];
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(xml)?;
        if let Some(index) = names
            .iter()
            .position(|name| *name == attribute.key.as_ref())
            && let Some(slot) = found.get_mut(index)
            && slot.is_none()
        {
            *slot = Some(attribute);
        }
    }
    Ok(found)
}

/// Normalize a selected attribute when its field is due for validation.
fn attribute_text(
    attribute: Option<Attribute<'_>>,
) -> Result<Option<std::borrow::Cow<'_, str>>, IoError> {
    attribute
        .map(|value| {
            value
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(xml)
        })
        .transpose()
}

/// Decode an XF while preserving its own components, independently of apply flags.
fn xf(
    reader: &mut MetadataXml<'_>,
    e: &BytesStart<'_>,
    losses: &mut BTreeSet<String>,
) -> Result<Xf, IoError> {
    let [
        apply_font,
        apply_fill,
        apply_border,
        apply_alignment,
        apply_number,
        apply_protection,
        pivot,
        quote,
        font,
        fill,
        border,
        number,
        parent,
    ] = attributes(
        e,
        [
            "applyFont",
            "applyFill",
            "applyBorder",
            "applyAlignment",
            "applyNumberFormat",
            "applyProtection",
            "pivotButton",
            "quotePrefix",
            "fontId",
            "fillId",
            "borderId",
            "numFmtId",
            "xfId",
        ],
    )?;
    for flag in [
        apply_font,
        apply_fill,
        apply_border,
        apply_alignment,
        apply_number,
        apply_protection,
    ] {
        parse_boolean(attribute_text(flag)?.as_deref(), false)?;
    }
    let pivot = parse_boolean(attribute_text(pivot)?.as_deref(), false)?;
    let quote = parse_boolean(attribute_text(quote)?.as_deref(), false)?;
    if quote || pivot {
        losses.insert("XF flags".into());
    }
    let mut xf = Xf {
        font: parse_number(attribute_text(font)?.as_deref())?.unwrap_or(0),
        fill: parse_number(attribute_text(fill)?.as_deref())?.unwrap_or(0),
        border: parse_number(attribute_text(border)?.as_deref())?.unwrap_or(0),
        number: parse_number(attribute_text(number)?.as_deref())?.unwrap_or(0),
        parent: parse_number(attribute_text(parent)?.as_deref())?,
        alignment: None,
    };
    children(reader, |r, e| {
        if e.local_name().as_ref() != "alignment" {
            return skip(r, e, losses);
        }
        xf.alignment = Some(alignment(e, losses)?);
        r.skip_subtree()?;
        Ok(())
    })?;
    Ok(xf)
}
/// Borrow a checked source component; collection absence supplies implicit component zero.
fn component<T>(values: &[T], index: usize) -> Result<Option<&T>, IoError> {
    if values.is_empty() && index == 0 {
        return Ok(None);
    }
    values.get(index).map(Some).ok_or(IoError::Style(
        crate::StyleDecodeError::DanglingStyleComponentIndex,
    ))
}
/// Decode actual style elements; source count attributes never drive allocation.
pub(crate) fn read_styles(
    bytes: Option<&[u8]>,
    theme: Option<Vec<u8>>,
    warnings: &mut Vec<ImportDiagnostic>,
) -> Result<ImportedStyles, IoError> {
    if let Some(bytes) = &theme {
        validate_theme(bytes)?;
    }
    let Some(bytes) = bytes else {
        return Ok(ImportedStyles {
            cells: vec![CellStyle::default()],
            context: StyleContext::new(theme, vec![])?,
        });
    };
    let reader = || {
        MetadataXml::new(
            bytes,
            "styleSheet",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
            true,
        )
    };
    match decode_styles(reader(), theme, warnings) {
        Ok(styles) => Ok(styles),
        Err(error) => {
            // The old reader validated the complete XML document before decoding
            // any fields. Preserve that diagnostic priority on a failed read.
            reader().validate()?;
            Err(error)
        }
    }
}

/// Decode components while validating their XML, including skipped records.
fn decode_styles(
    mut reader: MetadataXml<'_>,
    theme: Option<Vec<u8>>,
    warnings: &mut Vec<ImportDiagnostic>,
) -> Result<ImportedStyles, IoError> {
    let mut fonts = Vec::new();
    let mut fills = Vec::new();
    let mut edges = Vec::new();
    let mut xfs = Vec::new();
    let mut parents = Vec::new();
    let mut palette = Vec::new();
    let mut formats: BTreeMap<u32, Arc<str>> = BTreeMap::new();
    let mut losses = BTreeSet::new();
    loop {
        match reader.read_event()? {
            Event::Start(root) if root.local_name().as_ref() == "styleSheet" => {
                children(&mut reader, |r, e| match e.local_name().as_ref() {
                    "fonts" => children(r, |r, e| {
                        if e.local_name().as_ref() != "font" {
                            return skip(r, e, &mut losses);
                        }
                        push(&mut fonts, font(r, &mut losses)?)
                    }),
                    "fills" => children(r, |r, e| {
                        if e.local_name().as_ref() != "fill" {
                            return skip(r, e, &mut losses);
                        }
                        push(&mut fills, fill(r, &mut losses)?)
                    }),
                    "borders" => children(r, |r, e| {
                        if e.local_name().as_ref() != "border" {
                            return skip(r, e, &mut losses);
                        }
                        push(&mut edges, borders(r, e, &mut losses)?)
                    }),
                    "cellXfs" | "cellStyleXfs" => {
                        let target = if e.local_name().as_ref() == "cellXfs" {
                            &mut xfs
                        } else {
                            &mut parents
                        };
                        children(r, |r, e| {
                            if e.local_name().as_ref() != "xf" {
                                return skip(r, e, &mut losses);
                            }
                            push(target, xf(r, e, &mut losses)?)
                        })
                    }
                    "numFmts" => children(r, |r, e| {
                        if e.local_name().as_ref() != "numFmt" {
                            return skip(r, e, &mut losses);
                        }
                        let id = number::<u32>(e, "numFmtId")?
                            .ok_or(IoError::Style(crate::StyleDecodeError::MissingNumfmtid))?;
                        let code = attr(e, "formatCode")?.ok_or({
                            IoError::Style(crate::StyleDecodeError::MissingFormatcode)
                        })?;
                        if formats.insert(id, Arc::from(code)).is_some() {
                            return Err(IoError::Style(crate::StyleDecodeError::DuplicateNumfmtid));
                        }
                        r.skip_subtree()?;
                        Ok(())
                    }),
                    "colors" => children(r, |r, e| {
                        if e.local_name().as_ref() != "indexedColors" {
                            return skip(r, e, &mut losses);
                        }
                        children(r, |r, e| {
                            if e.local_name().as_ref() != "rgbColor" {
                                return skip(r, e, &mut losses);
                            }
                            if palette.len() >= 64 {
                                return Err(IoError::Style(
                                    crate::StyleDecodeError::IndexedPaletteExceeds64,
                                ));
                            }
                            push(
                                &mut palette,
                                argb(&attr(e, "rgb")?.ok_or({
                                    IoError::Style(crate::StyleDecodeError::PaletteRgb)
                                })?)?,
                            )?;
                            r.skip_subtree()?;
                            Ok(())
                        })
                    }),
                    _ => skip(r, e, &mut losses),
                })?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if reader.foreign() {
        losses.insert("foreign namespace extensions".into());
    }
    // Microsoft MS-OE376 2.1.747 defines cellXF apply* as future named-style
    // propagation flags. The current direct component IDs are not parent merges.
    for xf in parents.iter().chain(&xfs) {
        component(&fonts, xf.font)?;
        component(&fills, xf.fill)?;
        component(&edges, xf.border)?;
        if let Some(parent) = xf.parent {
            component(&parents, parent)?;
        }
        if xf.number >= 164 && !formats.contains_key(&xf.number) {
            return Err(IoError::Style(
                crate::StyleDecodeError::DanglingCustomNumfmtid,
            ));
        }
    }
    if xfs.is_empty() {
        xfs.push(Xf::default());
    }
    let fonts = ComponentIds::new(&fonts, |font| {
        (font != &FontStyle::default()).then_some(font)
    })?;
    let fills = ComponentIds::new(&fills, |fill| {
        fill.as_ref().filter(|fill| {
            fill.pattern != PatternType::None
                || fill.foreground.is_some()
                || fill.background.is_some()
        })
    })?;
    let edges = ComponentIds::new(&edges, |edge| (edge != &Borders::default()).then_some(edge))?;
    let mut cells = Vec::new();
    let mut visuals: HashMap<_, Arc<VisualStyle>> = HashMap::new();
    for xf in xfs {
        let font = fonts.get(xf.font)?;
        let fill = fills.get(xf.fill)?;
        let borders = edges.get(xf.border)?;
        let key = (
            font.map(|(id, _)| id),
            fill.map(|(id, _)| id),
            borders.map(|(id, _)| id),
            xf.alignment,
        );
        let number_format = if xf.number == 0 {
            None
        } else {
            match formats.entry(xf.number) {
                std::collections::btree_map::Entry::Occupied(entry) => {
                    Some(Arc::clone(entry.get()))
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    super::super::metadata::builtin(xf.number)
                        .map(|code| Arc::clone(entry.insert(Arc::from(code))))
                }
            }
        };
        if xf.number != 0 && number_format.is_none() {
            losses.insert("locale-dependent built-in number formats".into());
        }
        let visual = if key == Default::default() {
            None
        } else if let Some(shared) = visuals.get(&key) {
            Some(Arc::clone(shared))
        } else {
            visuals.try_reserve(1).map_err(|source| {
                IoError::Allocation(crate::AllocationError::SharedStyles(source))
            })?;
            // SAFETY: VisualStyle has a fixed-size layout, so this Arc size cannot overflow.
            let shared = Arc::new(VisualStyle {
                font: font.map(|(_, value)| value.clone()),
                fill: fill.map(|(_, value)| value.clone()),
                borders: borders.map(|(_, value)| value.clone()),
                alignment: key.3.clone(),
            });
            visuals.insert(key, Arc::clone(&shared));
            Some(shared)
        };
        push(
            &mut cells,
            CellStyle {
                number_format,
                visual,
            },
        )?;
    }
    let context = StyleContext::new(theme, palette)?
        .with_default_style(cells.first().cloned().unwrap_or_default());
    if !losses.is_empty() {
        warnings.push(ImportDiagnostic {
            kind: ImportDiagnosticKind::RepresentationLoss,
            detail: format!(
                "Unsupported XLSX style records omitted (named-style provenance flattened): {}",
                losses.into_iter().collect::<Vec<_>>().join(", ")
            ),
        });
    }
    Ok(ImportedStyles { cells, context })
}
