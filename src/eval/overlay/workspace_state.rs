//! Detached overlay source and retained results awaiting a baseline workbook.

use super::evaluation::Evaluation;
use super::incremental::Overlay;
use super::overlay_error::OverlayError;
use super::registry_handle::RegistryHandle;
use super::source::Source;
use crate::eval::compile::compiler::FormulaCompiler;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::reuse::{Cache, Key};
use crate::model::Workbook;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::sync::Arc;

/// Validated detached source and optional incremental results, awaiting a baseline handle.
pub struct WorkspaceState {
    /// Postcard-encoded source snapshot of the baseline. It is decoded only to check a
    /// caller-supplied workbook, so restoration keeps no second owned model.
    pub(super) baseline: Vec<u8>,
    pub(super) inputs: crate::eval::inputs::RunInputs,
    pub(super) compiler: FormulaCompiler,
    pub(super) inspected_prefixes: Vec<Key>,
    pub(super) cache: Cache,
    pub(super) policy: EvaluationPolicy,
    pub(super) preparation: crate::eval::compile::compiler::PreparationPolicy,
}

impl WorkspaceState {
    /// Materialize effective authored source for independent persistence/recovery.
    /// Compiled programs and retained computed results are not promoted to inputs.
    /// The supplied baseline must match exactly, just as for runtime attachment.
    pub fn source_workbook(&self, workbook: &Workbook) -> Result<Workbook, OverlayError> {
        self.check_baseline(workbook)?;
        Ok(workbook.with_changes(self.inputs.workbook_changes())?)
    }

    /// Encode a baseline workbook in the form [`Self::check_baseline`] compares against.
    pub(super) fn encode_baseline(workbook: &Workbook) -> Result<Vec<u8>, OverlayError> {
        postcard::to_allocvec(&workbook.snapshot())
            .map_err(|error| OverlayError::Snapshot(error.to_string()))
    }

    /// Require `workbook` to match the baseline this state was restored against.
    fn check_baseline(&self, workbook: &Workbook) -> Result<(), OverlayError> {
        let baseline = postcard::from_bytes::<crate::model::snapshot::Snapshot<'_>>(&self.baseline)
            .map_err(|error| OverlayError::Snapshot(error.to_string()))?;
        if !baseline.matches(workbook) {
            return Err(OverlayError::Snapshot(
                "baseline differs from detached state".into(),
            ));
        }
        Ok(())
    }

    /// Attach to an exactly matching baseline and rebuild saved source-discovery indexes.
    /// This inspects formulas without evaluating them; execution starts with reusable discovery.
    pub fn attach<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.attach_registry(workbook, None)
    }

    /// Attach to an exactly matching baseline and extension registry, rebuilding source discovery.
    pub fn attach_with_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: &'a crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.attach_registry(workbook, Some(RegistryHandle::Borrowed(extensions)))
    }

    /// Reattach shared providers and rebuild source discovery for worker-thread transfer.
    pub fn attach_with_shared_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: Arc<crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.attach_registry(workbook, Some(RegistryHandle::Shared(extensions)))
    }

    /// Attach to the workbook restored beside this state, skipping the baseline comparison.
    /// Only artifact restoration calls this, with the workbook it decoded for this state.
    pub(in crate::eval) fn attach_restored<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.rebuild(workbook, None)
    }

    /// Direct evaluation on the workbook restored beside this state; see [`Self::attach_restored`].
    pub(in crate::eval) fn into_restored_evaluation<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(self.attach_restored(workbook)?.source, context)
    }

    /// Check the baseline, then attach with the given extension registry.
    fn attach_registry<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: Option<RegistryHandle<'a>>,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.check_baseline(workbook.borrow())?;
        self.rebuild(workbook, extensions)
    }

    /// Check the extension registry identity and rebuild source discovery on `workbook`.
    fn rebuild<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: Option<RegistryHandle<'a>>,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        let registry = extensions.as_ref().map(RegistryHandle::get).map_or_else(
            crate::eval::compile::compiler::RegistryIdentity::default,
            crate::eval::compile::compiler::RegistryIdentity::with_extensions,
        );
        if self.preparation.registry != registry {
            return Err(OverlayError::RegistryMismatch);
        }
        let mut source = Source {
            workbook,
            inputs: self.inputs,
            compiler: RefCell::new(self.compiler),
            spill_candidates: Default::default(),
            policy: self.policy,
            preparation: self.preparation,
            extensions,
        };
        source.spill_candidates = RefCell::new(crate::eval::spill::restore_candidates(
            source.workbook(),
            &source.inputs,
            &source.compiler,
            &source.preparation,
            source.extensions.as_ref().map(RegistryHandle::get),
            &self.inspected_prefixes,
        )?);
        Ok(Overlay {
            source,
            cache: RefCell::new(self.cache),
        })
    }

    /// Attach as direct evaluation, reusing compilation but discarding incremental results.
    pub fn into_evaluation<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(self.attach(workbook)?.source, context)
    }

    /// Attach direct evaluation with the exact runtime extension registry.
    pub fn into_evaluation_with_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
        extensions: &'a crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(
            self.attach_with_extensions(workbook, extensions)?.source,
            context,
        )
    }

    /// Restore direct evaluation with shared providers and a fresh context.
    pub fn into_evaluation_with_shared_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
        extensions: Arc<crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(
            self.attach_with_shared_extensions(workbook, extensions)?
                .source,
            context,
        )
    }
}
