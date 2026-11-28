/// Handler identity selected during compilation, without retaining a provider object.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CallTarget {
    /// Registered builtin handler.
    Builtin(crate::eval::functions::registry::FunctionId),
    /// Explicit extension registry binding.
    Extension(crate::eval::functions::extensions::ExtensionBinding),
    /// Call resolved at execution against runtime aliases or workbook defined names.
    Unresolved,
}

impl CallTarget {
    /// Resolve direct handlers before considering formula-local or workbook names.
    pub(super) fn resolve<'a>(
        name: &'a str,
        registry: Option<&'a crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> (Option<&'a str>, Self) {
        let lookup = match registry.map(|r| r.external_name_lookup(name)) {
            Some(crate::eval::functions::external_namespace::ExternalNameLookup::Stored(
                stored,
            )) => Some(stored),
            Some(
                crate::eval::functions::external_namespace::ExternalNameLookup::UnsupportedName,
            ) => None,
            _ => Some(name),
        };
        let call_target = match lookup {
            Some(lookup) => match crate::eval::functions::registry::resolve_function(
                &poirot_parsing::formula::FunctionName::from(lookup),
            ) {
                Some(d) => Self::Builtin(d.handler),
                None => registry
                    .and_then(|r| r.resolve(lookup))
                    .map_or(Self::Unresolved, Self::Extension),
            },
            None => Self::Unresolved,
        };
        (lookup, call_target)
    }
}
