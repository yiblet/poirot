use super::call_context::CallContext;
use super::function_parameters::FunctionParameters;
use super::function_value::FunctionValue;
use crate::eval::compile::bytecode::Compiled;
use crate::eval::functions::registry::{FunctionDescriptor, FunctionId};
use crate::eval::limits::EvaluationError;
use poirot_functions::scalar::ParamVec;
use poirot_parsing::formula::FunctionName;

/// A family registers names and executes its own handlers. Arguments stay deferred.
/// An adapter chain implements this exact interface, allowing nested composition.
/// Arity and coercion remain adapter policies because error ordering is observable.
/// This connects builtin handler families to the evaluator; use
/// [`crate::eval::functions::extensions::ExtensionRegistry`] for user-defined callback functions.
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
    ) -> Option<Result<FunctionValue, EvaluationError>>;

    /// Resolve a default-catalog spelling under builtin prefix and case rules.
    fn resolve(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        let name = name.as_str();
        let name = name.strip_prefix("_xlfn.").unwrap_or(name);
        self.functions()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Whether this adapter owns the typed handler.
    fn supports(&self, function: FunctionId) -> bool {
        self.functions().any(|entry| entry.handler == function)
    }

    /// Resolve, admit, and execute a named call with deferred arguments.
    fn call(
        &self,
        context: &mut CallContext<'_, '_>,
        name: &FunctionName,
        parameters: ParamVec<Compiled>,
    ) -> Result<FunctionValue, EvaluationError> {
        let descriptor = self.resolve(name);
        context.invoke_registration(
            name,
            descriptor,
            parameters,
            |context, function, parameters| self.execute(context, function, parameters),
        )
    }

    /// Build lookup tables once, keeping this adapter as the owning executor.
    fn indexed(self) -> super::super::registry::AdapterRegistry
    where
        Self: Sized + 'static,
    {
        self.into_registry()
    }

    /// Move this adapter into an indexed registry that owns its executor.
    fn into_registry(self) -> super::super::registry::AdapterRegistry
    where
        Self: Sized + 'static,
    {
        super::super::registry::AdapterRegistry::from_adapter(self)
    }

    /// Parent registries flatten their children's executors, so nesting does not
    /// introduce a runtime search through adapters.
    fn chain<Next: FunctionAdapter + 'static>(
        self,
        next: Next,
    ) -> super::super::registry::AdapterRegistry
    where
        Self: Sized + 'static,
    {
        self.into_registry().append(next.into_registry())
    }
}
