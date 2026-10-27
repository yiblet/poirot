use super::potential_qualified_name::PotentialQualifiedName;
use super::potential_reference::PotentialReference;
use crate::eval::compile::bytecode::ReferenceBindings;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// One source's program index, concrete references, and conservative dependency guards.
#[derive(Serialize, Deserialize)]
pub(super) struct SourceDto<'a> {
    pub(super) source: Cow<'a, str>,
    pub(super) program: usize,
    pub(super) references: Cow<'a, ReferenceBindings>,
    pub(super) potential_references: Cow<'a, [PotentialReference]>,
    pub(super) names: Cow<'a, [String]>,
    pub(super) qualified_names: Cow<'a, [PotentialQualifiedName]>,
    pub(super) opaque_functions: Cow<'a, [String]>,
    pub(super) dynamic_references: bool,
}
