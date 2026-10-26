//! Canonical style identities registered during the existing backend traversal.
use super::{codec::*, validate_theme};
use crate::ooxml::{
    budget::{SizeCounter, next_total},
    package::Package,
    xml::{events, write_event, xml},
};
use crate::{IoError, WriteOptions};
use poirot_model::*;
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, Event},
};
use std::{collections::HashMap, io::Write};

/// Borrowed source style identities; no coordinate map or lossy backend conversion.
pub(crate) struct StyleExport<'a> {
    styles: Vec<&'a CellStyle>,
    ids: HashMap<&'a CellStyle, usize>,
}
impl<'a> StyleExport<'a> {
    /// Reserve cell-XF zero for the workbook default, including unstored cells.
    pub(crate) fn new(context: &'a StyleContext) -> Result<Self, IoError> {
        let mut out = Self {
            styles: vec![],
            ids: HashMap::new(),
        };
        out.register(context.default_style())?;
        Ok(out)
    }
    /// Intern one source style during the existing cell writing traversal.
    pub(crate) fn register(&mut self, style: &'a CellStyle) -> Result<(), IoError> {
        if self.ids.contains_key(style)
            || (style == &CellStyle::default() && !self.styles.is_empty())
        {
            return Ok(());
        }
        self.styles
            .try_reserve(1)
            .map_err(|_| IoError::Allocation("export style identities"))?;
        self.ids
            .try_reserve(1)
            .map_err(|_| IoError::Allocation("export style identities"))?;
        self.ids.insert(style, self.styles.len());
        self.styles.push(style);
        Ok(())
    }
    /// Resolve a registered style while projecting the source worksheet.
    pub(crate) fn id(&self, style: &CellStyle) -> Result<usize, IoError> {
        if style == &CellStyle::default() {
            return Ok(0);
        }
        self.ids
            .get(style)
            .copied()
            .ok_or_else(|| IoError::Read("unregistered export style".into()))
    }
    /// Serialize a canonical stylesheet to either the sizing or output sink.
    pub(crate) fn execute<W: Write>(
        &self,
        output: W,
        context: &StyleContext,
    ) -> Result<W, IoError> {
        let mut w = Writer::new(output);
        start(
            &mut w,
            "styleSheet",
            &[(
                "xmlns",
                "http://schemas.openxmlformats.org/spreadsheetml/2006/main".into(),
            )],
        )?;
        let numbered = self
            .styles
            .iter()
            .filter(|s| s.number_format.is_some())
            .count();
        start(&mut w, "numFmts", &[("count", numbered.to_string())])?;
        for (i, s) in self.styles.iter().enumerate() {
            if let Some(code) = &s.number_format {
                empty(
                    &mut w,
                    "numFmt",
                    &[("numFmtId", format_id(i)?), ("formatCode", code.clone())],
                )?;
            }
        }
        end(&mut w, "numFmts")?;
        start(&mut w, "fonts", &[("count", offset(self.styles.len(), 1)?)])?;
        font(&mut w, &FontStyle::default())?;
        for s in &self.styles {
            font(
                &mut w,
                &s.visual
                    .as_deref()
                    .and_then(|v| v.font.as_ref())
                    .cloned()
                    .unwrap_or_default(),
            )?;
        }
        end(&mut w, "fonts")?;
        start(&mut w, "fills", &[("count", offset(self.styles.len(), 2)?)])?;
        fill(&mut w, None)?;
        fill(
            &mut w,
            Some(&PatternFill {
                pattern: PatternType::Gray125,
                foreground: None,
                background: None,
            }),
        )?;
        for s in &self.styles {
            fill(&mut w, s.visual.as_deref().and_then(|v| v.fill.as_ref()))?;
        }
        end(&mut w, "fills")?;
        start(
            &mut w,
            "borders",
            &[("count", offset(self.styles.len(), 1)?)],
        )?;
        borders(&mut w, None)?;
        for s in &self.styles {
            borders(&mut w, s.visual.as_deref().and_then(|v| v.borders.as_ref()))?;
        }
        end(&mut w, "borders")?;
        start(&mut w, "cellStyleXfs", &[("count", "1".into())])?;
        empty(
            &mut w,
            "xf",
            &[
                ("numFmtId", "0".into()),
                ("fontId", "0".into()),
                ("fillId", "0".into()),
                ("borderId", "0".into()),
            ],
        )?;
        end(&mut w, "cellStyleXfs")?;
        start(
            &mut w,
            "cellXfs",
            &[("count", self.styles.len().to_string())],
        )?;
        for (i, s) in self.styles.iter().enumerate() {
            let visual = s.visual.as_deref();
            start(
                &mut w,
                "xf",
                &[
                    (
                        "numFmtId",
                        if s.number_format.is_some() {
                            format_id(i)?
                        } else {
                            "0".into()
                        },
                    ),
                    (
                        "fontId",
                        if visual.and_then(|v| v.font.as_ref()).is_some() {
                            offset(i, 1)?
                        } else {
                            "0".into()
                        },
                    ),
                    (
                        "fillId",
                        if visual.and_then(|v| v.fill.as_ref()).is_some() {
                            offset(i, 2)?
                        } else {
                            "0".into()
                        },
                    ),
                    (
                        "borderId",
                        if visual.and_then(|v| v.borders.as_ref()).is_some() {
                            offset(i, 1)?
                        } else {
                            "0".into()
                        },
                    ),
                    ("xfId", "0".into()),
                    ("applyFont", "1".into()),
                    ("applyFill", "1".into()),
                    ("applyBorder", "1".into()),
                    ("applyNumberFormat", "1".into()),
                    ("applyAlignment", "1".into()),
                ],
            )?;
            if let Some(a) = visual.and_then(|v| v.alignment.as_ref()) {
                alignment(&mut w, a)?;
            }
            end(&mut w, "xf")?;
        }
        end(&mut w, "cellXfs")?;
        start(&mut w, "cellStyles", &[("count", "1".into())])?;
        empty(
            &mut w,
            "cellStyle",
            &[
                ("name", "Normal".into()),
                ("xfId", "0".into()),
                ("builtinId", "0".into()),
            ],
        )?;
        end(&mut w, "cellStyles")?;
        if !context.indexed_palette().is_empty() {
            start(&mut w, "colors", &[])?;
            start(&mut w, "indexedColors", &[])?;
            for c in context.indexed_palette() {
                empty(&mut w, "rgbColor", &[("rgb", hex(*c))])?;
            }
            end(&mut w, "indexedColors")?;
            end(&mut w, "colors")?;
        }
        end(&mut w, "styleSheet")?;
        Ok(w.into_inner())
    }
}
/// Add a component offset within OOXML's unsigned 32-bit index domain.
fn offset(index: usize, extra: u32) -> Result<String, IoError> {
    u32::try_from(index)
        .ok()
        .and_then(|n| n.checked_add(extra))
        .map(|n| n.to_string())
        .ok_or_else(|| IoError::Limit("style index overflow".into()))
}
/// Generate checked custom number-format IDs outside the built-in range.
fn format_id(i: usize) -> Result<String, IoError> {
    u32::try_from(i)
        .ok()
        .and_then(|i| i.checked_add(164))
        .map(|i| i.to_string())
        .ok_or_else(|| IoError::Limit("number-format ID overflow".into()))
}
/// Write a style tag with escaped and checked attribute values.
fn tag<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    attrs: &[(&str, String)],
    is_empty: bool,
) -> Result<(), IoError> {
    let mut e = BytesStart::new(name);
    for (key, value) in attrs {
        valid(value)?;
        e.push_attribute((*key, value.as_str()));
    }
    write_event(
        w,
        if is_empty {
            Event::Empty(e)
        } else {
            Event::Start(e)
        },
    )
}
/// Open an element with safe attributes.
fn start<W: Write>(w: &mut Writer<W>, name: &str, attrs: &[(&str, String)]) -> Result<(), IoError> {
    tag(w, name, attrs, false)
}
/// Write an empty element with safe attributes.
fn empty<W: Write>(w: &mut Writer<W>, name: &str, attrs: &[(&str, String)]) -> Result<(), IoError> {
    tag(w, name, attrs, true)
}
/// Close a known element.
fn end<W: Write>(w: &mut Writer<W>, name: &str) -> Result<(), IoError> {
    write_event(w, Event::End(BytesEnd::new(name)))
}
/// Write a scalar font child.
fn val<W: Write>(w: &mut Writer<W>, name: &str, value: impl ToString) -> Result<(), IoError> {
    empty(w, name, &[("val", value.to_string())])
}
/// Format bytes without losing the alpha channel.
fn hex([a, r, g, b]: [u8; 4]) -> String {
    format!("{a:02X}{r:02X}{g:02X}{b:02X}")
}
/// Preserve exact color source identity and tint.
fn color<W: Write>(w: &mut Writer<W>, name: &str, c: StyleColor) -> Result<(), IoError> {
    let mut attrs = vec![match c.source {
        ColorSource::Argb(v) => ("rgb", hex(v)),
        ColorSource::Theme(v) => ("theme", v.token().into()),
        ColorSource::Indexed(v) => ("indexed", v.get().to_string()),
        ColorSource::Automatic => ("auto", "1".into()),
    }];
    if let Some(t) = c.tint {
        attrs.push(("tint", t.get().to_string()));
    }
    empty(w, name, &attrs)
}
/// Serialize one complete direct font record.
fn font<W: Write>(w: &mut Writer<W>, f: &FontStyle) -> Result<(), IoError> {
    start(w, "font", &[])?;
    if let Some(v) = &f.name {
        val(w, "name", v)?;
    }
    if let Some(v) = f.size {
        val(w, "sz", v.get())?;
    }
    for (name, on) in [("b", f.bold), ("i", f.italic), ("strike", f.strike)] {
        if on {
            empty(w, name, &[])?;
        }
    }
    if let Some(v) = f.underline {
        val(w, "u", v.token())?;
    }
    if let Some(v) = f.color {
        color(w, "color", v)?;
    }
    if let Some(v) = f.scheme {
        val(w, "scheme", v.token())?;
    }
    if let Some(v) = f.family {
        val(w, "family", v.token())?;
    }
    if let Some(v) = f.charset {
        val(w, "charset", v)?;
    }
    end(w, "font")
}
/// Serialize the ordinary pattern fill subset.
fn fill<W: Write>(w: &mut Writer<W>, f: Option<&PatternFill>) -> Result<(), IoError> {
    start(w, "fill", &[])?;
    start(
        w,
        "patternFill",
        &[(
            "patternType",
            f.map_or("none", |f| f.pattern.token()).into(),
        )],
    )?;
    if let Some(f) = f {
        if let Some(v) = f.foreground {
            color(w, "fgColor", v)?;
        }
        if let Some(v) = f.background {
            color(w, "bgColor", v)?;
        }
    }
    end(w, "patternFill")?;
    end(w, "fill")
}
/// Serialize independent edge records with independently identified colors.
fn borders<W: Write>(w: &mut Writer<W>, b: Option<&Borders>) -> Result<(), IoError> {
    start(w, "border", &[])?;
    if let Some(b) = b {
        for (name, side) in [
            ("left", &b.left),
            ("right", &b.right),
            ("top", &b.top),
            ("bottom", &b.bottom),
        ] {
            if let Some(s) = side {
                let attrs = s
                    .line
                    .map(|v| vec![("style", v.token().into())])
                    .unwrap_or_default();
                start(w, name, &attrs)?;
                if let Some(v) = s.color {
                    color(w, "color", v)?;
                }
                end(w, name)?;
            }
        }
    }
    end(w, "border")
}
/// Serialize common text layout with the OOXML rotation encoding.
fn alignment<W: Write>(w: &mut Writer<W>, a: &Alignment) -> Result<(), IoError> {
    let mut attrs = vec![
        ("wrapText", u8::from(a.wrap_text).to_string()),
        ("shrinkToFit", u8::from(a.shrink_to_fit).to_string()),
        ("indent", a.indent.to_string()),
    ];
    if let Some(v) = a.horizontal {
        attrs.push(("horizontal", v.token().into()));
    }
    if let Some(v) = a.vertical {
        attrs.push(("vertical", v.token().into()));
    }
    if let Some(v) = a.reading_order {
        attrs.push(("readingOrder", v.token().into()));
    }
    if let Some(v) = a.rotation {
        attrs.push((
            "textRotation",
            match v {
                TextRotation::Stacked => 255,
                TextRotation::Degrees(v) => {
                    if v.get() < 0 {
                        90 - v.get()
                    } else {
                        v.get()
                    }
                }
            }
            .to_string(),
        ));
    }
    empty(w, "alignment", &attrs)
}
/// Add theme declarations through the same count-first package replacement contract.
pub(crate) fn install_theme(
    package: &mut Package,
    workbook_part: &str,
    existing: Option<&str>,
    context: &StyleContext,
    options: WriteOptions,
    total: &mut u64,
) -> Result<(), IoError> {
    let Some(theme) = context.theme() else {
        return Ok(());
    };
    validate_theme(theme)?;
    if let Some(path) = existing {
        *total = next_total(
            theme.len() as u64,
            package.get(path).map_or(0, Vec::len) as u64,
            *total,
            options,
        )?;
        package.insert(path.into(), theme.to_vec());
        return Ok(());
    }
    let path = "xl/theme/poirot.xml";
    *total = next_total(
        theme.len() as u64,
        package.get(path).map_or(0, |v| v.len()) as u64,
        *total,
        options,
    )?;
    for (path, root, tag, attrs) in [
        (
            "[Content_Types].xml".to_string(),
            "Types",
            "Override",
            vec![
                ("PartName", "/xl/theme/poirot.xml".into()),
                (
                    "ContentType",
                    "application/vnd.openxmlformats-officedocument.theme+xml".into(),
                ),
            ],
        ),
        (
            crate::opc::relationship_part(workbook_part),
            "Relationships",
            "Relationship",
            vec![
                ("Id", "poirotTheme".into()),
                (
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme"
                        .into(),
                ),
                ("Target", "/xl/theme/poirot.xml".into()),
            ],
        ),
    ] {
        let source = package
            .get(&path)
            .ok_or_else(|| IoError::Xml(format!("missing generated part {path}")))?;
        let size = declaration(source, root, tag, &attrs, SizeCounter::default())?.0;
        *total = next_total(size, source.len() as u64, *total, options)?;
        let output = declaration(source, root, tag, &attrs, Vec::new())?;
        package.insert(path, output);
    }
    package.insert(path.into(), theme.to_vec());
    Ok(())
}
/// Project one package declaration using either a counter or output sink.
fn declaration<W: Write>(
    bytes: &[u8],
    root: &str,
    name: &str,
    attrs: &[(&str, String)],
    output: W,
) -> Result<W, IoError> {
    let mut r = events(bytes);
    let mut w = Writer::new(output);
    loop {
        let e = r.read_event().map_err(xml)?;
        if let Event::End(end) = &e
            && end.local_name().as_ref() == root
        {
            empty(&mut w, name, attrs)?;
        }
        if matches!(e, Event::Eof) {
            break;
        }
        write_event(&mut w, e.into_owned())?;
    }
    Ok(w.into_inner())
}
