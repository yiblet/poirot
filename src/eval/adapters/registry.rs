//! Parent-owned name indexes and direct executor ownership, built at composition time.
use super::interface::{CallContext, FunctionAdapter, FunctionParameters, FunctionValue};
use crate::eval::compile::bytecode::Compiled;
use crate::eval::functions::registry::{FunctionDescriptor, FunctionId, FunctionNames};
use crate::eval::limits::EvaluationError;
use poirot_parsing::formula::FunctionName;
use std::sync::Arc;

/// Flattened adapter callback stored by the parent builtin registry.
type Executor = dyn Fn(
        &mut CallContext<'_, '_>,
        FunctionId,
        FunctionParameters<'_>,
    ) -> Option<Result<FunctionValue, EvaluationError>>
    + Send
    + Sync;

/// Links a function descriptor to the executor that owns it in a composed registry.
/// Profile membership keeps ordinary and compatibility registrations in their respective lookup namespaces.
#[derive(Clone, Copy)]
struct Registration {
    descriptor: &'static FunctionDescriptor,
    executor: usize,
}

/// Immutable parent registry. Child adapters report registrations once; each
/// registration retains its executor, including through nested composition.
/// Earlier adapters win duplicate names and typed handler identities.
#[derive(Clone)]
pub struct AdapterRegistry {
    executors: Vec<Arc<Executor>>,
    registrations: Vec<Registration>,
    names: FunctionNames,
    handlers: Vec<Option<usize>>,
}

impl AdapterRegistry {
    /// Collect registrations and retain the adapter's executor behind shared ownership.
    pub(in crate::eval) fn from_adapter<A: FunctionAdapter + 'static>(adapter: A) -> Self {
        let registrations = adapter
            .functions()
            .map(|descriptor| Registration {
                descriptor,
                executor: 0,
            })
            .collect();
        let adapter = Arc::new(adapter);
        let executor: Arc<Executor> = Arc::new(move |context, function, parameters| {
            adapter.execute(context, function, parameters)
        });
        let mut registry = Self {
            executors: vec![executor],
            registrations,
            names: FunctionNames::new(),
            handlers: vec![None; FunctionId::SLOT_COUNT],
        };
        registry.index();
        registry
    }

    /// Flatten another registry while preserving earlier duplicate precedence.
    pub(in crate::eval) fn append(mut self, next: Self) -> Self {
        let offset = self.executors.len();
        self.executors.extend(next.executors);
        self.registrations
            .extend(next.registrations.into_iter().map(|mut entry| {
                entry.executor += offset;
                entry
            }));
        self.index();
        self
    }

    fn index(&mut self) {
        self.names = FunctionNames::new();
        self.handlers.fill(None);
        for (index, entry) in self.registrations.iter().enumerate() {
            if let Some(slot) = self.handlers.get_mut(entry.descriptor.handler.index()) {
                slot.get_or_insert(entry.executor);
            }
            self.names.insert(entry.descriptor.name, index);
        }
    }
}

impl FunctionAdapter for AdapterRegistry {
    fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        self.registrations.iter().map(|entry| entry.descriptor)
    }

    fn resolve(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        self.names
            .resolve(name)
            .and_then(|index| self.registrations.get(index).map(|entry| entry.descriptor))
    }

    fn call(
        &self,
        context: &mut CallContext<'_, '_>,
        name: &FunctionName,
        parameters: poirot_functions::scalar::ParamVec<Compiled>,
    ) -> Result<FunctionValue, EvaluationError> {
        let registration = self
            .names
            .resolve(name)
            .and_then(|index| self.registrations.get(index).copied());
        context.invoke_registration(
            name,
            registration.map(|entry| entry.descriptor),
            parameters,
            |context, function, parameters| {
                let entry = registration?;
                self.executors.get(entry.executor)?(context, function, parameters)
            },
        )
    }

    fn supports(&self, function: FunctionId) -> bool {
        self.handlers
            .get(function.index())
            .is_some_and(Option::is_some)
    }

    fn execute(
        &self,
        context: &mut CallContext<'_, '_>,
        function: FunctionId,
        parameters: FunctionParameters<'_>,
    ) -> Option<Result<FunctionValue, EvaluationError>> {
        let executor = self.handlers.get(function.index()).copied().flatten()?;
        self.executors.get(executor)?(context, function, parameters)
    }

    fn into_registry(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::config::EvaluationInputs;
    use crate::eval::coordinates::EvaluationTarget;
    use crate::eval::functions::registry::{
        Arity, DependencyPolicy, EvaluationStrategy, MathFunction,
    };
    use crate::eval::session::Session;
    use crate::model::{CellAddress, CellValue, Workbook};
    use poirot_parsing::formula::CommonFunction;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static EXTRA: FunctionDescriptor = FunctionDescriptor {
        result: crate::eval::functions::registry::ResultRule::Scalar,
        name: "TEAM.CUSTOM",
        handler: FunctionId::Math(MathFunction::Acosh),
        arity: Arity {
            minimum: 0,
            maximum: 0,
        },
        strategy: EvaluationStrategy::FamilyControlled,
        dependencies: DependencyPolicy::ARGUMENTS,
    };

    /// Adapter fixture counting registration discovery and handler dispatch.
    struct Probe {
        descriptor: &'static FunctionDescriptor,
        reports: Arc<AtomicUsize>,
        calls: Arc<AtomicUsize>,
        result: f64,
    }

    impl FunctionAdapter for Probe {
        fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
            self.reports.fetch_add(1, Ordering::Relaxed);
            std::iter::once(self.descriptor)
        }

        fn execute(
            &self,
            _context: &mut CallContext<'_, '_>,
            function: FunctionId,
            _parameters: FunctionParameters<'_>,
        ) -> Option<Result<FunctionValue, EvaluationError>> {
            if function != self.descriptor.handler {
                return None;
            }
            self.calls.fetch_add(1, Ordering::Relaxed);
            Some(Ok(FunctionValue::scalar(CellValue::Number(self.result))))
        }
    }

    fn invoke(registry: &AdapterRegistry, name: FunctionName) -> CellValue {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut context = CallContext {
            session: &mut session,
            target: EvaluationTarget {
                sheet: 0,
                address: CellAddress { row: 0, col: 0 },
            },
        };
        let value = registry
            .call(&mut context, &name, Default::default())
            .unwrap();
        context.scalar(value).unwrap()
    }

    #[test]
    fn parent_reports_once_and_retains_the_direct_owner() {
        let reports = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = super::super::families::MathAdapter.chain(
            super::super::families::FinancialAdapter.chain(Probe {
                descriptor: &EXTRA,
                reports: reports.clone(),
                calls: calls.clone(),
                result: 42.,
            }),
        );
        assert_eq!(
            registry.executors.len(),
            3,
            "nested parents must flatten their executors"
        );
        assert!(registry.names.common_slot(CommonFunction::Fact).is_some());
        assert_eq!(
            registry.names.uncommon_count(),
            1,
            "only the uncommon name needs a hash entry"
        );
        for _ in 0..3 {
            assert_eq!(
                invoke(&registry, "team.custom".into()),
                CellValue::Number(42.)
            );
        }
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert_eq!(
            reports.load(Ordering::Relaxed),
            1,
            "calling must not rediscover child functions"
        );
    }

    #[test]
    fn duplicate_registrations_preserve_first_owner() {
        let calls = Arc::new(AtomicUsize::new(0));
        let ignored = Arc::new(AtomicUsize::new(0));
        let registry = Probe {
            descriptor: &EXTRA,
            reports: Default::default(),
            calls: calls.clone(),
            result: 1.,
        }
        .chain(Probe {
            descriptor: &EXTRA,
            reports: Default::default(),
            calls: ignored.clone(),
            result: 2.,
        });
        assert_eq!(
            invoke(&registry, "TEAM.CUSTOM".into()),
            CellValue::Number(1.)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(ignored.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn all_advertised_names_have_numeric_slots() {
        let registry = super::super::builtin_adapters::builtin_registry();
        for descriptor in crate::eval::functions::registry::FUNCTION_DESCRIPTORS.iter() {
            let name = FunctionName::from(descriptor.name);
            assert!(
                name.index().is_some(),
                "{} needs a common numeric slot",
                descriptor.name
            );
            assert!(registry.supports(descriptor.handler));
        }
        assert_eq!(
            CommonFunction::COUNT,
            crate::eval::functions::registry::FUNCTION_DESCRIPTORS.len()
        );
        let name = FunctionName::Known(CommonFunction::RankEq);
        assert!(registry.resolve(&name).is_some());
        assert!(registry.resolve(&"_xlfn.RANK.EQ".into()).is_some());
        assert!(registry.resolve(&"_XLFN.RANK.EQ".into()).is_none());
    }
}
