//! Prepared execution, recursive cell reads, memoization, and completion lifecycle.
use super::*;

/// Borrowed authored content and incoming ownership eligibility under the current array policy.
#[derive(Clone, Copy)]
pub(super) struct ResolvedSource<'a> {
    /// Authored content stays immutable while the execution publishes derived values.
    pub(super) cell: Option<&'a poirot_model::Cell>,
    /// Dynamic spill resolution is relevant to blank coordinates and saved children.
    pub(super) accepts_spill: bool,
}

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
        self.source_preparation().with_compiler(prepare)
    }

    /// Borrow only source-compilation services for discovery and reached formulas.
    pub(super) fn source_preparation(&self) -> source_preparation::SourcePreparation<'a> {
        source_preparation::SourcePreparation {
            extensions: self.extensions,
            policy: self.preparation_policy,
            compiler: self.lazy_compiler,
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
        Self::with_configuration(
            workbook,
            configuration::SessionConfiguration::from_inputs(context),
            budget,
        )
    }
    /// Initialize owned execution storage with borrowed configuration settings.
    pub(super) fn with_configuration(
        workbook: &'a Workbook,
        configuration: configuration::SessionConfiguration<'a>,
        budget: BudgetMeter,
    ) -> Self {
        Self::with_runtime(
            workbook,
            configuration,
            // SAFETY: A single fixed-size Runtime has a representable allocation layout.
            runtime::Storage::Owned(Box::new(runtime::Runtime::new(budget))),
        )
    }
    /// Attach workbook ports to owned or persistently borrowed execution storage.
    pub(super) fn with_runtime(
        workbook: &'a Workbook,
        configuration: configuration::SessionConfiguration<'a>,
        runtime: runtime::Storage<'a>,
    ) -> Self {
        Self {
            runtime,
            workbook,
            inputs: None,
            context: configuration.context,
            currency: configuration.currency,
            number_locale: configuration.number_locale,
            regex: configuration.regex,

            builtin_policy: configuration.builtin_policy,
            array_result_policy: configuration.array_result_policy,
            producer_policy: configuration.producer_policy,
            expression_demand: Default::default(),
            spill_candidates: None,
            text_policy: configuration.text_policy,
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
    /// Record spill geometry without claiming every coordinate's value was consumed.
    pub(super) fn read_spill_membership(
        &mut self,
        sheet: usize,
        first: CellAddress,
        last: CellAddress,
    ) {
        if self.overlay_reads.is_none() && self.dependency_sink.is_none() && !self.settling_spills()
        {
            return;
        }
        let read = DependencyRead::SparseMembership { sheet, first, last };
        self.record_spill_effect(&read);
        if let Some(reads) = &mut self.overlay_reads {
            reads.spill_membership(
                self.runtime.consumer,
                poirot_model::query::RangeMembership { sheet, first, last },
            );
        }
        if let Some(sink) = &mut self.dependency_sink {
            sink.read(ReadEvent {
                consumer: self.runtime.consumer,
                read,
            });
        }
    }
    /// Attribute a consumed read to the active cell and forward it to installed sinks.
    pub(super) fn read(&mut self, read: DependencyRead) {
        self.record_spill_effect(&read);
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let DependencyRead::SparseMembership { sheet, last, .. } = &read
            && let Some(reads) = &mut self.overlay_reads
        {
            // Sparse consumers may observe an empty interval without reading
            // individual blank coordinates. Retain their ownership-query guard
            // even when an indexed lookup supplied the values from another caller.
            reads.spill_membership(
                self.runtime.consumer,
                poirot_model::query::RangeMembership {
                    sheet: *sheet,
                    first: CellAddress { row: 0, col: 0 },
                    last: *last,
                },
            );
        }
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
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && prepared.spill_shape().is_some()
            && let Some(reads) = &mut self.overlay_reads
        {
            reads.mark_spill_sensitive((sheet, at));
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
            let value = self.finish_expression(target.sheet, target.address, value)?;
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
            .and_then(|v| self.finish_expression(target.sheet, target.address, v));
        self.runtime.budget.pop_cell();
        self.runtime.consumer = previous;
        self.expression_demand = previous_demand;
        result
    }
    /// Resolve immutable source and saved ownership once for an ordinary or range read.
    pub(super) fn resolve_source(
        &self,
        sheet: usize,
        address: CellAddress,
    ) -> Option<ResolvedSource<'a>> {
        if address.validate().is_err() {
            return None;
        }
        let worksheet = self.workbook.sheet(sheet)?;
        let cell =
            run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs).cell(sheet, address);
        Some(ResolvedSource {
            cell,
            accepts_spill: self.array_result_policy == ArrayResultPolicy::DynamicSpill
                && (cell.is_none_or(|cell| cell.formula.is_none() && cell.value == Blank)
                    || worksheet.spill_owner(address).is_some()),
        })
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
            || self.runtime.preparation.has_admitted_role((sheet, address))
        {
            return self.cell(sheet, address);
        }
        let source = self.resolve_source(sheet, address);
        let Some(cell) = source.and_then(|source| {
            source.cell.filter(|cell| {
                cell.formula.is_none()
                    && cell.value != Blank
                    && !source.accepts_spill
                    && (self.array_result_policy == ArrayResultPolicy::DynamicSpill
                        || self
                            .workbook
                            .sheet(sheet)
                            .is_some_and(|worksheet| worksheet.spill_owner(address).is_none()))
            })
        }) else {
            return self.cell_from_source(sheet, address, source);
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
        let source = self.resolve_source(sheet, address);
        self.cell_from_source(sheet, address, source)
    }
    /// Preserve the ordinary read lifecycle around an already resolved immutable source.
    fn cell_from_source(
        &mut self,
        sheet: usize,
        address: CellAddress,
        source: Option<ResolvedSource<'a>>,
    ) -> EvalResult<CellValue> {
        let attempt = CellAttempt {
            target: EvaluationTarget { sheet, address },
            consumer: self.runtime.consumer,
            provenance: CellProvenance::Evaluated,
            completion_candidate: false,
        };
        // A dependency formula has its own worksheet demand. Restore its caller's
        // demand before completion callbacks, on both success and operational failure.
        let demand = self.root_demand();
        let previous_demand = std::mem::replace(&mut self.expression_demand, demand);
        let read = attempt.read(self, source);
        self.expression_demand = previous_demand;
        let settling_spills = self.settling_spills();
        let runtime = &mut *self.runtime;
        read.complete(CompletionContext {
            array_result_policy: self.array_result_policy,
            settling_spills,
            preparation: &mut runtime.preparation,
            budget: &mut runtime.budget,
            visiting: &runtime.visiting,
            reads: &mut self.overlay_reads,
            consumer: runtime.consumer,
            observer: &mut self.cell_observer,
        })
    }
    /// Settle only dirty immediate causes, keeping successful validation local until publication.
    pub(super) fn revalidate_cached(
        &mut self,
        key: (usize, CellAddress),
    ) -> EvalResult<Option<CellValue>> {
        let Some(cache) = self.overlay_cache else {
            return Ok(None);
        };
        let mut evidence = {
            let cache = cache
                .try_borrow()
                .map_err(|_| EvaluationFailure::StalePlan)?;
            let Some(entry) = cache.retained(key) else {
                return Ok(None);
            };
            let Some(causes) = entry
                .dependencies
                .ordered_causes(&entry.validity, &mut self.runtime.budget)?
            else {
                return Ok(None);
            };
            (std::sync::Arc::clone(&entry.value), causes)
        };
        // Validating evidence must not replace the original consumed dependency trace
        // with the narrower set of dirty causes, or leak these reads into a caller.
        let previous = self.runtime.consumer.take();
        let unchanged = (|| {
            loop {
                let prior = {
                    let cache = cache
                        .try_borrow()
                        .map_err(|_| EvaluationFailure::StalePlan)?;
                    let entry = cache.retained(key).ok_or(EvaluationFailure::StalePlan)?;
                    let Some((cause, expected_version)) =
                        evidence.1.next(&entry.validity, &mut self.runtime.budget)?
                    else {
                        return Ok(true);
                    };
                    cache.retained(cause).map(|entry| {
                        (
                            cause,
                            expected_version,
                            entry.version,
                            std::sync::Arc::clone(&entry.value),
                            entry.validity.is_clean(),
                        )
                    })
                };
                let Some((cause, expected_version, version, prior, clean)) = prior else {
                    return Ok(false);
                };
                if version != expected_version {
                    return Ok(false);
                }
                self.runtime.budget.charge(WorkKind::Comparison, 1)?;
                if clean {
                    continue;
                }
                // Stop at the first changed value: normal execution decides which
                // old lazy dependencies are still relevant, and in what order.
                let unchanged = match self.runtime.cache.get(&cause) {
                    Some(value) => crate::same_result_identity(&prior, value),
                    None => crate::same_result_identity(&prior, &self.cell(cause.0, cause.1)?),
                };
                if !unchanged {
                    return Ok(false);
                }
            }
        })();
        self.runtime.consumer = previous;
        if !unchanged? {
            return Ok(None);
        }
        let value = copy_cached_value(
            &mut self.runtime.budget,
            EvaluationTarget {
                sheet: key.0,
                address: key.1,
            },
            &evidence.0,
        )?;
        if let Some(reads) = &mut self.overlay_reads {
            reads.validated.insert(key);
        }
        Ok(Some(value))
    }
}

/// One cell read's attribution and publication eligibility, retained even on failure.
struct CellAttempt {
    target: EvaluationTarget,
    consumer: Option<EvaluationTarget>,
    provenance: CellProvenance,
    completion_candidate: bool,
}

/// A finished attempt and its value or operational failure, ready for completion delivery.
struct CellRead {
    attempt: CellAttempt,
    result: EvalResult<CellValue>,
}

/// Completion-only capabilities, excluding workbook, compiler, caches and extensions.
struct CompletionContext<'s, 'observer> {
    array_result_policy: ArrayResultPolicy,
    settling_spills: bool,
    preparation: &'s mut spill_publication::Preparation,
    budget: &'s mut BudgetMeter,
    visiting: &'s CellSet,
    reads: &'s mut Option<overlay::dependencies::Reads>,
    consumer: Option<EvaluationTarget>,
    observer: &'s mut Option<&'observer mut dyn CellObserver>,
}

/// Reuse either produces a value or requires execution, possibly with retained evidence.
enum CellReuse {
    Value(CellValue),
    Execute { needs_validation: bool },
}

impl CellAttempt {
    /// Run admission, reuse and execution while preserving failure attribution.
    fn read<'a>(
        mut self,
        session: &mut Session<'a>,
        source: Option<ResolvedSource<'a>>,
    ) -> CellRead {
        let result = self.resolve_and_read(session, source);
        CellRead {
            attempt: self,
            result,
        }
    }

    /// Admit an effective source read before consulting caches or executing dependencies.
    fn resolve_and_read<'a>(
        &mut self,
        session: &mut Session<'a>,
        source: Option<ResolvedSource<'a>>,
    ) -> EvalResult<CellValue> {
        let Some(source) = source else {
            return Ok(Error(Ref));
        };
        session
            .runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(self.target);
                failure
            })?;
        session.read_lazy(|| DependencyRead::Cell(self.target));
        match self.reuse(session, source)? {
            CellReuse::Value(value) => Ok(value),
            CellReuse::Execute { needs_validation } => {
                self.execute(session, source, needs_validation)
            }
        }
    }

    /// Preserve cache precedence and resolve incoming spill ownership before ordinary reuse.
    fn reuse<'a>(
        &mut self,
        session: &mut Session<'a>,
        source: ResolvedSource<'a>,
    ) -> EvalResult<CellReuse> {
        let EvaluationTarget { sheet, address } = self.target;
        let incoming = source.accepts_spill;
        // A clean unowned blank certificate already includes its empty-owner
        // query guard. Owned coordinates still restore a whole footprint for
        // coherent export and spill-reference readiness. Active arbitration and
        // locally admitted values retain their ordinary protocol.
        if incoming
            && !session.settling_spills()
            && !session.runtime.cache.contains_key(&(sheet, address))
            && let Some(value) = session.overlay_cache.and_then(|cache| {
                let cache = cache.try_borrow().ok()?;
                let entry = cache.get((sheet, address))?;
                if entry.dependencies.spill.owner(address).is_some() {
                    return None;
                }
                Some(std::sync::Arc::clone(&entry.value))
            })
        {
            self.completion_candidate = true;
            self.external_hit(session);
            return copy_cached_value(&mut session.runtime.budget, self.target, value.as_ref())
                .map(CellReuse::Value);
        }
        self.resolve_spills(session, source)?;
        self.completion_candidate = incoming;
        let runtime = &mut *session.runtime;
        if let Some(value) = runtime.cache.get(&(sheet, address)) {
            self.provenance = CellProvenance::SessionMemo;
            if runtime.budget.profiling {
                runtime.run_usage.session_memo_hits =
                    runtime.run_usage.session_memo_hits.saturating_add(1);
                runtime.budget.usage.cache_hits = runtime.budget.usage.cache_hits.saturating_add(1);
            }
            return copy_cached_value(&mut runtime.budget, self.target, value)
                .map(CellReuse::Value);
        }
        if session.inputs.is_none()
            && let Some(reader) = &mut session.cell_reader
            && let Some(value) = reader
                .cached_cell(self.target)
                .inspect_err(|_| self.provenance = CellProvenance::ExternalCache)?
        {
            self.external_hit(session);
            return copy_cached_value(&mut session.runtime.budget, self.target, value.as_ref())
                .map(CellReuse::Value);
        }
        let (cached, needs_validation) = session
            .overlay_cache
            .and_then(|cache| {
                let cache = cache.try_borrow().ok()?;
                let entry = cache.retained((sheet, address))?;
                Some((
                    entry
                        .validity
                        .is_clean()
                        .then(|| std::sync::Arc::clone(&entry.value)),
                    matches!(
                        entry.validity,
                        overlay::dependencies::Validity::Cause { .. }
                            | overlay::dependencies::Validity::Causes(_)
                    ),
                ))
            })
            .unwrap_or_default();
        if let Some(value) = cached {
            self.external_hit(session);
            return copy_cached_value(&mut session.runtime.budget, self.target, value.as_ref())
                .map(CellReuse::Value);
        }
        Ok(CellReuse::Execute { needs_validation })
    }

    /// Retain cache provenance before copying a value, including a copy-budget failure.
    fn external_hit(&mut self, session: &mut Session<'_>) {
        self.provenance = CellProvenance::ExternalCache;
        if session.runtime.budget.profiling {
            session.runtime.run_usage.external_cache_hits = session
                .runtime
                .run_usage
                .external_cache_hits
                .saturating_add(1);
            session.runtime.budget.usage.cache_hits =
                session.runtime.budget.usage.cache_hits.saturating_add(1);
        }
    }

    /// Attribute ownership queries to the coordinate, then restore the consuming cell.
    fn resolve_spills<'a>(
        &self,
        session: &mut Session<'a>,
        source: ResolvedSource<'a>,
    ) -> EvalResult<()> {
        let EvaluationTarget { sheet, address } = self.target;
        let incoming = source.accepts_spill;
        // Empty coordinates own their spatial-query dependency, including empty
        // query results. Their callers depend on the coordinate's actual value.
        let previous = if incoming {
            session.runtime.consumer.replace(self.target)
        } else {
            session.runtime.consumer
        };
        let resolution = session.resolve_cell_spills(sheet, address, source);
        if incoming && resolution.is_ok() {
            session.read_spill_membership(sheet, CellAddress { row: 0, col: 0 }, address);
            if let Some(owner) = session
                .runtime
                .preparation
                .admitted_role((sheet, address))
                .owner(address)
                && let Some(reads) = &mut session.overlay_reads
            {
                crate::DependencyReadSink::read(
                    reads,
                    ReadEvent {
                        consumer: session.runtime.consumer,
                        read: DependencyRead::Cell(EvaluationTarget {
                            sheet,
                            address: owner,
                        }),
                    },
                );
            }
        }
        session.runtime.consumer = previous;
        resolution?;
        // Preserve the public read observer's owner notification for its caller;
        // retained incremental edges are attached to the coordinate node above.
        if incoming
            && let Some(sink) = &mut session.dependency_sink
            && let Some(owner) = session
                .runtime
                .preparation
                .admitted_role((sheet, address))
                .owner(address)
        {
            sink.read(ReadEvent {
                consumer: previous,
                read: DependencyRead::Cell(EvaluationTarget {
                    sheet,
                    address: owner,
                }),
            });
        }
        Ok(())
    }

    /// Enter the recursive cell frame and restore its caller before returning any outcome.
    fn execute<'a>(
        &mut self,
        session: &mut Session<'a>,
        source: ResolvedSource<'a>,
        needs_validation: bool,
    ) -> EvalResult<CellValue> {
        let EvaluationTarget { sheet, address } = self.target;
        // Source and inputs are immutable for this execution. Spill publication
        // updates runtime ownership and values, never the authored source cell.
        let cell = source.cell;
        self.completion_candidate |= cell.is_some_and(|cell| cell.formula.is_some());
        // Scalar literals and absent cells cannot recurse. Dynamic spill execution
        // also uses visiting for ownership and footprint tracking, so retain its path.
        let track_visiting = session.array_result_policy == ArrayResultPolicy::DynamicSpill
            || cell.is_some_and(|cell| cell.formula.is_some());
        if track_visiting && session.runtime.visiting.contains(&(sheet, address)) {
            if session.array_result_policy == ArrayResultPolicy::DynamicSpill
                && session.spill_candidate(sheet, address)
            {
                return Err(session.spill_unsupported(
                    sheet,
                    address,
                    "dynamic execution encountered a circular unpublished footprint read",
                ));
            }
            return Ok(Error(Ref));
        }
        if track_visiting {
            session
                .runtime
                .visiting
                .try_reserve(1)
                .map_err(|_| cell_allocation_failure(self.target))?;
            // SAFETY: One new coordinate slot was reserved above; coordinate hashing cannot panic.
            session.runtime.visiting.insert((sheet, address));
        }
        if let Err(mut failure) = session.runtime.budget.enter_depth() {
            if track_visiting {
                session.runtime.visiting.remove(&(sheet, address));
            }
            failure.set_target(self.target);
            return Err(failure);
        }
        if let Err(failure) = session.runtime.budget.try_push_cell(self.target) {
            // SAFETY: enter_depth succeeded above, and this path has not left that depth.
            session.runtime.budget.leave_depth();
            if track_visiting {
                session.runtime.visiting.remove(&(sheet, address));
            }
            return Err(failure);
        }
        let previous = session.runtime.consumer.replace(self.target);
        // All fallible work happens inside this call. No `?` can bypass restoration.
        let result = self.execute_source(session, source.cell, needs_validation);
        session.runtime.budget.pop_cell();
        session.runtime.consumer = previous;
        // SAFETY: enter_depth succeeded above; execute_source balances only its nested calls.
        session.runtime.budget.leave_depth();
        if track_visiting {
            session.runtime.visiting.remove(&(sheet, address));
        }
        session
            .runtime
            .preparation
            .producer_reads
            .forget((sheet, address));
        result
    }

    /// Validate retained evidence or compute authored content, then admit the result locally.
    fn execute_source(
        &mut self,
        session: &mut Session<'_>,
        cell: Option<&poirot_model::Cell>,
        needs_validation: bool,
    ) -> EvalResult<CellValue> {
        let EvaluationTarget { sheet, address } = self.target;
        if needs_validation && let Some(value) = session.revalidate_cached((sheet, address))? {
            let value = session.admit_cell(
                sheet,
                address,
                spill_publication::ComputedCell::scalar(value),
            )?;
            self.external_hit(session);
            if let Some(reads) = &mut session.overlay_reads {
                reads
                    .fresh_results
                    .try_reserve(1)
                    .map_err(|_| cell_allocation_failure(self.target))?;
                // SAFETY: One result slot was reserved above; numeric coordinate hashing cannot panic.
                reads.fresh_results.insert((sheet, address));
            }
            return Ok(value);
        }
        let value = match cell {
            None => Ok(spill_publication::ComputedCell::scalar(Blank)),
            Some(cell) => match &cell.formula {
                None => {
                    if let Some(text) = cell.value.text_ref() {
                        session.runtime.budget.text(text.storage_bytes())?;
                    }
                    Ok(spill_publication::ComputedCell::scalar(match &cell.value {
                        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                        Number(value) => numeric_outcome::coerce_numeric_outcome(*value),
                        value => value.clone(),
                    }))
                }
                Some(formula) => {
                    session.runtime.budget.text(formula.len())?;
                    if session.runtime.budget.profiling {
                        session.runtime.run_usage.formula_executions = session
                            .runtime
                            .run_usage
                            .formula_executions
                            .saturating_add(1);
                    }
                    let compiled;
                    let prepared = match session.plan.and_then(|plan| plan.formula(self.target)) {
                        Some(prepared) => prepared.as_ref(),
                        None => {
                            compiled = session.prepare_source(formula);
                            compiled.as_ref()
                        }
                    };
                    let value = session.prepared_expression(sheet, address, prepared)?;
                    session.publish_worksheet_result(sheet, address, value)
                }
            },
        }?;
        session.admit_cell(sheet, address, value)
    }
}

impl CellRead {
    /// Record completion eligibility and notify observers without masking the original failure.
    fn complete(self, context: CompletionContext<'_, '_>) -> EvalResult<CellValue> {
        let CellRead {
            attempt:
                CellAttempt {
                    target: EvaluationTarget { sheet, address },
                    consumer,
                    provenance,
                    completion_candidate,
                },
            mut result,
        } = self;
        if context.array_result_policy == ArrayResultPolicy::DynamicSpill
            && matches!(result, Ok(Blank))
            && let Some(consumer) = consumer
        {
            // Private ownership arbitration tracks blank observations, including
            // memo hits: a producer can later grow into an ancestor's footprint.
            let tracked = (|| {
                if !context.settling_spills {
                    return Ok(());
                }
                // A dependency's blank can participate in an ancestor's future
                // footprint as well as its immediate caller's footprint.
                for owner in context
                    .visiting
                    .iter()
                    .copied()
                    .filter(|owner| *owner != (consumer.sheet, consumer.address))
                    .chain(std::iter::once((consumer.sheet, consumer.address)))
                {
                    context.preparation.producer_reads.record_blank(
                        owner,
                        (sheet, address),
                        context.budget,
                    )?;
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
                || (context.array_result_policy == ArrayResultPolicy::DynamicSpill
                    && context.preparation.has_admitted_role((sheet, address))))
            && let Some(reads) = context.reads
        {
            let recorded = (|| {
                let target = EvaluationTarget { sheet, address };
                reads
                    .completed
                    .try_reserve(1)
                    .map_err(|_| cell_allocation_failure(target))?;
                if provenance == CellProvenance::Evaluated {
                    reads
                        .fresh_results
                        .try_reserve(1)
                        .map_err(|_| cell_allocation_failure(target))?;
                    // SAFETY: One result slot was reserved above; coordinate hashing cannot panic.
                    reads.fresh_results.insert((sheet, address));
                }
                // SAFETY: The completed set reserved one slot above with no intervening insertion.
                reads.completed.insert((sheet, address));
                Ok::<_, EvaluationFailure>(())
            })();
            if let Err(failure) = recorded {
                result = Err(failure);
            }
        }
        {
            let notification = context.preparation.complete_cell(
                context.budget,
                context.consumer,
                context.observer,
                CellCompletion {
                    consumer,
                    target: EvaluationTarget { sheet, address },
                    provenance,
                    result: result.as_ref(),
                },
            );
            // Notification remains required on abort, but cannot mask its original cause.
            if result.is_ok() {
                notification?;
            }
        }
        result
    }
}

/// Attribute exhausted bookkeeping storage to the cell whose attempt needed it.
fn cell_allocation_failure(target: EvaluationTarget) -> EvaluationFailure {
    EvaluationFailure::Resource {
        kind: crate::ResourceFailureKind::Allocation,
        capability: CapabilityId::FunctionAdapter,
        source: Some(target),
        detail: "cell execution bookkeeping allocation failed".into(),
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

    /// Reject every notification to test completion without attaching an execution session.
    struct RejectingObserver {
        observed_failures: Vec<bool>,
    }
    impl CellObserver for RejectingObserver {
        fn completed(&mut self, completion: CellCompletion<'_>) -> EvalResult<()> {
            self.observed_failures.push(completion.result.is_err());
            Err(EvaluationFailure::StalePlan)
        }
    }

    #[test]
    fn completion_ports_notify_aborts_and_preserve_the_original_failure() {
        let target = EvaluationTarget {
            sheet: 0,
            address: "A1".parse().unwrap(),
        };
        let original = EvaluationFailure::ResourceLimit {
            capability: CapabilityId::FormulaParsing,
            source: Some(target),
            detail: "original failure".into(),
        };
        let mut preparation = spill_publication::Preparation::default();
        let mut budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
        let visiting = CellSet::default();
        let mut reads = None;
        let mut observer = RejectingObserver {
            observed_failures: Vec::new(),
        };
        for (result, expected) in [
            (Err(original.clone()), original),
            (Ok(Number(6.)), EvaluationFailure::StalePlan),
        ] {
            let read = CellRead {
                attempt: CellAttempt {
                    target,
                    consumer: None,
                    provenance: CellProvenance::ExternalCache,
                    completion_candidate: true,
                },
                result,
            };
            let mut port: Option<&mut dyn CellObserver> = Some(&mut observer);
            assert_eq!(
                read.complete(CompletionContext {
                    array_result_policy: ArrayResultPolicy::ScalarIntersection,
                    settling_spills: false,
                    preparation: &mut preparation,
                    budget: &mut budget,
                    visiting: &visiting,
                    reads: &mut reads,
                    consumer: None,
                    observer: &mut port,
                })
                .unwrap_err(),
                expected
            );
        }
        assert_eq!(observer.observed_failures, [true, false]);
    }

    #[test]
    fn recursive_depth_failure_restores_caller_attribution_and_expression_demand() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        let output = "A1".parse().unwrap();
        let dependency = "B1".parse().unwrap();
        let literal = "C1".parse().unwrap();
        sheet.set_formula(output, "B1+1").unwrap();
        sheet.set_value(dependency, 7.).unwrap();
        sheet.set_value(literal, 11.).unwrap();
        let budget = BudgetMeter::new(EvaluationBudgets {
            depth: 1,
            ..Default::default()
        })
        .unwrap();
        let mut session = Session::new(&workbook, EvaluationInputs::default(), budget);
        let caller = EvaluationTarget {
            sheet: 0,
            address: "D1".parse().unwrap(),
        };
        session.runtime.consumer = Some(caller);
        session.expression_demand = expression_execution::ExpressionDemand::Array;
        assert!(session.cell(0, output).is_err());
        assert_eq!(session.runtime.consumer, Some(caller));
        assert!(session.expression_demand == expression_execution::ExpressionDemand::Array);
        assert!(session.runtime.visiting.is_empty());
        // This succeeds under the same depth-one meter only if the failed frame was left.
        assert_eq!(session.cell(0, literal).unwrap(), Number(11.));
        assert_eq!(session.runtime.consumer, Some(caller));
        assert!(session.expression_demand == expression_execution::ExpressionDemand::Array);
    }

    #[test]
    fn source_resolution_keeps_formula_blanks_distinct_from_incoming_spill_coordinates() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        sheet.set_formula("A1".parse().unwrap(), "{1,2}").unwrap();
        sheet
            .set_spill_cached_value(
                "B1".parse().unwrap(),
                "A1".parse().unwrap(),
                "{1,2}",
                Number(99.),
            )
            .unwrap();
        sheet.set_formula("D1".parse().unwrap(), "\"\"").unwrap();
        sheet.set_value("E1".parse().unwrap(), Blank).unwrap();
        sheet.set_value("F1".parse().unwrap(), 0.).unwrap();
        let inputs = run_overlay::RunInputs::new(
            &workbook,
            [(0, "B1".parse().unwrap(), Number(7.))],
            1,
            std::sync::Arc::new(()),
        )
        .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.inputs = Some(&inputs);
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        for (address, accepts) in [
            ("A1", false),
            ("B1", true),
            ("D1", false),
            ("E1", true),
            ("F1", false),
            ("G1", true),
        ] {
            assert_eq!(
                session
                    .resolve_source(0, address.parse().unwrap())
                    .unwrap()
                    .accepts_spill,
                accepts,
                "{address}"
            );
        }
        assert_eq!(
            session
                .resolve_source(0, "B1".parse().unwrap())
                .unwrap()
                .cell
                .unwrap()
                .value,
            Number(7.)
        );
        assert!(session.resolve_source(1, "A1".parse().unwrap()).is_none());
        assert!(
            session
                .resolve_source(
                    0,
                    CellAddress {
                        row: MAX_ROWS,
                        col: 0
                    }
                )
                .is_none()
        );
    }

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
            session.runtime.preparation.admitted_role((0, b)).owner(b),
            Some(a)
        );
        assert!(session.runtime.visiting.is_empty());
    }
}
