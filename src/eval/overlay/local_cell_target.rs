//! Pure ingress parsing; workbook-dependent identifier lookup stays in the shell.
use crate::model;
use crate::model::ModelError;
use poirot_parsing::formula;
use std::borrow::Cow;

/// Parsed local sheet name and address, before workbook lookup.
pub(super) struct LocalCellTarget<'a> {
    /// Sheet spelling, decoded when quoted.
    pub sheet: Cow<'a, str>,
    /// Validated single-cell address.
    pub address: model::CellAddress,
}

impl<'a> LocalCellTarget<'a> {
    /// Parse one sheet-qualified cell, rejecting ranges and external references.
    pub fn parse(reference: &'a str) -> Result<Self, ModelError> {
        let invalid = || ModelError("target must be one local sheet-qualified cell".into());
        if reference.starts_with('=') {
            return Err(invalid());
        }
        let (sheet, cell) = reference.rsplit_once('!').ok_or_else(invalid)?;
        // The bounded model parser rejects cell whitespace, expressions and ranges.
        let address = cell.parse::<model::CellAddress>()?;
        let sheet = if sheet.starts_with('\'') {
            // Avoid importing formula conveniences (equals, whitespace, grouping).
            if !sheet.ends_with('\'') {
                return Err(invalid());
            }
            let formula::Expr::Cell(parsed) =
                formula::parse_formula(reference).map_err(|_| invalid())?
            else {
                return Err(invalid());
            };
            let decoded = parsed.sheet.ok_or_else(invalid)?;
            if parsed.row != address.row || parsed.column != address.col {
                return Err(invalid());
            }
            Cow::Owned(decoded)
        } else {
            // Preserve legacy literal unquoted names, including embedded/edge spaces.
            Cow::Borrowed(sheet)
        };
        // Workbook names cannot contain these characters: exclude external books
        // and sheet spans even when their spelling is quoted.
        if sheet.is_empty() || sheet.contains(['[', ']', ':', '/', '\\', '?', '*']) {
            return Err(invalid());
        }
        Ok(Self { sheet, address })
    }
}
