//! Store and edit workbook data independently of file handling and calculation.
//!
//! Use this module as the data layer when you want to supply your own evaluator or
//! file adapters, or build a workbook model programmatically.
//!
//! [`Workbook`](crate::model::Workbook) holds sparse worksheets, cell values, formulas, named formulas, and
//! a date system. [`Worksheet`] provides cell access through zero-based [`CellAddress`]
//! coordinates.
//!
//! A formula cell stores both its formula and a saved result. Changing workbook data
//! does not calculate a new result. This module stores data without running file codecs or the evaluator;
//! use [poirot](https://docs.rs/poirot) for those workflows.
//!
//! Each formula cell owns its complete source. Optional [`SharedFormula`](crate::model::SharedFormula)
//! provenance can describe an XLSX group; its original anchor is not a live
//! dependency. Source setters detach membership, while cache/style edits preserve
//! it. Executable templates belong to the evaluator, independently of these cells.
//!
//! ```rust
//! use poirot::model::{CellValue, Workbook};
//!
//! let mut book = Workbook::new();
//! book.add_sheet("Sales")?.set_value("B2".parse()?, 125.)?;
//! book.define_name("TaxRate", "0.08", None)?;
//! let sheet = book.sheet_by_name_mut("sales").ok_or("missing sheet")?;
//! sheet.set_formula_with_cached_value("C2".parse()?, "B2*TaxRate", CellValue::Number(10.))?;
//! assert_eq!(sheet.get("C2".parse()?).ok_or("missing result")?.value, CellValue::Number(10.));
//! // Source edits preserve the saved result until an evaluator publishes a new one.
//! sheet.set_value("B2".parse()?, 200.)?;
//! assert_eq!(sheet.get("C2".parse()?).ok_or("missing result")?.value, CellValue::Number(10.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod formula_types;
use poirot_io::styles;
pub use styles::*;
// Possible formula-result vocabulary, independent of parsing and execution.
pub use formula_types::{FormulaType, ScalarKinds, ShapeBounds};
use poirot_io::numeric_fidelity;
// Numeric file-representation checks shared by import and export adapters.
pub use numeric_fidelity::*;
use poirot_io::numeric_bits;

// Exact binary64 identity and pinned source exceptions retained across evaluation boundaries.
pub use numeric_bits::{CommonsConvergence, IeeeNumberBits, IeeeNumberClass, KnownSourceException};
// Shared scalar, exact-text, and error values used by source cells and formula kernels.
pub use poirot_functions::scalar::{
    CellError, CellValue, ExcelText, TextRef, TextUnits, TextUnitsError, ValueError as ModelError,
};
mod cell_pages;
// Generic sparse storage and ordered views used by workbook and overlay grids.
pub use cell_pages::{CellRange, CellStore, ColumnCells, OccupiedRows, WorksheetCells};
mod rows;
// Borrowed row access and explicit visibility metadata.
pub use poirot_io::RowVisibility;
pub use rows::{Row, RowMut};
mod changes;
/// Inspection of the source data copied when cloning a workbook.
pub mod clone_footprint;
mod edit;
/// Owned source/cache mutations applied atomically to a workbook.
pub use changes::WorkbookChange;
/// Checked case-insensitive comparison for worksheet and defined-name identities.
pub use poirot_io::identifiers;
/// Bounded sparse query views that distinguish stored blanks from absent coordinates.
pub mod query;
// Source-checked formula replacements used by structural workbook edits.
pub use edit::{EditOutcome, FormulaReplacement, FormulaSource, RenameSheetPlan};

use poirot_io::address;
mod cell;
use poirot_io::file_metadata;
mod workbook;
mod worksheet;

pub use address::{CellAddress, MAX_COLUMNS, MAX_ROWS};
pub use cell::CellRead;
use cell::normalize_formula;
pub use file_metadata::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
pub use poirot_functions::scalar::DateSystem;
pub use poirot_io::NamedRange;
pub use poirot_io::{Cell, CellStyle, SharedFormula, SharedFormulaId};
pub use poirot_io::{SheetPresentation, SheetVisibility};
pub use workbook::Workbook;
pub use worksheet::Worksheet;
pub(in crate::model) use worksheet::{SavedSpillExtent, SpillCell};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses_round_trip_and_validate() {
        for (text, row, col) in [
            ("A1", 0, 0),
            ("Z9", 8, 25),
            ("AA1", 0, 26),
            ("XFD1048576", MAX_ROWS - 1, MAX_COLUMNS - 1),
        ] {
            let a: CellAddress = text.parse().unwrap();
            assert_eq!(a, CellAddress::new(row, col).unwrap());
            assert_eq!(a.to_string(), text);
        }
        assert_eq!("$a$1".parse::<CellAddress>().unwrap().to_string(), "A1");
        for text in [
            "A0",
            "XFE1",
            "A1048577",
            "1A",
            "A",
            "A-1",
            "A1junk",
            "💥1",
            "A42949672960",
        ] {
            assert!(text.parse::<CellAddress>().is_err(), "{text}");
        }
    }
    #[test]
    fn sheet_names_and_lookup() {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        assert!(w.add_sheet("DATA").is_err());
        assert!(w.sheet_by_name("data").is_some());
        for name in ["", "bad/name", "'quoted", "a:b"] {
            assert!(w.add_sheet(name).is_err());
        }
        assert!(w.add_sheet("😀".repeat(16)).is_err());
        w.rename_sheet(0, "Results").unwrap();
        assert_eq!(w.sheet(0).unwrap().name(), "Results");
        assert!(w.rename_sheet(4, "Other").is_err());
    }
    #[test]
    fn formulas_preserve_cache_and_style() {
        let mut s = Worksheet::new("Sheet1").unwrap();
        let a = "A1".parse().unwrap();
        s.set_formula_with_cached_value(a, " = SUM(B1:B2) ", CellValue::Number(3.0))
            .unwrap();
        let c = s.get(a).unwrap();
        assert_eq!(c.formula.as_deref(), Some("SUM(B1:B2)"));
        assert_eq!(c.value, CellValue::Number(3.0));
        assert!(s.set_formula(a, "=").is_err());
        assert_eq!(s.get(a).unwrap().value, CellValue::Number(3.0));
        s.get_mut(a).unwrap().style.number_format = Some("0.00".into());
        s.set_value(a, true).unwrap();
        assert_eq!(s.get(a).unwrap().formula, None);
        assert_eq!(
            s.get(a).unwrap().style.number_format.as_deref(),
            Some("0.00")
        );
        assert!(
            s.set_value(
                CellAddress {
                    row: MAX_ROWS,
                    col: 0
                },
                1.0
            )
            .is_err()
        );
    }
    #[test]
    fn errors_round_trip() {
        for error in [
            CellError::Null,
            CellError::Div0,
            CellError::Value,
            CellError::Ref,
            CellError::Name,
            CellError::Num,
            CellError::NA,
            CellError::GettingData,
            CellError::Spill,
            CellError::Calc,
        ] {
            assert_eq!(error.to_string().parse::<CellError>().unwrap(), error);
        }
        assert!("nope".parse::<CellError>().is_err());
    }
    #[test]
    fn names_resolve_local_before_global() {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        w.define_name("Rate", "=0.1", None).unwrap();
        w.define_name("Rate", "0.2", Some(0)).unwrap();
        assert_eq!(w.defined_name("RATE", 0).unwrap().formula, "0.2");
        assert!(w.define_name("A1", "1", None).is_err());
        assert!(w.define_name("Rate", "1", None).is_err());
        assert!(w.define_name("Other", "1", Some(1)).is_err());
    }
}

/// Serialization of workbook data and stored formula results.
pub mod snapshot;
#[cfg(test)]
mod scalar_boundary_tests {
    use super::*;
    #[test]
    fn formula_cache_and_source_remain_separate() {
        let mut sheet = crate::model::Worksheet::new("Data").unwrap();
        let address = crate::model::CellAddress::new(0, 0).unwrap();
        let value = CellValue::from(ExcelText::from_units(vec![0xd83d]));
        sheet
            .set_formula_with_cached_value(address, "LEFT(\"😀\",1)", value.clone())
            .unwrap();
        assert_eq!(sheet.get(address).unwrap().value, value);
        assert_eq!(
            sheet.get(address).unwrap().formula.as_deref(),
            Some("LEFT(\"😀\",1)")
        );
        sheet.set_value(address, "😀").unwrap();
        assert!(matches!(
            sheet.get(address).unwrap().value,
            CellValue::Text(_)
        ));
    }
}

#[cfg(test)]
mod sheet_management_tests {
    use super::*;
    #[test]
    fn reorder_remaps_scopes_and_duplicate_retains_typed_sources_and_presentation() {
        let mut w = Workbook::new();
        for name in ["A", "B", "C"] {
            w.add_sheet(name).unwrap();
        }
        w.define_name("local", "A1", Some(0)).unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), ExcelText::from_units(vec![0xd800]))
            .unwrap();
        w.sheet_mut(0).unwrap().set_presentation(SheetPresentation {
            visibility: SheetVisibility::VeryHidden,
            tab_color: Some([1, 2, 3]),
        });
        let copy = w.duplicate_sheet(0, "Copy").unwrap();
        assert_eq!(copy, 3);
        assert_eq!(
            w.sheet(copy).unwrap().presentation(),
            SheetPresentation {
                visibility: SheetVisibility::Visible,
                tab_color: Some([1, 2, 3])
            }
        );
        assert_eq!(
            w.sheet(copy).unwrap().get("A1".parse().unwrap()),
            w.sheet(0).unwrap().get("A1".parse().unwrap())
        );
        assert!(w.named_ranges.iter().any(|n| n.sheet_scope == Some(3)));
        w.move_sheet(0, 3).unwrap();
        assert_eq!(
            w.sheets().iter().map(|s| s.name()).collect::<Vec<_>>(),
            ["B", "C", "Copy", "A"]
        );
        assert_eq!(
            w.named_ranges
                .iter()
                .map(|n| n.sheet_scope)
                .collect::<Vec<_>>(),
            [Some(3), Some(2)]
        );
        w.move_sheet(3, 0).unwrap();
        let original = w.clone();
        assert!(w.move_sheet(0, 99).is_err());
        assert!(w.duplicate_sheet(0, "a").is_err());
        assert!(w.duplicate_sheet(0, "bad/name").is_err());
        assert_eq!(w, original);
    }
}
