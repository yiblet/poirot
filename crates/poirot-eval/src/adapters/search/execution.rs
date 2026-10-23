//! Classic lookup host adapter: admission, bounded reads and result projection.
use crate::*;
use std::cmp::Ordering;
#[path = "execution/classic_lookup_compare.rs"]
mod comparison;
#[path = "execution/classic_lookup_plan.rs"]
mod plan;
#[path = "execution/classic_lookup_prefix.rs"]
mod prefix;
#[path = "execution/classic_lookup_select.rs"]
mod selection;
#[path = "execution/classic_lookup_sparse.rs"]
mod sparse;
use comparison::lookup_compare;
use plan::{LookupTable, LookupVector};
/// Bounded exact-lookup prefixes, retained across successful overlay evaluations.
pub(crate) use prefix::Prefixes as LookupPrefixes;
#[cfg(test)]
use selection::select_index;
use selection::select_with_reader;

impl Session<'_> {
    fn lookup_read(
        &mut self,
        table: &LookupTable,
        vector: LookupVector,
        index: usize,
    ) -> EvalResult<CellValue> {
        let (row, col) = match vector.coordinates(index) {
            Ok(v) => v,
            Err(e) => return Ok(Error(e)),
        };
        match table {
            LookupTable::SheetCells(areas) => {
                let a = *areas.get(row).ok_or(EvaluationFailure::StalePlan)?;
                self.cell(
                    a.sheet,
                    CellAddress {
                        row: a.r0,
                        col: a.c0,
                    },
                )
            }
            LookupTable::Reference(a) => self.cell(
                a.sheet,
                CellAddress {
                    row: a.r0 + row as u32,
                    col: a.c0 + col as u32,
                },
            ),
            LookupTable::Signed(a) => self.signed_read(
                a.read(row as u64, col as u64)
                    .ok_or(EvaluationFailure::StalePlan)?,
            ),
            LookupTable::Array(a) => {
                let value = a
                    .get(row)
                    .and_then(|values| values.get(col))
                    .ok_or(EvaluationFailure::StalePlan)?;
                if let Some(text) = value.text_ref() {
                    self.budget.text(text.storage_bytes())?;
                }
                Ok(value.clone())
            }
        }
    }
    /// Execute a lookup handler while preserving source operand and candidate read order.
    pub(crate) fn search_function(
        &mut self,
        s: usize,
        at: CellAddress,
        name: SearchFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        if let Some(function) = name.helper() {
            return adapters::lookup::execute(self, s, at, function, args);
        }
        let is_match = name == SearchFunction::Match;
        if !FunctionId::Search(name)
            .descriptor()
            .arity
            .accepts(args.len())
        {
            return Ok(err(VALUE_ERROR));
        }
        // Match.java resolves match_type before resolving the key/range.
        let match_mode = if is_match && args.len() == 3 {
            let value = self.argument(s, at, args, 2)?;
            if value == Blank {
                SearchMode::Ascending
            } else {
                match semantic(self.coercev2_scalar_number(&value))? {
                    Ok(n) => SearchMode::from_number(n),
                    Err(_) => return Ok(err(Ref)),
                }
            }
        } else {
            SearchMode::Ascending
        };
        let key = self.argument(s, at, args, 0)?;
        if let Error(e) = key {
            return Ok(err(e));
        }
        let table_value = self.expr(s, at, args.get(1).ok_or(EvaluationFailure::StalePlan)?)?;
        if is_match
            && let Value::Scalar(value) | Value::SelectedScalar(value, _) = &table_value
            && value.text_ref().is_some()
        {
            let parsed = semantic(self.coercev2_scalar_number(value))?;
            return Ok(err(if parsed.is_ok() { NA } else { VALUE_ERROR }));
        }
        let table = match LookupTable::admit(table_value, is_match) {
            Ok(t) => t,
            Err(e) => return Ok(err(e)),
        };
        let (height, width) = table.shape();
        if height == 0 || width == 0 {
            return Ok(err(NA));
        }
        let axis = if is_match {
            if height != 1 && width != 1 {
                return Ok(err(NA));
            }
            if width == 1 {
                SearchAxis::Vertical
            } else {
                SearchAxis::Horizontal
            }
        } else if name == SearchFunction::VLookup {
            SearchAxis::Vertical
        } else {
            SearchAxis::Horizontal
        };
        let mode = if is_match {
            match_mode
        } else if args.len() == 4 {
            match coercev2_logical_boolean(&self.argument(s, at, args, 3)?) {
                Ok(true) => SearchMode::Ascending,
                Ok(false) => SearchMode::Exact,
                Err(e) => return Ok(err(e)),
            }
        } else {
            SearchMode::Ascending
        };
        let vector = table
            .vector(axis, 0)
            .map_err(|_| EvaluationFailure::StalePlan)?;
        // Wildcard comparison is a predicate, not a total order suitable for bisection.
        let pattern_match = is_match
            && key
                .text_ref()
                .is_some_and(|text| text.units().any(|unit| matches!(unit, 0x2a | 0x3f)));
        let excel_match = is_match
            && !pattern_match
            && mode != SearchMode::Exact
            && self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation;
        if is_match && mode != SearchMode::Exact {
            self.read(DependencyRead::BuiltinPolicy);
        }
        let found = if excel_match {
            poirot_functions_lookup::classic_match_index(
                vector.len,
                if mode == SearchMode::Ascending {
                    poirot_functions_lookup::ClassicMatchMode::Ascending
                } else {
                    poirot_functions_lookup::ClassicMatchMode::Descending
                },
                &mut sparse::LookupReader {
                    session: self,
                    table: &table,
                    vector,
                    key: &key,
                    mode,
                    is_match,
                    forward: None,
                },
            )
            .map_err(|failure| match failure {
                poirot_functions_lookup::MatchSelectionFailure::Reader(failure) => failure,
                poirot_functions_lookup::MatchSelectionFailure::InvalidPosition => {
                    EvaluationFailure::StalePlan.into()
                }
            })
        } else {
            prefix::select(self, &table, vector, &key, mode, is_match).unwrap_or_else(|| {
                select_with_reader(
                    vector.len,
                    mode,
                    !is_match,
                    &mut sparse::LookupReader {
                        session: self,
                        table: &table,
                        vector,
                        key: &key,
                        mode,
                        is_match,
                        forward: None,
                    },
                )
            })
        };
        let index = match found {
            Ok(Some(i)) => i,
            Ok(None) => return Ok(err(NA)),
            Err(FunctionFailure::Spreadsheet(e)) => return Ok(err(e)),
            Err(FunctionFailure::Operational(e)) => return Err(e),
        };
        if is_match {
            return Ok(Number((index + 1) as f64).into());
        }
        // Vlookup.java/Hlookup.java defer the result index until selection succeeds.
        let value = self.argument(s, at, args, 2)?;
        let result_index = match semantic(self.coercev2_scalar_number(&value))? {
            Ok(n) if n.floor() >= 1. => n.floor() as usize - 1,
            Ok(_) => return Ok(err(VALUE_ERROR)),
            Err(_) => return Ok(err(Ref)),
        };
        let result = match table.vector(axis, result_index) {
            Ok(v) => v,
            Err(e) => return Ok(err(e)),
        };
        let origin = match &table {
            LookupTable::Array(_) => host::OperandOrigin::ArrayElement,
            _ => host::OperandOrigin::ReferencedCell,
        };
        Ok(Value::SelectedScalar(
            self.lookup_read(&table, result, index)?,
            origin,
        ))
    }
}
/// Exact matching or approximate matching against an ordered vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    /// Select an exact match, applying source wildcard rules where supported.
    Exact,
    /// Approximate matching under ascending-order rules.
    Ascending,
    /// Approximate matching under descending-order rules.
    Descending,
}
impl SearchMode {
    fn from_number(value: f64) -> Self {
        if value == 0. {
            Self::Exact
        } else if value > 0. {
            Self::Ascending
        } else {
            Self::Descending
        }
    }
}
/// Zero-based position selected within the logical lookup vector, not an absolute worksheet row/column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchSelection {
    /// Zero-based logical position in the lookup vector.
    pub index: usize,
}
/// Direction in which a logical lookup vector traverses its table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchAxis {
    /// Advance down rows within a fixed column.
    Vertical,
    /// Advance across columns within a fixed row.
    Horizontal,
}

#[cfg(test)]
#[path = "execution/classic_match_tests.rs"]
mod classic_match_tests;
#[cfg(test)]
#[path = "execution/classic_lookup_historical_select.rs"]
mod historical_selection;
#[cfg(test)]
#[path = "execution/classic_lookup_incremental_tests.rs"]
mod incremental_tests;
#[cfg(test)]
#[path = "execution/classic_lookup_sparse_tests.rs"]
mod sparse_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boolean_match_modes_follow_numeric_modes_and_source_read_order() {
        /// Collects actual read events for lookup ordering assertions.
        #[derive(Default)]
        struct Reads(Vec<ReadEvent>);
        impl DependencyReadSink for Reads {
            fn read(&mut self, event: ReadEvent) {
                self.0.push(event);
            }
        }
        for (mode, expected, candidates) in [
            ("TRUE", Number(3.), vec!["B4", "B3"]),
            ("1", Number(3.), vec!["B4", "B3"]),
            ("FALSE", Number(2.), vec!["B1", "B2"]),
            ("0", Number(2.), vec!["B1", "B2"]),
            ("1/0", Number(99.), vec![]),
        ] {
            let mut w = Workbook::new();
            w.add_sheet("Sheet1").unwrap();
            let sheet = w.sheet_mut(0).unwrap();
            sheet
                .set_formula_with_cached_value("C1".parse().unwrap(), mode, Number(-999.))
                .unwrap();
            sheet
                .set_formula_with_cached_value("A1".parse().unwrap(), "1+1", Number(-999.))
                .unwrap();
            for (cell, value) in [("B1", 1.), ("B2", 2.), ("B3", 2.), ("B4", 3.)] {
                sheet.set_value(cell.parse().unwrap(), value).unwrap();
            }
            let syntax =
                poirot_parsing::formula::parse_formula("IFERROR(MATCH(A1,B1:B4,C1),99)").unwrap();
            let selected = bound::FormulaPlan::bind(&syntax).unwrap();
            let mut reads = Reads::default();
            let mut session = Session::with_context(&w, EvaluationInputs::default());
            session.builtin_policy = BuiltinSemanticsPolicy::PinnedPoi;
            session.array_result_policy = ArrayResultPolicy::ScalarIntersection;
            session.dependency_sink = Some(&mut reads);
            let at = "Z20".parse().unwrap();
            let value = session.expr(0, at, &selected.expression()).unwrap();
            assert_eq!(session.scalar(at, value).unwrap(), expected, "{mode}");
            assert_eq!(
                session.run_usage.formula_executions,
                if mode == "1/0" { 1 } else { 2 }
            );
            assert_eq!(
                session.cache.contains_key(&(0, "A1".parse().unwrap())),
                mode != "1/0"
            );
            drop(session);
            let cells: Vec<_> = reads
                .0
                .into_iter()
                .filter_map(|event| match event.read {
                    DependencyRead::Cell(target) => Some(target),
                    _ => None,
                })
                .collect();
            let mut expected_reads = vec!["C1"];
            if mode != "1/0" {
                expected_reads.push("A1");
            }
            expected_reads.extend(candidates);
            assert_eq!(
                cells,
                expected_reads
                    .into_iter()
                    .map(|cell| EvaluationTarget {
                        sheet: 0,
                        address: cell.parse().unwrap(),
                    })
                    .collect::<Vec<_>>(),
                "{mode}"
            );
        }
    }
    fn select_match(
        key: &CellValue,
        candidates: &[CellValue],
        mode: SearchMode,
    ) -> FunctionResult<Option<SearchSelection>> {
        {
            let mut budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
            select_index(candidates.len(), mode, false, |i| {
                lookup_compare(key, &candidates[i], mode, true, &mut budget)
            })
            .map(|i| i.map(|index| SearchSelection { index }))
        }
    }
    #[test]
    fn sheet_spans_use_source_specific_vector_geometry() {
        let mut w = Workbook::new();
        for name in ["First", "Last"] {
            w.add_sheet(name).unwrap();
        }
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), 7.)
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("B1".parse().unwrap(), 42.)
            .unwrap();
        w.sheet_mut(1)
            .unwrap()
            .set_value("A1".parse().unwrap(), 9.)
            .unwrap();
        for (formula, expected) in [
            ("MATCH(9,First:Last!A1,0)", Number(2.)),
            ("VLOOKUP(7,First:Last!A:B,2,FALSE)", Number(42.)),
            ("MATCH(7,First:Last!A1:A2,0)", Number(1.)),
            (
                "VLOOKUP(7,(First!A1:B1,Last!A1:B1),2,FALSE)",
                Error(VALUE_ERROR),
            ),
        ] {
            assert_eq!(
                match crate::run_formula(
                    &w,
                    0,
                    "Z20".parse().unwrap(),
                    formula,
                    Default::default(),
                    Default::default()
                ) {
                    crate::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                expected,
                "{formula}"
            );
        }
    }
    #[test]
    fn whole_columns_read_only_selected_prefix_and_result() {
        let mut w = Workbook::new();
        w.add_sheet("Sheet1").unwrap();
        let sheet = w.sheet_mut(0).unwrap();
        sheet.set_value("A1".parse().unwrap(), 7.).unwrap();
        sheet
            .set_formula_with_cached_value("B1".parse().unwrap(), "6*7", Number(-99.))
            .unwrap();
        for (formula, expected) in [
            ("MATCH(7,A:A,0)", Number(1.)),
            ("VLOOKUP(7,A:B,2,FALSE)", Number(42.)),
            ("HLOOKUP(7,1:3,3,FALSE)", Blank),
            ("VLOOKUP(8,A1:B1,1/0,FALSE)", Error(NA)),
            ("VLOOKUP(7,A1:B1,1/0,FALSE)", Error(Ref)),
        ] {
            assert!(
                matches!(run_formula(&w, 0, "Z20".parse().unwrap(), formula, EvaluationInputs::default(), EvaluationBudgets::default()), EvaluationOutcome::Complete { value, .. } if value == expected),
                "{formula}"
            );
        }
    }
    #[test]
    fn exact_reads_do_not_consume_unused_formulas_or_errors() {
        #[derive(Default)]
        /// Records exact-match reads to detect accidental evaluation of unused candidates.
        struct Reads(Vec<ReadEvent>);
        impl DependencyReadSink for Reads {
            fn read(&mut self, event: ReadEvent) {
                self.0.push(event);
            }
        }
        let mut w = Workbook::new();
        w.add_sheet("Sheet1").unwrap();
        let sheet = w.sheet_mut(0).unwrap();
        sheet.set_value("A1".parse().unwrap(), 7.).unwrap();
        sheet
            .set_formula_with_cached_value("B1".parse().unwrap(), "6*7", Number(-99.))
            .unwrap();
        for (cell, formula) in [("A2", "RAND()"), ("B2", "UNSUPPORTED()"), ("B3", "1/0")] {
            sheet
                .set_formula_with_cached_value(cell.parse().unwrap(), formula, Number(-99.))
                .unwrap();
        }
        let mut reads = Reads::default();
        let mut session = Session::with_context(&w, EvaluationInputs::default());
        session.dependency_sink = Some(&mut reads);
        let syntax = poirot_parsing::formula::parse_formula("VLOOKUP(7,A:B,2,FALSE)").unwrap();
        let plan = bound::FormulaPlan::bind(&syntax).unwrap();
        let at = "Z20".parse().unwrap();
        let value = session.expr(0, at, &plan.expression()).unwrap();
        assert_eq!(session.scalar(at, value).unwrap(), Number(42.));
        assert_eq!(session.run_usage.formula_executions, 1);
        for cell in ["A2", "B2", "B3"] {
            assert!(!session.cache.contains_key(&(0, cell.parse().unwrap())));
        }
        drop(session);
        let cells: Vec<_> = reads
            .0
            .into_iter()
            .filter_map(|event| match event.read {
                DependencyRead::Cell(target) => Some(target),
                _ => None,
            })
            .collect();
        assert_eq!(
            cells,
            ["A1", "B1"].map(|cell| EvaluationTarget {
                sheet: 0,
                address: cell.parse().unwrap(),
            })
        );
    }
    #[test]
    fn real_read_and_array_limits_escape_iferror() {
        let mut w = Workbook::new();
        w.add_sheet("Sheet1").unwrap();
        // Proven absence now skips memo/read work; retain a genuine stored-read refusal.
        for row in 0..100 {
            w.sheet_mut(0)
                .unwrap()
                .set_value(CellAddress { row, col: 0 }, 1.)
                .unwrap();
        }
        for (formula, budgets, kind) in [
            (
                "IFERROR(MATCH(7,A:A,0),99)",
                EvaluationBudgets {
                    cell_work: 100,
                    ..EvaluationBudgets::default()
                },
                BudgetKind::CellWork,
            ),
            (
                "IFERROR(MATCH(1,{1;2;3},0),99)",
                EvaluationBudgets {
                    materialized_values: 2,
                    ..EvaluationBudgets::default()
                },
                BudgetKind::Materialization,
            ),
        ] {
            let outcome = run_formula(
                &w,
                0,
                "Z20".parse().unwrap(),
                formula,
                EvaluationInputs::default(),
                budgets,
            );
            let usage = outcome.usage();
            if kind == BudgetKind::CellWork {
                // Operational refusals must escape IFERROR regardless of the
                // sparse traversal algorithm's page/row counter granularity.
                assert_eq!(usage.expressions, 7);
                // Cold sheet classification may exhaust the allowance before
                // the first candidate comparison, and remains an operational failure.
                assert!(usage.sparse_entries > 0);
                assert_eq!(usage.text_bytes, 26);
            } else {
                assert_eq!(usage.work_units, 35);
                assert_eq!(usage.expressions, 5);
                assert_eq!(usage.cell_reads, 0);
                assert_eq!(usage.sparse_entries, 0);
                assert_eq!(usage.materialized_values, 0);
                assert_eq!(usage.comparisons, 0);
                assert_eq!(usage.text_bytes, 30);
                assert_eq!(usage.completed_cells, 0);
            }
            let EvaluationOutcome::Aborted {
                failure: EvaluationFailure::Budget(failure),
                ..
            } = outcome
            else {
                panic!("{formula}: {outcome:?}")
            };
            assert_eq!(failure.kind, kind, "{formula}");
            assert_eq!(
                failure.limit,
                if kind == BudgetKind::CellWork { 100 } else { 2 }
            );
            assert!(failure.consumed <= failure.limit);
            assert!(failure.consumed.saturating_add(failure.requested) > failure.limit);
            assert_eq!(
                failure.consumed,
                if kind == BudgetKind::CellWork { 100 } else { 0 }
            );
            assert_eq!(
                failure.requested,
                if kind == BudgetKind::CellWork { 1 } else { 3 }
            );
            // Sparse pages let more candidate reads fit than row-wise queries;
            // refusal can occur at either a candidate read or its consumer.
            // Dense array admission precedes reads and belongs to the consumer.
            let target = failure.target.expect("failure must identify its cell");
            assert_eq!(target.sheet, 0);
            if kind == BudgetKind::CellWork {
                assert!(target.address.col == 0 || target.address == "Z20".parse().unwrap());
            } else {
                assert_eq!(target.address, "Z20".parse().unwrap());
            }
        }
    }
    #[test]
    fn typed_selection_preserves_duplicates_type_filtering_and_wildcards() {
        let key = Number(2.);
        let ascending = vec![Number(1.), Number(2.), Number(2.), Number(3.)];
        assert_eq!(
            select_match(&key, &ascending, SearchMode::Exact),
            Ok(Some(SearchSelection { index: 1 }))
        );
        assert_eq!(
            select_match(&key, &ascending, SearchMode::Ascending),
            Ok(Some(SearchSelection { index: 2 }))
        );
        let descending = vec![Number(3.), Number(2.), Number(2.), Number(1.)];
        assert_eq!(
            select_match(&key, &descending, SearchMode::Descending),
            Ok(Some(SearchSelection { index: 1 }))
        );
        let mixed = vec![Text("2".into()), Error(NA), Number(2.)];
        assert_eq!(
            select_match(&key, &mixed, SearchMode::Exact),
            Ok(Some(SearchSelection { index: 2 }))
        );
        assert_eq!(
            select_match(
                &Text("a*".into()),
                &[Text("ALPHA".into())],
                SearchMode::Exact
            ),
            Ok(Some(SearchSelection { index: 0 }))
        );
        assert_eq!(
            select_match(&Number(9.), &ascending, SearchMode::Exact),
            Ok(None)
        );
        assert_eq!(SearchMode::from_number(0.5), SearchMode::Ascending);
        assert_eq!(SearchMode::from_number(-0.5), SearchMode::Descending);
    }
    #[test]
    fn bound_search_modes_and_error_order_remain_runtime_coercions() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        let at = CellAddress { row: 0, col: 0 };
        for (formula, expected) in [
            ("MATCH(2,{1;2;2;3},0)", Number(2.)),
            ("MATCH(2,{1;2;2;3},0.5)", Number(3.)),
            ("MATCH(2,{3;2;2;1},-0.5)", Number(2.)),
            ("MATCH(2,{\"2\";2},0)", Number(2.)),
            ("MATCH(\"a\",{\"A\";\"a\"},0)", Number(1.)),
            ("MATCH(8,{1;2},0)", Error(NA)),
            ("VLOOKUP(2,{1,10;2,20;2,21},2,FALSE)", Number(20.)),
            ("VLOOKUP(2,{1,10;2,20;2,21},2,TRUE)", Number(21.)),
            ("HLOOKUP(2,{1,2,2;10,20,21},2,FALSE)", Number(20.)),
            ("HLOOKUP(2,{1,2,2;10,20,21},2,TRUE)", Number(21.)),
            ("MATCH(#REF!,#N/A,#DIV/0!)", Error(Ref)),
            ("MATCH(1,{1},\"bad\")", Error(Ref)),
            ("VLOOKUP(1,#N/A,#DIV/0!)", Error(VALUE_ERROR)),
            ("VLOOKUP(#REF!,#N/A)", Error(VALUE_ERROR)),
        ] {
            let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
            let plan = bound::FormulaPlan::bind(&syntax).unwrap();
            assert_eq!(
                match crate::run_plan(
                    &workbook,
                    0,
                    at,
                    &plan,
                    Default::default(),
                    Default::default()
                ) {
                    crate::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                expected,
                "{formula}"
            );
        }
    }
}

#[cfg(test)]
#[path = "execution/classic_lookup_prefix_tests.rs"]
mod prefix_tests;
