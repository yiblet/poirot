//! Common registration, deferred inputs, and composition for evaluator adapters.
use crate::{
    ArgumentsRef, CellValue, EvaluationFailure, EvaluationTarget, ExecutableRef, Session, Value,
    bytecode::Compiled,
    call_admission::{AdmissionOrder, ResolvedCall},
    registry::{FunctionDescriptor, FunctionId},
};
use poirot_formula::FunctionName;
use poirot_functions_core::{Input, ParamVec};

/// Borrowed lazy argument blocks from compiled bytecode.
/// This view never materializes a bytecode tree just to call a function family.
#[derive(Clone, Copy)]
pub struct FunctionParameters<'a>(
    /// Deferred bytecode argument handles.
    pub(crate) ArgumentsRef<'a>,
);
impl<'a> From<&'a [Compiled]> for FunctionParameters<'a> {
    fn from(expressions: &'a [Compiled]) -> Self {
        Self(expressions.into())
    }
}
impl FunctionParameters<'_> {
    /// Count syntactic argument positions, including omissions.
    pub fn len(self) -> usize {
        self.0.len()
    }
    /// Whether the call has no argument positions.
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}

/// An adapter result may retain references or arrays until the caller consumes it.
#[derive(Clone)]
pub struct FunctionValue(
    /// Unprojected engine value channel.
    pub(crate) Value,
);
impl FunctionValue {
    /// Whether this value represents an omitted argument.
    pub fn is_missing(&self) -> bool {
        matches!(self.0, Value::Missing)
    }
    /// Wrap a scalar result without reference or array projection.
    pub fn scalar(value: CellValue) -> Self {
        Self(Value::Scalar(value))
    }
}

/// The evaluator supplies execution state; function libraries never depend on it.
pub struct CallContext<'call, 'workbook> {
    /// Shared bounded execution state for the current call.
    pub(crate) session: &'call mut Session<'workbook>,
    /// Caller coordinates used for implicit intersection and attribution.
    pub target: EvaluationTarget,
}
impl<'call, 'workbook> CallContext<'call, 'workbook> {
    /// Wrap one compiled expression as a memoized deferred input.
    pub fn input<'input>(
        &'input mut self,
        expression: &'input Compiled,
    ) -> FunctionInput<'input, 'call, 'workbook> {
        FunctionInput {
            context: self,
            expression: ExecutableRef::Bytecode {
                program: expression,
                start: 0,
            },
            value: None,
        }
    }
    /// Request one lazy argument without inspecting or forcing any other argument.
    pub fn argument<'input>(
        &'input mut self,
        parameters: FunctionParameters<'input>,
        index: usize,
    ) -> Option<FunctionInput<'input, 'call, 'workbook>> {
        Some(FunctionInput {
            context: self,
            expression: parameters.0.get(index)?,
            value: None,
        })
    }
    /// Admit a resolved call, observe it, and dispatch without forcing its operands.
    pub(super) fn invoke_registration(
        &mut self,
        name: &FunctionName,
        default: bool,
        descriptor: Option<&'static FunctionDescriptor>,
        parameters: ParamVec<Compiled>,
        execute: impl FnOnce(
            &mut Self,
            FunctionId,
            FunctionParameters<'_>,
        ) -> Option<Result<FunctionValue, EvaluationFailure>>,
    ) -> Result<FunctionValue, EvaluationFailure> {
        self.session.admit_resolved_call(
            ResolvedCall {
                invocation: descriptor.map(|entry| crate::ConsumedBuiltinInvocation {
                    target: self.target,
                    handler: entry.handler,
                    parsed_spelling: name.as_str(),
                    resolved_spelling: name.as_str(),
                    argument_count: parameters.len(),
                }),
                read_builtin_policy: !default,
            },
            AdmissionOrder::PolicyBeforeObserver,
        )?;
        let Some(descriptor) = descriptor else {
            self.session
                .read_lazy(|| crate::DependencyRead::OpaqueProvider(name.as_str().to_owned()));
            return Ok(FunctionValue::scalar(CellValue::Error(
                crate::CellError::Name,
            )));
        };
        execute(self, descriptor.handler, parameters.as_slice().into()).unwrap_or_else(|| {
            Err(EvaluationFailure::UnsupportedCapability {
                capability: crate::CapabilityId::FunctionAdapter,
                source: Some(self.target),
                detail: format!("registered function {name} has no executable adapter"),
            })
        })
    }
    /// Select a scalar at the current caller coordinates, resolving reference or
    /// array values only when this projection is requested.
    pub fn scalar(&mut self, value: FunctionValue) -> Result<CellValue, EvaluationFailure> {
        self.session.scalar(self.target.address, value.0)
    }
}

/// A deferred expression memoized after successful evaluation. Repeated forces copy that value
/// under the host budget, without repeating volatile operations.
pub struct FunctionInput<'input, 'call, 'workbook> {
    context: &'input mut CallContext<'call, 'workbook>,
    expression: ExecutableRef<'input>,
    value: Option<Value>,
}
impl Input for FunctionInput<'_, '_, '_> {
    /// Adapter argument value returned by deferred evaluation.
    type Value = FunctionValue;
    /// Operational failure propagated when forcing the argument.
    type Error = EvaluationFailure;
    fn force(&mut self) -> Result<FunctionValue, EvaluationFailure> {
        if self.value.is_none() {
            let target = self.context.target;
            self.context.session.budget.allocation(1)?;
            let value = self
                .context
                .session
                .expr(target.sheet, target.address, self.expression)?;
            self.value = Some(if self.expression.is_missing() {
                Value::Missing
            } else {
                value
            });
        }
        crate::host::CallHost::copy_value(
            &mut self.context.session.budget,
            // SAFETY: If value is None this function assigns Some before reaching this access; no intervening code clears it.
            self.value.as_ref().unwrap(),
        )
        .map(FunctionValue)
    }
}

/// A family registers names and executes its own handlers. Arguments stay deferred.
/// An adapter chain implements this exact interface, allowing nested composition.
/// Arity and coercion remain adapter policies because error ordering is observable.
/// This connects builtin handler families to the evaluator; use
/// [`crate::extensions::ExtensionRegistry`] for user-defined callback functions.
pub trait FunctionAdapter: Send + Sync {
    /// Supported builtin descriptors in precedence order. Every advertised handler
    /// must be executable by this adapter; composition keeps the first duplicate.
    fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor>;
    /// Return `None` for a handler this adapter does not own. A handled call returns
    /// `Some`: spreadsheet errors are scalar error values, while `Err` aborts
    /// operationally. Request only operands required by the function's semantics.
    fn execute(
        &self,
        context: &mut CallContext<'_, '_>,
        function: FunctionId,
        parameters: FunctionParameters<'_>,
    ) -> Option<Result<FunctionValue, EvaluationFailure>>;

    /// Resolve a default-catalog spelling under builtin prefix and case rules.
    fn resolve(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        let name = name.as_str();
        let name = if let Some(name) = name.strip_prefix("_xlfn.") {
            if !crate::atp::ATP_SUPPORTED
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(name))
            {
                return None;
            }
            name
        } else {
            name
        };
        self.functions()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }
    /// Whether this adapter owns the typed handler.
    fn supports(&self, function: FunctionId) -> bool {
        self.functions().any(|entry| entry.handler == function)
    }
    /// Resolve a handler, allowing profile-only registrations when policy selects them.
    fn resolve_with_policy(
        &self,
        name: &FunctionName,
        policy: crate::BuiltinSemanticsPolicy,
    ) -> Option<FunctionId> {
        self.resolve(name).map(|entry| entry.handler).or_else(|| {
            (policy == crate::BuiltinSemanticsPolicy::ExcelDocumentation)
                .then(|| self.resolve_profile(name))
                .flatten()
                .map(|entry| entry.handler)
        })
    }
    /// Resolve, admit, and execute a named call with deferred arguments.
    fn call(
        &self,
        context: &mut CallContext<'_, '_>,
        name: &FunctionName,
        parameters: ParamVec<Compiled>,
    ) -> Result<FunctionValue, EvaluationFailure> {
        let builtin = self.resolve(name);
        let descriptor = builtin.or_else(|| {
            (context.session.builtin_policy == crate::BuiltinSemanticsPolicy::ExcelDocumentation)
                .then(|| self.resolve_profile(name))
                .flatten()
        });
        context.invoke_registration(
            name,
            builtin.is_some(),
            descriptor,
            parameters,
            |context, function, parameters| self.execute(context, function, parameters),
        )
    }

    /// Profile-only registrations are separate from the default function catalog.
    fn profile_functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        std::iter::empty()
    }
    /// Resolve a profile-only spelling supported by this adapter.
    fn resolve_profile(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        super::profile_descriptor(name).filter(|entry| self.supports(entry.handler))
    }
    /// Build lookup tables once, keeping this adapter as the owning executor.
    fn indexed(self) -> super::AdapterRegistry
    where
        Self: Sized + 'static,
    {
        self.into_registry()
    }
    /// Move this adapter into an indexed registry that owns its executor.
    fn into_registry(self) -> super::AdapterRegistry
    where
        Self: Sized + 'static,
    {
        super::AdapterRegistry::from_adapter(self)
    }
    /// Parent registries flatten their children's executors, so nesting does not
    /// introduce a runtime search through adapters.
    fn chain<Next: FunctionAdapter + 'static>(self, next: Next) -> super::AdapterRegistry
    where
        Self: Sized + 'static,
    {
        self.into_registry().append(next.into_registry())
    }
}

impl crate::EvaluationRun<'_> {
    /// Call a builtin by name with a small, deferred argument vector.
    /// Parse individual expressions with poirot_formula and compile them with bytecode::compile. Unknown names return the spreadsheet #NAME? error.
    pub fn call(
        &mut self,
        target: EvaluationTarget,
        name: &FunctionName,
        parameters: ParamVec<Compiled>,
    ) -> Result<CellValue, EvaluationFailure> {
        self.call_with_adapter(&super::builtins(), target, name, parameters)
    }

    /// Evaluate through one family or a user-composed chain of family adapters.
    pub fn call_with_adapter(
        &mut self,
        adapter: &impl FunctionAdapter,
        target: EvaluationTarget,
        name: &FunctionName,
        parameters: ParamVec<Compiled>,
    ) -> Result<CellValue, EvaluationFailure> {
        if self.session.workbook.sheet(target.sheet).is_none() || target.address.validate().is_err()
        {
            return Ok(CellValue::Error(crate::CellError::Ref));
        }
        let previous = self.session.consumer.replace(target);
        self.session.budget.push_cell(target);
        self.session.run_usage.formula_executions += 1;
        let result = (|| {
            self.session.budget.charge(crate::WorkKind::Expression, 1)?;
            self.session.budget.enter_depth()?;
            let result = adapter
                .call(
                    &mut CallContext {
                        session: &mut self.session,
                        target,
                    },
                    name,
                    parameters,
                )
                .map(|value| value.0);
            self.session.budget.leave_depth();
            result
        })()
        .and_then(|value| {
            self.session
                .result_scalar(target.sheet, target.address, value)
        })
        // TODO(coercion-non-argument): [MEDIUM] Standalone function results lose blank identity; nested calls must not use this publication path.
        .map(crate::coerce_cell_publication);
        self.session.budget.pop_cell();
        self.session.consumer = previous;
        result
    }
}
