//! Revisioned provider catalog and atomic registration mutations.
use super::extension_binding::ExtensionBinding;
use super::extension_contract::ExtensionContract;
use super::extension_function::ExtensionFunction;
use super::extension_id::ExtensionId;
use super::extension_identity::ExtensionIdentity;
use super::extension_namespace::ExtensionNamespace;
use super::extension_registration_error::ExtensionRegistrationError;
use super::extension_registration_identity::ExtensionRegistrationIdentity;
use std::{collections::BTreeMap, sync::Arc};

/// Exact extension binding paired with its runtime provider owner.
struct Registration {
    binding: ExtensionBinding,
    implementation: Arc<dyn ExtensionFunction>,
}

/// User-defined implementations and registerable ATP placeholders used by a run.
/// Builtins take precedence and cannot be replaced. Name lookup is ASCII case
/// insensitive; registration changes advance the epoch and invalidate old bindings.
/// Sessions borrow the registry immutably, so it cannot change during execution.
#[derive(Default)]
pub struct ExtensionRegistry {
    epoch: u64,
    next_id: u64,
    entries: BTreeMap<(u8, String), Registration>,
    by_id: BTreeMap<u64, Registration>,
    external_namespace:
        Option<crate::eval::functions::external_namespace::ExternalFunctionNamespace>,
}

fn key(namespace: ExtensionNamespace, name: &str) -> (u8, String) {
    (
        match namespace {
            ExtensionNamespace::UserDefined => 0,
            ExtensionNamespace::AnalysisToolPak => 1,
        },
        name.to_ascii_uppercase(),
    )
}

impl ExtensionRegistry {
    /// Create an empty provider registry with no external namespace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture exact registration contracts and ordered external-name identity.
    pub fn identity(&self) -> ExtensionIdentity {
        ExtensionIdentity {
            epoch: self.epoch,
            external_namespace: self
                .external_namespace
                .as_ref()
                .map(|namespace| namespace.identity()),
            registrations: self
                .entries
                .values()
                .map(|r| r.binding.registration.clone())
                .collect(),
        }
    }

    /// Import an explicit frozen HSSF external-name namespace. Replacements invalidate plans.
    pub fn set_external_namespace(
        &mut self,
        namespace: Option<crate::eval::functions::external_namespace::ExternalFunctionNamespace>,
    ) -> Result<(), ExtensionRegistrationError> {
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(ExtensionRegistrationError::EpochExhausted)?;
        self.external_namespace = namespace;
        self.epoch = epoch;
        Ok(())
    }

    /// Resolve preserved external spelling before ordinary provider lookup.
    pub fn external_name_lookup(
        &self,
        name: &str,
    ) -> crate::eval::functions::external_namespace::ExternalNameLookup<'_> {
        self.external_namespace.as_ref().map_or(
            crate::eval::functions::external_namespace::ExternalNameLookup::Absent,
            |namespace| namespace.lookup(name),
        )
    }

    /// Register a new provider. Names use ASCII letters, digits, `_`, or `.`, and
    /// cannot start with a digit. `implementation_signature` must be nonempty and
    /// identify both code and captured configuration. Builtin/protected names and
    /// duplicate registrations are rejected without mutation.
    pub fn register(
        &mut self,
        namespace: ExtensionNamespace,
        name: &str,
        implementation_signature: &str,
        contract: ExtensionContract,
        function: impl ExtensionFunction + 'static,
    ) -> Result<ExtensionBinding, ExtensionRegistrationError> {
        self.install(
            namespace,
            name,
            implementation_signature,
            contract,
            Arc::new(function),
            false,
        )
    }

    /// Replace an existing provider with a new binding and revision. Already-bound
    /// formulas must be rebound; replacement does not patch their executable identity.
    pub fn replace(
        &mut self,
        namespace: ExtensionNamespace,
        name: &str,
        implementation_signature: &str,
        contract: ExtensionContract,
        function: impl ExtensionFunction + 'static,
    ) -> Result<ExtensionBinding, ExtensionRegistrationError> {
        self.install(
            namespace,
            name,
            implementation_signature,
            contract,
            Arc::new(function),
            true,
        )
    }

    fn install(
        &mut self,
        namespace: ExtensionNamespace,
        name: &str,
        signature: &str,
        contract: ExtensionContract,
        function: Arc<dyn ExtensionFunction>,
        replace: bool,
    ) -> Result<ExtensionBinding, ExtensionRegistrationError> {
        use ExtensionRegistrationError::{
            AlreadyRegistered, Builtin, EpochExhausted, InvalidArity, InvalidName,
            MissingSignature, NotAtpPlaceholder, NotRegistered, ReservedAtpName,
        };
        if name.is_empty()
            || !name.is_ascii()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
            // SAFETY: Short-circuit evaluation returns for an empty name before reading its first byte.
            || name.as_bytes()[0].is_ascii_digit()
        {
            return Err(InvalidName);
        }
        if contract.arity.minimum > contract.arity.maximum {
            return Err(InvalidArity);
        }
        if signature.is_empty() {
            return Err(MissingSignature);
        }
        if crate::eval::functions::registry::resolve_function(
            &poirot_parsing::formula::FunctionName::from(name),
        )
        .is_some()
            || crate::eval::functions::atp::PINNED_ATP_FUNCTIONS
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(name))
        {
            return Err(Builtin);
        }
        if namespace == ExtensionNamespace::UserDefined
            && crate::eval::functions::atp::REGISTERABLE_ATP_PLACEHOLDERS
                .iter()
                .any(|entry| entry.eq_ignore_ascii_case(name))
        {
            return Err(ReservedAtpName);
        }
        if namespace == ExtensionNamespace::AnalysisToolPak
            && !crate::eval::functions::atp::REGISTERABLE_ATP_PLACEHOLDERS.contains(&name)
        {
            return Err(NotAtpPlaceholder);
        }
        let entry_key = key(namespace, name);
        match (self.entries.contains_key(&entry_key), replace) {
            (true, false) => return Err(AlreadyRegistered),
            (false, true) => return Err(NotRegistered),
            _ => {}
        }
        let epoch = self.epoch.checked_add(1).ok_or(EpochExhausted)?;
        let id = self.next_id.checked_add(1).ok_or(EpochExhausted)?;
        let binding = ExtensionBinding {
            id: ExtensionId(id),
            registration: ExtensionRegistrationIdentity {
                binding_id: ExtensionId(id),
                name: entry_key.1.clone(),
                namespace,
                implementation_signature: signature.into(),
                contract,
            },
        };
        if let Some(previous) = self.entries.get(&entry_key) {
            self.by_id.remove(&previous.binding.id.0);
        }
        self.by_id.insert(
            id,
            Registration {
                binding: binding.clone(),
                implementation: Arc::clone(&function),
            },
        );
        self.entries.insert(
            entry_key,
            Registration {
                binding: binding.clone(),
                implementation: function,
            },
        );
        self.epoch = epoch;
        self.next_id = id;
        Ok(binding)
    }

    /// Resolve an ASCII function spelling to its exact registration binding.
    pub fn resolve(&self, name: &str) -> Option<ExtensionBinding> {
        // ATP prefix stripping is case-sensitive, unlike the UDF name lookup.
        let atp = name.strip_prefix("_xlfn.").unwrap_or(name);
        self.entries
            .get(&key(ExtensionNamespace::AnalysisToolPak, atp))
            .or_else(|| {
                self.entries
                    .get(&key(ExtensionNamespace::UserDefined, name))
            })
            .map(|r| r.binding.clone())
    }

    /// Find a provider only when its complete binding still matches this registry.
    pub(in crate::eval) fn implementation(
        &self,
        binding: &ExtensionBinding,
    ) -> Option<Arc<dyn ExtensionFunction>> {
        self.by_id
            .get(&binding.id.0)
            .filter(|r| r.binding == *binding)
            .map(|r| Arc::clone(&r.implementation))
    }
}

impl super::binding_resolver::BindingResolver for ExtensionRegistry {
    fn external_name_lookup<'a>(
        &'a self,
        name: &'a str,
    ) -> crate::eval::functions::external_namespace::ExternalNameLookup<'a> {
        self.external_name_lookup(name)
    }

    fn resolve(
        &self,
        name: &str,
    ) -> Option<crate::eval::functions::extensions::extension_binding::ExtensionBinding> {
        self.resolve(name)
    }
}
