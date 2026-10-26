//! Shared sparse source, parsing policies, and lazy compilation for both evaluation paths.
use super::*;
use crate::eval::run_overlay::CellEdit;
use std::borrow::Borrow;

/// Runtime provider owner, excluded from serialized binding identity.
pub(super) enum Registry<'a> {
    /// Caller-managed immutable registry.
    Borrowed(&'a crate::eval::extensions::ExtensionRegistry),
    /// Providers shared with worker-owned workspaces.
    Shared(Arc<crate::eval::extensions::ExtensionRegistry>),
}
impl Registry<'_> {
    /// Read implementations independently of handle ownership.
    pub fn get(&self) -> &crate::eval::extensions::ExtensionRegistry {
        match self {
            Self::Borrowed(registry) => registry,
            Self::Shared(registry) => registry,
        }
    }
}

/// Whether an authored operation replaces content or preserves it while changing style.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKind {
    Content,
    Style,
}

/// Whether value-only dependency cutoffs are sound for an authored mutation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceChange {
    /// Literal content changed without changing formula or style metadata.
    Value,
    /// Formula, style, or stored spill ownership may have changed.
    Metadata,
}
/// Baseline handle plus authored diff and compilation; contains no evaluated results.
pub(super) struct Source<'a, W> {
    /// Immutable borrowed or shared baseline handle.
    pub workbook: W,
    /// Sparse authored overrides and removals.
    pub inputs: crate::eval::run_overlay::RunInputs,
    /// Lazily interned programs independent of evaluated values.
    pub compiler: RefCell<FormulaCompiler>,
    /// Typed producer classification shared across unchanged evaluations.
    pub spill_candidates: RefCell<crate::eval::spill_publication::CandidateIndex>,
    /// Parsing and calculation rules shared by both paths.
    pub policy: EvaluationPolicy,
    /// Exact parser and provider binding identity.
    pub preparation: crate::eval::PreparationPolicy,
    /// Runtime implementations, never serialized into a snapshot.
    pub extensions: Option<Registry<'a>>,
}
impl<'a, W: Borrow<Workbook>> Source<'a, W> {
    /// Validate preparation policy and allocate an empty source diff and compiler.
    pub fn new(
        workbook: W,
        policy: EvaluationPolicy,
        extensions: Option<Registry<'a>>,
    ) -> Result<Self, OverlayError> {
        let registry = extensions.as_ref().map(Registry::get).map_or_else(
            crate::eval::RegistryIdentity::default,
            crate::eval::RegistryIdentity::with_extensions,
        );
        Ok(Self {
            workbook,
            inputs: crate::eval::run_overlay::RunInputs::empty(),
            compiler: RefCell::new(FormulaCompiler::new()),
            spill_candidates: Default::default(),
            preparation: crate::eval::PreparationPolicy {
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
    pub fn effective(&self) -> crate::eval::run_overlay::EffectiveWorkbook<'_> {
        crate::eval::run_overlay::EffectiveWorkbook::new(self.workbook(), Some(&self.inputs))
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
    /// Classify literal writes before changing effective source or saved ownership.
    pub fn change_kind(
        &self,
        target: EvaluationTarget,
        new: Option<&Cell>,
        kind: EditKind,
    ) -> SourceChange {
        let old = self.source_cell(target);
        let saved_spill = self.workbook().sheet(target.sheet).is_some_and(|sheet| {
            sheet.spill_owner(target.address).is_some()
                || sheet.spill_extent(target.address).is_some()
        });
        let style_unchanged = old.map(|cell| &cell.style) == new.map(|cell| &cell.style)
            || (old.is_none() && new.is_some_and(|cell| cell.style == CellStyle::default()));
        let occupancy_unchanged = old
            .is_none_or(|cell| cell.formula.is_none() && cell.value == CellValue::Blank)
            == new.is_none_or(|cell| cell.formula.is_none() && cell.value == CellValue::Blank);
        if kind == EditKind::Content
            && occupancy_unchanged
            && !saved_spill
            && style_unchanged
            && old.is_none_or(|cell| cell.formula.is_none())
            && new.is_none_or(|cell| cell.formula.is_none())
        {
            SourceChange::Value
        } else {
            SourceChange::Metadata
        }
    }
    /// Replace source, dropping redundant overrides; report whether invalidation is needed.
    pub fn write_cell(
        &mut self,
        target: EvaluationTarget,
        cell: Cell,
        kind: EditKind,
    ) -> Result<bool, ModelError> {
        self.validate_target(target)?;
        let prior_edit = self.inputs.edit(target.sheet, target.address);
        let prior_style_only = prior_edit.is_some_and(CellEdit::is_style_only);
        let saved_ownership = self.workbook().sheet(target.sheet).is_some_and(|sheet| {
            sheet.spill_owner(target.address).is_some()
                || sheet.spill_extent(target.address).is_some()
                || (sheet
                    .source_cell(target.address)
                    .is_some_and(|cell| cell.formula.is_some())
                    && sheet
                        .spill_children()
                        .any(|(_, owner)| owner == target.address))
        });
        let new_content_intent = kind == EditKind::Content
            && (prior_style_only
                || (saved_ownership && prior_edit.and_then(CellEdit::cell).is_none()));
        let style_only = kind == EditKind::Style && prior_edit.is_none_or(CellEdit::is_style_only);
        if self
            .source_cell(target)
            .is_some_and(|old| same_cell(old, &cell))
            && !new_content_intent
        {
            return Ok(false);
        }
        let old_formula = self
            .source_cell(target)
            .and_then(|old| old.formula.as_deref());
        if old_formula != cell.formula.as_deref() {
            self.spill_candidates.get_mut().remove(&target.sheet);
        }
        let baseline = self
            .workbook()
            .sheet(target.sheet)
            .and_then(|s| s.source_cell(target.address));
        if baseline.is_some_and(|old| same_cell(old, &cell)) && !(!style_only && saved_ownership) {
            self.remove_override(target);
        } else {
            self.inputs
                .sheets
                .entry(target.sheet)
                .or_default()
                .set_cell(target.address, cell, style_only)?;
        }
        Ok(true)
    }
    /// Hide baseline source or remove an override; report effective source changes.
    pub fn remove_cell(&mut self, target: EvaluationTarget) -> Result<bool, ModelError> {
        self.validate_target(target)?;
        if self.source_cell(target).is_none() {
            return Ok(false);
        }
        if self
            .source_cell(target)
            .is_some_and(|cell| cell.formula.is_some())
        {
            self.spill_candidates.get_mut().remove(&target.sheet);
        }
        self.remove_override(target);
        if self
            .workbook()
            .sheet(target.sheet)
            .is_some_and(|s| s.source_cell(target.address).is_some())
        {
            self.inputs
                .sheets
                .entry(target.sheet)
                .or_default()
                .set(target.address, CellEdit::Delete)?;
        }
        Ok(true)
    }
    /// Restore baseline inputs, invalidating source proofs only where formulas change.
    pub fn discard_inputs(&mut self) {
        let workbook = self.workbook.borrow();
        let candidates = self.spill_candidates.get_mut();
        for (&sheet, overrides) in &self.inputs.sheets {
            let baseline = workbook.sheet(sheet);
            if overrides.cells().iter().any(|(&address, edit)| {
                edit.cell().and_then(|cell| cell.formula.as_deref())
                    != baseline
                        .and_then(|sheet| sheet.source_cell(address))
                        .and_then(|cell| cell.formula.as_deref())
            }) {
                candidates.remove(&sheet);
            }
        }
        self.inputs = crate::eval::run_overlay::RunInputs::empty();
    }
    /// Count authored overrides and removal markers rather than edit history.
    pub fn edited_cells(&self) -> usize {
        self.inputs
            .sheets
            .values()
            .map(|s| s.cells().len())
            .sum::<usize>()
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
        let effective =
            crate::eval::run_overlay::EffectiveWorkbook::new(workbook, Some(&self.inputs));
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
        // Keep deletion-before-write export order without retaining a second edit index.
        let mut changes = self
            .inputs
            .edits()
            .filter_map(|(sheet, address, edit)| {
                matches!(edit, CellEdit::Delete)
                    .then_some(WorkbookChange::RemoveCell { sheet, address })
            })
            .collect::<Vec<_>>();
        for (sheet, address, edit) in self.inputs.edits() {
            match edit {
                CellEdit::Delete => {}
                CellEdit::StyleOnly(cell) => changes.push(WorkbookChange::SetStyle {
                    sheet,
                    address,
                    style: cell.style.clone(),
                }),
                CellEdit::Replace(cell) => append_cell_changes(
                    &mut changes,
                    (sheet, address),
                    self.workbook()
                        .sheet(sheet)
                        .and_then(|s| s.source_cell(address)),
                    cell,
                ),
            }
        }
        changes
    }
    /// Attach shared execution to source, policies, providers, and a fixed context.
    pub fn session<'e>(
        &'e self,
        context: &'e EvaluationContext,
        meter: crate::eval::budget::BudgetMeter,
    ) -> crate::eval::Session<'e> {
        self.session_with_runtime(
            context,
            crate::eval::runtime::Storage::Owned(Box::new(crate::eval::runtime::Runtime::new(
                meter,
            ))),
        )
    }
    /// Reborrow persistent execution storage without moving its stacks or caches.
    pub fn session_with_runtime<'e>(
        &'e self,
        context: &'e EvaluationContext,
        runtime: crate::eval::runtime::Storage<'e>,
    ) -> crate::eval::Session<'e> {
        let mut session = crate::eval::Session::with_runtime(
            self.workbook(),
            crate::eval::configuration::SessionConfiguration::from_overlay(context, &self.policy),
            runtime,
        );
        session.inputs = Some(&self.inputs);
        session.lazy_compiler = Some(&self.compiler);
        session.spill_candidates = Some(&self.spill_candidates);
        session.preparation_policy = Some(&self.preparation);
        session.extensions = self.extensions.as_ref().map(Registry::get);
        session.memo_limits = Some(context.retention);
        session.lookup_prefixes.enable();
        session
    }
}

/// Source edits shared by immediate workspaces and write-only overlay batches.
macro_rules! edit_methods {
    () => {
        /// Replace a cell with a literal, preserving its style and invalidating results.
        pub fn set_value(
            &mut self,
            target: EvaluationTarget,
            value: impl Into<CellValue>,
        ) -> Result<(), ModelError> {
            let style = self
                .source
                .source_cell(target)
                .map(|c| c.style.clone())
                .unwrap_or_default();
            self.write_cell(
                target,
                Cell {
                    value: value.into(),
                    formula: None,
                    shared_formula: None,
                    style,
                },
                source::EditKind::Content,
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
                .source
                .source_cell(target)
                .map(|c| c.style.clone())
                .unwrap_or_default();
            self.write_cell(target, cell, source::EditKind::Content)
        }
        /// Replace authored style and invalidate potentially affected results.
        pub fn set_style(
            &mut self,
            target: EvaluationTarget,
            style: CellStyle,
        ) -> Result<(), ModelError> {
            let mut cell = self.source.source_cell(target).cloned().unwrap_or_default();
            cell.style = style;
            self.write_cell(target, cell, source::EditKind::Style)
        }
        /// Change a formula's saved source value; the default Compute policy still recalculates it.
        pub fn set_cached_value(
            &mut self,
            target: EvaluationTarget,
            value: impl Into<CellValue>,
        ) -> Result<(), ModelError> {
            let mut cell = self
                .source
                .source_cell(target)
                .cloned()
                .ok_or_else(|| ModelError("cache target is absent".into()))?;
            if cell.formula.is_none() {
                return Err(ModelError("cache target has no formula".into()));
            }
            cell.value = value.into();
            self.write_cell(target, cell, source::EditKind::Content)
        }
        /// Remove effective authored content and invalidate results.
        pub fn remove_cell(&mut self, target: EvaluationTarget) -> Result<(), ModelError> {
            let change = self
                .source
                .change_kind(target, None, source::EditKind::Content);
            if self.source.remove_cell(target)? {
                self.source_changed(target, change);
            }
            Ok(())
        }
        fn write_cell(
            &mut self,
            target: EvaluationTarget,
            cell: Cell,
            kind: source::EditKind,
        ) -> Result<(), ModelError> {
            let change = self.source.change_kind(target, Some(&cell), kind);
            if self.source.write_cell(target, cell, kind)? {
                self.source_changed(target, change);
            }
            Ok(())
        }
    };
}
/// Shared edit expansion with a caller-supplied invalidation hook.
pub(super) use edit_methods;

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
        pub fn policy(&self) -> &EvaluationPolicy {
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
        source::edit_methods!();
        /// Replace policies; reusable programs survive changes outside parser preparation.
        pub fn set_policy(&mut self, policy: EvaluationPolicy) -> Result<(), OverlayError> {
            if self.source.policy != policy {
                if self.source.policy.parse != policy.parse {
                    self.source.compiler = RefCell::new(FormulaCompiler::new());
                    self.source.spill_candidates.get_mut().clear();
                }
                self.source.preparation.parse = policy.parse;
                let previous = std::mem::replace(&mut self.source.policy, policy);
                self.policy_changed(&previous);
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
    inputs: Option<&crate::eval::run_overlay::RunInputs>,
) -> std::collections::BTreeSet<Key> {
    let mut targets = std::collections::BTreeSet::new();
    for (sheet, source) in workbook.sheets().iter().enumerate() {
        targets.extend(
            source
                .cells()
                .keys()
                .filter(|at| {
                    source.source_cell(**at).is_some()
                        && !inputs.is_some_and(|i| i.is_deleted(sheet, **at))
                })
                .map(|at| (sheet, *at)),
        );
    }
    if let Some(inputs) = inputs {
        for (&sheet, cells) in &inputs.sheets {
            targets.extend(
                cells
                    .cells()
                    .iter()
                    .filter(|(_, edit)| edit.cell().is_some())
                    .map(|(at, _)| (sheet, *at)),
            );
        }
    }
    targets
}
