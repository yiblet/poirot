//! Checked forwards to evaluator-owned context, selection, and sparse services.
use super::*;
use crate::run::ContextRead;
use std::time::SystemTime;

impl<'call, 'workbook> CallHost<'call, 'workbook> {
    /// Record a volatile builtin dependency for this checked call.
    pub(crate) fn record_volatile(&mut self, function: FunctionId) {
        self.session.read(DependencyRead::Volatile(function));
    }

    /// Inspect a retained error-handler source for scalar observation evidence.
    pub(crate) fn inspect_error_handler_scalar(&mut self, value: &Value) -> EvalResult<CellValue> {
        // Keep the existing noncharging clone for the observation-only projection.
        self.session
            .select_legacy_scalar(self.address, value.clone())
    }

    /// Publish an uncaught scalar check under its original source operation.
    pub(crate) fn error_handler_mask(
        &mut self,
        function: LogicalFunction,
        source: Option<execution_observation::OperationId>,
        checked: &CellValue,
    ) -> EvalResult<()> {
        self.session
            .observe_mask(function, source, checked, None, None)
    }

    /// Evaluate the caught handler's fallback with its actual observation identity.
    pub(crate) fn error_handler_fallback(
        &mut self,
        function: LogicalFunction,
        source: Option<execution_observation::OperationId>,
        checked: &CellValue,
    ) -> EvalResult<Value> {
        let expression = self.arguments.get(1).ok_or(EvaluationFailure::StalePlan)?;
        self.session.observed_fallback(
            self.sheet,
            self.address,
            expression,
            function,
            source,
            checked,
        )
    }

    /// Inspect the captured numeric locale before a helper requests its dependency.
    pub(crate) fn number_locale_snapshot(
        &self,
    ) -> &'workbook poirot_parsing::number::NumberLocaleV2 {
        self.session.number_locale
    }

    /// Inspect text configuration after the caller has recorded its policy dependency.
    pub(crate) fn text_policy_snapshot(&self) -> poirot_text::TextSemanticsPolicy {
        self.session.text_policy
    }
    /// Inspect formatter context without adding a read for nondate formats.
    pub(crate) fn date_system_snapshot(&self) -> poirot_model::DateSystem {
        self.session.workbook.date_system
    }
    /// Borrow Unicode scalar text, allocating only when UTF-16 projection requires it.
    pub(crate) fn scalar_text_view<'a>(
        &mut self,
        text: poirot_model::TextRef<'a>,
    ) -> FunctionResult<std::borrow::Cow<'a, str>> {
        checked_text_view(text, &mut self.session.budget)
    }
    /// Parse scalar text through the legacy, consumer-attributed calendar path.
    pub(crate) fn legacy_numeric_date_time(&mut self, text: &str) -> FunctionResult<f64> {
        self.session.coercev2_numeric_date_time(text)
    }

    /// Charge evaluator work under its original accounting kind.
    pub(crate) fn charge_kind(&mut self, kind: WorkKind, units: u64) -> EvalResult<()> {
        self.session.runtime.budget.charge(kind, units)
    }

    /// Evaluate one aggregate argument without retaining a VM operand slot.
    pub(crate) fn aggregate_argument_once(&mut self, index: usize) -> FunctionResult<HostOperand> {
        let expression = self.arguments.get(index).ok_or(VALUE_ERROR)?;
        let missing = expression.is_missing();
        let array_missing = expression.array_missing(&mut self.session.runtime.budget)?;
        let value = self.session.expr_with_demand(
            self.sheet,
            self.address,
            expression,
            self.argument_demand.at(index),
        )?;
        Ok(HostOperand {
            missing: missing || matches!(value, Value::Missing),
            value,
            array_missing,
        })
    }

    /// Admit a call-local selection vector with the caller's source diagnostic.
    pub(crate) fn selection_buffer(
        &mut self,
        count: usize,
        fill: Selection,
        allocation_detail: &'static str,
    ) -> EvalResult<Vec<Selection>> {
        self.session.runtime.budget.allocation(count)?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(count)
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: self.session.consumer,
                detail: allocation_detail.into(),
            })?;
        // SAFETY: Reservation admitted exactly count slots; resize fills no more than count.
        selected.resize(count, fill);
        Ok(selected)
    }

    /// Expand selected coordinates with the evaluator's checked broadcast logic.
    pub(crate) fn expand_selection(
        &mut self,
        selected: Vec<Selection>,
        previous: Shape,
        output: Shape,
    ) -> EvalResult<Vec<Selection>> {
        self.session.expand_selection(selected, previous, output)
    }

    /// Charge selected-branch scanning as helper work.
    pub(crate) fn charge_selection_work(&mut self, units: u64) -> EvalResult<()> {
        self.session
            .runtime
            .budget
            .charge(WorkKind::HelperOperation, units)
    }

    /// Inspect a selected rectangle or return the evaluator's spill diagnostic.
    pub(crate) fn selection_shape(&self, value: &Value, detail: &'static str) -> EvalResult<Shape> {
        Shape::inspect(value).ok_or_else(|| {
            self.session
                .spill_unsupported(self.sheet, self.address, detail)
        })
    }

    /// Read the captured clock and record the exact context dependency.
    pub(crate) fn as_of_time(&mut self) -> SystemTime {
        self.session
            .read(DependencyRead::ContextField(ContextRead::AsOf));
        self.session.context.as_of_time
    }

    /// Read the fixed UTC offset and record its context dependency.
    pub(crate) fn utc_offset_seconds(&mut self) -> i32 {
        self.session
            .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        self.session.context.localization.utc_offset_seconds
    }

    /// Read the numeric slash-date order and record its context dependency.
    pub(crate) fn date_order(&mut self) -> crate::DateOrder {
        self.session
            .read(DependencyRead::ContextField(ContextRead::DateOrder));
        self.session.context.localization.date_order
    }

    /// Read the workbook date system and record its context dependency.
    pub(crate) fn selected_date_system(&mut self) -> poirot_model::DateSystem {
        self.session
            .read(DependencyRead::ContextField(ContextRead::DateSystem));
        self.session.workbook.date_system
    }

    /// Read the selected text policy and record its dependency.
    pub(crate) fn selected_text_policy(&mut self) -> poirot_text::TextSemanticsPolicy {
        self.session.read(DependencyRead::TextPolicy);
        self.session.text_policy
    }

    /// Parse formatted numeric text with the evaluator's locale and work charges.
    pub(crate) fn formatted_number_text(
        &mut self,
        text: &str,
    ) -> FunctionResult<poirot_coercionv2::compatibility::NumericTextDecision> {
        self.session.coercev2_formatted_text(text)
    }

    /// Convert the captured clock through the date family's existing serial rule.
    pub(crate) fn clock_serial(&mut self) -> Result<f64, CellError> {
        self.session.clock_serial()
    }

    /// Evaluate a raw argument with the evaluator's original expression demand.
    pub(crate) fn raw_argument(&mut self, index: usize) -> EvalResult<Value> {
        let expression = self
            .arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.session.expr(self.sheet, self.address, expression)
    }

    /// Resolve the finalized spill footprint of one admitted physical anchor.
    pub(crate) fn spill_extent(&mut self, area: Area) -> FunctionResult<Area> {
        Ok(self.session.spill_extent(area)??)
    }

    /// Materialize a supported value with the evaluator's original matrix admission and reads.
    pub(crate) fn matrix(
        &mut self,
        value: Value,
    ) -> EvalResult<Result<Vec<Vec<CellValue>>, CellError>> {
        self.session.matrix(value)
    }

    /// Count blank logical positions in a reference using sparse membership and reads.
    /// Nonreference values return None for the caller's scalar error precedence.
    pub(crate) fn countblank_references(&mut self, value: &Value) -> FunctionResult<Option<u64>> {
        self.session.countblank_references(value)
    }
}

impl Session<'_> {
    /// Count blank logical positions after sparse membership and spill discovery.
    pub(crate) fn countblank_references(&mut self, value: &Value) -> FunctionResult<Option<u64>> {
        let mut count = 0u64;
        match value {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                for area in areas {
                    let addresses = self.sparse_addresses(*area)?;
                    let mut used = 0u64;
                    for address in addresses {
                        let value = self.range_cell(area.sheet, address)?;
                        if !matches!(value, Blank) && value != Text(String::new()) {
                            used += 1;
                        }
                    }
                    count += area.size() - used;
                }
            }
            Value::SignedRefs(areas) => {
                // Pinned COUNTBLANK accepts RefEval/ThreeDEval, not a RefListEval union.
                if areas.len() != 1 {
                    return Err(VALUE_ERROR.into());
                }
                for area in areas {
                    let mut used = 0u64;
                    if let Some(physical) = area.physical() {
                        for address in self.sparse_addresses(physical)? {
                            let value = self.range_cell(area.sheet, address)?;
                            if !matches!(value, Blank) && value != Text(String::new()) {
                                used += 1;
                            }
                        }
                    }
                    count += area.size() - used;
                }
            }
            _ => return Ok(None),
        }
        Ok(Some(count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_forwards_preserve_lossless_text_and_exact_work() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let at = CellAddress { row: 0, col: 0 };
        for value in [
            Text("abc".into()),
            CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d, 0xde00])),
            CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d])),
        ] {
            for limit in [0, 2, 100] {
                let budgets = EvaluationBudgets {
                    text_bytes: limit,
                    ..Default::default()
                };
                let mut baseline = Session::new(
                    &workbook,
                    Default::default(),
                    BudgetMeter::new(budgets).unwrap(),
                );
                let mut session = Session::new(
                    &workbook,
                    Default::default(),
                    BudgetMeter::new(budgets).unwrap(),
                );
                let text = value.text_ref().unwrap();
                let expected = checked_text_view(text, &mut baseline.budget);
                let actual =
                    CallHost::new(&mut session, 0, at, ArgumentsRef::Empty).scalar_text_view(text);
                assert_eq!(actual, expected);
                assert_eq!(session.budget.usage, baseline.budget.usage);
            }
        }
        for text in ["1/2/2020", "invalid", "12:30"] {
            let mut baseline = Session::with_context(&workbook, Default::default());
            let mut session = Session::with_context(&workbook, Default::default());
            let expected = baseline.coercev2_numeric_date_time(text);
            let mut host = CallHost::new(&mut session, 0, at, ArgumentsRef::Empty);
            assert_eq!(host.text_policy_snapshot(), baseline.text_policy);
            assert_eq!(host.date_system_snapshot(), workbook.date_system);
            assert_eq!(host.number_locale_snapshot(), baseline.number_locale);
            let actual = host.legacy_numeric_date_time(text);
            drop(host);
            assert_eq!(actual, expected);
            assert_eq!(session.budget.usage, baseline.budget.usage);
        }
    }

    #[test]
    fn matrix_forward_preserves_values_errors_and_resource_admission() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        let at = CellAddress { row: 0, col: 0 };
        for limit in [0, 1, 4] {
            for value in [
                Value::Array(ArrayView::unknown(vec![vec![Number(2.), Number(3.)]])),
                Value::Scalar(Error(Ref)),
                Value::Scalar(Number(7.)),
                Value::Missing,
            ] {
                let budgets = EvaluationBudgets {
                    materialized_values: limit,
                    ..EvaluationBudgets::default()
                };
                let mut baseline = Session::new(
                    &workbook,
                    EvaluationInputs::default(),
                    BudgetMeter::new(budgets).unwrap(),
                );
                let mut session = Session::new(
                    &workbook,
                    EvaluationInputs::default(),
                    BudgetMeter::new(budgets).unwrap(),
                );
                let expected = baseline.matrix(value.clone());
                let actual = CallHost::new(&mut session, 0, at, ArgumentsRef::Empty).matrix(value);
                assert_eq!(actual, expected);
                assert_eq!(session.budget.usage, baseline.budget.usage);
            }
        }
    }
}
