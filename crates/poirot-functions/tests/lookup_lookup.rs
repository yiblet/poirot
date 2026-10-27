use poirot_functions::lookup::*;

/// In-memory lookup host for reference, metadata, and search contract tests.
struct Book {
    reads: Vec<(u32, u32, u32)>,
    queries: Vec<DynamicReference>,
}
impl Workbook for Book {
    fn preflight(&mut self, _: BudgetRequest) -> Result<(), LookupError> {
        Ok(())
    }
    fn argument(&mut self, _: usize, _: Context) -> Result<Value, LookupError> {
        Err(LookupError::Unsupported(
            LookupUnsupportedError::DeferredArguments,
        ))
    }
    fn coerce_numeric_text(&mut self, text: &str) -> Result<f64, LookupError> {
        text.trim()
            .parse()
            .map_err(|_| LookupError::Spreadsheet(LookupValueError::Value))
    }
    fn coerce_number_to_text(&mut self, number: f64) -> Result<String, LookupError> {
        Ok(number.to_string())
    }
    fn cell(
        &mut self,
        _: &Reference,
        sheet: u32,
        row: u32,
        col: u32,
    ) -> Result<Scalar, LookupError> {
        self.reads.push((sheet, row, col));
        Ok(Scalar::Number((100 * sheet + 10 * row + col) as f64))
    }

    fn dynamic_reference(
        &mut self,
        q: &DynamicReference,
        _: Context,
    ) -> Result<Value, LookupError> {
        self.queries.push(q.clone());
        if q.first == "bad" {
            Err(LookupValueError::Name.into())
        } else {
            Ok(area(2, 0, 0, 0, 0))
        }
    }
    fn structured_reference(&mut self, text: &str, _: Context) -> Result<Value, LookupError> {
        if text == "Table[Column]" {
            Ok(area(2, 0, 2, 1, 1))
        } else {
            Err(LookupValueError::Ref.into())
        }
    }
}
fn book() -> Book {
    Book {
        reads: Vec::new(),
        queries: Vec::new(),
    }
}
fn area(sheet: u32, fr: u32, lr: u32, fc: u32, lc: u32) -> Value {
    Value::Reference(
        Reference::new(
            ReferenceKind::Area,
            Some("source.xlsx".into()),
            sheet..=sheet,
            fr..=lr,
            fc..=lc,
        )
        .unwrap(),
    )
}
fn n(n: f64) -> Value {
    Value::Scalar(Scalar::Number(n))
}
fn s(s: &str) -> Value {
    Value::Scalar(Scalar::Text(s.into()))
}
fn error(e: LookupValueError) -> Value {
    Value::Scalar(Scalar::Error(e))
}
fn array(rows: usize, cols: usize, values: &[f64]) -> Value {
    Value::Array(
        Array::new(
            rows,
            cols,
            values.iter().copied().map(Scalar::Number).collect(),
        )
        .unwrap(),
    )
}
const C: Context = Context {
    sheet: 0,
    row: 3,
    col: 4,
};
fn run(f: ReferenceFunction, args: &[Value]) -> Result<Value, LookupError> {
    evaluate(Function::Reference(f), args, C, &mut book())
}

#[test]
fn indirect_parses_quoted_workbook_and_trims_range_only() {
    assert_eq!(
        parse_dynamic_reference("'[O''Brien.xlsx]Data'! A1 : B2 ", true, &mut book()),
        Ok(DynamicReference {
            workbook: Some("O'Brien.xlsx".into()),
            sheet: Some("Data".into()),
            first: "A1".into(),
            last: Some("B2".into()),
            a1: true
        })
    );
    assert_eq!(
        parse_dynamic_reference(" Data!A1", true, &mut book()),
        Err(LookupValueError::Ref.into())
    );
    assert_eq!(
        parse_dynamic_reference("'O'Brien'!A1", true, &mut book()),
        Err(LookupValueError::Ref.into())
    );
    assert_eq!(
        run(ReferenceFunction::Indirect, &[s("bad")]),
        Err(LookupValueError::Ref.into())
    );
    let mut b = book();
    assert!(
        evaluate(
            Function::Reference(ReferenceFunction::Indirect),
            &[s("R1C1"), Value::Scalar(Scalar::Missing)],
            C,
            &mut b
        )
        .is_ok()
    );
    assert!(!b.queries[0].a1);
    assert_eq!(
        run(ReferenceFunction::Indirect, &[s("Table[Column]")]),
        Ok(area(2, 0, 2, 1, 1))
    );
}
#[test]
fn lookup_array_orientation_and_last_duplicate() {
    let mut b = book();
    assert_eq!(
        evaluate(
            Function::Search(Search::Lookup),
            &[n(2.), array(2, 3, &[1., 2., 3., 11., 22., 33.])],
            C,
            &mut b
        ),
        Ok(Value::SelectedScalar(
            Scalar::Number(22.),
            poirot_functions::coercion::Origin::Array
        ))
    );
    assert_eq!(
        evaluate(
            Function::Search(Search::Lookup),
            &[n(2.), array(3, 2, &[1., 11., 2., 22., 3., 33.])],
            C,
            &mut b
        ),
        Ok(Value::SelectedScalar(
            Scalar::Number(22.),
            poirot_functions::coercion::Origin::Array
        ))
    );
    assert_eq!(
        approximate_index(
            &Scalar::Number(2.),
            &[
                Scalar::Number(1.),
                Scalar::Number(2.),
                Scalar::Number(2.),
                Scalar::Number(3.)
            ],
            &mut book()
        ),
        Ok(2)
    );
    assert_eq!(
        approximate_index(&Scalar::Number(0.), &[Scalar::Number(1.)], &mut book()),
        Err(LookupValueError::Na.into())
    );
}
#[test]
fn xmatch_exact_reverse_nearest_and_binary() {
    let values = [
        Scalar::Number(1.),
        Scalar::Number(3.),
        Scalar::Number(3.),
        Scalar::Number(5.),
    ];
    assert_eq!(
        xmatch_index(
            &Scalar::Number(3.),
            &values,
            MatchMode::Exact,
            SearchMode::Forward,
            &mut book()
        ),
        Ok(1)
    );
    assert_eq!(
        xmatch_index(
            &Scalar::Number(3.),
            &values,
            MatchMode::Exact,
            SearchMode::Backward,
            &mut book()
        ),
        Ok(2)
    );
    assert_eq!(
        xmatch_index(
            &Scalar::Number(4.),
            &values,
            MatchMode::Smaller,
            SearchMode::Forward,
            &mut book()
        ),
        Ok(1)
    );
    assert_eq!(
        xmatch_index(
            &Scalar::Number(4.),
            &values,
            MatchMode::Larger,
            SearchMode::Forward,
            &mut book()
        ),
        Ok(3)
    );
    assert_eq!(
        xmatch_index(
            &Scalar::Number(4.),
            &values,
            MatchMode::Smaller,
            SearchMode::BinaryAscending,
            &mut book()
        ),
        Ok(1)
    );
    let descending = [Scalar::Number(5.), Scalar::Number(3.), Scalar::Number(1.)];
    assert_eq!(
        xmatch_index(
            &Scalar::Number(3.),
            &descending,
            MatchMode::Exact,
            SearchMode::BinaryDescending,
            &mut book()
        ),
        Ok(1)
    );
}
#[test]
fn xmatch_wildcard_escape_and_type_mismatch() {
    let values = [
        Scalar::Number(1.),
        Scalar::Text("abc".into()),
        Scalar::Text("a*c".into()),
    ];
    assert_eq!(
        xmatch_index(
            &Scalar::Text("a?c".into()),
            &values,
            MatchMode::Wildcard,
            SearchMode::Forward,
            &mut book()
        ),
        Ok(1)
    );
    assert_eq!(
        xmatch_index(
            &Scalar::Text("a~*c".into()),
            &values,
            MatchMode::Wildcard,
            SearchMode::Forward,
            &mut book()
        ),
        Ok(2)
    );
    assert_eq!(
        xmatch_index(
            &Scalar::Number(1.),
            &[Scalar::Text("1".into())],
            MatchMode::Exact,
            SearchMode::Forward,
            &mut book()
        ),
        Err(LookupValueError::Na.into())
    );
}
#[test]
fn xlookup_preserves_reference_and_eager_fallback_error() {
    let args = [n(2.), array(3, 1, &[1., 2., 3.]), area(4, 7, 9, 2, 4)];
    assert_eq!(
        evaluate(Function::Search(Search::XLookup), &args, C, &mut book()),
        Ok(area(4, 8, 8, 2, 4))
    );
    let mut args = args.to_vec();
    args.push(error(LookupValueError::Div0));
    assert_eq!(
        evaluate(Function::Search(Search::XLookup), &args, C, &mut book()),
        Err(LookupValueError::Div0.into())
    );
}
#[test]
fn xlookup_not_found_row_and_xmatch_first_row_of_rectangle() {
    assert_eq!(
        evaluate(
            Function::Search(Search::XLookup),
            &[
                n(9.),
                array(2, 1, &[1., 2.]),
                area(4, 7, 8, 2, 4),
                s("none")
            ],
            C,
            &mut book()
        ),
        Ok(Value::Array(
            Array::new(
                1,
                3,
                vec![
                    Scalar::Text("none".into()),
                    Scalar::Text("".into()),
                    Scalar::Text("".into())
                ]
            )
            .unwrap()
        ))
    );
    assert_eq!(
        evaluate(
            Function::Search(Search::XMatch),
            &[n(2.), array(2, 2, &[1., 2., 3., 4.])],
            C,
            &mut book()
        ),
        Ok(n(2.))
    );
}
#[test]
fn malformed_array_rejected_at_construction() {
    for (rows, cols, values) in [
        (0, 1, vec![]),
        (1, 0, vec![]),
        (2, 2, vec![]),
        (usize::MAX, 2, vec![]),
        (1, 1, vec![Scalar::Blank; 2]),
    ] {
        assert_eq!(
            Array::new(rows, cols, values),
            Err(LookupValueError::Value.into())
        );
    }
}

#[test]
fn optional_search_modes_default_only_for_absent_or_missing_operands() {
    let missing = Value::Scalar(Scalar::Missing);
    let blank = Value::Scalar(Scalar::Blank);
    let values = array(3, 1, &[2., 1., 2.]);
    // Documented XLOOKUP/XMATCH defaults are exact match0, forward search1.
    for (mode, search, expected) in [
        (missing.clone(), missing.clone(), Ok(n(1.))),
        (missing.clone(), n(-1.), Ok(n(3.))),
        (blank.clone(), missing.clone(), Ok(n(1.))),
        (n(0.), missing.clone(), Ok(n(1.))),
        (n(0.), n(0.), Err(LookupValueError::Value.into())),
        (missing.clone(), blank, Err(LookupValueError::Value.into())),
        (
            error(LookupValueError::Div0),
            n(1.),
            Err(LookupValueError::Div0.into()),
        ),
        (
            missing,
            error(LookupValueError::Na),
            Err(LookupValueError::Na.into()),
        ),
    ] {
        assert_eq!(
            evaluate(
                Function::Search(Search::XMatch),
                &[n(2.), values.clone(), mode, search],
                C,
                &mut book()
            ),
            expected
        );
    }
}

#[test]
fn xlookup_retains_fallback_presence_and_defaults_missing_modes() {
    let missing = Value::Scalar(Scalar::Missing);
    let blank = Value::Scalar(Scalar::Blank);
    let values = array(3, 1, &[2., 1., 2.]);
    let result = area(4, 7, 9, 2, 2);
    for (direction, expected) in [
        (missing.clone(), area(4, 7, 7, 2, 2)),
        (n(-1.), area(4, 9, 9, 2, 2)),
    ] {
        assert_eq!(
            evaluate(
                Function::Search(Search::XLookup),
                &[
                    n(2.),
                    values.clone(),
                    result.clone(),
                    missing.clone(),
                    missing.clone(),
                    direction
                ],
                C,
                &mut book()
            ),
            Ok(expected)
        );
    }
    for (fallback, expected) in [
        (missing.clone(), Err(LookupValueError::Na.into())),
        (blank.clone(), Ok(blank)),
        (n(0.), Ok(n(0.))),
        (s(""), Ok(s(""))),
        (s("absent"), Ok(s("absent"))),
        (
            error(LookupValueError::Div0),
            Err(LookupValueError::Div0.into()),
        ),
    ] {
        assert_eq!(
            evaluate(
                Function::Search(Search::XLookup),
                &[
                    n(9.),
                    values.clone(),
                    result.clone(),
                    fallback,
                    missing.clone(),
                    missing.clone()
                ],
                C,
                &mut book()
            ),
            expected
        );
    }
    assert_eq!(
        evaluate(
            Function::Search(Search::XLookup),
            &[n(9.), values, result],
            C,
            &mut book()
        ),
        Err(LookupValueError::Na.into())
    );
}
