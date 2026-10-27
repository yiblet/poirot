use super::incremental::Overlay;
use super::overlay_error::OverlayError;
use super::registry_handle::RegistryHandle;
use super::source::Source;
use crate::eval::config::EvaluationPolicy;
use crate::eval::reuse::Cache;
use crate::model::Workbook;
use std::{borrow::Borrow, cell::RefCell, sync::Arc};

/// Configures an overlay's workbook, formula policies, and custom functions.
pub struct OverlayBuilder<'a, W> {
    pub(super) workbook: W,
    pub(super) policy: EvaluationPolicy,
    pub(super) extensions: Option<RegistryHandle<'a>>,
}

impl<'a, W: Borrow<Workbook>> OverlayBuilder<'a, W> {
    /// Select formula preparation, computation, and materialization policies.
    pub fn with_policy(mut self, policy: EvaluationPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Borrow the exact registry used to compile and execute extension calls.
    pub fn with_extensions(
        mut self,
        registry: &'a crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Self {
        self.extensions = Some(RegistryHandle::Borrowed(registry));
        self
    }

    /// Share providers so an already-used workspace can move to an ordinary worker thread.
    pub fn with_shared_extensions(
        mut self,
        registry: Arc<crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Self {
        self.extensions = Some(RegistryHandle::Shared(registry));
        self
    }

    /// Validate policies and create empty authored and result diffs.
    pub fn build(self) -> Result<Overlay<'a, W>, OverlayError> {
        Ok(Overlay {
            source: Source::new(self.workbook, self.policy, self.extensions)?,
            cache: RefCell::new(Cache::default()),
        })
    }
}
