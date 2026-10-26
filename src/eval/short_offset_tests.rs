//! Modern OFFSET grid checks follow Microsoft documentation; direct SignedArea
//! tests retain independent logical padding and overflowed-source invariants.
use super::*;
use signed_reference::{GridRead, SignedArea, SourceDimension};
fn fixture() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        ("A1", 1.),
        ("B1", 10.),
        ("A2", 2.),
        ("B2", 20.),
        ("A3", 3.),
        ("B3", 30.),
        ("A1048576", 17.),
        ("XFD1", 23.),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    w
}
fn formula(w: &Workbook, expression: &str) -> CellValue {
    let plan =
        bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(expression).unwrap())
            .unwrap();
    match crate::eval::run_plan_with_configuration(
        w,
        0,
        "P1".parse().unwrap(),
        &plan,
        &RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi)
            .with_array_result_policy(ArrayResultPolicy::ScalarIntersection),
        Default::default(),
    ) {
        crate::eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
#[test]
fn modern_offset_crosses_short_boundaries_without_narrowing() {
    let w = fixture();
    for (expression, value) in [
        ("SUM(OFFSET(A1,32768,0,1,1))", 0.),
        ("ROW(OFFSET(A1,32768,0,1,1))", 32769.),
        ("ROWS(OFFSET(A1,32768,0,1,1))", 1.),
        ("COUNTBLANK(OFFSET(A1,32768,0,1,1))", 1.),
        ("SUM(OFFSET(A1,65536,0,1,1))", 0.),
        ("SUM(OFFSET(OFFSET(A1,65535,0,1,1),1,0))", 0.),
        ("ROWS(OFFSET(A1,32767,0,2,1))", 2.),
        ("COUNTBLANK(OFFSET(A1,32767,0,2,1))", 2.),
        ("SUM(OFFSET(A1,32767,0,2,1))", 0.),
        ("SUM(OFFSET(OFFSET(A1,32768,0,1,1),32768,0))", 0.),
        ("SUMPRODUCT(OFFSET(A1,32768,0,2,1),{1;1})", 0.),
    ] {
        assert_eq!(formula(&w, expression), Number(value), "{expression}");
    }
    assert_eq!(formula(&w, "ROW(OFFSET(A1,2147483647,0,1,1))"), Error(Ref));
    assert_eq!(
        formula(&w, "ROWS(OFFSET(A1,2147483647,0,-2147483648,1))"),
        Error(Ref)
    );
    assert_eq!(formula(&w, "SUM(OFFSET(A2,2147483647,0,1,1))"), Error(Ref));
    assert_eq!(formula(&w, "SUM(OFFSET(A1,0,0,-2147483648,1))"), Error(Ref));
}
#[test]
fn modern_offset_rejects_upper_grid_and_nested_reentry() {
    let w = fixture();
    for expression in [
        "SUM(OFFSET(A1048576,1,0,1,1))",
        "ROW(OFFSET(A1048576,1,0,1,1))",
        "COUNTBLANK(OFFSET(A1048576,1,0,1,1))",
        "SUM(OFFSET(OFFSET(A1048576,1,0,1,1),-1,0))",
        "SUM(OFFSET(XFD1,0,16385,1,1))",
        "COLUMN(OFFSET(XFD1,0,16385,1,1))",
        "COUNTBLANK(OFFSET(XFD1,0,16385,1,1))",
        "SUM(OFFSET(OFFSET(XFD1,0,16385,1,1),0,-16385))",
    ] {
        assert_eq!(formula(&w, expression), Error(Ref), "{expression}");
    }
    let args = [bound::bind(
        &poirot_parsing::formula::parse_formula("OFFSET(A1048576,1,0,1,1)").unwrap(),
    )];
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    let mut host = host::CallHost::new(&mut session, 0, "P1".parse().unwrap(), &args);
    let blank = host.read_cell(0, 0, 0).unwrap();
    assert_eq!(blank.value, Error(Ref));
    assert_eq!(host.shape(0).unwrap(), (1, 1));
}
fn area(rows: (i32, i32)) -> SignedArea {
    SignedArea {
        kind: AreaKind::Area,
        sheet: 0,
        r0: rows.0,
        r1: rows.1,
        c0: 0,
        c1: 0,
    }
}
#[test]
fn wrapped_positive_dimension_physical_projection_matches_signed_arc() {
    let near_max = area((i32::MAX, i32::MIN));
    assert_eq!(near_max.row_dimension(), SourceDimension::Positive(2));
    assert!(near_max.physical().is_none());
    assert_eq!(near_max.read(0, 0), Some(GridRead::Blank));
    assert_eq!(near_max.read(1, 0), Some(GridRead::Blank));
    assert_eq!(near_max.read(2, 0), None);
    // A positive Java dimension crossing MAX->MIN has only its starting positive segment in-grid.
    let crossing = area((10, i32::MIN));
    assert!(matches!(
        crossing.row_dimension(),
        SourceDimension::Positive(_)
    ));
    let physical = crossing.physical().unwrap();
    assert_eq!((physical.r0, physical.r1), (10, MAX_ROWS - 1));
    assert_eq!(
        crossing.read(0, 0),
        Some(GridRead::Cell {
            sheet: 0,
            address: CellAddress { row: 10, col: 0 }
        })
    );
    assert_eq!(
        crossing.read((i32::MAX as u64) - 9, 0),
        Some(GridRead::Blank)
    );
    // Negative->positive nonoverflowing dimensions cross zero normally and retain physicalgrid.
    let across_zero = area((-32768, 32767));
    assert_eq!(
        across_zero.row_dimension(),
        SourceDimension::Positive(65536)
    );
    let physical = across_zero.physical().unwrap();
    assert_eq!((physical.r0, physical.r1), (0, 32767));
    assert_eq!(
        across_zero.read(32768, 0),
        Some(GridRead::Cell {
            sheet: 0,
            address: CellAddress { row: 0, col: 0 }
        })
    );
    assert_eq!(
        area((i32::MIN, 0)).row_dimension(),
        SourceDimension::NegativeOverflow(-2147483647)
    );
    assert_eq!(
        area((i32::MIN, i32::MAX)).row_dimension(),
        SourceDimension::Zero
    );
}
#[test]
fn zero_negative_source_dimensions_keep_metadata_and_empty_sparse_loop_policy() {
    let w = fixture();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    for (logical, dimension) in [
        (area((i32::MIN, 0)), -2147483647),
        (area((i32::MIN, i32::MAX)), 0),
    ] {
        assert_eq!(logical.row_dimension().value(), dimension);
        assert_eq!(logical.size(), 0);
        assert!(logical.physical().is_none());
        assert!(
            session
                .items(Value::SignedRefs(vec![logical]))
                .unwrap()
                .is_empty()
        );
        // OperandResolver contained absolute-coordinate selection is independent of overflowing dimension.
        assert_eq!(
            logical.implicit("A1".parse().unwrap()),
            Some(GridRead::Cell {
                sheet: 0,
                address: "A1".parse().unwrap()
            })
        );
        assert!(logical.positive_shape().is_none());
        assert!(matches!(
            session.matrix(Value::SignedRefs(vec![logical])),
            Err(EvaluationFailure::UnsupportedCapability { .. })
        ));
    }
}
#[test]
fn modern_offset_small_area_stays_sparse_under_small_materialization_budget() {
    let w = fixture();
    let plan = bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula("SUM(OFFSET(A1,32767,0,2,1))").unwrap(),
    )
    .unwrap();
    assert!(matches!(
        run_plan(
            &w,
            0,
            "P1".parse().unwrap(),
            &plan,
            EvaluationInputs::default(),
            EvaluationBudgets {
                materialized_values: 4,
                ..EvaluationBudgets::default()
            }
        ),
        EvaluationOutcome::Complete {
            value: Number(0.),
            ..
        }
    ));
}
