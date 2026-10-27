use super::call::Call;
use super::operand_cursor::OperandCursor;
use super::template::Template;
use crate::eval::limits::{EvalResult, EvaluationFailure};

#[derive(Clone, Debug, PartialEq)]
/// A template with resolved builtin/extension call identities. Programs can be
/// shared only when this complete binding identity agrees, not just their opcodes.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Executable {
    /// Possible root and intermediate properties inferred during preparation.
    pub typing: crate::eval::compile::typing::FormulaTyping,
    /// Shared instruction and literal layout.
    pub template: Template,
    /// Bound call identities indexed by invocation instructions.
    pub calls: Vec<Call>,
    /// Exact registry identity required by extension bindings.
    pub extension_identity: Option<crate::eval::functions::extensions::ExtensionIdentity>,
}

impl Executable {
    /// Borrow a checked UTF-8 span from operand storage.
    pub(in crate::eval) fn text(&self, offset: u32, length: u32) -> EvalResult<&str> {
        let end = offset
            .checked_add(length)
            .ok_or(EvaluationFailure::StalePlan)?;
        let bytes = self
            .template
            .operands
            .get(offset as usize..end as usize)
            .ok_or(EvaluationFailure::StalePlan)?;
        std::str::from_utf8(bytes).map_err(|_| EvaluationFailure::StalePlan)
    }

    /// Borrow a call's parsed function spelling, rejecting invalid packed storage.
    pub fn source_spelling(&self, index: usize) -> EvalResult<&str> {
        let call = self.calls.get(index).ok_or(EvaluationFailure::StalePlan)?;
        let bytes = self
            .template
            .operands
            .get(call.source_offset as usize..)
            .ok_or(EvaluationFailure::StalePlan)?;
        OperandCursor(bytes).string()
    }

    /// Borrow the exact lookup spelling, rejecting an invalid call index.
    pub fn resolved_spelling(&self, index: usize) -> EvalResult<&str> {
        let call = self.calls.get(index).ok_or(EvaluationFailure::StalePlan)?;
        match call.resolved_name.as_deref() {
            Some(name) => Ok(name),
            None => self.source_spelling(index),
        }
    }
}

impl crate::eval::functions::extensions::BindingResolver for Executable {
    fn external_name_lookup<'a>(
        &'a self,
        name: &'a str,
    ) -> crate::eval::functions::external_namespace::ExternalNameLookup<'a> {
        use crate::eval::functions::external_namespace::ExternalNameLookup;
        self.calls
            .iter()
            .find_map(|call| {
                let start = call.source_offset as usize;
                let header_end = start.checked_add(4)?;
                let length = u32::from_le_bytes(
                    self.template
                        .operands
                        .get(start..header_end)?
                        .try_into()
                        .ok()?,
                ) as usize;
                let source = self
                    .template
                    .operands
                    .get(header_end..header_end.checked_add(length)?)?;
                (source == name.as_bytes()).then(|| {
                    if matches!(
                        call.target,
                        crate::eval::compile::bytecode::call_target::CallTarget::Unresolved
                    ) && call.resolved_name.is_none()
                    {
                        ExternalNameLookup::UnsupportedName
                    } else {
                        ExternalNameLookup::Stored(call.resolved_name.as_deref().unwrap_or(name))
                    }
                })
            })
            .unwrap_or(ExternalNameLookup::UnsupportedName)
    }

    fn resolve(&self, name: &str) -> Option<crate::eval::functions::extensions::ExtensionBinding> {
        self.calls.iter().find_map(|call| match &call.target {
            crate::eval::compile::bytecode::call_target::CallTarget::Extension(binding) => {
                let start = call.source_offset as usize;
                let header_end = start.checked_add(4)?;
                let length = u32::from_le_bytes(
                    self.template
                        .operands
                        .get(start..header_end)?
                        .try_into()
                        .ok()?,
                ) as usize;
                let source_name = std::str::from_utf8(
                    self.template
                        .operands
                        .get(header_end..header_end.checked_add(length)?)?,
                )
                .ok()?;
                (call.resolved_name.as_deref().unwrap_or(source_name) == name)
                    .then(|| binding.clone())
            }
            _ => None,
        })
    }
}
