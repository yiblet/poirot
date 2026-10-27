use crate::eval::config::{
    ArrayResultPolicy, CurrencyContext, EvaluationInputs, ProducerPolicy, RunConfiguration,
};
use crate::eval::session::expression_demand::ExpressionDemand;

/// Execution policies and borrowed formatting settings selected for one run.
pub(in crate::eval) struct RunPolicies<'a> {
    /// Clock, random seed and locale inputs.
    pub(in crate::eval) context: EvaluationInputs,
    /// Caller-supplied currency formatting, when any.
    pub(in crate::eval) currency: Option<&'a CurrencyContext>,
    /// Numeric token conventions for parsing text arguments.
    pub(in crate::eval) number_locale: &'a poirot_parsing::number::NumberLocale,

    /// Builtin contracts selected at execution time.

    /// Publication of final array results.
    pub(in crate::eval) array_result_policy: ArrayResultPolicy,
    /// Whether formulas compute or trust saved results.
    pub(in crate::eval) producer_policy: ProducerPolicy,
    /// Native regex guards.
    pub(in crate::eval) regex: poirot_functions::text::regex::ExecutionLimits,
}

impl<'a> RunPolicies<'a> {
    /// Select default execution policies for captured inputs.
    pub(in crate::eval) fn from_inputs(context: EvaluationInputs) -> Self {
        Self {
            context,
            currency: None,
            number_locale: &poirot_parsing::number::NumberLocale::EN_US,

            array_result_policy: ArrayResultPolicy::default(),
            producer_policy: ProducerPolicy::default(),
            regex: Default::default(),
        }
    }

    /// Borrow localization and formatting from an explicit run configuration.
    pub(in crate::eval) fn from_run_configuration(configuration: &'a RunConfiguration) -> Self {
        Self {
            context: configuration.context(),
            currency: configuration.formatting().currency(),
            number_locale: configuration.number_locale(),

            array_result_policy: configuration.array_result_policy(),
            producer_policy: configuration.producer_policy(),
            regex: Default::default(),
        }
    }

    /// Combine per-evaluation inputs with policies owned by the evaluation workspace.
    pub(in crate::eval) fn from_overlay(
        context: &'a crate::eval::config::EvaluationContext,
        policy: &crate::eval::config::EvaluationPolicy,
    ) -> Self {
        Self {
            context: context.inputs(),
            currency: context.formatting.currency(),
            number_locale: &context.number_locale,

            array_result_policy: policy.arrays,
            producer_policy: policy.producers,
            regex: context.regex,
        }
    }

    /// Select the worksheet expression demand independently of nested scalar consumers.
    pub(in crate::eval) fn root_demand(&self) -> ExpressionDemand {
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill {
            ExpressionDemand::Array
        } else {
            ExpressionDemand::Scalar
        }
    }
}
