#[cfg(test)]
mod consumer_tests {

    use crate::model::NamedRange;
    use crate::model::Workbook;

    #[test]
    fn model_lookup_duplicate_and_scope_share_policy() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Ä Data").unwrap();
        workbook.add_sheet("Other").unwrap();
        assert_eq!(workbook.try_sheet_index_by_name("ä dATA"), Ok(Some(0)));
        assert_eq!(workbook.sheet_by_name("ä data").unwrap().name(), "Ä Data");
        assert!(workbook.add_sheet("ä DATA").is_err());
        assert!(workbook.rename_sheet(1, "ä data").apply().is_err());
        workbook.define_name("ÉRate", "1", None).unwrap();
        workbook.define_name("érate", "2", Some(0)).unwrap();
        assert!(workbook.define_name("ÉRATE", "3", Some(0)).is_err());
        assert_eq!(
            workbook
                .try_defined_name("éRATE", 0)
                .unwrap()
                .unwrap()
                .formula,
            "2"
        );
        assert_eq!(
            workbook
                .try_defined_name("érate", 1)
                .unwrap()
                .unwrap()
                .formula,
            "1"
        );
        assert_eq!(workbook.named_ranges[0].name, "ÉRate");
    }

    #[test]
    fn unresolved_case_is_diagnostic_and_raw_import_stays_permissive() {
        let mut workbook = Workbook::new();
        workbook.import_sheet("Σ").unwrap();
        assert!(workbook.try_sheet_index_by_name("σ").is_err());
        assert!(
            matches!(workbook.add_sheet("σ"), Err(crate::model::ModelError::Identifier(error)) if error.stored == "Σ" && error.requested == "σ")
        );
        workbook.import_sheet("σ").unwrap();
        workbook
            .import_sheet("'Imported invalid authoring name'")
            .unwrap();
        assert_eq!(workbook.try_sheet_index_by_name("σ"), Ok(Some(1)));
        workbook.named_ranges.push(NamedRange {
            name: "Σ".into(),
            formula: "1".into(),
            sheet_scope: Some(0),
        });
        workbook.named_ranges.push(NamedRange {
            name: "σ".into(),
            formula: "2".into(),
            sheet_scope: None,
        });
        assert!(workbook.try_defined_name("σ", 0).is_err());
        assert_eq!(
            workbook.try_defined_name("σ", 1).unwrap().unwrap().formula,
            "2"
        );
    }
}

#[cfg(test)]
mod utf16_length_consumer_tests {

    use crate::model::Workbook;

    #[test]
    fn distinct_utf16_lengths_allow_sheet_creation_and_checked_lookup() {
        for (stored, requested) in [("Σ", "Étage"), ("😀", "a")] {
            let mut workbook = Workbook::new();
            workbook.add_sheet(stored).unwrap();
            assert_eq!(workbook.try_sheet_index_by_name(requested), Ok(None));
            workbook.add_sheet(requested).unwrap();
            assert_eq!(workbook.try_sheet_index_by_name(requested), Ok(Some(1)));
            assert_eq!(workbook.try_sheet_index_by_name(stored), Ok(Some(0)));
        }
    }

    #[test]
    fn equal_utf16_lengths_keep_sheet_creation_and_lookup_diagnostics() {
        for (stored, requested) in [("Σ", "σ"), ("😀", "ab")] {
            let mut workbook = Workbook::new();
            workbook.add_sheet(stored).unwrap();
            assert!(workbook.try_sheet_index_by_name(requested).is_err());
            assert!(
                matches!(workbook.add_sheet(requested), Err(crate::model::ModelError::Identifier(error)) if error.stored == stored && error.requested == requested)
            );
            assert_eq!(workbook.sheets().len(), 1);
        }
    }
}
