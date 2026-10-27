/// Binding-only resolver permits lazy compatibility trees without provider capture.
pub(in crate::eval) trait BindingResolver {
    /// Recover preserved external spelling before extension lookup.
    fn external_name_lookup<'a>(
        &'a self,
        name: &'a str,
    ) -> crate::eval::functions::external_namespace::ExternalNameLookup<'a>;

    /// Resolve an exact extension binding without capturing its provider.
    fn resolve(&self, name: &str) -> Option<super::extension_binding::ExtensionBinding>;
}
