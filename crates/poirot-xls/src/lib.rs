//! Checked BIFF8 workbook export to XLS files and byte buffers.
use poirot_model::{CellValue, Workbook};
use poirot_parsing::formula::parse_formula;
use std::{
    collections::BTreeMap,
    fmt,
    io::{Cursor, Write},
    path::Path,
};
/// XLS serialization failure: invalid model data, unsupported BIFF representation,
/// resource limits, numeric fidelity, or filesystem output failure.
#[derive(Debug)]
pub enum XlsError {
    /// Source data cannot be represented by the supported BIFF encoder.
    Representation(String),
    /// Numeric encoding would violate the selected fidelity contract.
    NumericFidelity(poirot_model::NumericFidelityRefusal),
    /// Planned or emitted output exceeds an explicit resource limit.
    ResourceLimit(String),
    /// Requested source feature has no supported BIFF encoding.
    Unsupported(String),
    /// Attach source/export context to an underlying encoding failure.
    Context {
        /// Source or export operation involved in the failure.
        context: String,
        /// Underlying checked export error.
        source: Box<XlsError>,
    },
    /// Filesystem or output-stream failure.
    Io {
        /// Output operation being performed.
        operation: String,
        /// Underlying I/O diagnostic.
        source: std::io::Error,
    },
}
impl fmt::Display for XlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NumericFidelity(e) => write!(f, "{e}"),
            Self::Representation(s) | Self::ResourceLimit(s) | Self::Unsupported(s) => {
                f.write_str(s)
            }
            Self::Context { context, source } => write!(f, "{context}: {source}"),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
        }
    }
}
impl std::error::Error for XlsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NumericFidelity(e) => Some(e),
            Self::Context { source, .. } => Some(source.as_ref()),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
impl From<std::io::Error> for XlsError {
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            operation: "encode CFB container".into(),
            source,
        }
    }
}
fn fail(message: &str) -> XlsError {
    XlsError::Representation(message.into())
}
fn context(context: String, source: XlsError) -> XlsError {
    XlsError::Context {
        context,
        source: Box::new(source),
    }
}
/// Wrap an already validated BIFF Workbook stream in a CFB container.
pub fn wrap_xls_container(globals: &[u8]) -> Result<Vec<u8>, XlsError> {
    let mut compound =
        cfb::CompoundFile::create_with_version(cfb::Version::V3, Cursor::new(Vec::new()))?;
    {
        let mut stream = compound.create_stream("/Workbook")?;
        stream.write_all(globals)?;
        stream.flush()?;
    }
    compound.flush()?;
    Ok(compound.into_inner().into_inner())
}
/// Export XLS bytes to a file after successful complete serialization.
pub fn write_xls(workbook: &Workbook, path: impl AsRef<Path>) -> Result<(), XlsError> {
    XlsExportPlan::new(workbook, XlsExportPolicy::default())?.write_file(path)?;
    Ok(())
}
mod record_layout;
mod representation;
mod workbook_encoding;
use record_layout::{MAX_RECORD, long, record, short};
use representation::{cached, error_code, unicode};
use workbook_encoding::encode_biff;
mod addins;
mod plan;
/// Checked BIFF formula and workbook planning/serialization entry points.
pub use plan::{
    BiffAddInReference, BiffFormulaPlan, XlsExportPlan, XlsExportPolicy, write_xls_to_buffer,
};
mod rpn;
mod strings;

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, Reader, Xls};
    use poirot_model::{CellError, DateSystem};
    use std::io::Read;
    fn book() -> Workbook {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        w
    }
    fn stream(bytes: Vec<u8>) -> Vec<u8> {
        let mut c = cfb::CompoundFile::open_strict(Cursor::new(bytes)).unwrap();
        let mut out = Vec::new();
        c.open_stream("/Workbook")
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        out
    }
    fn records(bytes: &[u8]) -> Vec<(u16, usize, &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 4 <= bytes.len() {
            if bytes[i..].iter().all(|b| *b == 0) {
                i = bytes.len();
                break;
            }
            let id = u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap());
            let len = u16::from_le_bytes(bytes[i + 2..i + 4].try_into().unwrap()) as usize;
            assert!(len <= MAX_RECORD);
            assert!(i + 4 + len <= bytes.len());
            out.push((id, i, &bytes[i + 4..i + 4 + len]));
            i += 4 + len;
        }
        assert_eq!(i, bytes.len());
        out
    }
    #[test]
    fn independent_reader_values_formulas_and_caches() {
        let mut w = book();
        let s = w.sheet_mut(0).unwrap();
        for (a, v) in [
            ("A1", CellValue::Number(2.)),
            ("A2", CellValue::Text("héllo 😀".into())),
            ("A3", CellValue::Boolean(true)),
            ("A4", CellValue::Error(CellError::NA)),
            ("A5", CellValue::Blank),
        ] {
            s.set_value(a.parse().unwrap(), v).unwrap();
        }
        for (a, f, v) in [
            ("B1", "A1*3+1", CellValue::Number(7.)),
            ("B2", "\"123\"", CellValue::Text("123".into())),
            ("B3", "1=1", CellValue::Boolean(true)),
            ("B4", "1/0", CellValue::Error(CellError::Div0)),
            ("B5", "IF(FALSE,1/0,4)", CellValue::Number(4.)),
            ("B6", "SUM($A$1:$A$1)", CellValue::Number(2.)),
            ("B7", "\"\"", CellValue::Text(String::new())),
        ] {
            s.set_formula(a.parse().unwrap(), f, v).unwrap();
        }
        let bytes = write_xls_to_buffer(&w).unwrap();
        assert!(bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]));
        let mut reader = Xls::new(Cursor::new(bytes)).unwrap();
        let cells = reader.worksheet_range("Data").unwrap();
        assert_eq!(cells.get_value((0, 0)), Some(&Data::Float(2.)));
        assert_eq!(
            cells.get_value((1, 0)),
            Some(&Data::String("héllo 😀".into()))
        );
        assert_eq!(cells.get_value((2, 0)), Some(&Data::Bool(true)));
        assert_eq!(cells.get_value((1, 1)), Some(&Data::String("123".into())));
        assert_eq!(cells.get_value((2, 1)), Some(&Data::Bool(true)));
        assert!(matches!(cells.get_value((3, 1)), Some(Data::Error(_))));
        assert_eq!(cells.get_value((6, 1)), Some(&Data::String(String::new())));
        let f = reader.worksheet_formula("Data").unwrap();
        for (a, want) in [
            ((0, 1), "A1*3+1"),
            ((1, 1), "\"123\""),
            ((2, 1), "1=1"),
            ((3, 1), "1/0"),
            ((4, 1), "IF(FALSE,1/0,4)"),
            ((5, 1), "SUM($A$1:$A$1)"),
        ] {
            assert_eq!(f.get_value(a).map(String::as_str), Some(want));
        }
    }
    #[test]
    fn long_unicode_sst_and_formula_cache_continue() {
        let mut w = book();
        let text = format!("{}END", "a😀é".repeat(6000));
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), text.clone())
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), text.clone())
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_formula("B1".parse().unwrap(), "A1", CellValue::Text(text.clone()))
            .unwrap();
        let bytes = write_xls_to_buffer(&w).unwrap();
        let biff = stream(bytes.clone());
        let r = records(&biff);
        assert!(r.iter().filter(|(id, _, _)| *id == 0x003c).count() >= 6);
        let sst = r.iter().find(|(id, _, _)| *id == 0x00fc).unwrap().2;
        assert_eq!(&sst[..8], &[2, 0, 0, 0, 1, 0, 0, 0]);
        let index = r.iter().position(|(id, _, _)| *id == 0x0207).unwrap();
        let mut utf16 = Vec::new();
        let initial = r[index].2;
        let count = u16::from_le_bytes(initial[..2].try_into().unwrap()) as usize;
        assert_eq!(initial[2], 1);
        utf16.extend_from_slice(&initial[3..]);
        for (id, _, part) in &r[index + 1..] {
            if *id != 0x003c {
                break;
            }
            assert_eq!(part[0], 1);
            utf16.extend_from_slice(&part[1..]);
        }
        let units: Vec<_> = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(count, units.len());
        assert!(
            String::from_utf16(&units).unwrap() == text,
            "cached STRING+CONTINUE differs"
        );
        let mut reader = Xls::new(Cursor::new(bytes)).unwrap();
        let cells = reader.worksheet_range("Data").unwrap();
        for a in [(0, 0), (1, 0), (0, 1)] {
            if a != (0, 1) {
                assert!(
                    cells.get_value(a) == Some(&Data::String(text.clone())),
                    "long SST cell {a:?} differs"
                );
            }
        }
    }
    #[test]
    fn cross_sheet_names_styles_and_date_system_records() {
        let mut w = book();
        w.add_sheet("Other Data")
            .unwrap()
            .set_value("A1".parse().unwrap(), 5.)
            .unwrap();
        w.define_name("Rate", "'Other Data'!$A$1", None).unwrap();
        w.define_name("Rate", "2", Some(0)).unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_formula(
                "A1".parse().unwrap(),
                "'Other Data'!$A$1+Rate",
                CellValue::Number(7.),
            )
            .unwrap();
        w.date_system = DateSystem::Excel1904;
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A1".parse().unwrap())
            .unwrap()
            .style
            .number_format = Some("0.00".into());
        w.sheet_mut(0)
            .unwrap()
            .set_value("B1".parse().unwrap(), 12.)
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .get_mut("B1".parse().unwrap())
            .unwrap()
            .style
            .number_format = Some("#,##0".into());
        let bytes = write_xls_to_buffer(&w).unwrap();
        let biff = stream(bytes.clone());
        let rs = records(&biff);
        assert_eq!(
            rs.iter().find(|(id, _, _)| *id == 0x0022).unwrap().2,
            &[1, 0]
        );
        for (_, _, data) in rs.iter().filter(|(id, _, _)| *id == 0x0085) {
            let offset = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            assert_eq!(&biff[offset..offset + 2], &[9, 8]);
        }
        assert_eq!(rs.iter().filter(|(id, _, _)| *id == 0x0018).count(), 2);
        let custom: Vec<_> = rs
            .iter()
            .filter(|(id, _, _)| *id == 0x00e0)
            .skip(2)
            .map(|(_, _, d)| u16::from_le_bytes(d[2..4].try_into().unwrap()))
            .collect();
        assert_eq!(custom, vec![164, 165]);
        let mut reader = Xls::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.sheet_names(), vec!["Data", "Other Data"]);
        let f = reader.worksheet_formula("Data").unwrap();
        assert_eq!(
            f.get_value((0, 0)).map(String::as_str),
            Some("Other Data!A1+Rate")
        );
        assert!(reader.defined_names().iter().any(|(n, _)| n == "Rate"));
    }
    #[test]
    fn bare_iferror_add_in_reads_back_and_evaluates_without_using_saved_cache() {
        let mut workbook = book();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(
                "A1".parse().unwrap(),
                "IFERROR(1/0,7)",
                CellValue::Number(-999.),
            )
            .unwrap();
        let plan = XlsExportPlan::new(&workbook, XlsExportPolicy::default()).unwrap();
        assert_eq!(plan.add_in_names(), &["IFERROR"]);
        let bytes = write_xls_to_buffer(&workbook).unwrap();
        let raw = stream(bytes.clone());
        let records = records(&raw);
        let formula = records.iter().find(|record| record.0 == 6).unwrap().2;
        assert_eq!(
            &formula[22..],
            &[
                0x59, 1, 0, 1, 0, 0, 0, 0x1e, 1, 0, 0x1e, 0, 0, 0x06, 0x1e, 7, 0, 0x42, 3, 0xff, 0,
            ]
        );
        let mut reader = Xls::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(
            reader.worksheet_range("Data").unwrap().get_value((0, 0)),
            Some(&Data::Float(-999.))
        );
        // Calamine does not resolve add-in NameX identities. poirot reconstructs them
        // from the original SUPBOOK/EXTERNNAME/XTI records before backend import.
        let imported = poirot_io::read_xls_bytes(&bytes).unwrap();
        let address = "A1".parse().unwrap();
        let cell = imported.workbook.sheet(0).unwrap().get(address).unwrap();
        assert_eq!(cell.value, CellValue::Number(-999.));
        let source = cell.formula.as_deref().unwrap();
        assert_eq!(source, "IFERROR(1/0,7)");
        assert_eq!(
            match poirot_eval::run_formula(
                &imported.workbook,
                0,
                address,
                source,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(7.)
        );
    }
    #[test]
    fn unsupported_features_are_contextual_errors() {
        let mut w = book();
        for formula in [
            "XFD1",
            "SUM(A1:A1048576)",
            "{1,2}",
            "MissingSheet!A1",
            "MissingName",
        ] {
            w.sheet_mut(0)
                .unwrap()
                .set_formula("A1".parse().unwrap(), formula, CellValue::Number(9.))
                .unwrap();
            let e = write_xls_to_buffer(&w).unwrap_err().to_string();
            assert!(e.contains("Data!A1"), "{formula}: {e}");
        }
        w.sheet_mut(0).unwrap().remove("A1".parse().unwrap());
        w.sheet_mut(0)
            .unwrap()
            .set_value("IW1".parse().unwrap(), 1.)
            .unwrap();
        assert!(
            write_xls_to_buffer(&w)
                .unwrap_err()
                .to_string()
                .contains("bounds")
        );
        let mut w = book();
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), CellValue::Error(CellError::Spill))
            .unwrap();
        assert!(write_xls_to_buffer(&w).is_err());
    }
    #[test]
    fn grouped_rpn_round_trips_preserve_calculation() {
        let mut w = book();
        for (formula, number) in [
            ("(1+2)*3", 9.),
            ("1-(2-3)", 2.),
            ("-(2^2)", -4.),
            ("2^(3^2)", 512.),
            ("(2^3)^2", 64.),
            ("1/(2/3)", 1.5),
        ] {
            w.sheet_mut(0)
                .unwrap()
                .set_formula("A1".parse().unwrap(), formula, CellValue::Number(-999.))
                .unwrap();
            let mut reader = Xls::new(Cursor::new(write_xls_to_buffer(&w).unwrap())).unwrap();
            let f = reader.worksheet_formula("Data").unwrap();
            let read = f.get_value((0, 0)).unwrap();
            assert_eq!(
                match poirot_eval::run_formula(
                    &w,
                    0,
                    "A1".parse().unwrap(),
                    read,
                    Default::default(),
                    Default::default()
                ) {
                    poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                CellValue::Number(number),
                "original={formula}, read={read}"
            );
        }
    }
    #[test]
    fn reference_tokens_and_cached_types_are_independently_inspected() {
        let mut w = book();
        w.add_sheet("Other Data").unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_formula(
                "A1".parse().unwrap(),
                "'Other Data'!$A1+A$2",
                CellValue::Text("123".into()),
            )
            .unwrap();
        let raw = stream(write_xls_to_buffer(&w).unwrap());
        let r = records(&raw);
        let f = r.iter().find(|(id, _, _)| *id == 6).unwrap().2;
        assert_eq!(&f[6..14], &[0, 0, 0, 0, 0, 0, 255, 255]);
        assert_eq!(
            &f[22..],
            &[0x5a, 1, 0, 0, 0, 0, 0x80, 0x44, 1, 0, 0, 0x40, 3]
        );
        w.sheet_mut(0)
            .unwrap()
            .set_formula(
                "A1".parse().unwrap(),
                "SUM($A$1:A2)",
                CellValue::Boolean(true),
            )
            .unwrap();
        let raw = stream(write_xls_to_buffer(&w).unwrap());
        let r = records(&raw);
        let f = r.iter().find(|(id, _, _)| *id == 6).unwrap().2;
        assert_eq!(&f[6..14], &[1, 0, 1, 0, 0, 0, 255, 255]);
        assert_eq!(&f[22..31], &[0x25, 0, 0, 1, 0, 0, 0, 0, 0xc0]);
    }
    #[test]
    fn repeated_reference_parameters_have_reference_tokens_and_reader_formulas() {
        for (name, id) in [("MEDIAN", 227u16), ("AVEDEV", 269), ("SUMSQ", 321)] {
            let mut w = book();
            let formula = format!("{name}($A$1,$A$2:$A$3,$B$1)");
            w.sheet_mut(0)
                .unwrap()
                .set_formula("C1".parse().unwrap(), &formula, CellValue::Number(-999.))
                .unwrap();
            let bytes = write_xls_to_buffer(&w).unwrap();
            let raw = stream(bytes.clone());
            let rs = records(&raw);
            let f = rs.iter().find(|(id, _, _)| *id == 6).unwrap().2;
            // Independent record bytes: RefR, AreaR, RefR, FuncVarV.
            let mut expected = vec![
                0x24, 0, 0, 0, 0, 0x25, 1, 0, 2, 0, 0, 0, 0, 0, 0x24, 0, 0, 1, 0, 0x42, 3,
            ];
            expected.extend(id.to_le_bytes());
            assert_eq!(&f[22..], expected, "{name} operand classes");
            let mut reader = Xls::new(Cursor::new(bytes)).unwrap();
            let formulas = reader.worksheet_formula("Data").unwrap();
            assert_eq!(
                formulas.get_value((0, 2)).map(String::as_str),
                Some(formula.as_str())
            );
        }
    }
    #[test]
    fn mixed_prefix_and_implicit_repeated_tail_use_checked_classes() {
        let w = book();
        let e = parse_formula("NPV($A$1,$A$2,$A$3)").unwrap();
        let tokens = rpn::compile(&w, 0, &e, 0x20, 8170).unwrap();
        assert_eq!(tokens[0], 0x44); // first parameter is Value
        assert_eq!(tokens[5], 0x24); // explicit Reference parameter
        assert_eq!(tokens[10], 0x24); // implicit repeat-last Reference
        // SUMPRODUCT's repeated A-class parameters now transport references;
        // constants/computed arrays and array-result functions remain refused.
        let e = parse_formula("SUMPRODUCT(A1,A2)").unwrap();
        let tokens = rpn::compile(&w, 0, &e, 0x20, 8170).unwrap();
        assert_eq!(
            tokens,
            [0x64, 0, 0, 0, 0xc0, 0x64, 1, 0, 0, 0xc0, 0x42, 2, 228, 0]
        );
        for formula in [
            "SUMPRODUCT({1,2},A2)",
            "SUMPRODUCT(A1+1,A2)",
            "FREQUENCY(A1,A2)",
            "MEDIAN()",
        ] {
            let e = parse_formula(formula).unwrap();
            assert!(rpn::compile(&w, 0, &e, 0x20, 8170).is_err(), "{formula}");
        }
    }
    #[test]
    fn real_poi_biff_fixture_has_matching_structural_contract() {
        // Reuse the independently sourced, hash-documented repository fixture;
        // tests must not require a separate Apache POI checkout. The byte-identical
        // poirot-io fixture is copied locally for package tests; see root LICENSES/ACKNOWLEDGEMENTS.
        let original = include_bytes!("../tests/fixtures/poi-SimpleMultiCell.xls");
        let mut c = cfb::CompoundFile::open(Cursor::new(original)).unwrap();
        let mut original = Vec::new();
        c.open_stream("/Workbook")
            .unwrap()
            .read_to_end(&mut original)
            .unwrap();
        let original = records(&original);
        let generated = stream(write_xls_to_buffer(&book()).unwrap());
        let generated = records(&generated);
        for id in [0x0809, 0x0085, 0x00e0, 0x0200, 0x000a] {
            assert!(
                original.iter().any(|(actual, _, _)| *actual == id),
                "POI missing{id:04x}"
            );
            assert!(
                generated.iter().any(|(actual, _, _)| *actual == id),
                "Poirot missing{id:04x}"
            );
        }
        assert_eq!(original[0].2[..4], generated[0].2[..4]);
    }
}

#[cfg(test)]
mod exact_text_tests;
