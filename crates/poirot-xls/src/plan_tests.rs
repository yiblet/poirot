use super::*;

#[test]
fn add_in_raw_records_for_defined_name_and_cell_share_slots() {
    let mut w = Workbook::new();
    w.add_sheet("Data")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "EDATE(3,4)")
        .unwrap();
    w.define_name("Advance", "edate(1,2)", None).unwrap();
    let plan = XlsExportPlan::new(&w, XlsExportPolicy::default()).unwrap();
    assert_eq!(plan.add_in_names(), &["edate"]);
    assert_eq!(
        plan.names[0].tokens(),
        &[
            0x59, 1, 0, 1, 0, 0, 0, 0x1e, 1, 0, 0x1e, 2, 0, 0x42, 3, 0xff, 0,
        ]
    );
    let stream = plan.encode_workbook_stream().unwrap();
    let mut links = Vec::new();
    let mut offset = 0;
    while offset < stream.len() {
        let id = u16::from_le_bytes([stream[offset], stream[offset + 1]]);
        let len = u16::from_le_bytes([stream[offset + 2], stream[offset + 3]]) as usize;
        if matches!(id, 0x01ae | 0x0023 | 0x0017) {
            links.push((id, stream[offset + 4..offset + 4 + len].to_vec()));
        }
        offset += 4 + len;
    }
    assert_eq!(
        links,
        vec![
            (0x01ae, vec![1, 0, 1, 4]),
            (0x01ae, vec![1, 0, 1, 0x3a]),
            (
                0x0023,
                vec![
                    0, 0, 0, 0, 0, 0, 5, 0, b'e', b'd', b'a', b't', b'e', 2, 0, 0x1c, 0x17
                ]
            ),
            (
                0x0017,
                vec![2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0xfe, 0xff, 0xfe, 0xff]
            ),
        ]
    );
    assert_eq!(
        plan.names[0].add_in_references(),
        plan.cells[&(0, "A1".parse().unwrap())].add_in_references()
    );
}

#[test]
fn add_in_tokens_links_and_exact_stream_budget() {
    let mut w = Workbook::new();
    let sheet = w.add_sheet("Data").unwrap();
    for (address, formula) in [
        ("A1", "edate(1,2)"),
        ("A2", "YEARFRAC(1,2,3)"),
        ("A3", "_xlfn.IFERROR(1/0,4)"),
        ("A4", "EDATE(3,4)"),
    ] {
        sheet
            .set_formula_with_cached_value(
                address.parse().unwrap(),
                formula,
                CellValue::Number(-999.),
            )
            .unwrap();
    }
    let plan = XlsExportPlan::new(&w, XlsExportPolicy::default()).unwrap();
    assert_eq!(plan.add_in_names(), &["edate", "YEARFRAC", "_xlfn.IFERROR"]);
    assert_eq!(
        plan.cells[&(0, "A1".parse().unwrap())].tokens(),
        &[
            0x59, 1, 0, 1, 0, 0, 0, 0x1e, 1, 0, 0x1e, 2, 0, 0x42, 3, 0xff, 0
        ]
    );
    assert_eq!(
        plan.cells[&(0, "A2".parse().unwrap())].tokens(),
        &[
            0x59, 1, 0, 2, 0, 0, 0, 0x1e, 1, 0, 0x1e, 2, 0, 0x1e, 3, 0, 0x42, 4, 0xff, 0
        ]
    );
    assert_eq!(
        plan.cells[&(0, "A4".parse().unwrap())].add_in_references(),
        &[BiffAddInReference {
            stored_name: "edate".into(),
            extern_sheet_index: 1,
            name_number: 1
        }]
    );
    let stream = plan.encode_workbook_stream().unwrap();
    let mut records = Vec::new();
    let mut offset = 0;
    while offset < stream.len() {
        let id = u16::from_le_bytes([stream[offset], stream[offset + 1]]);
        let len = u16::from_le_bytes([stream[offset + 2], stream[offset + 3]]) as usize;
        records.push((id, offset, &stream[offset + 4..offset + 4 + len]));
        offset += 4 + len;
    }
    let ext = records.iter().find(|(id, _, _)| *id == 0x17).unwrap().2;
    assert_eq!(ext, &[2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0xfe, 0xff, 0xfe, 0xff]);
    let size = plan.predicted_stream_bytes();
    assert_eq!(stream.len(), size);
    let exact = XlsExportPolicy {
        max_stream_bytes: size,
        ..XlsExportPolicy::default()
    };
    assert_eq!(
        XlsExportPlan::new(&w, exact)
            .unwrap()
            .encode_workbook_stream()
            .unwrap()
            .len(),
        size
    );
    assert!(
        XlsExportPlan::new(
            &w,
            XlsExportPolicy {
                max_stream_bytes: size - 1,
                ..exact
            }
        )
        .is_err()
    );
}
#[test]
fn unsupported_add_in_and_defined_function_shadow_fail() {
    for formula in [
        "RUNTIME_UDF(1)",
        "_XLFN.IFERROR(1,2)",
        "_xlfn.UNKNOWN(1)",
        "EDATE(1)",
        "YEARFRAC(1,2,3,4)",
    ] {
        let mut w = Workbook::new();
        w.add_sheet("Data")
            .unwrap()
            .set_formula("A1".parse().unwrap(), formula)
            .unwrap();
        assert!(
            XlsExportPlan::new(&w, XlsExportPolicy::default()).is_err(),
            "{formula}"
        );
    }
    let mut w = Workbook::new();
    w.add_sheet("Data")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "EDATE(1,2)")
        .unwrap();
    w.define_name("EDATE", "1", Some(0)).unwrap();
    assert!(
        XlsExportPlan::new(&w, XlsExportPolicy::default())
            .err()
            .unwrap()
            .to_string()
            .contains("shadows add-in")
    );
}
#[test]
fn planned_size_matches_records_and_continuations() {
    let mut w = Workbook::new();
    let sheet = w.add_sheet("Data").unwrap();
    for (address, text) in [
        ("A1", "😀".repeat(5000)),
        ("A2", "x".repeat(8220)),
        ("A3", "small".into()),
    ] {
        sheet
            .set_value(address.parse().unwrap(), CellValue::Text(text))
            .unwrap();
    }
    sheet
        .set_formula_with_cached_value(
            "B1".parse().unwrap(),
            "1+2",
            CellValue::Text("😀".repeat(6000)),
        )
        .unwrap();
    w.define_name("Local", "Data!A1", Some(0)).unwrap();
    let plan = XlsExportPlan::new(&w, XlsExportPolicy::default()).unwrap();
    let stream = plan.encode_workbook_stream().unwrap();
    assert_eq!(plan.predicted_stream_bytes(), stream.len());
    assert_eq!(stream, plan.encode_workbook_stream().unwrap());
    let mut pos = 0;
    while pos < stream.len() {
        let size = u16::from_le_bytes([stream[pos + 2], stream[pos + 3]]) as usize;
        assert!(size <= MAX_RECORD);
        pos += 4 + size;
    }
    assert_eq!(pos, stream.len());
}
#[test]
fn limits_and_identity_checks_precede_formula_parsing() {
    let mut w = Workbook::new();
    w.add_sheet("Data")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "invalid(")
        .unwrap();
    let policy = XlsExportPolicy {
        max_cells: 0,
        ..XlsExportPolicy::default()
    };
    assert!(matches!(
        XlsExportPlan::new(&w, policy),
        Err(XlsError::ResourceLimit(_))
    ));
    w.named_ranges.push(poirot_model::NamedRange {
        name: String::new(),
        formula: "invalid(".into(),
        sheet_scope: None,
    });
    assert!(
        XlsExportPlan::new(&w, XlsExportPolicy::default())
            .err()
            .unwrap()
            .to_string()
            .contains("invalid BIFF defined name")
    );
}
#[test]
fn invalid_plan_leaves_existing_file_untouched_and_io_retains_source() {
    let path = std::env::temp_dir().join(format!("poirot-xls-plan-{}", std::process::id()));
    std::fs::write(&path, b"existing").unwrap();
    assert!(write_xls(&Workbook::new(), &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    std::fs::remove_file(&path).unwrap();
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    let error = write_xls(&w, path.join("missing")).unwrap_err();
    assert!(matches!(&error, XlsError::Io { .. }));
    assert!(std::error::Error::source(&error).is_some());
}
#[test]
fn stream_budget_preflights_strings_and_formula_caches_before_parsing() {
    for cached in [false, true] {
        let mut w = Workbook::new();
        let sheet = w.add_sheet("Data").unwrap();
        let value = CellValue::Text("😀".repeat(1000));
        if cached {
            sheet
                .set_formula_with_cached_value("A1".parse().unwrap(), "invalid(", value)
                .unwrap();
        } else {
            sheet.set_value("A1".parse().unwrap(), value).unwrap();
            sheet
                .set_formula("A2".parse().unwrap(), "invalid(")
                .unwrap();
        }
        let policy = XlsExportPolicy {
            max_stream_bytes: 300,
            ..XlsExportPolicy::default()
        };
        let error = XlsExportPlan::new(&w, policy).err().unwrap();
        assert!(
            matches!(error, XlsError::Context { ref source, .. } if matches!(source.as_ref(), XlsError::ResourceLimit(_)))
        );
        assert!(error.to_string().contains("Data!A1"));
    }
}
#[test]
fn token_budget_stops_before_later_unsupported_subtree() {
    let mut w = Workbook::new();
    w.add_sheet("Data")
        .unwrap()
        .set_formula_with_cached_value(
            "A1".parse().unwrap(),
            "SUM(1,2,FREQUENCY(A1,A2))",
            CellValue::Number(-999.),
        )
        .unwrap();
    let policy = XlsExportPolicy {
        max_formula_tokens: 3,
        ..XlsExportPolicy::default()
    };
    let error = XlsExportPlan::new(&w, policy).err().unwrap();
    assert!(
        matches!(error, XlsError::Context { ref source, .. } if matches!(source.as_ref(), XlsError::ResourceLimit(_)))
    );
    assert!(error.to_string().contains("Data!A1"));
}
#[test]
fn exact_stream_budget_and_formula_token_boundaries() {
    let mut w = Workbook::new();
    w.add_sheet("Data")
        .unwrap()
        .set_formula_with_cached_value("A1".parse().unwrap(), "1+2", CellValue::Number(3.))
        .unwrap();
    let size = XlsExportPlan::new(&w, XlsExportPolicy::default())
        .unwrap()
        .predicted_stream_bytes();
    let policy = XlsExportPolicy {
        max_stream_bytes: size,
        max_formula_tokens: 7,
        ..XlsExportPolicy::default()
    };
    let plan = XlsExportPlan::new(&w, policy).unwrap();
    assert_eq!(plan.encode_workbook_stream().unwrap().len(), size);
    assert_eq!(
        plan.cells[&(0, "A1".parse().unwrap())].predicted_record_bytes(),
        33
    );
    assert!(
        XlsExportPlan::new(
            &w,
            XlsExportPolicy {
                max_stream_bytes: size - 1,
                ..policy
            }
        )
        .is_err()
    );
    assert!(
        XlsExportPlan::new(
            &w,
            XlsExportPolicy {
                max_formula_tokens: 6,
                ..policy
            }
        )
        .is_err()
    );
}
#[test]
fn imported_name_policy_relaxes_only_authoring_syntax() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    w.named_ranges.push(poirot_model::NamedRange {
        name: "Imported name".into(),
        formula: "1".into(),
        sheet_scope: None,
    });
    assert!(XlsExportPlan::new(&w, XlsExportPolicy::default()).is_err());
    let policy = XlsExportPolicy {
        strict_names: false,
        ..XlsExportPolicy::default()
    };
    let plan = XlsExportPlan::new(&w, policy).unwrap();
    assert_eq!(
        plan.predicted_stream_bytes(),
        plan.encode_workbook_stream().unwrap().len()
    );
}
