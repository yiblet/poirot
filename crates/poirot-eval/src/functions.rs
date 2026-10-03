//! Common execution adapters; operational failures propagate independently of cell errors.
use super::*;
impl Session<'_> {
    /// Evaluate and scalarize one argument, observing only the requested position.
    pub(super) fn argument(
        &mut self,
        s: usize,
        at: CellAddress,
        args: ArgumentsRef<'_>,
        index: usize,
    ) -> EvalResult<CellValue> {
        if self.execution_observer.is_none() {
            return match args.get(index) {
                Some(expression) => {
                    let value = self.expr(s, at, expression)?;
                    self.scalar(at, value)
                }
                None => Ok(Blank),
            };
        }
        self.observed_scalar_argument(s, at, args, index)
    }
    /// Evaluate arguments in order and return their first spreadsheet error separately.
    pub(super) fn eager_arguments(
        &mut self,
        s: usize,
        at: CellAddress,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Result<Vec<CellValue>, CellError>> {
        self.budget.allocation(args.len())?;
        let values = (0..args.len())
            .map(|index| self.argument(s, at, args, index))
            .collect::<EvalResult<Vec<_>>>()?;
        if let Some(Error(error)) = values.iter().find(|value| matches!(value, Error(_))) {
            Ok(Err(*error))
        } else {
            Ok(Ok(values))
        }
    }
    /// Materialize a supported scalar, array, or single reference as a budgeted matrix.
    pub(super) fn matrix(
        &mut self,
        value: Value,
    ) -> EvalResult<Result<Vec<Vec<CellValue>>, CellError>> {
        match value {
            Value::Array(rows) => Ok(Ok(rows.values)),
            Value::Missing => {
                self.budget.allocation(1)?;
                Ok(Ok(vec![vec![Blank]]))
            }
            Value::Scalar(Error(error)) => Ok(Err(error)),
            Value::Scalar(value) => {
                self.budget.allocation(1)?;
                Ok(Ok(vec![vec![value]]))
            }
            Value::SheetSpan(areas) if areas.len() != 1 => {
                Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: None,
                    detail: "matrix materialization of a multi-sheet reference is not verified"
                        .into(),
                })
            }
            Value::SignedRefs(areas) => {
                if areas.len() != 1 {
                    return Ok(Err(VALUE_ERROR));
                }
                // SAFETY: The preceding branch rejects areas.len() != 1 before selecting the first area.
                let area = areas[0];
                if area.positive_shape().is_none() {
                    return Err(EvaluationFailure::UnsupportedCapability { capability: CapabilityId::FunctionAdapter, source: None,
                        detail: "dense consumer policy for zero/negative Java source dimensions is not verified".into() });
                }
                self.budget.allocation_u64(area.size())?;
                let mut rows = Vec::with_capacity(area.height() as usize);
                for row in 0..area.height() {
                    let mut values = Vec::with_capacity(area.width() as usize);
                    for col in 0..area.width() {
                        values.push(self.signed_read(
                            area.read(row, col).ok_or(EvaluationFailure::StalePlan)?,
                        )?);
                    }
                    rows.push(values);
                }
                Ok(Ok(rows))
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                if areas.len() != 1 {
                    return Ok(Err(VALUE_ERROR));
                }
                // SAFETY: The preceding branch rejects areas.len() != 1 before selecting the first area.
                let area = areas[0];
                self.budget.allocation_u64(area.size())?;
                let mut rows = Vec::with_capacity((area.r1 - area.r0 + 1) as usize);
                for row in area.r0..=area.r1 {
                    let mut values = Vec::with_capacity((area.c1 - area.c0 + 1) as usize);
                    for col in area.c0..=area.c1 {
                        values.push(self.cell(area.sheet, CellAddress { row, col })?);
                    }
                    rows.push(values);
                }
                Ok(Ok(rows))
            }
        }
    }
    /// Discover spill values and record membership before collecting occupied coordinates.
    pub(super) fn sparse_addresses(&mut self, area: Area) -> EvalResult<Vec<CellAddress>> {
        self.discover_spills_in(area, None)?;
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let Some(consumer) = self.consumer
        {
            for owner in self
                .visiting
                .iter()
                .copied()
                .chain(std::iter::once((consumer.sheet, consumer.address)))
            {
                let ranges = self.spill_read_ranges.entry(owner).or_default();
                self.budget
                    .check_allocation(ranges.len().saturating_add(1))?;
                self.budget.charge(WorkKind::Materialization, 1)?;
                ranges.push(area);
            }
        }
        self.read(DependencyRead::SparseMembership {
            sheet: area.sheet,
            first: CellAddress {
                row: area.r0,
                col: area.c0,
            },
            last: CellAddress {
                row: area.r1,
                col: area.c1,
            },
        });
        let membership = poirot_model::query::RangeMembership::new(
            area.sheet,
            CellAddress {
                row: area.r0,
                col: area.c0,
            },
            CellAddress {
                row: area.r1,
                col: area.c1,
            },
        )
        .map_err(|_| EvaluationFailure::StalePlan)?;
        let view = crate::run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs);
        // Count before reserving: sparse collections retain the same allocation limits
        // as dense consumers, without charging unrelated worksheet entries.
        let mut count = 0usize;
        let mut cells = view
            .stored(membership)
            .map_err(|_| EvaluationFailure::StalePlan)?;
        while sparse_next(&mut cells, &mut self.budget)?.is_some() {
            self.budget.check_allocation(count.saturating_add(1))?;
            count += 1;
        }
        self.budget.allocation(count)?;
        let mut addresses = Vec::with_capacity(count);
        let mut cells = view
            .stored(membership)
            .map_err(|_| EvaluationFailure::StalePlan)?;
        while let Some(address) = sparse_next(&mut cells, &mut self.budget)? {
            addresses.push(address);
        }
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill {
            let mut seen: HashSet<_> = addresses.iter().copied().collect();
            for &(sheet, address) in self.spill_owners.keys() {
                self.budget.charge(WorkKind::SparseScan, 1)?;
                if sheet == area.sheet && area.contains(address) && !seen.contains(&address) {
                    self.budget
                        .check_allocation(addresses.len().saturating_add(1))?;
                    self.budget.charge(WorkKind::Materialization, 1)?;
                    seen.insert(address);
                    addresses.push(address);
                }
            }
            addresses.sort();
            addresses.dedup();
        }
        Ok(addresses)
    }
}

/// Charge seeks and visits after every iterator step, including the terminating step.
/// Empty rows/columns can require seeks even when no stored cell is returned.
fn sparse_next(
    cells: &mut crate::run_overlay::EffectiveStoredCells<'_>,
    budget: &mut BudgetMeter,
) -> EvalResult<Option<CellAddress>> {
    let before = cells.counters();
    let address = cells.next().map(|(address, _)| *address);
    let after = cells.counters();
    for amount in [
        after.row_seeks - before.row_seeks,
        after.column_seeks - before.column_seeks,
        after.cells_visited - before.cells_visited,
    ] {
        budget.charge(WorkKind::SparseScan, amount)?;
    }
    Ok(address)
}

#[cfg(test)]
mod sparse_tests {
    use super::*;

    fn book() -> Workbook {
        let mut book = Workbook::new();
        book.add_sheet("Data").unwrap();
        book
    }
    fn area(r0: u32, r1: u32, c0: u32, c1: u32) -> Area {
        Area {
            kind: AreaKind::Area,
            sheet: 0,
            r0,
            r1,
            c0,
            c1,
        }
    }
    fn session(book: &Workbook, limits: EvaluationBudgets) -> Session<'_> {
        Session::new(
            book,
            EvaluationInputs::default(),
            BudgetMeter::new(limits).unwrap(),
        )
    }
    fn evaluate(book: &Workbook, formula: &str) -> EvaluationOutcome<CellValue> {
        run_formula(
            book,
            0,
            CellAddress { row: 20, col: 20 },
            formula,
            EvaluationInputs::default(),
            EvaluationBudgets {
                run_work: 500,
                cell_work: 500,
                ..EvaluationBudgets::default()
            },
        )
    }
    #[test]
    fn tiny_range_budget_ignores_far_away_stored_cells() {
        let mut book = book();
        let sheet = book.sheet_mut(0).unwrap();
        sheet
            .set_value(CellAddress { row: 1, col: 0 }, 7.0)
            .unwrap();
        for row in 1000..3000 {
            sheet
                .set_value(CellAddress { row, col: 500 }, 99.0)
                .unwrap();
        }
        let result = evaluate(&book, "SUM(A1:A3)");
        assert!(matches!(
            result,
            EvaluationOutcome::Complete {
                value: Number(7.0),
                ..
            }
        ));
        // Two passes: each seeks the occupied row and its columns, visits one
        // cell, then performs a final unsuccessful row seek within A1:A3.
        assert_eq!(result.usage().sparse_entries, 8);
    }
    #[test]
    fn empty_query_charges_terminal_seek_and_honors_limit() {
        let book = book();
        let mut session = session(&book, EvaluationBudgets::default());
        assert!(
            session
                .sparse_addresses(area(0, 9, 0, 0))
                .unwrap()
                .is_empty()
        );
        assert_eq!(session.budget.usage.sparse_entries, 2);
        let mut limited = session_for_empty(&book);
        assert!(matches!(
            limited.sparse_addresses(area(0, 9, 0, 0)),
            Err(EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::RunWork,
                ..
            }))
        ));
    }
    fn session_for_empty(book: &Workbook) -> Session<'_> {
        session(
            book,
            EvaluationBudgets {
                run_work: 1,
                ..EvaluationBudgets::default()
            },
        )
    }
    #[test]
    fn row_major_order_overlaps_and_allocation_limit_are_preserved() {
        let mut book = book();
        for (row, col) in [(2, 0), (0, 2), (0, 0), (1, 1)] {
            book.sheet_mut(0)
                .unwrap()
                .set_value(CellAddress { row, col }, 1.0)
                .unwrap();
        }
        let mut session = session(&book, EvaluationBudgets::default());
        assert_eq!(
            session.sparse_addresses(area(0, 2, 0, 2)).unwrap(),
            vec![
                CellAddress { row: 0, col: 0 },
                CellAddress { row: 0, col: 2 },
                CellAddress { row: 1, col: 1 },
                CellAddress { row: 2, col: 0 }
            ]
        );
        assert!(matches!(
            evaluate(&book, "SUM((A1:C3,A1:C3))"),
            EvaluationOutcome::Complete {
                value: Number(8.0),
                ..
            }
        ));
        let mut limited = Session::new(
            &book,
            EvaluationInputs::default(),
            BudgetMeter::new(EvaluationBudgets {
                materialized_values: 3,
                ..EvaluationBudgets::default()
            })
            .unwrap(),
        );
        assert!(matches!(
            limited.sparse_addresses(area(0, 2, 0, 2)),
            Err(EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::Materialization,
                ..
            }))
        ));
    }
    #[test]
    fn absent_range_membership_reads_fresh_inserted_and_removed_cells() {
        let mut book = book();
        let query = "SUM(A1:A10)";
        assert!(matches!(
            evaluate(&book, query),
            EvaluationOutcome::Complete {
                value: Number(0.0),
                ..
            }
        ));
        let address = "A5".parse().unwrap();
        book.sheet_mut(0).unwrap().set_value(address, 4.0).unwrap();
        assert!(matches!(
            evaluate(&book, query),
            EvaluationOutcome::Complete {
                value: Number(4.0),
                ..
            }
        ));
        book.sheet_mut(0).unwrap().remove(address);
        assert!(matches!(
            evaluate(&book, query),
            EvaluationOutcome::Complete {
                value: Number(0.0),
                ..
            }
        ));
    }

    #[test]
    fn whole_column_blank_coercion_and_error_order_are_preserved() {
        let mut book = book();
        let sheet = book.sheet_mut(0).unwrap();
        sheet.set_value("A1".parse().unwrap(), true).unwrap();
        sheet.set_value("A2".parse().unwrap(), "2").unwrap();
        sheet.set_value("A3".parse().unwrap(), 3.0).unwrap();
        sheet
            .set("A4".parse().unwrap(), poirot_model::Cell::default())
            .unwrap();
        sheet
            .set_formula("A5".parse().unwrap(), "\"\"", Blank)
            .unwrap();
        for (formula, expected) in [
            ("SUM(A:A)", Number(3.0)),
            ("COUNTBLANK(A:A)", Number(f64::from(MAX_ROWS - 3))),
            ("SUM((A3:A5,A3:A5))", Number(6.0)),
        ] {
            assert!(
                matches!(evaluate(&book, formula),
                EvaluationOutcome::Complete { value, .. } if value == expected),
                "{formula}"
            );
        }
        book.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), Error(NA))
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), Error(Div0))
            .unwrap();
        assert!(matches!(
            evaluate(&book, "SUM(A:A)"),
            EvaluationOutcome::Complete {
                value: Error(NA),
                ..
            }
        ));
    }
}
