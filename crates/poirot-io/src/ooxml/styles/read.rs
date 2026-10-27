//! Indexed source style records; component IDs describe the direct cell XF.
use super::codec::{Token, argb, boolean, choice, color, invalid, number, valid};
use crate::IoError;
use crate::ooxml::xml::{attr, xml};
use crate::{
    Alignment, BorderLine, BorderSide, Borders, CellStyle, FontSize, FontStyle, ImportDiagnostic,
    ImportDiagnosticKind, PatternFill, PatternType, RotationDegrees, StyleContext, TextRotation,
    Underline, VisualStyle,
};
use quick_xml::{
    NsReader,
    events::{BytesStart, Event},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
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
        .map_err(|_| IoError::Allocation("style table"))?;
    records.push(value);
    Ok(())
}
/// Visit direct children, letting each visitor consume its own subtree.
fn children(
    reader: &mut NsReader<&[u8]>,
    mut visit: impl FnMut(&mut NsReader<&[u8]>, &BytesStart<'_>) -> Result<(), IoError>,
) -> Result<(), IoError> {
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) => {
                let (ns, _) = reader.resolver().resolve_element(e.name());
                if matches!(ns, quick_xml::name::ResolveResult::Bound(n) if n.as_ref() == "http://schemas.openxmlformats.org/spreadsheetml/2006/main")
                {
                    visit(reader, &e)?;
                } else {
                    reader.read_to_end(e.name()).map_err(xml)?;
                }
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Err(invalid("truncated style record")),
            Event::DocType(_) => return Err(invalid("DTD in style metadata")),
            _ => {}
        }
    }
}
/// Skip an unrepresented record and retain one warning per element family.
fn skip(
    reader: &mut NsReader<&[u8]>,
    e: &BytesStart<'_>,
    losses: &mut BTreeSet<String>,
) -> Result<(), IoError> {
    losses.insert(e.local_name().as_ref().to_string());
    reader.read_to_end(e.name()).map_err(xml)?;
    Ok(())
}
/// Validate standalone XML metadata without DTDs, unresolved references, or multiple roots.
fn validate_root(bytes: &[u8], local: &str, namespace: &str) -> Result<bool, IoError> {
    use quick_xml::name::ResolveResult;
    let mut reader = NsReader::from_reader(bytes);
    reader.config_mut().expand_empty_elements = true;
    let mut depth = 0usize;
    let mut seen = false;
    let mut foreign = false;
    let mut declaration = false;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) => {
                if depth == 0 {
                    let (ns, name) = reader.resolver().resolve_element(e.name());
                    if seen
                        || name.as_ref() != local
                        || !matches!(ns, ResolveResult::Bound(n) if n.as_ref() == namespace)
                    {
                        return Err(invalid("metadata root/namespace"));
                    }
                    seen = true;
                }
                let (ns, _) = reader.resolver().resolve_element(e.name());
                foreign |= !matches!(ns, ResolveResult::Bound(n) if n.as_ref() == namespace);
                depth = depth.checked_add(1).ok_or_else(|| invalid("XML depth"))?;
                for a in e.attributes() {
                    let a = a.map_err(xml)?;
                    let value = a
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(xml)?;
                    valid(&value)?;
                }
            }
            Event::End(_) => depth = depth.checked_sub(1).ok_or_else(|| invalid("XML depth"))?,
            Event::DocType(_) => return Err(invalid("DTD in metadata")),
            Event::Decl(_) => {
                if seen || declaration {
                    return Err(invalid("misplaced XML declaration"));
                }
                declaration = true;
            }
            Event::CData(e) => {
                if depth == 0 {
                    return Err(invalid("CDATA outside metadata root"));
                }
                valid(e.as_ref())?;
            }
            Event::GeneralRef(e) => {
                if depth == 0 {
                    return Err(invalid("reference outside metadata root"));
                }
                if let Some(c) = e.resolve_char_ref().map_err(xml)? {
                    valid(&c.to_string())?;
                }
                if e.resolve_char_ref().map_err(xml)?.is_none()
                    && !matches!(e.as_ref(), "amp" | "lt" | "gt" | "apos" | "quot")
                {
                    return Err(invalid("unknown entity in metadata"));
                }
            }
            Event::Text(t) => {
                let value = t.xml10_content();
                valid(&value)?;
                if depth == 0 && !value.trim().is_empty() {
                    return Err(invalid("text outside metadata root"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen || depth != 0 {
        return Err(invalid("incomplete metadata root"));
    }
    Ok(foreign)
}
/// Validate authored or imported DrawingML themes before retaining or packaging bytes.
pub(crate) fn validate_theme(bytes: &[u8]) -> Result<(), IoError> {
    validate_root(
        bytes,
        "theme",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    )
    .map(|_| ())
}
/// Decode one ordinary font record without resolving theme or indexed colors.
fn font(reader: &mut NsReader<&[u8]>, losses: &mut BTreeSet<String>) -> Result<FontStyle, IoError> {
    let mut f = FontStyle::default();
    children(reader, |r, e| {
        match e.local_name().as_ref() {
            "name" => {
                f.name = Some(Arc::from(
                    attr(e, "val")?.ok_or_else(|| invalid("missing font name"))?,
                ))
            }
            "sz" => {
                f.size = Some(FontSize::new(
                    number(e, "val")?.ok_or_else(|| invalid("missing font size"))?,
                )?)
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
                f.scheme = Some(choice(e, "val")?.ok_or_else(|| invalid("missing font scheme"))?)
            }
            "family" => {
                f.family = Some(choice(e, "val")?.ok_or_else(|| invalid("missing font family"))?)
            }
            "charset" => {
                f.charset = Some(number(e, "val")?.ok_or_else(|| invalid("missing font charset"))?)
            }
            _ => return skip(r, e, losses),
        }
        r.read_to_end(e.name()).map_err(xml)?;
        Ok(())
    })?;
    Ok(f)
}
/// Decode a pattern fill, reporting gradients rather than claiming preservation.
fn fill(
    reader: &mut NsReader<&[u8]>,
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
            r.read_to_end(e.name()).map_err(xml)?;
            Ok(())
        })?;
        result = Some(fill);
        Ok(())
    })?;
    Ok(result)
}
/// Decode four independent edges; omitted edge records remain omitted.
fn borders(
    reader: &mut NsReader<&[u8]>,
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
            r.read_to_end(e.name()).map_err(xml)?;
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
            _ => Err(invalid("textRotation")),
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
/// Decode an XF while preserving its own components, independently of apply flags.
fn xf(
    reader: &mut NsReader<&[u8]>,
    e: &BytesStart<'_>,
    losses: &mut BTreeSet<String>,
) -> Result<Xf, IoError> {
    for name in [
        "applyFont",
        "applyFill",
        "applyBorder",
        "applyAlignment",
        "applyNumberFormat",
        "applyProtection",
        "pivotButton",
        "quotePrefix",
    ] {
        if attr(e, name)?.is_some() {
            boolean(e, name, false)?;
        }
    }
    if boolean(e, "quotePrefix", false)? || boolean(e, "pivotButton", false)? {
        losses.insert("XF flags".into());
    }
    let mut xf = Xf {
        font: number(e, "fontId")?.unwrap_or(0),
        fill: number(e, "fillId")?.unwrap_or(0),
        border: number(e, "borderId")?.unwrap_or(0),
        number: number(e, "numFmtId")?.unwrap_or(0),
        parent: number(e, "xfId")?,
        alignment: None,
    };
    children(reader, |r, e| {
        if e.local_name().as_ref() != "alignment" {
            return skip(r, e, losses);
        }
        xf.alignment = Some(alignment(e, losses)?);
        r.read_to_end(e.name()).map_err(xml)?;
        Ok(())
    })?;
    Ok(xf)
}
/// Borrow a checked source component; collection absence supplies implicit component zero.
fn component<T>(values: &[T], index: usize) -> Result<Option<&T>, IoError> {
    if values.is_empty() && index == 0 {
        return Ok(None);
    }
    values
        .get(index)
        .map(Some)
        .ok_or_else(|| invalid("dangling style component index"))
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
    let foreign = validate_root(
        bytes,
        "styleSheet",
        "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    )?;
    let mut fonts = Vec::new();
    let mut fills = Vec::new();
    let mut edges = Vec::new();
    let mut xfs = Vec::new();
    let mut parents = Vec::new();
    let mut palette = Vec::new();
    let mut formats = BTreeMap::new();
    let mut losses = BTreeSet::new();
    if foreign {
        losses.insert("foreign namespace extensions".into());
    }
    let mut reader = NsReader::from_reader(bytes);
    reader.config_mut().expand_empty_elements = true;
    loop {
        match reader.read_event().map_err(xml)? {
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
                            .ok_or_else(|| invalid("missing numFmtId"))?;
                        let code =
                            attr(e, "formatCode")?.ok_or_else(|| invalid("missing formatCode"))?;
                        if formats.insert(id, code).is_some() {
                            return Err(invalid("duplicate numFmtId"));
                        }
                        r.read_to_end(e.name()).map_err(xml)?;
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
                                return Err(invalid("indexed palette exceeds 64"));
                            }
                            push(
                                &mut palette,
                                argb(&attr(e, "rgb")?.ok_or_else(|| invalid("palette rgb"))?)?,
                            )?;
                            r.read_to_end(e.name()).map_err(xml)?;
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
            return Err(invalid("dangling custom numFmtId"));
        }
    }
    if xfs.is_empty() {
        xfs.push(Xf::default());
    }
    let mut cells = Vec::new();
    let mut visuals: HashSet<Arc<VisualStyle>> = HashSet::new();
    for xf in xfs {
        let visual = VisualStyle {
            font: component(&fonts, xf.font)?
                .filter(|f| **f != FontStyle::default())
                .cloned(),
            fill: component(&fills, xf.fill)?.cloned().flatten().filter(|f| {
                f.pattern != PatternType::None || f.foreground.is_some() || f.background.is_some()
            }),
            borders: component(&edges, xf.border)?
                .filter(|b| **b != Borders::default())
                .cloned(),
            alignment: xf.alignment,
        };
        let number_format = if xf.number == 0 {
            None
        } else {
            formats
                .get(&xf.number)
                .cloned()
                .or_else(|| super::super::metadata::builtin(xf.number).map(str::to_owned))
        };
        if xf.number != 0 && number_format.is_none() {
            losses.insert("locale-dependent built-in number formats".into());
        }
        let visual = if visual == VisualStyle::default() {
            None
        } else if let Some(shared) = visuals.get(&visual) {
            Some(Arc::clone(shared))
        } else {
            visuals
                .try_reserve(1)
                .map_err(|_| IoError::Allocation("shared visual styles"))?;
            let shared = Arc::new(visual);
            visuals.insert(Arc::clone(&shared));
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
