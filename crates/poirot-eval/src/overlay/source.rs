//! Shared sparse source, parsing policies, and lazy compilation for both evaluation paths.
use super::*;
use std::borrow::Borrow;

/// Runtime provider owner, excluded from serialized binding identity.
pub(super) enum Registry<'a> {
    /// Caller-managed immutable registry.
    Borrowed(&'a crate::extensions::ExtensionRegistry),
    /// Providers shared with worker-owned workspaces.
    Shared(Arc<crate::extensions::ExtensionRegistry>),
}
impl Registry<'_> {
    /// Read implementations independently of handle ownership.
    pub fn get(&self) -> &crate::extensions::ExtensionRegistry {
        match self {
            Self::Borrowed(registry) => registry,
            Self::Shared(registry) => registry,
        }
    }
}

/// Baseline handle plus authored diff and compilation; contains no evaluated results.
pub(super) struct Source<'a, W> {
    /// Immutable borrowed or shared baseline handle.
    pub workbook: W,
    /// Sparse authored overrides and removals.
    pub inputs: crate::run_overlay::RunInputs,
    /// Lazily interned programs independent of evaluated values.
    pub compiler: RefCell<FormulaCompiler>,
    /// Parsing and calculation rules shared by both paths.
    pub policy: OverlayPolicy,
    /// Exact parser and provider binding identity.
    pub preparation: crate::EvaluationPolicy,
    /// Runtime implementations, never serialized into a snapshot.
    pub extensions: Option<Registry<'a>>,
}
impl<'a, W: Borrow<Workbook>> Source<'a, W> {
    /// Validate preparation policy and allocate an empty source diff and compiler.
    pub fn new(
        workbook: W,
        policy: OverlayPolicy,
        extensions: Option<Registry<'a>>,
    ) -> Result<Self, OverlayError> {
        policy.materialization.validate(policy.numeric)?;
        let registry = extensions.as_ref().map(Registry::get).map_or_else(
            crate::RegistryIdentity::default,
            crate::RegistryIdentity::with_extensions,
        );
        Ok(Self {
            workbook,
            inputs: crate::run_overlay::RunInputs::empty(),
            compiler: RefCell::new(FormulaCompiler::new()),
            preparation: crate::EvaluationPolicy {
                parse: policy.parse,
                registry,
            },
            policy,
            extensions,
        })
    }
    /// Borrow the baseline without exposing its handle representation.
    pub fn workbook(&self) -> &Workbook {
        self.workbook.borrow()
    }
    /// Resolve a local sheet-qualified cell against baseline sheet metadata.
    pub fn target(&self, reference: &str) -> Result<EvaluationTarget, ModelError> {
        let target = target::LocalCellTarget::parse(reference)?;
        let sheet = self
            .workbook()
            .try_sheet_index_by_name(&target.sheet)
            .map_err(|e| ModelError(e.to_string()))?
            .ok_or_else(|| ModelError("target worksheet is absent".into()))?;
        Ok(EvaluationTarget {
            sheet,
            address: target.address,
        })
    }
    /// Read the effective authored source before evaluating formulas.
    pub fn source_cell(&self, target: EvaluationTarget) -> Option<&Cell> {
        self.effective().cell(target.sheet, target.address)
    }
    /// Combine baseline and sparse edits for the common execution read port.
    pub fn effective(&self) -> crate::run_overlay::EffectiveWorkbook<'_> {
        crate::run_overlay::EffectiveWorkbook::new(self.workbook(), Some(&self.inputs))
    }
    /// Reject invalid addresses and absent sheets before mutating source.
    pub fn validate_target(&self, target: EvaluationTarget) -> Result<(), ModelError> {
        target.address.validate()?;
        if self.workbook().sheet(target.sheet).is_none() {
            return Err(ModelError("worksheet index out of bounds".into()));
        }
        Ok(())
    }
    fn remove_override(&mut self, target: EvaluationTarget) {
        if let Some(cells) = self.inputs.sheets.get_mut(&target.sheet) {
            cells.remove(&target.address);
            if cells.cells().is_empty() {
                self.inputs.sheets.remove(&target.sheet);
            }
        }
    }
    /// Replace source, dropping redundant overrides; report whether invalidation is needed.
    pub fn write_cell(&mut self, target: EvaluationTarget, cell: Cell) -> Result<bool, ModelError> {
        self.validate_target(target)?;
        if self
            .source_cell(target)
            .is_some_and(|old| same_cell(old, &cell))
        {
            return Ok(false);
        }
        self.inputs.deleted.remove(&(target.sheet, target.address));
        let baseline = self
            .workbook()
            .sheet(target.sheet)
            .and_then(|s| s.source_cell(target.address));
        if baseline.is_some_and(|old| same_cell(old, &cell)) {
            self.remove_override(target);
        } else {
            self.inputs
                .sheets
                .entry(target.sheet)
                .or_default()
                .set(target.address, cell)?;
        }
        Ok(true)
    }
    /// Hide baseline source or remove an override; report effective source changes.
    pub fn remove_cell(&mut self, target: EvaluationTarget) -> Result<bool, ModelError> {
        self.validate_target(target)?;
        if self.source_cell(target).is_none() {
            return Ok(false);
        }
        self.remove_override(target);
        if self
            .workbook()
            .sheet(target.sheet)
            .is_some_and(|s| s.source_cell(target.address).is_some())
        {
            self.inputs.deleted.insert((target.sheet, target.address));
        }
        Ok(true)
    }
    /// Count authored overrides and removal markers rather than edit history.
    pub fn edited_cells(&self) -> usize {
        self.inputs
            .sheets
            .values()
            .map(|s| s.cells().len())
            .sum::<usize>()
            + self.inputs.deleted.len()
    }
    /// Project compiler counters without exposing retained program internals.
    pub fn compiled_usage(&mut self) -> CompilationUsage {
        let compiler = self.compiler.get_mut();
        let usage = compiler.usage();
        CompilationUsage {
            parse_attempts: usage.parse_attempts,
            bind_attempts: usage.bind_attempts,
            reused_formulas: usage.reused_formulas,
            template_hits: compiler.template_hits(),
            unique_programs: compiler.unique_programs(),
        }
    }
    /// Prepare effective cell and name sources without executing formulas.
    pub fn compile_all(&mut self) -> CompilationUsage {
        let workbook = self.workbook.borrow();
        let effective = crate::run_overlay::EffectiveWorkbook::new(workbook, Some(&self.inputs));
        let targets = source_targets(workbook, Some(&self.inputs));
        let compiler = self.compiler.get_mut();
        let mut prepare = |source: &str| match self.extensions.as_ref().map(Registry::get) {
            Some(registry) => {
                compiler.compile_formula_with_extensions(source, &self.preparation, registry);
            }
            None => {
                compiler.compile_formula(source, &self.preparation);
            }
        };
        for (sheet, address) in targets {
            if let Some(source) = effective
                .cell(sheet, address)
                .and_then(|cell| cell.formula.as_deref())
            {
                prepare(source);
            }
        }
        for name in &workbook.named_ranges {
            prepare(&name.formula);
        }
        self.compiled_usage()
    }
    /// Export owned source mutations, independent of any evaluated result cache.
    pub fn source_changes(&self) -> Vec<WorkbookChange> {
        let mut changes = self
            .inputs
            .deleted
            .iter()
            .map(|&(sheet, address)| WorkbookChange::RemoveCell { sheet, address })
            .collect::<Vec<_>>();
        for (&sheet, cells) in &self.inputs.sheets {
            for (&address, cell) in cells.cells() {
                append_cell_changes(
                    &mut changes,
                    (sheet, address),
                    self.workbook()
                        .sheet(sheet)
                        .and_then(|s| s.source_cell(address)),
                    cell,
                );
            }
        }
        changes
    }
    /// Attach shared execution to source, policies, providers, and a fixed context.
    pub fn session<'e>(
        &'e self,
        context: &'e EvaluationContext,
        meter: crate::budget::BudgetMeter,
    ) -> crate::Session<'e> {
        let mut session = crate::Session::new(self.workbook(), context.inputs(), meter);
        session.inputs = Some(&self.inputs);
        session.lazy_compiler = Some(&self.compiler);
        session.preparation_policy = Some(&self.preparation);
        session.extensions = self.extensions.as_ref().map(Registry::get);
        session.currency = context.formatting.currency();
        session.number_locale = &context.number_locale;
        session.text_policy = self.policy.text;
        session.numeric_policy = self.policy.numeric;
        session.builtin_policy = self.policy.builtin;
        session.array_result_policy = self.policy.arrays;
        session.memo_limits = Some(context.retention);
        session.lookup_prefixes.enable();
        session
    }
}

/// Shared public source operations; each workspace supplies its own invalidation hook.
macro_rules! source_methods {
    () => {
        /// Resolve a local sheet-qualified output or input address.
        pub fn target(&self, reference: &str) -> Result<EvaluationTarget, ModelError> {
            self.source.target(reference)
        }
        /// Borrow the immutable baseline before authored edits.
        pub fn workbook(&self) -> &Workbook {
            self.source.workbook()
        }
        /// Read authored content after applying sparse overrides and removals.
        pub fn source_cell(&self, target: EvaluationTarget) -> Option<&Cell> {
            self.source.source_cell(target)
        }
        /// Return parsing, computation, and materialization policies.
        pub fn policy(&self) -> &OverlayPolicy {
            &self.source.policy
        }
        /// Count authored overrides and removals.
        pub fn edited_cells(&self) -> usize {
            self.source.edited_cells()
        }
        /// Report cumulative compilation and bytecode reuse.
        pub fn compiled_usage(&mut self) -> CompilationUsage {
            self.source.compiled_usage()
        }
        /// Eagerly compile effective cell/name formulas; execution normally compiles lazily.
        pub fn compile_all(&mut self) -> CompilationUsage {
            self.source.compile_all()
        }
        /// Replace a cell with a literal, preserving its style and invalidating results.
        pub fn set_value(
            &mut self,
            target: EvaluationTarget,
            value: impl Into<CellValue>,
        ) -> Result<(), ModelError> {
            let style = self
                .source_cell(target)
                .map(|c| c.style.clone())
                .unwrap_or_default();
            self.write_cell(
                target,
                Cell {
                    value: value.into(),
                    formula: None,
                    style,
                },
            )
        }
        /// Replace a formula and clear its saved value, preserving its style.
        pub fn set_formula(
            &mut self,
            target: EvaluationTarget,
            formula: impl Into<String>,
        ) -> Result<(), ModelError> {
            let mut cell = Cell::with_formula(formula, CellValue::Blank)?;
            cell.style = self
                .source_cell(target)
                .map(|c| c.style.clone())
                .unwrap_or_default();
            self.write_cell(target, cell)
        }
        /// Replace authored style and invalidate potentially affected results.
        pub fn set_style(
            &mut self,
            target: EvaluationTarget,
            style: CellStyle,
        ) -> Result<(), ModelError> {
            let mut cell = self.source_cell(target).cloned().unwrap_or_default();
            cell.style = style;
            self.write_cell(target, cell)
        }
        /// Change a formula's saved workbook value without trusting it as an evaluation result.
        pub fn set_cached_value(
            &mut self,
            target: EvaluationTarget,
            value: impl Into<CellValue>,
        ) -> Result<(), ModelError> {
            let mut cell = self
                .source_cell(target)
                .cloned()
                .ok_or_else(|| ModelError("cache target is absent".into()))?;
            if cell.formula.is_none() {
                return Err(ModelError("cache target has no formula".into()));
            }
            cell.value = value.into();
            self.write_cell(target, cell)
        }
        /// Remove effective authored content and invalidate results.
        pub fn remove_cell(&mut self, target: EvaluationTarget) -> Result<(), ModelError> {
            if self.source.remove_cell(target)? {
                self.source_changed(target);
            }
            Ok(())
        }
        /// Replace policies; reusable programs survive changes outside parser preparation.
        pub fn set_policy(&mut self, policy: OverlayPolicy) -> Result<(), OverlayError> {
            policy.materialization.validate(policy.numeric)?;
            if self.source.policy != policy {
                if self.source.policy.parse != policy.parse {
                    self.source.compiler = RefCell::new(FormulaCompiler::new());
                }
                self.source.preparation.parse = policy.parse;
                let previous = std::mem::replace(&mut self.source.policy, policy);
                self.policy_changed(&previous);
            }
            Ok(())
        }
        fn write_cell(&mut self, target: EvaluationTarget, cell: Cell) -> Result<(), ModelError> {
            if self.source.write_cell(target, cell)? {
                self.source_changed(target);
            }
            Ok(())
        }
    };
}
/// Shared source-operation expansion for direct and persistent workspaces.
pub(super) use source_methods;

/// Enumerate authored cells, excluding stale saved spill-only children and tombstones.
pub(super) fn source_targets(
    workbook: &Workbook,
    inputs: Option<&crate::run_overlay::RunInputs>,
) -> std::collections::BTreeSet<Key> {
    let mut targets = std::collections::BTreeSet::new();
    for (sheet, source) in workbook.sheets().iter().enumerate() {
        targets.extend(
            source
                .cells()
                .keys()
                .filter(|at| {
                    source.source_cell(**at).is_some()
                        && !inputs.is_some_and(|i| i.deleted.contains(&(sheet, **at)))
                })
                .map(|at| (sheet, *at)),
        );
    }
    if let Some(inputs) = inputs {
        for (&sheet, cells) in &inputs.sheets {
            targets.extend(cells.cells().keys().map(|at| (sheet, *at)));
        }
    }
    targets
}
