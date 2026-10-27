use super::*;
fn text(s: &str) -> Cell {
    Cell::Text(s.into())
}
#[test]
fn numeric_criteria_type_rules() {
    assert!(Criterion::parse(&text("2")).matches(&text("2")));
    assert!(Criterion::parse(&text("<>2")).matches(&text("2")));
    assert!(!Criterion::parse(&text(">1")).matches(&text("2")));
    assert!(!Criterion::parse(&Cell::Number(0.0)).matches(&Cell::Blank));
    assert!(Criterion::parse(&text("<>0")).matches(&Cell::Blank));
}
#[test]
fn blank_empty_and_boolean_rules() {
    assert!(!Criterion::parse(&Cell::Blank).matches(&Cell::Blank));
    assert!(Criterion::parse(&text("")).matches(&text("")));
    assert!(!Criterion::parse(&text("=")).matches(&text("")));
    assert!(Criterion::parse(&text("=")).matches(&Cell::Blank));
    assert!(!Criterion::parse(&text("TRUE")).matches(&text("TRUE")));
    assert!(Criterion::parse(&text("TRUE")).matches(&Cell::Bool(true)));
}
#[test]
fn wildcard_escape_and_metacharacters() {
    assert!(wildcard_match("a~*?", "A*x"));
    assert!(!wildcard_match("a~*?", "abx"));
    assert!(wildcard_match("[a].*", "[a].xyz"));
    assert!(!wildcard_match("*", "a\nb"));
    assert!(wildcard_match("~~", "~"));
}
#[test]
fn aggregation_shape_and_selected_error() {
    let criteria = [Cell::Number(1.0), Cell::Number(0.0)];
    let target = [Cell::Number(4.0), Cell::Error(CriteriaError::Na)];
    let r = Range {
        rows: 2,
        columns: 1,
        cells: &criteria,
    };
    let t = Range {
        rows: 2,
        columns: 1,
        cells: &target,
    };
    let plan = CriteriaPlan::new(
        AggregateKind::Sum,
        vec![r],
        vec![Criterion::parse(&Cell::Number(1.0))],
        Some(t),
        true,
    )
    .unwrap();
    assert_eq!(plan.evaluate(), Ok(4.0));
    assert!(
        CriteriaPlan::new(
            AggregateKind::Sum,
            vec![r],
            vec![Criterion::parse(&Cell::Number(1.0))],
            Some(Range {
                rows: 1,
                columns: 2,
                cells: &target
            }),
            true
        )
        .is_err()
    );
}
#[test]
fn nonnumeric_extrema_and_empty_average() {
    let cells = [text("x"), Cell::Number(-2.0)];
    let criteria = [Cell::Number(1.0), Cell::Number(1.0)];
    let r = Range {
        rows: 2,
        columns: 1,
        cells: &criteria,
    };
    let t = Range {
        rows: 2,
        columns: 1,
        cells: &cells,
    };
    let p = CriteriaPlan::new(
        AggregateKind::Max,
        vec![r],
        vec![Criterion::parse(&Cell::Number(1.0))],
        Some(t),
        true,
    )
    .unwrap();
    assert_eq!(p.evaluate(), Ok(0.0));
    let p = CriteriaPlan::new(
        AggregateKind::Average,
        vec![r],
        vec![Criterion::parse(&Cell::Number(2.0))],
        Some(t),
        true,
    )
    .unwrap();
    assert_eq!(p.evaluate(), Err(CriteriaError::DivZero));
}
#[test]
fn subtotal_visibility_and_nested_metadata() {
    use subtotal::*;
    let nums = [Cell::Number(2.0), Cell::Number(3.0), Cell::Number(100.0)];
    let cells = [
        SubtotalCell {
            value: &nums[0],
            hidden: false,
            nested_subtotal: false,
            referenced: true,
        },
        SubtotalCell {
            value: &nums[1],
            hidden: true,
            nested_subtotal: false,
            referenced: true,
        },
        SubtotalCell {
            value: &nums[2],
            hidden: false,
            nested_subtotal: true,
            referenced: true,
        },
    ];
    assert_eq!(subtotal(9, &cells), Ok(5.0));
    assert_eq!(subtotal(109, &cells), Ok(2.0));
    assert_eq!(subtotal(7, &cells), Ok(0.5f64.sqrt()));
}
