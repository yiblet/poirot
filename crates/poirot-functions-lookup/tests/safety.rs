use poirot_functions_lookup::*;

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
    Value::Reference(Reference {
        kind: ReferenceKind::Area,
        workbook: Some("book".into()),
        first_sheet: 0,
        last_sheet: 0,
        first_row: row,
        last_row: row + height - 1,
        first_col: 0,
        last_col: width - 1,
    })
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
    fn sheet_index(&mut self, _: &str) -> Result<Option<u32>, Failure> {
        Err(self
            .query_failure
            .clone()
            .unwrap_or(Failure::Unsupported(Unsupported::WorkbookMetadata)))
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
fn unavailable_sheet_metadata_is_not_na() {
    assert_eq!(
        evaluate(
            Function::Reference(ReferenceFunction::Sheet),
            &[s("Data")],
            C,
            &mut guard()
        ),
        Err(Failure::Unsupported(Unsupported::WorkbookMetadata))
    );
}
#[test]
fn deferred_hyperlink_address_areas_and_sheet_do_not_request_ignored_slots() {
    let mut w = guard();
    w.args = vec![n(1.), s("label")];
    let plan = CallPlan::prepare(Function::Reference(ReferenceFunction::Hyperlink), 2).unwrap();
    assert_eq!(plan.execute(C, &mut w), Ok(s("label")));
    assert_eq!(w.requested, vec![1]);
    w.requested.clear();
    w.args = vec![
        n(2.),
        n(3.),
        n(1.),
        Value::Scalar(Scalar::Error(Error::Div0)),
        s("Data"),
    ];
    let plan = CallPlan::prepare(Function::Reference(ReferenceFunction::Address), 5).unwrap();
    assert_eq!(plan.execute(C, &mut w), Ok(s("Data!$C$2")));
    assert_eq!(w.requested, vec![0, 1, 2, 4]);
    w.requested.clear();
    w.args = vec![n(2.), Value::Scalar(Scalar::Error(Error::Div0))];
    assert_eq!(
        CallPlan::prepare(Function::Reference(ReferenceFunction::Areas), 2)
            .unwrap()
            .execute(C, &mut w),
        Ok(n(1.))
    );
    assert_eq!(w.requested, vec![0]);
    w.requested.clear();
    w.args = vec![
        reference(0, 1, 1),
        Value::Scalar(Scalar::Error(Error::Div0)),
    ];
    assert_eq!(
        CallPlan::prepare(Function::Reference(ReferenceFunction::Sheet), 2)
            .unwrap()
            .execute(C, &mut w),
        Ok(n(1.))
    );
    assert_eq!(w.requested, vec![0]);
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
    w.requested.clear();
    w.args = vec![
        reference(0, 1, 1),
        n(0.),
        n(0.),
        Value::Scalar(Scalar::Error(Error::Na)),
        Value::Scalar(Scalar::Error(Error::Div0)),
    ];
    assert_eq!(
        CallPlan::prepare(Function::Reference(ReferenceFunction::Offset), 5)
            .unwrap()
            .execute(C, &mut w),
        Err(Error::Div0.into())
    );
    assert_eq!(w.requested, vec![0, 1, 2, 4]);
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
fn input_budget_precedes_deferred_operand_materialization() {
    let mut w = guard();
    w.args = vec![s("label")];
    w.deny = Some(Work::InputMaterialization);
    assert_eq!(
        CallPlan::prepare(Function::Reference(ReferenceFunction::Hyperlink), 1)
            .unwrap()
            .execute(C, &mut w),
        Err(Failure::Operational(OperationalFailure::BudgetExceeded))
    );
    assert!(w.requested.is_empty());
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
    assert!(
        matches!(evaluate(Function::Reference(ReferenceFunction::Offset),
        &[reference(0,1,1), n(32768.), n(0.)], C, &mut w), Ok(Value::SignedReference(r))
        if r.first_row == -32768 && r.last_row == -32768 && r.workbook.as_deref() == Some("book"))
    );
    assert_eq!(
        evaluate(
            Function::Reference(ReferenceFunction::Offset),
            &[reference(0, 1, 1)],
            C,
            &mut w
        ),
        Err(Failure::Unsupported(Unsupported::OffsetMissingOperands))
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
fn unicode_search_and_unknown_scalar_array_shape_are_explicit() {
    let mut w = guard();
    assert_eq!(
        xmatch_index(
            &Scalar::Text("İ".into()),
            &[Scalar::Text("i".into())],
            MatchMode::Exact,
            SearchMode::Forward,
            &mut w
        ),
        Err(Failure::Unsupported(Unsupported::UnicodeSearch))
    );
    assert_eq!(
        evaluate(
            Function::Reference(ReferenceFunction::Address),
            &[array(vec![Scalar::Number(1.)]), n(1.)],
            C,
            &mut w
        ),
        Err(Failure::Unsupported(Unsupported::ScalarArrayIntersection))
    );
}
