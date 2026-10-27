//! Source wire schema, independently versioned by file envelopes. No request state is retained.
//!
//! ```rust
//! use poirot::model::{CellValue, Workbook};
//! use poirot::model::snapshot::Snapshot;
//!
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_formula_with_cached_value(
//!     "A1".parse()?, "2+3", CellValue::Number(5.),
//! )?;
//! // Owning the capture releases the borrow and freezes source plus saved results.
//! let captured = Snapshot::capture(&book).into_owned();
//! book.sheet_mut(0).ok_or("missing sheet")?.set_value("A1".parse()?, 99.)?;
//! assert!(!captured.matches(&book));
//! let restored = captured.restore()?;
//! let cell = restored.sheet(0).ok_or("missing sheet")?
//!     .get("A1".parse()?).ok_or("missing cell")?;
//! assert_eq!(cell.formula.as_deref(), Some("2+3"));
//! assert_eq!(cell.value, CellValue::Number(5.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::model::{
    Cell, CellAddress, CellStyle, CellValue, DateSystem, ExcelText, FileMetadata, MAX_ROWS,
    ModelError, NamedRange, RowVisibility, SavedSpillExtent, SharedFormula, SheetPresentation,
    SpillCell, StyleContext, VisualStyle, Workbook,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// Source snapshot wire revision, checked by the public artifact envelope before decoding.
/// Bump whenever the serialized source layout changes; provide an explicit migration or
/// reject older revisions. Prepared engine compatibility is versioned separately.
pub const SCHEMA_VERSION: u16 = 8;

/// Borrowed-or-owned Serde transport for source state, separate from runtime workbook types.
/// See the [module example](crate::model::snapshot) for capturing an owned copy before editing the source.
#[derive(Serialize, Deserialize)]
pub struct Snapshot<'a> {
    sheets: Vec<Sheet<'a>>,
    date_system: DateSystem,
    names: Cow<'a, [NamedRange]>,
    file_metadata: Option<FileMetadata>,
    styles: Vec<Cow<'a, CellStyle>>,
    style_context: StyleContext,
}
/// Coordinate and serialized cell payload within a sheet snapshot.
type SnapshotCell<'a> = (
    CellAddress,
    Value<'a>,
    Option<Cow<'a, str>>,
    usize,
    Option<SharedFormula>,
);

/// Serde worksheet payload including authored rows and saved spill provenance.
#[derive(Serialize, Deserialize)]
struct Sheet<'a> {
    name: Cow<'a, str>,
    presentation: SheetPresentation,
    rows: Vec<u32>,
    visibility: Vec<(u32, RowVisibility)>,
    cells: Vec<SnapshotCell<'a>>,
    spill_cache: Vec<(CellAddress, CellAddress, Option<Cow<'a, Cell>>)>,
    spill_extents: Vec<(CellAddress, SavedSpillExtent)>,
}
pub use poirot_io::value_wire::{Value, value_serde};

/// Cheap style lookup key: the number format text plus the shared visual style's pointer.
/// Equal keys imply equal styles, so a hit can reuse the canonical id without deep hashing.
/// Pointers stay valid and unique because the captured workbook is borrowed for `'a`.
type StyleIdentity<'a> = (Option<&'a str>, Option<*const VisualStyle>);

/// Return the canonical table index for `style`, appending it on first sight.
/// `last` short-circuits runs of cells sharing a style. Identity lookup skips deep-hashing shared visual styles; a miss falls back to deep
/// equality so equal styles behind distinct `Arc`s still share one table entry.
fn intern_style<'a>(
    style: &'a CellStyle,
    last: &mut Option<(StyleIdentity<'a>, usize)>,
    by_identity: &mut HashMap<StyleIdentity<'a>, usize>,
    by_value: &mut HashMap<&'a CellStyle, usize>,
    styles: &mut Vec<Cow<'a, CellStyle>>,
) -> usize {
    let identity = (
        style.number_format.as_deref(),
        style.visual.as_ref().map(Arc::as_ptr),
    );
    if let Some((previous, id)) = last
        && *previous == identity
    {
        return *id;
    }
    let id = *by_identity.entry(identity).or_insert_with(|| {
        let next = styles.len();
        *by_value.entry(style).or_insert_with(|| {
            styles.push(Cow::Borrowed(style));
            next
        })
    });
    *last = Some((identity, id));
    id
}

impl<'a> Snapshot<'a> {
    /// Compare source and saved-cache identity, including exact numeric bits.
    /// File metadata is excluded because import diagnostics do not affect evaluation.
    pub fn matches(&self, workbook: &Workbook) -> bool {
        let other = Snapshot::capture(workbook);
        self.styles == other.styles
            && self.style_context == other.style_context
            && self.date_system == other.date_system
            && self.names == other.names
            && self.sheets.len() == other.sheets.len()
            && self.sheets.iter().zip(&other.sheets).all(|(a, b)| {
                a.name == b.name
                    && a.presentation == b.presentation
                    && a.rows == b.rows
                    && a.visibility == b.visibility
                    && a.cells == b.cells
                    && a.spill_extents == b.spill_extents
                    && a.spill_cache.len() == b.spill_cache.len()
                    && a.spill_cache.iter().zip(&b.spill_cache).all(|(a, b)| {
                        a.0 == b.0
                            && a.1 == b.1
                            && match (&a.2, &b.2) {
                                (None, None) => true,
                                (Some(a), Some(b)) => {
                                    a.formula == b.formula
                                        && a.shared_formula == b.shared_formula
                                        && a.style == b.style
                                        && Value::capture(&a.value) == Value::capture(&b.value)
                                }
                                _ => false,
                            }
                    })
            })
    }
    /// Borrow source records for serialization without requiring Workbook itself to implement Serde.
    pub fn capture(workbook: &'a Workbook) -> Self {
        let mut last = None;
        let mut by_identity = HashMap::new();
        let mut by_value = HashMap::new();
        let mut styles = Vec::new();
        Self {
            sheets: workbook
                .sheets()
                .iter()
                .map(|s| Sheet {
                    name: Cow::Borrowed(s.name()),
                    presentation: s.presentation(),
                    rows: s.row_indices().collect(),
                    visibility: s.row_visibility_entries().collect(),
                    spill_extents: s
                        .spill_extents
                        .iter()
                        .map(|(at, extent)| (*at, extent.clone()))
                        .collect(),
                    spill_cache: s
                        .spill_cache
                        .iter()
                        .map(|(at, spill)| {
                            (*at, spill.owner, spill.source.as_ref().map(Cow::Borrowed))
                        })
                        .collect(),
                    cells: s
                        .cells()
                        .into_iter()
                        .map(|(a, c)| {
                            let style_id = intern_style(
                                &c.style,
                                &mut last,
                                &mut by_identity,
                                &mut by_value,
                                &mut styles,
                            );
                            (
                                *a,
                                match &c.value {
                                    CellValue::Blank => Value::Blank,
                                    CellValue::Number(n) => Value::Number(n.to_bits()),
                                    CellValue::Text(s) => Value::Text(Cow::Borrowed(s)),
                                    CellValue::TextUtf16(s) => {
                                        Value::Units(Cow::Borrowed(s.units()))
                                    }
                                    CellValue::Boolean(b) => Value::Boolean(*b),
                                    CellValue::Error(e) => Value::Error(*e),
                                },
                                c.formula.as_deref().map(Cow::Borrowed),
                                style_id,
                                c.shared_formula,
                            )
                        })
                        .collect(),
                })
                .collect(),
            date_system: workbook.date_system,
            names: Cow::Borrowed(&workbook.named_ranges),
            file_metadata: workbook.file_metadata.clone(),
            styles,
            style_context: workbook.style_context.clone(),
        }
    }
    /// Copy borrowed payloads only when an independently owned snapshot is needed.
    pub fn into_owned(self) -> Snapshot<'static> {
        Snapshot {
            sheets: self
                .sheets
                .into_iter()
                .map(|s| Sheet {
                    name: Cow::Owned(s.name.into_owned()),
                    presentation: s.presentation,
                    rows: s.rows,
                    visibility: s.visibility,
                    spill_extents: s.spill_extents,
                    spill_cache: s
                        .spill_cache
                        .into_iter()
                        .map(|(at, owner, source)| {
                            (at, owner, source.map(|c| Cow::Owned(c.into_owned())))
                        })
                        .collect(),
                    cells: s
                        .cells
                        .into_iter()
                        .map(|(a, v, f, n, shared)| {
                            (
                                a,
                                match v {
                                    Value::Blank => Value::Blank,
                                    Value::Number(b) => Value::Number(b),
                                    Value::Text(s) => Value::Text(Cow::Owned(s.into_owned())),
                                    Value::Units(u) => Value::Units(Cow::Owned(u.into_owned())),
                                    Value::Boolean(b) => Value::Boolean(b),
                                    Value::Error(e) => Value::Error(e),
                                },
                                f.map(|v| Cow::Owned(v.into_owned())),
                                n,
                                shared,
                            )
                        })
                        .collect(),
                })
                .collect(),
            date_system: self.date_system,
            names: Cow::Owned(self.names.into_owned()),
            file_metadata: self.file_metadata,
            styles: self
                .styles
                .into_iter()
                .map(|s| Cow::Owned(s.into_owned()))
                .collect(),
            style_context: self.style_context,
        }
    }
    /// Validate and construct an owned workbook from serialized source records.
    pub fn restore(self) -> Result<Workbook, ModelError> {
        if self.sheets.len() > 1024 || self.names.len() > 1_000_000 {
            return Err(ModelError("snapshot model count limit".into()));
        }
        let mut workbook = Workbook::new();
        workbook.date_system = self.date_system;
        workbook.file_metadata = self.file_metadata;
        workbook.style_context = self.style_context;
        for s in self.sheets {
            if s.cells.len() > 10_000_000
                || s.spill_extents.len() > 10_000_000
                || s.spill_cache.len() > 10_000_000
                || s.rows.len() > MAX_ROWS as usize
                || s.visibility.len() > MAX_ROWS as usize
            {
                return Err(ModelError("snapshot sheet count limit".into()));
            }
            let sheet = workbook.import_sheet(s.name.into_owned())?;
            sheet.set_presentation(s.presentation);
            for row in s.rows {
                sheet.create_row(row)?;
            }
            for (row, flags) in s.visibility {
                sheet.set_row_visibility(row, flags)?;
            }
            for (a, v, formula, style_id, shared_formula) in s.cells {
                let style = self
                    .styles
                    .get(style_id)
                    .ok_or_else(|| ModelError("invalid snapshot style reference".into()))?
                    .as_ref()
                    .clone();
                let value = match v {
                    Value::Blank => CellValue::Blank,
                    Value::Number(b) => CellValue::Number(f64::from_bits(b)),
                    Value::Text(s) => CellValue::Text(s.into_owned()),
                    Value::Units(u) => CellValue::TextUtf16(ExcelText::from_units(u.into_owned())),
                    Value::Boolean(b) => CellValue::Boolean(b),
                    Value::Error(e) => CellValue::Error(e),
                };
                if sheet
                    .set(
                        a,
                        Cell {
                            value,
                            formula: formula.map(Cow::into_owned),
                            shared_formula,
                            style,
                        },
                    )?
                    .is_some()
                {
                    return Err(ModelError("snapshot duplicate cell".into()));
                }
            }
            let mut extent_anchors = std::collections::BTreeSet::new();
            for (anchor, extent) in s.spill_extents {
                anchor.validate()?;
                extent.last.validate()?;
                if extent.last.row < anchor.row || extent.last.col < anchor.col {
                    return Err(ModelError("invalid snapshot spill extent geometry".into()));
                }
                if !extent_anchors.insert(anchor) {
                    return Err(ModelError("duplicate snapshot spill extent".into()));
                }
                // A raw source edit may leave stale saved geometry. Preserve source and
                // decline the stale extent rather than promoting it to live authority.
                if sheet.get(anchor).and_then(|cell| cell.formula.as_deref())
                    == Some(extent.formula.as_str())
                {
                    sheet.set_spill_extent(anchor, extent.last, &extent.formula)?;
                }
            }
            for (at, owner, source) in s.spill_cache {
                at.validate()?;
                owner.validate()?;
                if at == owner
                    || at.row < owner.row
                    || at.col < owner.col
                    || sheet.get(owner).is_none_or(|c| c.formula.is_none())
                    || sheet.get(at).is_none_or(|c| c.formula.is_some())
                    || source.as_ref().is_some_and(|c| {
                        c.formula.is_some()
                            || c.shared_formula.is_some()
                            || c.value != CellValue::Blank
                    })
                {
                    return Err(ModelError("invalid snapshot spill cache".into()));
                }
                if sheet
                    .spill_cache
                    .insert(
                        at,
                        SpillCell {
                            owner,
                            source: source.map(Cow::into_owned),
                        },
                    )
                    .is_some()
                {
                    return Err(ModelError("duplicate snapshot spill cache".into()));
                }
            }
        }
        if self
            .names
            .iter()
            .any(|n| n.sheet_scope.is_some_and(|s| s >= workbook.sheets().len()))
        {
            return Err(ModelError("snapshot name scope".into()));
        }
        workbook.named_ranges = self.names.into_owned();
        Ok(workbook)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileFormat, ImportDiagnostic, ImportDiagnosticKind};
    #[test]
    fn capture_borrows_text_units_formulas_styles_and_names() {
        let mut w = Workbook::new();
        w.named_ranges.push(NamedRange {
            name: "name".into(),
            formula: "1".into(),
            sheet_scope: None,
        });
        let sheet = w.add_sheet("S").unwrap();
        sheet
            .set(
                CellAddress { row: 0, col: 0 },
                Cell {
                    value: CellValue::Text("payload".repeat(100)),
                    formula: Some("1+1".into()),
                    shared_formula: None,
                    style: CellStyle {
                        number_format: Some("0.00".into()),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        sheet
            .set(
                CellAddress { row: 0, col: 1 },
                Cell::new(ExcelText::from_units(vec![0xd800])),
            )
            .unwrap();
        let dto = Snapshot::capture(&w);
        assert!(matches!(dto.names, Cow::Borrowed(_)));
        assert!(matches!(dto.sheets[0].name, Cow::Borrowed(_)));
        let (_, Value::Text(Cow::Borrowed(text)), Some(Cow::Borrowed(formula)), style_id, _) =
            &dto.sheets[0].cells[0]
        else {
            panic!("text fields copied")
        };
        let original = w
            .sheet(0)
            .unwrap()
            .get(CellAddress { row: 0, col: 0 })
            .unwrap();
        let CellValue::Text(original_text) = &original.value else {
            panic!()
        };
        assert_eq!(text.as_ptr(), original_text.as_ptr());
        assert_eq!(
            formula.as_ptr(),
            original.formula.as_ref().unwrap().as_ptr()
        );
        assert_eq!(
            dto.styles[*style_id]
                .number_format
                .as_ref()
                .unwrap()
                .as_ptr(),
            original.style.number_format.as_ref().unwrap().as_ptr()
        );
        assert!(matches!(
            dto.sheets[0].cells[1].1,
            Value::Units(Cow::Borrowed(_))
        ));
        let owned = dto.into_owned();
        drop(w);
        assert!(matches!(owned.names, Cow::Owned(_)));
        assert!(owned.restore().is_ok());
    }
    #[test]
    fn snapshot_wire_preserves_sparse_import_semantics_and_exact_values() {
        let mut w = Workbook::new();
        w.date_system = DateSystem::Excel1904;
        w.file_metadata = Some(FileMetadata {
            format: FileFormat::Xls,
            import_warnings: vec![ImportDiagnostic {
                kind: ImportDiagnosticKind::RepresentationLoss,
                detail: "source warning".into(),
            }],
        });
        w.named_ranges.push(NamedRange {
            name: "raw name".into(),
            formula: "S!A1".into(),
            sheet_scope: Some(0),
        });
        let s = w.import_sheet("'raw:/sheet'").unwrap();
        s.create_row(101).unwrap();
        s.set_row_visibility(
            103,
            RowVisibility {
                manually_hidden: true,
                filtered_out: true,
            },
        )
        .unwrap();
        let bits = [0x8000000000000000, 0x7ff0000000000000, 0x7ff8000000000042];
        for (col, b) in bits.iter().enumerate() {
            s.set(
                CellAddress {
                    row: 0,
                    col: col as u32,
                },
                Cell::new(f64::from_bits(*b)),
            )
            .unwrap();
        }
        s.set(CellAddress { row: 2, col: 0 }, Cell::default())
            .unwrap();
        s.set(
            CellAddress { row: 3, col: 0 },
            Cell {
                value: CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0x61, 0xdc00])),
                formula: Some("1+1".into()),
                shared_formula: None,
                style: CellStyle {
                    number_format: Some("0.000".into()),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let encoded = postcard::to_allocvec(&Snapshot::capture(&w)).unwrap();
        let decoded: Snapshot<'_> = postcard::from_bytes(&encoded).unwrap();
        assert!(decoded.matches(&w));
        let restored = decoded.into_owned().restore().unwrap();
        assert_eq!(restored.file_metadata, w.file_metadata);
        assert!(Snapshot::capture(&restored).matches(&w));
        assert_eq!(restored.date_system, w.date_system);
        assert_eq!(restored.named_ranges, w.named_ranges);
        let r = restored.sheet(0).unwrap();
        assert_eq!(r.name(), "'raw:/sheet'");
        assert_eq!(
            r.row_indices().collect::<Vec<_>>(),
            w.sheet(0).unwrap().row_indices().collect::<Vec<_>>()
        );
        assert_eq!(
            r.row_visibility(103),
            w.sheet(0).unwrap().row_visibility(103)
        );
        for (col, bits) in bits.iter().enumerate() {
            let CellValue::Number(n) = r
                .get(CellAddress {
                    row: 0,
                    col: col as u32,
                })
                .unwrap()
                .value
            else {
                panic!()
            };
            assert_eq!(n.to_bits(), *bits);
        }
        assert!(r.get(CellAddress { row: 2, col: 0 }).is_some());
        assert_eq!(
            r.get(CellAddress { row: 3, col: 0 }),
            w.sheet(0).unwrap().get(CellAddress { row: 3, col: 0 })
        );
    }
}

#[cfg(test)]
mod spill_extent_validation_tests {
    use super::*;
    fn at(s: &str) -> CellAddress {
        s.parse().unwrap()
    }
    #[test]
    fn stale_source_identity_does_not_exempt_malformed_extent_coordinates() {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula(at("A1"), "{1}")
            .unwrap();
        let mut snapshot = Snapshot::capture(&book).into_owned();
        snapshot.sheets[0].spill_extents.push((
            at("A1"),
            SavedSpillExtent {
                last: CellAddress {
                    row: MAX_ROWS,
                    col: 0,
                },
                formula: "stale".into(),
            },
        ));
        assert!(snapshot.restore().is_err());
        let mut snapshot = Snapshot::capture(&book).into_owned();
        snapshot.sheets[0].spill_extents.push((
            at("A1"),
            SavedSpillExtent {
                last: at("A1"),
                formula: "stale".into(),
            },
        ));
        let restored = snapshot.restore().unwrap();
        assert_eq!(
            restored
                .sheet(0)
                .unwrap()
                .get(at("A1"))
                .unwrap()
                .formula
                .as_deref(),
            Some("{1}")
        );
        assert_eq!(restored.sheet(0).unwrap().spill_extent(at("A1")), None);
    }
}

#[cfg(test)]
mod style_reference_tests {
    use super::*;
    #[test]
    fn invalid_style_reference_rejects_instead_of_defaulting() {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_style("A1".parse().unwrap(), CellStyle::default())
            .unwrap();
        let mut snapshot = Snapshot::capture(&book).into_owned();
        snapshot.sheets[0].cells[0].3 = snapshot.styles.len();
        assert!(snapshot.restore().is_err());
    }
}

#[cfg(test)]
mod style_interning_tests {
    use super::*;
    use crate::model::FontStyle;

    /// Build a bold or italic visual style behind a fresh `Arc`.
    fn visual(bold: bool) -> Arc<VisualStyle> {
        Arc::new(VisualStyle {
            font: Some(FontStyle {
                bold,
                italic: !bold,
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    /// Two sheets mixing shared `Arc`s, equal-but-distinct `Arc`s, and distinct styles.
    /// With `share` false every cell gets its own `Arc`; `italic_last` changes one style.
    fn book(share: bool, italic_last: bool) -> Workbook {
        let bold = visual(true);
        let pick = |shared: &Arc<VisualStyle>| {
            if share {
                shared.clone()
            } else {
                Arc::new(shared.as_ref().clone())
            }
        };
        let mut book = Workbook::new();
        let first = book.add_sheet("A").unwrap();
        let styles = [
            CellStyle {
                number_format: None,
                visual: Some(pick(&bold)),
            },
            CellStyle {
                number_format: None,
                visual: Some(pick(&bold)),
            },
            // Equal content behind a separate allocation must reuse the first entry.
            CellStyle {
                number_format: None,
                visual: Some(visual(true)),
            },
            CellStyle {
                number_format: Some("0.00".into()),
                visual: Some(pick(&bold)),
            },
            CellStyle {
                number_format: Some("0.00".into()),
                visual: None,
            },
            CellStyle {
                number_format: None,
                visual: Some(visual(false)),
            },
            CellStyle::default(),
        ];
        for (row, style) in styles.into_iter().enumerate() {
            first
                .set_value(
                    CellAddress {
                        row: row as u32,
                        col: 0,
                    },
                    row as f64,
                )
                .unwrap();
            first
                .set_style(
                    CellAddress {
                        row: row as u32,
                        col: 0,
                    },
                    style,
                )
                .unwrap();
        }
        let second = book.add_sheet("B").unwrap();
        second
            .set_style(
                CellAddress { row: 0, col: 0 },
                CellStyle {
                    number_format: Some("0.00".into()),
                    visual: Some(visual(true)),
                },
            )
            .unwrap();
        second
            .set_style(
                CellAddress { row: 1, col: 0 },
                CellStyle {
                    number_format: None,
                    visual: Some(if italic_last {
                        visual(false)
                    } else {
                        pick(&bold)
                    }),
                },
            )
            .unwrap();
        book
    }

    /// Encode a capture after replacing its style table with deep-equality-only interning.
    fn deep_only_encoding(book: &Workbook) -> Vec<u8> {
        let mut snapshot = Snapshot::capture(book);
        let mut ids: HashMap<&CellStyle, usize> = HashMap::new();
        let mut styles = Vec::new();
        for (sheet, captured) in book.sheets().iter().zip(&mut snapshot.sheets) {
            for ((_, cell), entry) in sheet.cells().into_iter().zip(&mut captured.cells) {
                let next = styles.len();
                entry.3 = *ids.entry(&cell.style).or_insert_with(|| {
                    styles.push(Cow::Borrowed(&cell.style));
                    next
                });
            }
        }
        snapshot.styles = styles;
        postcard::to_allocvec(&snapshot).unwrap()
    }

    #[test]
    fn identity_interning_encodes_like_deep_interning() {
        for share in [true, false] {
            let book = book(share, false);
            let snapshot = Snapshot::capture(&book);
            assert_eq!(snapshot.styles.len(), 5);
            assert_eq!(
                postcard::to_allocvec(&snapshot).unwrap(),
                deep_only_encoding(&book)
            );
        }
        assert_eq!(
            postcard::to_allocvec(&Snapshot::capture(&book(true, false))).unwrap(),
            postcard::to_allocvec(&Snapshot::capture(&book(false, false))).unwrap()
        );
    }

    #[test]
    fn matches_ignores_style_sharing_but_not_style_values() {
        let shared = Snapshot::capture(&book(true, false)).into_owned();
        assert!(shared.matches(&book(false, false)));
        assert!(Snapshot::capture(&book(false, false)).matches(&book(true, false)));
        assert!(!shared.matches(&book(true, true)));
        assert!(!shared.matches(&book(false, true)));
    }
}
