//! Independent arithmetic contracts for typed statistical execution strategies.
use poirot::eval::{EvaluationOutcome, run_formula};
use poirot::model::{CellAddress, CellValue, Workbook};

/// Evaluate a public formula and compare its numeric result with independent arithmetic.
fn number(formula: &str, expected: f64) {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let EvaluationOutcome::Complete {
        value: CellValue::Number(actual),
        ..
    } = run_formula(
        &workbook,
        0,
        CellAddress { row: 10, col: 10 },
        formula,
        Default::default(),
        Default::default(),
    )
    else {
        panic!("formula did not produce a number: {formula}");
    };
    assert!(
        (actual - expected).abs() < 1e-12,
        "{formula}: {actual} != {expected}"
    );
}

#[test]
fn scalar_distributions_and_order_parameters_broadcast_independently() {
    // Each standard-normal CDF at zero is one half by symmetry.
    number("SUM(NORM.S.DIST({0,0},TRUE))", 1.0);
    // The two largest members of the fixed three-element sample are three and two.
    number("SUM(LARGE({1;3;2},{1,2}))", 5.0);
    // The fitted line is y=2x; broadcasting x=2 and x=3 produces four and six.
    number("SUM(FORECAST.LINEAR({2,3},{2;4},{1;2}))", 10.0);
}

#[test]
fn statistical_collections_keep_origin_and_aligned_exclusions() {
    // Text excludes its whole pair; the remaining points lie exactly on y=2x.
    number("CORREL({1;\"x\";3},{2;99;6})", 1.0);
    number("SLOPE({2;\"x\";6},{1;2;3})", 2.0);
    // A-family variance admits TRUE as one and text as zero: variance(1,1,0)=1/3.
    number("VARA(1,TRUE,\"x\")", 1.0 / 3.0);
}

#[test]
fn regression_maps_known_y_then_known_x_to_domain_coordinates() {
    // The points (1,5), (2,7), (3,9) define y=2x+3 exactly.
    number("SLOPE({5;7;9},{1;2;3})", 2.0);
    number("INTERCEPT({5;7;9},{1;2;3})", 3.0);
    number("FORECAST(4,{5;7;9},{1;2;3})", 11.0);
    number("SUM(FORECAST.LINEAR({4,5},{5;7;9},{1;2;3}))", 24.0);
    // Positional pair operations retain their original first-minus-second order.
    number("SUMX2MY2({1;2},{2;4})", -15.0);
}

#[test]
fn countblank_reduces_sparse_geometry_without_narrowing_the_count() {
    // An empty Excel grid has 1,048,576 rows and 16,384 columns.
    number("COUNTBLANK(A1:XFD1048576)", 1_048_576.0 * 16_384.0);
}

#[test]
fn lifted_statistics_discover_spill_children_before_their_anchor_is_requested() {
    use poirot::eval::EvaluationTarget;
    use poirot::eval::overlay::{EvaluationContext, Overlay};
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    for (row, formula, expected) in [
        (0, "NORM.S.DIST({0,0},TRUE)", 0.5),
        (2, "RANK.EQ({3,1},{1;2;3})", 3.0),
        (4, "LARGE({1;2;3},{1,2})", 2.0),
        (6, "FORECAST.LINEAR({2,3},{2;4},{1;2})", 6.0),
    ] {
        overlay
            .set_formula(
                EvaluationTarget {
                    sheet: 0,
                    address: CellAddress { row, col: 0 },
                },
                formula,
            )
            .unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(EvaluationTarget {
                    sheet: 0,
                    address: CellAddress { row, col: 1 }
                })
                .unwrap(),
            CellValue::Number(expected)
        );
    }
}
