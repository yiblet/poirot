//! Prepared execution, recursive cell reads, memoization, and completion lifecycle.
use super::*;

impl<'a> Session<'a> {
    /// Compile a reached source using the overlay's shared compiler when available.
    pub(super) fn prepare_source(&self, source: &str) -> std::sync::Arc<PreparedFormula> {
        self.with_source_compiler(|compiler, policy| match self.extensions {
            Some(registry) => compiler.compile_formula_with_extensions(source, policy, registry),
            None => compiler.compile_formula(source, policy),
        })
    }
    /// Borrow the shared source compiler and preparation policy, falling back locally.
    pub(super) fn with_source_compiler<R>(
        &self,
        prepare: impl FnOnce(&mut FormulaCompiler, &EvaluationPolicy) -> R,
    ) -> R {
        let default;
        let policy = match self.preparation_policy {
            Some(policy) => policy,
            None => {
                default = self
                    .extensions
                    .map_or_else(EvaluationPolicy::default, |registry| {
                        EvaluationPolicy::with_extensions(
                            poirot_parsing::formula::FormulaParsePolicy::default(),
                            registry,
                        )
                    });
                &default
            }
        };
        if let Some(mut compiler) = self
            .lazy_compiler
            .and_then(|compiler| compiler.try_borrow_mut().ok())
        {
            return prepare(&mut compiler, policy);
        }
        prepare(&mut FormulaCompiler::new(), policy)
    }

    #[cfg(test)]
    /// Start a test session with explicit context and default work limits.
    pub(super) fn with_context(workbook: &'a Workbook, context: EvaluationInputs) -> Self {
        Self::new(
            workbook,
            context,
            BudgetMeter::new(EvaluationBudgets::default()).unwrap(),
        )
    }
    /// Create empty run state around a borrowed workbook and validated work meter.
    pub(super) fn new(
        workbook: &'a Workbook,
        context: EvaluationInputs,
        budget: BudgetMeter,
    ) -> Self {
        Self::with_runtime(
            workbook,
            context,
            runtime::Storage::Owned(Box::new(runtime::Runtime::new(budget))),
        )
    }
    /// Attach workbook ports to owned or persistently borrowed execution storage.
    pub(super) fn with_runtime(
        workbook: &'a Workbook,
        context: EvaluationInputs,
        runtime: runtime::Storage<'a>,
    ) -> Self {
        Self {
            runtime,
            workbook,
            inputs: None,
            context,
            currency: None,
            number_locale: &poirot_parsing::number::NumberLocaleV2::EN_US,
            regex: Default::default(),

            builtin_policy: BuiltinSemanticsPolicy::default(),
            array_result_policy: ArrayResultPolicy::default(),
            producer_policy: ProducerPolicy::default(),
            expression_demand: Default::default(),
            spill_candidates: None,
            text_policy: poirot_text::TextSemanticsPolicy::default(),
            memo_limits: None,
            extensions: None,
            plan: None,
            lazy_compiler: None,
            preparation_policy: None,
            overlay_cache: None,
            overlay_reads: None,
            dependency_sink: None,
            cell_reader: None,
            cell_observer: None,
            builtin_invocation_observer: None,
            execution_observer: None,
        }
    }
    /// Construct dependency-only payloads only when a tracker or external sink needs them.
    pub(super) fn read_lazy(&mut self, read: impl FnOnce() -> DependencyRead) {
        if self.overlay_reads.is_some() || self.dependency_sink.is_some() || self.settling_spills()
        {
            self.read(read());
        }
    }
    /// Attribute a consumed read to the active cell and forward it to installed sinks.
    pub(super) fn read(&mut self, read: DependencyRead) {
        self.record_spill_effect(&read);
        record_read(
            &mut self.overlay_reads,
            &mut self.dependency_sink,
            ReadEvent {
                consumer: self.runtime.consumer,
                read,
            },
        );
    }

    /// Execute prepared source, preserving deferred parse failures and registry checks.
    pub(super) fn prepared_expression(
        &mut self,
        sheet: usize,
        at: CellAddress,
        prepared: &PreparedFormula,
    ) -> EvalResult<Value> {
        if self
            .plan
            .is_some_and(|plan| plan.policy() != prepared.policy())
        {
            return Err(EvaluationFailure::StalePlan);
        }
        if self.runtime.budget.profiling {
            self.runtime.run_usage.compiled_lookups =
                self.runtime.run_usage.compiled_lookups.saturating_add(1);
        }
        match prepared {
            PreparedFormula::Ready(formula) => self.bytecode_expr(sheet, at, formula.bytecode()),
            PreparedFormula::CompilationFailure { error, .. } => {
                let source = Some(EvaluationTarget { sheet, address: at });
                let capability = CapabilityId::FormulaCompilation;
                let detail = error.to_string();
                Err(
                    if matches!(error, crate::bytecode::CompileError::ResourceLimit) {
                        EvaluationFailure::ResourceLimit {
                            capability,
                            source,
                            detail,
                        }
                    } else {
                        EvaluationFailure::UnsupportedCapability {
                            capability,
                            source,
                            detail,
                        }
                    },
                )
            }
            PreparedFormula::ParseFailure { error, .. } => {
                let source = Some(EvaluationTarget { sheet, address: at });
                let capability = CapabilityId::FormulaParsing;
                let detail = error.to_string();
                match error.kind {
                    poirot_parsing::formula::ParseErrorKind::Syntax => Ok(err(VALUE_ERROR)),
                    poirot_parsing::formula::ParseErrorKind::ResourceLimit => {
                        Err(EvaluationFailure::ResourceLimit {
                            capability,
                            source,
                            detail,
                        })
                    }
                    poirot_parsing::formula::ParseErrorKind::InvalidPolicy => {
                        Err(EvaluationFailure::UnsupportedCapability {
                            capability,
                            source,
                            detail,
                        })
                    }
                }
            }
            PreparedFormula::RegistryMismatch { .. } => Err(EvaluationFailure::StalePlan),
        }
    }
    /// Execute a supplied prepared formula within a cell-attributed budget frame.
    pub(super) fn prepared_at(
        &mut self,
        target: EvaluationTarget,
        prepared: &PreparedFormula,
    ) -> EvalResult<CellValue> {
        if self.workbook.sheet(target.sheet).is_none() || target.address.validate().is_err() {
            return Ok(Error(Ref));
        }
        let demand = self.root_demand();
        let previous_demand = std::mem::replace(&mut self.expression_demand, demand);
        let previous = self.runtime.consumer.replace(target);
        self.runtime.budget.push_cell(target);
        if self.runtime.budget.profiling {
            self.runtime.run_usage.formula_executions =
                self.runtime.run_usage.formula_executions.saturating_add(1);
        }
        let result = (|| {
            self.runtime.budget.text(prepared.source().len())?;
            let value = self.prepared_expression(target.sheet, target.address, prepared)?;
            let value = self.result_scalar(target.sheet, target.address, value)?;
            Ok(value)
        })();
        self.runtime.budget.pop_cell();
        self.runtime.consumer = previous;
        self.expression_demand = previous_demand;
        result
    }
    /// Execute a supplied bytecode plan after checking its extension identity and target.
    pub(super) fn bound_at(
        &mut self,
        target: EvaluationTarget,
        formula: &bound::FormulaPlan,
    ) -> EvalResult<CellValue> {
        if formula.extension_identity().is_some_and(|identity| {
            self.extensions
                .is_none_or(|registry| identity != &registry.identity())
        }) {
            return Err(EvaluationFailure::StalePlan);
        }
        if self.workbook.sheet(target.sheet).is_none() || target.address.validate().is_err() {
            return Ok(Error(Ref));
        }
        let demand = self.root_demand();
        let previous_demand = std::mem::replace(&mut self.expression_demand, demand);
        let previous = self.runtime.consumer.replace(target);
        self.runtime.budget.push_cell(target);
        if self.runtime.budget.profiling {
            self.runtime.run_usage.formula_executions =
                self.runtime.run_usage.formula_executions.saturating_add(1);
        }
        let result = self
            .bytecode_expr(target.sheet, target.address, formula.bytecode())
            .and_then(|v| self.result_scalar(target.sheet, target.address, v));
        self.runtime.budget.pop_cell();
        self.runtime.consumer = previous;
        self.expression_demand = previous_demand;
        result
    }
    /// Read an encountered range member without memoizing authored nonblank literals.
    /// Observation ports and spill-owned content retain the ordinary cell protocol.
    pub(super) fn range_cell(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> EvalResult<CellValue> {
        if self.extensions.is_some()
            || self.dependency_sink.is_some()
            || self.cell_reader.is_some()
            || self.cell_observer.is_some()
            || self.builtin_invocation_observer.is_some()
            || self.execution_observer.is_some()
            || self.runtime.preparation.failed()
            || self
                .runtime
                .preparation
                .roles
                .contains_key(&(sheet, address))
            || address.validate().is_err()
        {
            return self.cell(sheet, address);
        }
        let Some(worksheet) = self.workbook.sheet(sheet) else {
            return self.cell(sheet, address);
        };
        let cell =
            run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs).cell(sheet, address);
        let Some(cell) = cell.filter(|cell| {
            cell.formula.is_none()
                && cell.value != Blank
                && worksheet.spill_owner(address).is_none()
        }) else {
            return self.cell(sheet, address);
        };
        let target = EvaluationTarget { sheet, address };
        self.runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(target);
                failure
            })?;
        self.read_lazy(|| DependencyRead::Cell(target));
        self.runtime.budget.enter_depth().map_err(|mut failure| {
            failure.set_target(target);
            failure
        })?;
        // Retain the literal's frame for quota attribution, but not its value: the
        // immutable source already owns it. Memo quotas count only retained entries.
        self.runtime.budget.push_cell(target);
        let value = if let Some(text) = cell.value.text_ref() {
            self.runtime
                .budget
                .text(text.storage_bytes())
                .map(|()| cell.value.clone())
        } else {
            Ok(match &cell.value {
                Number(value) => numeric_outcome::coerce_numeric_outcome(*value),
                value => value.clone(),
            })
        };
        self.runtime.budget.pop_cell();
        self.runtime.budget.leave_depth();
        value
    }
    /// Read or evaluate a cell, recording dependencies, spill effects, and completion provenance.
    pub(super) fn cell(&mut self, sheet: usize, address: CellAddress) -> EvalResult<CellValue> {
        let consumer = self.runtime.consumer;
        let mut provenance = CellProvenance::Evaluated;
        let mut completion_candidate = false;
        // A dependency formula has its own worksheet consumer. Its caller's collection
        // demand must never change the dependency's value or memoized result.
        let demand = self.root_demand();
        let previous_demand = std::mem::replace(&mut self.expression_demand, demand);
        let mut result =
            self.cell_value(sheet, address, &mut provenance, &mut completion_candidate);
        self.expression_demand = previous_demand;
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && matches!(result, Ok(Blank))
            && let Some(consumer) = consumer
        {
            // Every consumer observes absence, including consumers of a memoized
            // blank. A dirty earlier producer can later grow into this coordinate.
            self.read_lazy(|| DependencyRead::SparseMembership {
                sheet,
                first: CellAddress { row: 0, col: 0 },
                last: address,
            });
            let tracked = (|| {
                if !self.settling_spills() {
                    return Ok(());
                }
                // A dependency's blank can participate in an ancestor's future
                // footprint as well as its immediate caller's footprint.
                let runtime = &mut *self.runtime;
                for owner in runtime
                    .visiting
                    .iter()
                    .copied()
                    .filter(|owner| *owner != (consumer.sheet, consumer.address))
                    .chain(std::iter::once((consumer.sheet, consumer.address)))
                {
                    let reads = runtime.blank_reads.entry(owner).or_default();
                    if !reads.contains(&(sheet, address)) {
                        runtime
                            .budget
                            .check_allocation(reads.len().saturating_add(1))?;
                        runtime.budget.charge(WorkKind::Materialization, 1)?;
                        reads.insert((sheet, address));
                    }
                }
                Ok::<_, EvaluationFailure>(())
            })();
            if let Err(failure) = tracked {
                result = Err(failure);
            }
        }
        if result.is_ok()
            && provenance != CellProvenance::SessionMemo
            && (provenance == CellProvenance::ExternalCache
                || completion_candidate
                || (self.array_result_policy == ArrayResultPolicy::DynamicSpill
                    && self
                        .runtime
                        .preparation
                        .roles
                        .contains_key(&(sheet, address))))
            && let Some(reads) = &mut self.overlay_reads
        {
            reads.completed.insert((sheet, address));
            if provenance == CellProvenance::Evaluated {
                reads.fresh_results.insert((sheet, address));
            }
        }
        {
            let notification = self.complete_cell(CellCompletion {
                consumer,
                target: EvaluationTarget { sheet, address },
                provenance,
                result: result.as_ref(),
            });
            // Notification remains required on abort, but cannot mask its original cause.
            if result.is_ok() {
                notification?;
            }
        }
        result
    }
    fn cell_value(
        &mut self,
        sheet: usize,
        address: CellAddress,
        provenance: &mut CellProvenance,
        completion_candidate: &mut bool,
    ) -> EvalResult<CellValue> {
        if address.validate().is_err() || self.workbook.sheet(sheet).is_none() {
            return Ok(Error(Ref));
        }
        self.runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(EvaluationTarget { sheet, address });
                failure
            })?;
        self.read_lazy(|| DependencyRead::Cell(EvaluationTarget { sheet, address }));
        self.resolve_spills_in(Area {
            kind: AreaKind::Cell,
            sheet,
            r0: address.row,
            r1: address.row,
            c0: address.col,
            c1: address.col,
        })?;

        if (self.overlay_reads.is_some() || self.dependency_sink.is_some())
            && let Some(owner) = self
                .runtime
                .preparation
                .roles
                .get(&(sheet, address))
                .and_then(|role| role.owner(address))
        {
            record_read(
                &mut self.overlay_reads,
                &mut self.dependency_sink,
                ReadEvent {
                    consumer: self.runtime.consumer,
                    read: DependencyRead::Cell(EvaluationTarget {
                        sheet,
                        address: owner,
                    }),
                },
            );
        }
        let runtime = &mut *self.runtime;
        if let Some(value) = runtime.cache.get(&(sheet, address)) {
            *provenance = CellProvenance::SessionMemo;
            if runtime.budget.profiling {
                runtime.run_usage.session_memo_hits =
                    runtime.run_usage.session_memo_hits.saturating_add(1);
                runtime.budget.usage.cache_hits = runtime.budget.usage.cache_hits.saturating_add(1);
            }
            return copy_cached_value(
                &mut runtime.budget,
                EvaluationTarget { sheet, address },
                value,
            );
        }
        if self.inputs.is_none()
            && let Some(reader) = &mut self.cell_reader
            && let Some(value) = reader
                .cached_cell(EvaluationTarget { sheet, address })
                .inspect_err(|_| *provenance = CellProvenance::ExternalCache)?
        {
            *provenance = CellProvenance::ExternalCache;
            if self.runtime.budget.profiling {
                self.runtime.run_usage.external_cache_hits =
                    self.runtime.run_usage.external_cache_hits.saturating_add(1);
                self.runtime.budget.usage.cache_hits =
                    self.runtime.budget.usage.cache_hits.saturating_add(1);
            }
            return copy_cached_value(
                &mut self.runtime.budget,
                EvaluationTarget { sheet, address },
                value.as_ref(),
            );
        }
        let cached = self.overlay_cache.and_then(|cache| {
            cache
                .try_borrow()
                .ok()?
                .get((sheet, address))
                .map(|entry| std::sync::Arc::clone(&entry.value))
        });
        if let Some(value) = cached {
            *provenance = CellProvenance::ExternalCache;
            if self.runtime.budget.profiling {
                self.runtime.run_usage.external_cache_hits =
                    self.runtime.run_usage.external_cache_hits.saturating_add(1);
                self.runtime.budget.usage.cache_hits =
                    self.runtime.budget.usage.cache_hits.saturating_add(1);
            }
            return copy_cached_value(
                &mut self.runtime.budget,
                EvaluationTarget { sheet, address },
                value.as_ref(),
            );
        }
        let cell =
            run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs).cell(sheet, address);
        *completion_candidate = cell.is_some_and(|cell| cell.formula.is_some());
        // Scalar literals and absent cells cannot recurse. Dynamic spill execution
        // also uses visiting for ownership and footprint tracking, so retain its path.
        let track_visiting = self.array_result_policy == ArrayResultPolicy::DynamicSpill
            || cell.is_some_and(|cell| cell.formula.is_some());
        if track_visiting && !self.runtime.visiting.insert((sheet, address)) {
            if self.array_result_policy == ArrayResultPolicy::DynamicSpill
                && self.spill_candidate(sheet, address)
            {
                return Err(self.spill_unsupported(
                    sheet,
                    address,
                    "dynamic execution encountered a circular unpublished footprint read",
                ));
            }
            return Ok(Error(Ref));
        }
        if let Err(mut failure) = self.runtime.budget.enter_depth() {
            if track_visiting {
                self.runtime.visiting.remove(&(sheet, address));
            }
            failure.set_target(EvaluationTarget { sheet, address });
            return Err(failure);
        }
        self.runtime
            .budget
            .push_cell(EvaluationTarget { sheet, address });
        let previous = self
            .runtime
            .consumer
            .replace(EvaluationTarget { sheet, address });
        let result = (|| {
            let value = match cell {
                None => Ok(Blank),
                Some(cell) => match &cell.formula {
                    None => {
                        if let Some(text) = cell.value.text_ref() {
                            self.runtime.budget.text(text.storage_bytes())?;
                        }
                        Ok(match &cell.value {
                            // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                            Number(value) => numeric_outcome::coerce_numeric_outcome(*value),
                            value => value.clone(),
                        })
                    }
                    Some(formula) => {
                        self.runtime.budget.text(formula.len())?;
                        if self.runtime.budget.profiling {
                            self.runtime.run_usage.formula_executions =
                                self.runtime.run_usage.formula_executions.saturating_add(1);
                        }
                        let compiled;
                        let prepared = match self
                            .plan
                            .and_then(|plan| plan.formula(EvaluationTarget { sheet, address }))
                        {
                            Some(prepared) => prepared.as_ref(),
                            None => {
                                compiled = self.prepare_source(formula);
                                compiled.as_ref()
                            }
                        };
                        let value = self.prepared_expression(sheet, address, prepared)?;
                        self.worksheet_result(sheet, address, value)
                    }
                },
            }?;
            self.admit_cell(sheet, address, &value)?;
            Ok(value)
        })();
        self.runtime.budget.pop_cell();
        self.runtime.consumer = previous;
        self.runtime.budget.leave_depth();
        if track_visiting {
            self.runtime.visiting.remove(&(sheet, address));
        }
        self.runtime.blank_reads.remove(&(sheet, address));
        self.runtime.spill_read_ranges.remove(&(sheet, address));
        result
    }
}

/// Forward a read while allowing the memo and sink fields to be borrowed independently.
fn record_read(
    reads: &mut Option<overlay::dependencies::Reads>,
    sink: &mut Option<&mut dyn DependencyReadSink>,
    event: ReadEvent,
) {
    match (reads, sink) {
        (Some(reads), Some(sink)) => {
            crate::DependencyReadSink::read(reads, event.clone());
            sink.read(event);
        }
        (Some(reads), None) => crate::DependencyReadSink::read(reads, event),
        (None, Some(sink)) => sink.read(event),
        _ => {}
    }
}

/// Copy a cached value, charging text in its own cell frame rather than its caller's.
#[inline]
fn copy_cached_value(
    budget: &mut BudgetMeter,
    target: EvaluationTarget,
    value: &CellValue,
) -> EvalResult<CellValue> {
    // Nontext copies have no charge or rejection point requiring a cell frame.
    if let Some(text) = value.text_ref() {
        budget.push_cell(target);
        let result = budget.text(text.storage_bytes());
        budget.pop_cell();
        result?;
    }
    Ok(value.clone())
}

#[cfg(test)]
mod literal_cycle_tests {
    use super::*;

    #[test]
    fn overlay_replaces_a_formula_cycle_with_a_literal() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        let a = "A1".parse().unwrap();
        let b = "B1".parse().unwrap();
        sheet.set_formula(a, "B1").unwrap();
        sheet.set_formula(b, "A1").unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        assert_eq!(session.cell(0, a).unwrap(), Error(Ref));
        assert!(session.runtime.visiting.is_empty());
        let inputs = run_overlay::RunInputs::new(
            &workbook,
            [(0, b, Number(7.))],
            1,
            std::sync::Arc::new(()),
        )
        .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.inputs = Some(&inputs);
        assert_eq!(session.cell(0, a).unwrap(), Number(7.));
        assert_eq!(session.runtime.run_usage.formula_executions, 1);
        assert_eq!(session.runtime.cache.get(&(0, b)), Some(&Number(7.)));
        assert!(session.runtime.visiting.is_empty());
    }

    #[test]
    fn dynamic_spill_keeps_blank_footprint_detection_and_child_discovery() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        let a = "A1".parse().unwrap();
        let b = "B1".parse().unwrap();
        sheet.set_formula(a, "IF(B1=0,{1,2},{3,4})").unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        assert!(
            matches!(session.cell(0, a), Err(EvaluationFailure::UnsupportedCapability { detail, .. }) if detail == "array result reads its own unpublished footprint")
        );
        assert!(!session.runtime.cache.contains_key(&(0, a)));
        assert!(session.runtime.visiting.is_empty());

        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(a, "{1,2}")
            .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        assert_eq!(session.cell(0, b).unwrap(), Number(2.));
        assert_eq!(session.runtime.cache.get(&(0, a)), Some(&Number(1.)));
        assert_eq!(
            session
                .runtime
                .preparation
                .roles
                .get(&(0, b))
                .and_then(|role| role.owner(b)),
            Some(a)
        );
        assert!(session.runtime.visiting.is_empty());
    }
}
