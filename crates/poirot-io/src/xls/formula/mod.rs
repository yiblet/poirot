//! BIFF8 formula token decoding into formula source text.
//!
//! Depends on: `xls::preflight`, `xls::result`, `xls::text`.
mod add_ins;
mod array_constant;
mod decompile;
mod metadata;
mod parsed_formula;

pub(super) use add_ins::AddIns;
#[cfg(test)]
pub(super) use decompile::formula;
pub(super) use decompile::{error_text, formula_with_addins};
pub(super) use parsed_formula::ParsedFormula;
