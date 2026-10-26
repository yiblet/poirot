//! Collection GCD is independent of caller coordinates; @ selects a scalar explicitly.
//! https://support.microsoft.com/en-us/excel/functions/gcd-function
use poirot::model::{CellValue, Workbook};

#[test]
fn gcd_reduces_complete_literal_and_computed_arrays_at_every_caller() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    // Every collection below contains only even numbers, including two.
    for address in ["A1", "B1", "B2", "J10"] {
        for formula in [
            "GCD({2,4})",
            "GCD({2;4})",
            "GCD({2,4;6,8})",
            "GCD(IFS(TRUE,{2,4}))",
            "GCD(MMULT({1,0;0,1},{2,4;6,8}))",
        ] {
            assert!(
                matches!(
                    poirot_eval::run_formula(
                        &workbook,
                        0,
                        address.parse().unwrap(),
                        formula,
                        Default::default(),
                        Default::default()
                    ),
                    poirot_eval::EvaluationOutcome::Complete {
                        value: CellValue::Number(2.),
                        ..
                    }
                ),
                "{address}: {formula}"
            );
        }
    }
}

#[test]
fn explicit_reference_intersection_selects_the_callers_member_before_gcd() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Sheet1").unwrap();
    for (address, value) in [("A1", 2.), ("B1", 4.), ("A2", 6.), ("B2", 8.)] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (formula, expected) in [("GCD(A1:B2)", 2.), ("GCD(@A1:A2)", 6.)] {
        assert!(
            matches!(poirot_eval::run_formula(&workbook,0,"B2".parse().unwrap(),formula,Default::default(),Default::default()),poirot_eval::EvaluationOutcome::Complete {value:CellValue::Number(n),..} if n==expected),
            "{formula}"
        );
    }
}
