use poirot_functions_lookup::*;

/// In-memory lookup host for reference, metadata, and search contract tests.
struct Book {
    reads: Vec<(u32, u32, u32)>,
    queries: Vec<DynamicReference>,
}
impl Workbook for Book {
    fn preflight(&mut self, _: BudgetRequest) -> Result<(), Failure> {
        Ok(())
    }
    fn argument(&mut self, _: usize, _: Context) -> Result<Value, Failure> {
        Err(Failure::Unsupported(Unsupported::DeferredArguments))
    }
    fn coercev2_numeric_text(&mut self, text: &str) -> Result<f64, Failure> {
        text.trim()
            .parse()
            .map_err(|_| Failure::Spreadsheet(Error::Value))
    }
    fn coercev2_number_to_text(&mut self, number: f64) -> Result<String, Failure> {
        Ok(number.to_string())
    }
    fn cell(&mut self, _: &Reference, sheet: u32, row: u32, col: u32) -> Result<Scalar, Failure> {
        self.reads.push((sheet, row, col));
        Ok(Scalar::Number((100 * sheet + 10 * row + col) as f64))
    }
    fn sheet_index(&mut self, name: &str) -> Result<Option<u32>, Failure> {
        Ok(if name == "Data" { Some(2) } else { None })
    }
    fn dynamic_reference(&mut self, q: &DynamicReference, _: Context) -> Result<Value, Failure> {
        self.queries.push(q.clone());
        if q.first == "bad" {
            Err(Error::Name.into())
        } else {
            Ok(area(2, 0, 0, 0, 0))
        }
    }
    fn structured_reference(&mut self, text: &str, _: Context) -> Result<Value, Failure> {
        if text == "Table[Column]" {
            Ok(area(2, 0, 2, 1, 1))
        } else {
            Err(Error::Ref.into())
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
    Value::Reference(Reference {
        kind: ReferenceKind::Area,
        workbook: Some("source.xlsx".into()),
        first_sheet: sheet,
        last_sheet: sheet,
        first_row: fr,
        last_row: lr,
        first_col: fc,
        last_col: lc,
    })
}
fn n(n: f64) -> Value {
    Value::Scalar(Scalar::Number(n))
}
fn s(s: &str) -> Value {
    Value::Scalar(Scalar::Text(s.into()))
}
fn error(e: Error) -> Value {
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
fn run(f: ReferenceFunction, args: &[Value]) -> Result<Value, Failure> {
    evaluate(Function::Reference(f), args, C, &mut book())
}

#[test]
fn address_absolute_relative_and_sheet_escape() {
    assert_eq!(
        run(ReferenceFunction::Address, &[n(3.), n(28.)]),
        Ok(s("$AB$3"))
    );
    assert_eq!(
        run(
            ReferenceFunction::Address,
            &[n(3.8), n(28.9), n(2.), error(Error::Div0), s("O'Brien")]
        ),
        Ok(s("'O''Brien'!AB$3"))
    );
    assert_eq!(
        run(
            ReferenceFunction::Address,
            &[n(3.), n(1.), n(4.), n(0.), s("R2")]
        ),
        Ok(s("'R2'!A3"))
    );
    assert_eq!(
        run(ReferenceFunction::Address, &[n(0.), n(1.)]),
        Err(Error::Value.into())
    );
    assert_eq!(
        run(ReferenceFunction::Address, &[error(Error::Div0), n(1.)]),
        Err(Error::Div0.into())
    );
}
#[test]
fn address_sheet_formatter_excel97_ambiguity() {
    assert_eq!(
        run(
            ReferenceFunction::Address,
            &[n(1.), n(1.), n(1.), n(1.), s("XFD1")]
        ),
        Ok(s("XFD1!$A$1"))
    );
    assert_eq!(
        run(
            ReferenceFunction::Address,
            &[n(1.), n(1.), n(1.), n(1.), s("")]
        ),
        Ok(s("!$A$1"))
    );
}
#[test]
fn hyperlink_returns_last_without_coercing_or_reading() {
    let reference = area(2, 4, 5, 6, 7);
    let mut b = book();
    assert_eq!(
        evaluate(
            Function::Reference(ReferenceFunction::Hyperlink),
            &[error(Error::Div0), reference.clone()],
            C,
            &mut b
        ),
        Ok(reference)
    );
    assert!(b.reads.is_empty());
    assert_eq!(
        run(ReferenceFunction::Hyperlink, &[]),
        Err(Error::Value.into())
    );
    assert_eq!(
        run(
            ReferenceFunction::Hyperlink,
            &[s("url"), Value::Scalar(Scalar::Missing)]
        ),
        Ok(Value::Scalar(Scalar::Missing))
    );
}
#[test]
fn areas_preserves_poi_scalar_and_extra_argument_behavior() {
    assert_eq!(
        run(ReferenceFunction::Areas, &[error(Error::Na), n(2.)]),
        Ok(n(1.))
    );
    assert_eq!(
        run(ReferenceFunction::Areas, &[Value::Union(vec![])]),
        Ok(n(0.))
    );
    assert_eq!(run(ReferenceFunction::Areas, &[]), Err(Error::Value.into()));
}
#[test]
fn sheet_requires_metadata_and_does_not_read_reference() {
    assert_eq!(run(ReferenceFunction::Sheet, &[]), Ok(n(1.)));
    assert_eq!(
        run(ReferenceFunction::Sheet, &[s("Data"), error(Error::Value)]),
        Ok(n(3.))
    );
    assert_eq!(
        run(ReferenceFunction::Sheet, &[area(4, 0, 1, 0, 1)]),
        Ok(n(5.))
    );
    assert_eq!(
        run(ReferenceFunction::Sheet, &[s("missing")]),
        Err(Error::Na.into())
    );
    assert_eq!(
        run(ReferenceFunction::Sheet, &[error(Error::Div0)]),
        Err(Error::Na.into())
    );
}
#[test]
fn offset_retains_identity_and_original_negative_endpoints() {
    assert_eq!(
        run(
            ReferenceFunction::Offset,
            &[area(2, 4, 5, 6, 7), n(1.), n(-1.), n(-2.), n(3.)]
        ),
        Ok(area(2, 2, 5, 5, 7))
    );
    assert_eq!(
        run(
            ReferenceFunction::Offset,
            &[area(2, 4, 5, 6, 7), n(-0.1), n(0.)]
        ),
        Ok(area(2, 3, 4, 6, 7))
    );
    assert_eq!(
        run(
            ReferenceFunction::Offset,
            &[area(0, 0, 0, 0, 0), n(-1.), n(0.)]
        ),
        Err(Error::Ref.into())
    );
    assert_eq!(
        run(
            ReferenceFunction::Offset,
            &[area(0, 0, 0, 0, 0), n(0.), n(0.), n(0.)]
        ),
        Err(Error::Ref.into())
    );
    assert_eq!(
        run(
            ReferenceFunction::Offset,
            &[
                area(0, 0, 0, 0, 0),
                n(0.),
                n(0.),
                error(Error::Na),
                error(Error::Div0)
            ]
        ),
        Err(Error::Div0.into())
    );
}
#[test]
fn single_intersects_areas_and_rejects_ambiguity() {
    assert_eq!(
        run(ReferenceFunction::Single, &[area(2, 2, 5, 8, 9)]),
        Ok(Value::SelectedScalar(
            Scalar::Number(238.),
            poirot_coercionv2::Origin::Reference
        ))
    );
    assert_eq!(
        run(ReferenceFunction::Single, &[area(2, 8, 9, 2, 5)]),
        Ok(Value::SelectedScalar(
            Scalar::Number(284.),
            poirot_coercionv2::Origin::Reference
        ))
    );
    assert_eq!(
        run(
            ReferenceFunction::Single,
            &[area(2, 2, 5, 2, 5), area(1, 2, 5, 2, 5)]
        ),
        Err(Error::Value.into())
    );
    assert_eq!(
        run(ReferenceFunction::Single, &[n(5.)]),
        Err(Error::Value.into())
    );
    let Value::Reference(mut r) = area(2, 3, 3, 4, 4) else {
        unreachable!()
    };
    r.kind = ReferenceKind::Cell;
    assert_eq!(
        run(ReferenceFunction::Single, &[Value::Reference(r)]),
        Err(Error::Value.into())
    );
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
        Err(Error::Ref.into())
    );
    assert_eq!(
        parse_dynamic_reference("'O'Brien'!A1", true, &mut book()),
        Err(Error::Ref.into())
    );
    assert_eq!(
        run(ReferenceFunction::Indirect, &[s("bad")]),
        Err(Error::Ref.into())
    );
    let mut b = book();
    assert!(evaluate(
        Function::Reference(ReferenceFunction::Indirect),
        &[s("R1C1"), Value::Scalar(Scalar::Missing)],
        C,
        &mut b
    )
    .is_ok());
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
            poirot_coercionv2::Origin::Array
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
            poirot_coercionv2::Origin::Array
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
        Err(Error::Na.into())
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
        Err(Error::Na.into())
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
    args.push(error(Error::Div0));
    assert_eq!(
        evaluate(Function::Search(Search::XLookup), &args, C, &mut book()),
        Err(Error::Div0.into())
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
fn malformed_array_rejected_without_index_panic() {
    let invalid = Value::Array(Array {
        rows: 2,
        cols: 2,
        values: vec![],
    });
    assert_eq!(
        evaluate(
            Function::Search(Search::XMatch),
            &[n(2.), invalid],
            C,
            &mut book()
        ),
        Err(Error::Value.into())
    );
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
        (n(0.), n(0.), Err(Error::Value.into())),
        (missing.clone(), blank, Err(Error::Value.into())),
        (error(Error::Div0), n(1.), Err(Error::Div0.into())),
        (missing, error(Error::Na), Err(Error::Na.into())),
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
        (missing.clone(), Err(Error::Na.into())),
        (blank.clone(), Ok(blank)),
        (n(0.), Ok(n(0.))),
        (s(""), Ok(s(""))),
        (s("absent"), Ok(s("absent"))),
        (error(Error::Div0), Err(Error::Div0.into())),
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
        Err(Error::Na.into())
    );
}
