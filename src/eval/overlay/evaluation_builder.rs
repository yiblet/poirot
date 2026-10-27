//! Configuration of a direct evaluation before its source is attached.

use super::evaluation::Evaluation;
use super::overlay_builder::OverlayBuilder;
use super::overlay_error::OverlayError;
use super::source::Source;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::model::Workbook;
use std::borrow::Borrow;
use std::sync::Arc;

/// Configures the workbook, formula policies, and extensions for an evaluation.
pub struct EvaluationBuilder<'a, W> {
    pub(super) source: OverlayBuilder<'a, W>,
}

impl<'a, W: Borrow<Workbook>> EvaluationBuilder<'a, W> {
    /// Select parsing, calculation, and materialization policies.
    pub fn with_policy(mut self, policy: EvaluationPolicy) -> Self {
        self.source = self.source.with_policy(policy);
        self
    }

    /// Borrow the exact runtime extension registry.
    pub fn with_extensions(
        mut self,
        registry: &'a crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Self {
        self.source = self.source.with_extensions(registry);
        self
    }

    /// Share immutable runtime providers with worker-owned direct evaluation.
    pub fn with_shared_extensions(
        mut self,
        registry: Arc<crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Self {
        self.source = self.source.with_shared_extensions(registry);
        self
    }

    /// Build an ordinary calculation without detailed profiling. Quotas remain enforced.
    pub fn build_calculation(
        self,
        context: EvaluationContext,
    ) -> Result<super::calculation::Calculation<'a, W>, OverlayError> {
        self.build_calculation_with_profiling(context, false)
    }

    /// Build ordinary calculation with profiling independently selected from quotas.
    pub fn build_calculation_with_profiling(
        self,
        context: EvaluationContext,
        profiling: bool,
    ) -> Result<super::calculation::Calculation<'a, W>, OverlayError> {
        let mut inner = self.build(context)?;
        inner.set_profiling(profiling);
        Ok(super::calculation::Calculation { inner, profiling })
    }

    /// Validate the options and create an evaluation with the supplied context.
    pub fn build(self, context: EvaluationContext) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(
            Source::new(
                self.source.workbook,
                self.source.policy,
                self.source.extensions,
            )?,
            context,
        )
    }
}
