use super::call_dto::CallDto;
use crate::eval::compile::bytecode::Template;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Shared bytecode template and serialized call bindings.
#[derive(Serialize, Deserialize)]
pub(super) struct ProgramDto<'a> {
    pub(super) typing: Cow<'a, crate::eval::compile::typing::FormulaTyping>,
    pub(super) template: Cow<'a, Template>,
    pub(super) calls: Vec<CallDto<'a>>,
}
