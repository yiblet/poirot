//! Parent-owned name indexes and direct executor ownership, built at composition time.
use super::{CallContext, FunctionAdapter, FunctionParameters, FunctionValue};
use crate::bytecode::Compiled;
use crate::{
    EvaluationFailure,
    registry::{FunctionDescriptor, FunctionId},
};
use poirot_parsing::formula::{CommonFunction, FunctionName};
use std::{collections::HashMap, sync::Arc};

/// Flattened adapter callback stored by the parent builtin registry.
type Executor = dyn Fn(
        &mut CallContext<'_, '_>,
        FunctionId,
        FunctionParameters<'_>,
    ) -> Option<Result<FunctionValue, EvaluationFailure>>
    + Send
    + Sync;

/// Catalog entry linked to its flattened executor and profile membership.
#[derive(Clone, Copy)]
struct Registration {
    descriptor: &'static FunctionDescriptor,
    executor: usize,
    profile: bool,
}

/// Common-name slots and fallback spelling map for one registration namespace.
#[derive(Clone)]
struct Names {
    core: Vec<Option<usize>>,
    other: HashMap<String, usize>,
}
impl Names {
    fn new() -> Self {
        Self {
            core: vec![None; CommonFunction::COUNT],
            other: HashMap::new(),
        }
    }
    fn insert(&mut self, name: &str, entry: usize) {
        if let Some(name) = CommonFunction::parse(name) {
            if let Some(slot) = self.core.get_mut(name.index()) {
                slot.get_or_insert(entry);
            }
        } else {
            self.other.entry(name.to_ascii_uppercase()).or_insert(entry);
        }
    }
    fn get(&self, name: &FunctionName) -> Option<usize> {
        match name {
            FunctionName::Known(name) => self.core.get(name.index()).copied().flatten(),
            // Also admit manually constructed Unknown values whose spelling is common.
            FunctionName::Unknown(name) => {
                if let Some(common) = CommonFunction::parse(name) {
                    return self.core.get(common.index()).copied().flatten();
                }
                // Normal extension names fold on the stack, avoiding per-call allocation.
                let mut folded = [0u8; 64];
                if let Some(bytes) = folded.get_mut(..name.len()) {
                    for (out, byte) in bytes.iter_mut().zip(name.bytes()) {
                        *out = byte.to_ascii_uppercase();
                    }
                    self.other.get(std::str::from_utf8(bytes).ok()?).copied()
                } else {
                    self.other.get(&name.to_ascii_uppercase()).copied()
                }
            }
        }
    }
}

/// Immutable parent registry. Child adapters report registrations once; each
/// registration retains its executor, including through nested composition.
/// Earlier adapters win duplicate names and typed handler identities.
#[derive(Clone)]
pub struct AdapterRegistry {
    executors: Vec<Arc<Executor>>,
    registrations: Vec<Registration>,
    names: Names,
    prefixed: Names,
    profile: Names,
    profile_prefixed: Names,
    handlers: Vec<Option<usize>>,
}
impl AdapterRegistry {
    /// Collect registrations and retain the adapter's executor behind shared ownership.
    pub(crate) fn from_adapter<A: FunctionAdapter + 'static>(adapter: A) -> Self {
        let registrations = adapter
            .functions()
            .map(|descriptor| Registration {
                descriptor,
                executor: 0,
                profile: false,
            })
            .chain(adapter.profile_functions().map(|descriptor| Registration {
                descriptor,
                executor: 0,
                profile: true,
            }))
            .collect();
        let adapter = Arc::new(adapter);
        let executor: Arc<Executor> = Arc::new(move |context, function, parameters| {
            adapter.execute(context, function, parameters)
        });
        let mut registry = Self {
            executors: vec![executor],
            registrations,
            names: Names::new(),
            prefixed: Names::new(),
            profile: Names::new(),
            profile_prefixed: Names::new(),
            handlers: vec![None; FunctionId::SLOT_COUNT],
        };
        registry.index();
        registry
    }
    /// Flatten another registry while preserving earlier duplicate precedence.
    pub(crate) fn append(mut self, next: Self) -> Self {
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
        self.names = Names::new();
        self.prefixed = Names::new();
        self.profile = Names::new();
        self.profile_prefixed = Names::new();
        self.handlers.fill(None);
        for (index, entry) in self.registrations.iter().enumerate() {
            if let Some(slot) = self.handlers.get_mut(entry.descriptor.handler.index()) {
                slot.get_or_insert(entry.executor);
            }
            if entry.profile {
                self.profile.insert(entry.descriptor.name, index);
                self.profile_prefixed.insert(entry.descriptor.name, index);
            } else {
                self.names.insert(entry.descriptor.name, index);
                if crate::atp::ATP_SUPPORTED
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(entry.descriptor.name))
                {
                    self.prefixed.insert(entry.descriptor.name, index);
                }
            }
        }
    }
    fn lookup(&self, name: &FunctionName, names: &Names, prefixed: &Names) -> Option<usize> {
        match name {
            FunctionName::Unknown(name) if name.starts_with("_xlfn.") => {
                // SAFETY: starts_with("_xlfn.") checks this six-byte ASCII prefix, so byte six is an in-bounds UTF-8 boundary.
                prefixed.get(&FunctionName::from(&name[6..]))
            }
            _ => names.get(name),
        }
    }
}
impl FunctionAdapter for AdapterRegistry {
    fn functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        self.registrations
            .iter()
            .filter(|entry| !entry.profile)
            .map(|entry| entry.descriptor)
    }
    fn profile_functions(&self) -> impl Iterator<Item = &'static FunctionDescriptor> {
        self.registrations
            .iter()
            .filter(|entry| entry.profile)
            .map(|entry| entry.descriptor)
    }
    fn resolve(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        self.lookup(name, &self.names, &self.prefixed)
            .and_then(|index| self.registrations.get(index).map(|entry| entry.descriptor))
    }
    fn resolve_profile(&self, name: &FunctionName) -> Option<&'static FunctionDescriptor> {
        self.lookup(name, &self.profile, &self.profile_prefixed)
            .and_then(|index| self.registrations.get(index).map(|entry| entry.descriptor))
    }
    fn call(
        &self,
        context: &mut CallContext<'_, '_>,
        name: &FunctionName,
        parameters: poirot_functions_core::ParamVec<Compiled>,
    ) -> Result<FunctionValue, EvaluationFailure> {
        let builtin = self.lookup(name, &self.names, &self.prefixed);
        let registration = builtin
            .or_else(|| {
                (context.session.builtin_policy
                    == crate::BuiltinSemanticsPolicy::ExcelDocumentation)
                    .then(|| self.lookup(name, &self.profile, &self.profile_prefixed))
                    .flatten()
            })
            .and_then(|index| self.registrations.get(index).copied());
        context.invoke_registration(
            name,
            builtin.is_some(),
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
    ) -> Option<Result<FunctionValue, EvaluationFailure>> {
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
    use crate::{
        CellValue, EvaluationInputs, EvaluationTarget, Session,
        registry::{Arity, DependencyPolicy, EvaluationStrategy, MathFunction},
    };
    use poirot_model::{CellAddress, Workbook};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static EXTRA: FunctionDescriptor = FunctionDescriptor {
        result: crate::registry::ResultRule::Scalar,
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
        ) -> Option<Result<FunctionValue, EvaluationFailure>> {
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
        let registry =
            super::super::MathAdapter.chain(super::super::FinancialAdapter.chain(Probe {
                descriptor: &EXTRA,
                reports: reports.clone(),
                calls: calls.clone(),
                result: 42.,
            }));
        assert_eq!(
            registry.executors.len(),
            3,
            "nested parents must flatten their executors"
        );
        assert!(registry.names.core[CommonFunction::Fact.index()].is_some());
        assert_eq!(
            registry.names.other.len(),
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
    fn all_advertised_names_have_numeric_slots_and_profile_names_remain_separate() {
        let registry = super::super::builtin_registry();
        for descriptor in crate::registry::FUNCTION_DESCRIPTORS
            .iter()
            .chain(crate::registry::EXCEL_DESCRIPTORS)
        {
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
            crate::registry::FUNCTION_DESCRIPTORS.len() + crate::registry::EXCEL_DESCRIPTORS.len()
        );
        let name = FunctionName::Known(CommonFunction::RankEq);
        assert!(registry.resolve(&name).is_none());
        assert!(registry.resolve_profile(&name).is_some());
        assert!(registry.resolve_profile(&"_xlfn.RANK.EQ".into()).is_some());
        assert!(registry.resolve_profile(&"_XLFN.RANK.EQ".into()).is_none());
    }
}
