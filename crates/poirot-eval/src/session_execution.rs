//! Prepared execution, recursive cell reads, memoization, and completion lifecycle.
use super::*;

impl<'a> Session<'a> {
    /// Compile a reached source using the overlay's shared compiler when available.
    pub(super) fn prepare_source(&self, source: &str) -> std::sync::Arc<PreparedFormula> {
        let default = self
            .extensions
            .map_or_else(EvaluationPolicy::default, |registry| {
                EvaluationPolicy::with_extensions(
                    poirot_formula::FormulaParsePolicy::default(),
                    registry,
                )
            });
        let policy = self.preparation_policy.unwrap_or(&default);
        let mut local = FormulaCompiler::new();
        let mut shared = self
            .lazy_compiler
            .and_then(|compiler| compiler.try_borrow_mut().ok());
        let compiler = shared.as_deref_mut().unwrap_or(&mut local);
        match self.extensions {
            Some(registry) => compiler.compile_formula_with_extensions(source, policy, registry),
            None => compiler.compile_formula(source, policy),
        }
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
        Self {
            bytecode_stacks: Vec::new(),
            lookup_prefixes: Default::default(),
            workbook,
            inputs: None,
            context,
            currency: None,
            number_locale: &poirot_coercionv2::NumberLocaleV2::EN_US,
            numeric_policy: NumericOutcomePolicy::LegacyFinite,
            builtin_policy: BuiltinSemanticsPolicy::default(),
            array_result_policy: ArrayResultPolicy::default(),
            expression_demand: Default::default(),
            spill_owners: Default::default(),
            pending_spills: Default::default(),
            blank_reads: Default::default(),
            spill_read_ranges: Default::default(),
            text_policy: poirot_text::TextSemanticsPolicy {
                source: poirot_text::SourceVersion::PoirotF6c572d,
            },
            cache: Default::default(),
            memo_limits: None,
            memo_usage: run::MemoRetentionUsage::default(),
            visiting: Default::default(),
            names: HashSet::new(),
            numeric_date_effects: Default::default(),
            extensions: None,
            extension_trace: extensions::ExtensionTrace::default(),
            random_streams: Default::default(),
            budget,
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
            observation_state: execution_observation::ObservationState::default(),
            consumer: None,
            run_usage: RunUsage::default(),
        }
    }
    /// Construct dependency-only payloads only when a tracker or external sink needs them.
    pub(super) fn read_lazy(&mut self, read: impl FnOnce() -> DependencyRead) {
        if self.overlay_reads.is_some() || self.dependency_sink.is_some() {
            self.read(read());
        }
    }
    /// Attribute a consumed read to the active cell and forward it to installed sinks.
    pub(super) fn read(&mut self, read: DependencyRead) {
        let event = ReadEvent {
            consumer: self.consumer,
            read,
        };
        match (&mut self.overlay_reads, &mut self.dependency_sink) {
            (Some(reads), Some(sink)) => {
                crate::DependencyReadSink::read(reads, event.clone());
                sink.read(event);
            }
            (Some(reads), None) => crate::DependencyReadSink::read(reads, event),
            (None, Some(sink)) => sink.read(event),
            _ => {}
        }
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
        self.run_usage.compiled_lookups += 1;
        match prepared {
            PreparedFormula::Ready(formula) => self.bytecode_expr(sheet, at, formula.bytecode()),
            PreparedFormula::CompilationFailure { error, .. } => {
                Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FormulaCompilation,
                    source: Some(EvaluationTarget { sheet, address: at }),
                    detail: error.to_string(),
                })
            }
            PreparedFormula::ParseFailure { .. } => Ok(err(VALUE_ERROR)),
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
        let previous = self.consumer.replace(target);
        self.budget.push_cell(target);
        self.run_usage.formula_executions += 1;
        let result = (|| {
            self.budget.text(prepared.source().len())?;
            let value = self.prepared_expression(target.sheet, target.address, prepared)?;
            let value = self.result_scalar(target.sheet, target.address, value)?;
            Ok(value)
        })();
        self.budget.pop_cell();
        self.consumer = previous;
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
        let previous = self.consumer.replace(target);
        self.budget.push_cell(target);
        self.run_usage.formula_executions += 1;
        let result = self
            .bytecode_expr(target.sheet, target.address, formula.bytecode())
            .and_then(|v| self.result_scalar(target.sheet, target.address, v));
        self.budget.pop_cell();
        self.consumer = previous;
        result
    }
    /// Read or evaluate a cell, recording dependencies, spill effects, and completion provenance.
    pub(super) fn cell(&mut self, sheet: usize, address: CellAddress) -> EvalResult<CellValue> {
        let consumer = self.consumer;
        let mut provenance = CellProvenance::Evaluated;
        let mut completion_candidate = false;
        // A dependency formula has its own worksheet consumer. Its caller's collection
        // demand must never change the dependency's value or memoized result.
        let previous_demand = std::mem::replace(
            &mut self.expression_demand,
            expression_execution::ExpressionDemand::Scalar,
        );
        let mut result =
            self.cell_value(sheet, address, &mut provenance, &mut completion_candidate);
        self.expression_demand = previous_demand;
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && matches!(result, Ok(Blank))
            && let Some(consumer) = consumer
        {
            let tracked = (|| {
                // A dependency's blank can participate in an ancestor's future
                // footprint as well as its immediate caller's footprint.
                for owner in self
                    .visiting
                    .iter()
                    .copied()
                    .filter(|owner| *owner != (consumer.sheet, consumer.address))
                    .chain(std::iter::once((consumer.sheet, consumer.address)))
                {
                    let reads = self.blank_reads.entry(owner).or_default();
                    if !reads.contains(&(sheet, address)) {
                        self.budget
                            .check_allocation(reads.len().saturating_add(1))?;
                        self.budget.charge(WorkKind::Materialization, 1)?;
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
                    && self.spill_owners.contains_key(&(sheet, address))))
            && let Some(reads) = &mut self.overlay_reads
        {
            reads.completed.insert((sheet, address));
            if provenance == CellProvenance::Evaluated {
                reads.evaluated.insert((sheet, address));
            }
        }
        if let Some(observer) = &mut self.cell_observer {
            let notification = observer.completed(CellCompletion {
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
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill && self.cell_reader.is_some()
        {
            return Err(self.spill_unsupported(
                sheet,
                address,
                "dynamic spill ownership is incompatible with an external scalar cell cache",
            ));
        }
        self.budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(EvaluationTarget { sheet, address });
                failure
            })?;
        self.read(DependencyRead::Cell(EvaluationTarget { sheet, address }));
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill {
            self.restore_cached_spill(sheet, address)?;
        }
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && !self.cache.contains_key(&(sheet, address))
            && run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs)
                .cell(sheet, address)
                .is_some_and(|cell| cell.formula.is_some())
        {
            let previous = self.consumer.replace(EvaluationTarget { sheet, address });
            let arbitration = self.arbitrate_spill(sheet, address);
            self.consumer = previous;
            arbitration?;
        }
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && !self.cache.contains_key(&(sheet, address))
            && run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs)
                .cell(sheet, address)
                .is_none_or(|cell| cell.formula.is_none() && cell.value == Blank)
        {
            self.discover_spill(sheet, address)?;
        }
        if let Some(owner) = self.spill_owners.get(&(sheet, address)).copied() {
            self.read(DependencyRead::Cell(EvaluationTarget {
                sheet,
                address: owner,
            }));
        }
        if let Some(value) = self.cache.get(&(sheet, address)) {
            *provenance = CellProvenance::SessionMemo;
            self.run_usage.session_memo_hits += 1;
            self.budget.usage.cache_hits += 1;
            // Copying a memoized dependency belongs to that target's own frame,
            // not to the caller: previous bulk workload cannot alter its cell allowance.
            self.budget.push_cell(EvaluationTarget { sheet, address });
            let result = (|| {
                if let Some(text) = value.text_ref() {
                    self.budget.text(text.storage_bytes())?;
                }
                Ok(value.clone())
            })();
            self.budget.pop_cell();
            return result;
        }
        if self.inputs.is_none()
            && let Some(reader) = &mut self.cell_reader
            && let Some(value) = reader
                .cached_cell(EvaluationTarget { sheet, address })
                .inspect_err(|_| *provenance = CellProvenance::ExternalCache)?
        {
            *provenance = CellProvenance::ExternalCache;
            self.run_usage.external_cache_hits += 1;
            self.budget.usage.cache_hits += 1;
            self.budget.push_cell(EvaluationTarget { sheet, address });
            let result = (|| {
                if let Some(text) = value.as_ref().text_ref() {
                    self.budget.text(text.storage_bytes())?;
                }
                Ok(value.as_ref().clone())
            })();
            self.budget.pop_cell();
            return result;
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
            self.run_usage.external_cache_hits += 1;
            self.budget.usage.cache_hits += 1;
            self.budget.push_cell(EvaluationTarget { sheet, address });
            let result = (|| {
                self.budget.text(value.text_storage_bytes())?;
                Ok(value.as_ref().clone())
            })();
            self.budget.pop_cell();
            return result;
        }
        let cell =
            run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs).cell(sheet, address);
        *completion_candidate = cell.is_some_and(|cell| cell.formula.is_some());
        // Scalar literals and absent cells cannot recurse. Dynamic spill execution
        // also uses visiting for ownership and footprint tracking, so retain its path.
        let track_visiting = self.array_result_policy == ArrayResultPolicy::DynamicSpill
            || cell.is_some_and(|cell| cell.formula.is_some());
        if track_visiting && !self.visiting.insert((sheet, address)) {
            if self.array_result_policy == ArrayResultPolicy::DynamicSpill {
                return Err(self.spill_unsupported(
                    sheet,
                    address,
                    "dynamic execution encountered a circular unpublished footprint read",
                ));
            }
            return Ok(Error(Ref));
        }
        if let Err(mut failure) = self.budget.enter_depth() {
            if track_visiting {
                self.visiting.remove(&(sheet, address));
            }
            failure.set_target(EvaluationTarget { sheet, address });
            return Err(failure);
        }
        self.budget.push_cell(EvaluationTarget { sheet, address });
        let previous = self.consumer.replace(EvaluationTarget { sheet, address });
        let result = (|| {
            let value = match cell {
                None => Ok(Blank),
                Some(cell) => match &cell.formula {
                    None => {
                        if let Some(text) = cell.value.text_ref() {
                            self.budget.text(text.storage_bytes())?;
                        }
                        Ok(match &cell.value {
                            // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                            Number(value) => self.numeric_policy.coerce_numeric_stage(
                                numeric_outcome::NumericConstruction::CellNumericInput,
                                *value,
                            ),
                            value => value.clone(),
                        })
                    }
                    Some(formula) => {
                        self.budget.text(formula.len())?;
                        self.run_usage.formula_executions += 1;
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
                        self.result_scalar(sheet, address, value)
                    }
                },
            }?;
            self.admit_cell(sheet, address, &value)?;
            Ok(value)
        })();
        self.budget.pop_cell();
        self.consumer = previous;
        self.budget.leave_depth();
        if track_visiting {
            self.visiting.remove(&(sheet, address));
        }
        self.blank_reads.remove(&(sheet, address));
        self.spill_read_ranges.remove(&(sheet, address));
        result
    }
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
        sheet.set_formula(a, "B1", Blank).unwrap();
        sheet.set_formula(b, "A1", Blank).unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        assert_eq!(session.cell(0, a).unwrap(), Error(Ref));
        assert!(session.visiting.is_empty());
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
        assert_eq!(session.run_usage.formula_executions, 1);
        assert_eq!(session.cache.get(&(0, b)), Some(&Number(7.)));
        assert!(session.visiting.is_empty());
    }

    #[test]
    fn dynamic_spill_keeps_blank_footprint_detection_and_child_discovery() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        let a = "A1".parse().unwrap();
        let b = "B1".parse().unwrap();
        sheet.set_formula(a, "IF(B1=0,{1,2},{3,4})", Blank).unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        assert!(
            matches!(session.cell(0, a), Err(EvaluationFailure::UnsupportedCapability { detail, .. }) if detail == "array result reads its own unpublished footprint")
        );
        assert!(!session.cache.contains_key(&(0, a)));
        assert!(session.visiting.is_empty());

        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(a, "{1,2}", Blank)
            .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        assert_eq!(session.cell(0, b).unwrap(), Number(2.));
        assert_eq!(session.cache.get(&(0, a)), Some(&Number(1.)));
        assert_eq!(session.spill_owners.get(&(0, b)), Some(&a));
        assert!(session.visiting.is_empty());
    }
}
