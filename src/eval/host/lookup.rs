//! Checked lookup and deferred reference services; caches and reads stay evaluator-owned.
use super::*;

pub(in crate::eval) use crate::eval::adapters::search::execution::LookupTable;

/// Candidate comparison and duplicate-selection contract of a classic lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum ClassicLookupKind {
    /// MATCH wildcard and approximate-position semantics.
    Match,
    /// VLOOKUP/HLOOKUP first-axis table selection semantics.
    Table,
}

impl CallHost<'_, '_> {
    /// Visit omission markers without allocating or forcing the argument again.
    pub(in crate::eval) fn visit_argument_array_missing(
        &self,
        index: usize,
        visit: impl FnMut(usize, bool) -> EvalResult<()>,
    ) -> EvalResult<()> {
        self.arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?
            .visit_array_missing(visit)
    }

    /// Read a local worksheet cell through dependency, overlay, and spill resolution.
    pub(in crate::eval) fn sheet_cell(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> EvalResult<CellValue> {
        self.session.cell(sheet, address)
    }

    /// Resolve a captured local sheet name with the evaluator's dependency tracking.
    pub(in crate::eval) fn resolve_sheet(&mut self, name: String) -> EvalResult<Option<usize>> {
        self.session.sheet(self.sheet, &Some(name))
    }

    /// Inspect the prepared formula's parser policy for dynamic reference endpoints.
    pub(in crate::eval) fn parse_policy(&self) -> poirot_parsing::formula::FormulaParsePolicy {
        self.session.plan.map_or_else(
            poirot_parsing::formula::FormulaParsePolicy::default,
            |plan| plan.policy().parse,
        )
    }

    /// Count a dynamic parse attempt when run profiling is enabled.
    pub(in crate::eval) fn record_dynamic_parse(&mut self) {
        if self.session.budget.profiling {
            self.session.run_usage.dynamic_parse_attempts = self
                .session
                .run_usage
                .dynamic_parse_attempts
                .saturating_add(1);
        }
    }

    /// Evaluate validated dynamic reference syntax with the captured extension registry.
    pub(in crate::eval) fn dynamic_reference(
        &mut self,
        sheet: usize,
        syntax: &poirot_parsing::formula::Expr,
    ) -> FunctionResult<Value> {
        let program = crate::eval::bytecode::compile_with_registry(syntax, self.session.extensions)
            .map_err(|_| CellError::Ref)?;
        Ok(self.session.bytecode_expr(sheet, self.address, &program)?)
    }

    /// Read a selected result vector after classic candidate selection succeeds.
    pub(in crate::eval) fn lookup_result(
        &mut self,
        table: &LookupTable,
        axis: SearchAxis,
        fixed: usize,
        index: usize,
    ) -> FunctionResult<CellValue> {
        let vector = table.vector(axis, fixed)?;
        Ok(self.session.lookup_read(table, vector, index)?)
    }

    /// Select a zero-based position in the first row or column with checked reads and prefix reuse.
    /// The caller forces key and table operands before invoking this service.
    pub(in crate::eval) fn lookup_index(
        &mut self,
        table: &LookupTable,
        axis: SearchAxis,
        key: &CellValue,
        mode: SearchMode,
        kind: ClassicLookupKind,
    ) -> FunctionResult<Option<usize>> {
        self.session
            .classic_lookup_index(table, axis, key, mode, kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: CellAddress = CellAddress { row: 19, col: 25 };

    fn table(session: &mut Session<'_>, source: &str) -> LookupTable {
        let parsed = poirot_parsing::formula::parse_formula(source).unwrap();
        let expression = bound::bind(&parsed);
        LookupTable::admit(
            session.expr(0, AT, &expression).unwrap(),
            ClassicLookupKind::Match,
        )
        .unwrap()
    }

    #[test]
    fn lookup_service_reuses_first_positions_and_preserves_streamed_results() {
        // First duplicates and exact misses follow the existing
        // documented contracts in tests/prepared_lookup_reuse.rs.
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        for (address, value) in [("A1", 1.), ("A2", 2.), ("A3", 2.)] {
            workbook
                .sheet_mut(0)
                .unwrap()
                .set_value(address.parse().unwrap(), value)
                .unwrap();
        }
        let mut cached = Session::with_context(&workbook, EvaluationInputs::default());
        cached.lookup_prefixes.enable();
        let mut streamed = Session::with_context(&workbook, EvaluationInputs::default());
        let cached_table = table(&mut cached, "A1:A3");
        let streamed_table = table(&mut streamed, "A1:A3");
        for (key, expected) in [(2., Some(1)), (2., Some(1)), (9., None), (9., None)] {
            let actual = CallHost::new(&mut cached, 0, AT, ArgumentsRef::Empty).lookup_index(
                &cached_table,
                SearchAxis::Vertical,
                &Number(key),
                SearchMode::Exact,
                ClassicLookupKind::Match,
            );
            let baseline = streamed.classic_lookup_index(
                &streamed_table,
                SearchAxis::Vertical,
                &Number(key),
                SearchMode::Exact,
                ClassicLookupKind::Match,
            );
            assert_eq!(actual, Ok(expected));
            assert_eq!(actual, baseline);
        }
        assert_eq!(cached.run_usage.lookup_prefix_scans, 3);
        assert_eq!(cached.run_usage.lookup_prefix_hits, 2);
        assert_eq!(cached.budget.usage.cell_reads, 3);
    }

    #[test]
    fn lookup_service_rejects_empty_geometry_before_candidate_work() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let table = LookupTable::admit(
            Value::Array(ArrayView::unknown(Vec::new())),
            ClassicLookupKind::Table,
        )
        .unwrap();
        let before = session.budget.usage.clone();
        let result = CallHost::new(&mut session, 0, AT, ArgumentsRef::Empty).lookup_index(
            &table,
            SearchAxis::Horizontal,
            &Number(2.),
            SearchMode::Exact,
            ClassicLookupKind::Table,
        );
        assert_eq!(result, Err(FunctionFailure::Spreadsheet(Ref)));
        assert_eq!(session.budget.usage, before);
    }

    #[test]
    fn lookup_service_preserves_failure_provenance_and_budget_accounting() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), 2.)
            .unwrap();
        for source in ["A1:A2", "{1;2;3}"] {
            for work in [0, 1, 100] {
                for cached in [false, true] {
                    let mut baseline =
                        Session::with_context(&workbook, EvaluationInputs::default());
                    let mut actual = Session::with_context(&workbook, EvaluationInputs::default());
                    if cached {
                        baseline.lookup_prefixes.enable();
                        actual.lookup_prefixes.enable();
                    }
                    let baseline_table = table(&mut baseline, source);
                    let actual_table = table(&mut actual, source);
                    let limits = EvaluationBudgets {
                        run_work: work,
                        ..Default::default()
                    };
                    baseline.runtime.budget = BudgetMeter::new(limits).unwrap();
                    actual.runtime.budget = BudgetMeter::new(limits).unwrap();
                    let target = EvaluationTarget {
                        sheet: 0,
                        address: AT,
                    };
                    baseline.budget.push_cell(target);
                    actual.budget.push_cell(target);
                    let expected = baseline.classic_lookup_index(
                        &baseline_table,
                        SearchAxis::Vertical,
                        &Number(2.),
                        SearchMode::Exact,
                        ClassicLookupKind::Match,
                    );
                    let result = CallHost::new(&mut actual, 0, AT, ArgumentsRef::Empty)
                        .lookup_index(
                            &actual_table,
                            SearchAxis::Vertical,
                            &Number(2.),
                            SearchMode::Exact,
                            ClassicLookupKind::Match,
                        );
                    assert_eq!(result, expected, "{source}, {work}, {cached}");
                    assert_eq!(actual.budget.usage, baseline.budget.usage);
                    assert_eq!(actual.run_usage, baseline.run_usage);
                    if work == 0 {
                        assert!(
                            matches!(result,
                                Err(FunctionFailure::Operational(EvaluationFailure::Budget(
                                    BudgetFailure { target: Some(failed_at), .. }
                                ))) if failed_at == target
                            ),
                            "{source}, {work}, {cached}: {result:?}"
                        );
                    }
                }
            }
        }
    }
}
