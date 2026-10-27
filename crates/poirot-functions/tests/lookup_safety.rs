use poirot_functions::lookup::*;

const C: Context = Context {
    sheet: 0,
    row: 0,
    col: 0,
};
fn n(value: f64) -> Value {
    Value::Scalar(Scalar::Number(value))
}
fn s(value: &str) -> Value {
    Value::Scalar(Scalar::Text(value.into()))
}
fn array(values: Vec<Scalar>) -> Value {
    Value::Array(Array::new(values.len(), 1, values).unwrap())
}
fn reference(row: u32, height: u32, width: u32) -> Value {
    Value::Reference(
        Reference::new(
            ReferenceKind::Area,
            Some("book".into()),
            0..=0,
            row..=row + height - 1,
            0..=width - 1,
        )
        .unwrap(),
    )
}
/// Test host that rejects unsafe access or excessive lookup work.
struct Guard {
    deny: Option<Work>,
    args: Vec<Value>,
    requested: Vec<usize>,
    query_failure: Option<Failure>,
    reads: usize,
    charges: Vec<BudgetRequest>,
}
fn guard() -> Guard {
    Guard {
        deny: None,
        args: vec![],
        requested: vec![],
        query_failure: None,
        reads: 0,
        charges: vec![],
    }
}
impl Workbook for Guard {
    fn preflight(&mut self, r: BudgetRequest) -> Result<(), Failure> {
        self.charges.push(r);
        if self.deny == Some(r.work) {
            Err(Failure::Operational(OperationalFailure::BudgetExceeded))
        } else {
            Ok(())
        }
    }
    fn argument(&mut self, index: usize, _: Context) -> Result<Value, Failure> {
        self.requested.push(index);
        self.args
            .get(index)
            .cloned()
            .ok_or(Failure::Unsupported(Unsupported::DeferredArguments))
    }
    fn coerce_numeric_text(&mut self, value: &str) -> Result<f64, Failure> {
        value.parse().map_err(|_| Error::Value.into())
    }
    fn coerce_number_to_text(&mut self, value: f64) -> Result<String, Failure> {
        Ok(value.to_string())
    }
    fn cell(&mut self, _: &Reference, _: u32, _: u32, _: u32) -> Result<Scalar, Failure> {
        self.reads += 1;
        Ok(Scalar::Number(1.))
    }

    fn dynamic_reference(&mut self, _: &DynamicReference, _: Context) -> Result<Value, Failure> {
        Err(self.query_failure.clone().unwrap_or(Error::Name.into()))
    }
    fn structured_reference(&mut self, _: &str, _: Context) -> Result<Value, Failure> {
        Err(self.query_failure.clone().unwrap_or(Error::Name.into()))
    }
}
#[test]
fn indirect_maps_only_semantic_resolution_failures() {
    for text in ["A1", "Table[Column]"] {
        let mut w = guard();
        assert_eq!(
            evaluate(
                Function::Reference(ReferenceFunction::Indirect),
                &[s(text)],
                C,
                &mut w
            ),
            Err(Error::Ref.into())
        );
        for failure in [
            Failure::Operational(OperationalFailure::BudgetExceeded),
            Failure::Operational(OperationalFailure::Cancelled),
            Failure::Unsupported(Unsupported::DynamicReference),
        ] {
            w.query_failure = Some(failure.clone());
            assert_eq!(
                evaluate(
                    Function::Reference(ReferenceFunction::Indirect),
                    &[s(text)],
                    C,
                    &mut w
                ),
                Err(failure)
            );
        }
    }
}
#[test]
fn deferred_error_order_requests_only_operands_before_failure() {
    let mut w = guard();
    w.args = vec![
        n(2.),
        reference(0, 3, 1),
        reference(0, 3, 2),
        Value::Scalar(Scalar::Error(Error::Div0)),
    ];
    assert_eq!(
        CallPlan::prepare(Function::Search(Search::XLookup), 4)
            .unwrap()
            .execute(C, &mut w),
        Err(Error::Div0.into())
    );
    assert_eq!(w.requested, vec![3]);
}
#[test]
fn vector_budget_fails_before_workbook_cells_are_read() {
    let mut w = guard();
    w.deny = Some(Work::VectorCopy);
    assert_eq!(
        evaluate(
            Function::Search(Search::XMatch),
            &[n(2.), reference(0, 100, 1)],
            C,
            &mut w
        ),
        Err(Failure::Operational(OperationalFailure::BudgetExceeded))
    );
    assert_eq!(w.reads, 0);
}
#[test]
fn search_budget_failure_never_becomes_no_match_or_fallback() {
    let mut w = guard();
    w.deny = Some(Work::SearchSteps);
    assert_eq!(
        evaluate(
            Function::Search(Search::XLookup),
            &[
                n(9.),
                array(vec![Scalar::Number(1.)]),
                reference(0, 1, 2),
                s("not found")
            ],
            C,
            &mut w
        ),
        Err(Failure::Operational(OperationalFailure::BudgetExceeded))
    );
}
#[test]
fn casefold_and_wildcard_states_have_preflight_boundaries() {
    let mut w = guard();
    w.deny = Some(Work::SearchText);
    assert_eq!(
        xmatch_index(
            &Scalar::Text("abc".into()),
            &[Scalar::Text("ABC".into())],
            MatchMode::Exact,
            SearchMode::Forward,
            &mut w
        ),
        Err(Failure::Operational(OperationalFailure::BudgetExceeded))
    );
    w.deny = Some(Work::WildcardStates);
    assert_eq!(
        xmatch_index(
            &Scalar::Text("a*".into()),
            &[Scalar::Text("abc".into())],
            MatchMode::Wildcard,
            SearchMode::Forward,
            &mut w
        ),
        Err(Failure::Operational(OperationalFailure::BudgetExceeded))
    );
}
#[test]
fn query_return_budget_precedes_fallback_row_allocation() {
    let mut w = guard();
    w.deny = Some(Work::QueryReturn);
    assert_eq!(
        evaluate(
            Function::Search(Search::XLookup),
            &[
                n(9.),
                array(vec![Scalar::Number(1.)]),
                reference(0, 1, 200),
                s("not found")
            ],
            C,
            &mut w
        ),
        Err(Failure::Operational(OperationalFailure::BudgetExceeded))
    );
}
#[test]
fn source_exception_regions_are_unsupported_and_short_casts_retain_identity() {
    let mut w = guard();
    let rectangle = Value::Array(Array::new(2, 2, vec![Scalar::Number(1.); 4]).unwrap());
    assert_eq!(
        evaluate(
            Function::Search(Search::Lookup),
            &[n(1.), rectangle, array(vec![Scalar::Number(2.)])],
            C,
            &mut w
        ),
        Err(Failure::Unsupported(Unsupported::LookupNonVector))
    );
    assert_eq!(
        evaluate(
            Function::Search(Search::Lookup),
            &[
                n(0.),
                array(vec![Scalar::Number(1.), Scalar::Number(2.)]),
                array(vec![Scalar::Number(3.)])
            ],
            C,
            &mut w
        ),
        Err(Failure::Unsupported(Unsupported::LookupShortResult))
    );
}

#[test]
fn lookup_resolves_result_table_before_copying_or_searching_lookup_vector() {
    let mut w = guard();
    w.deny = Some(Work::VectorCopy);
    assert_eq!(
        evaluate(
            Function::Search(Search::Lookup),
            &[
                n(0.),
                reference(0, 100, 1),
                Value::Scalar(Scalar::Error(Error::Div0))
            ],
            C,
            &mut w
        ),
        Err(Error::Div0.into())
    );
    assert_eq!(w.reads, 0);
}
#[test]
fn unicode_search_and_unknown_scalar_array_shape_remain_typed() {
    let mut w = guard();
    assert_eq!(
        xmatch_index(
            &Scalar::Text("İ".into()),
            &[Scalar::Text("i".into())],
            MatchMode::Exact,
            SearchMode::Forward,
            &mut w
        ),
        Err(Error::Na.into())
    );
}
