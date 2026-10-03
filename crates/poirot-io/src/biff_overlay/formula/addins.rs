//! Checked original SUPBOOK/EXTERNNAME identity, restricted to supported add-ins.
use super::super::text::{take, text, word};
use super::super::{PreflightError, ResourceBudget, Result};

/// External-book/name bindings collected from BIFF globals for formula reconstruction.
#[derive(Default)]
pub(in crate::biff_overlay) struct AddIns {
    books: Vec<Book>,
    declarations: usize,
    bytes: usize,
}
/// External-book classification distinguishing add-in names from ordinary workbook links.
enum Book {
    Internal,
    AddIn(Vec<String>),
    External,
}
impl AddIns {
    /// Record supported external-book and name metadata from one BIFF record.
    pub(in crate::biff_overlay) fn observe(
        &mut self,
        id: u16,
        mut data: &[u8],
        budget: ResourceBudget,
    ) -> Result<()> {
        if !matches!(id, 0x1ae | 0x23) {
            return Ok(());
        }
        let declarations = self
            .declarations
            .checked_add(1)
            .ok_or(PreflightError::Representation("add-in count overflow"))?;
        let bytes = self
            .bytes
            .checked_add(data.len())
            .ok_or(PreflightError::Representation("add-in bytes overflow"))?;
        for (resource, requested, limit) in [
            (
                "add-in declarations",
                declarations,
                budget.max_metadata_entries,
            ),
            ("add-in declaration bytes", bytes, budget.max_string_bytes),
        ] {
            if requested > limit {
                return Err(PreflightError::Resource {
                    resource,
                    requested: requested as u64,
                    limit: limit as u64,
                });
            }
        }
        self.declarations = declarations;
        self.bytes = bytes;
        if id == 0x1ae {
            self.books.push(match data {
                [_, _, 1, 4] => Book::Internal,
                [1, 0, 1, 0x3a] => Book::AddIn(Vec::new()),
                _ => Book::External,
            });
        } else if let Some(Book::AddIn(names)) = self.books.last_mut() {
            if take(&mut data, 6)? != [0; 6] {
                return Err(PreflightError::Representation(
                    "unsupported add-in EXTERNNAME flags",
                ));
            }
            let n = *take(&mut data, 1)?
                .first()
                .ok_or(PreflightError::Malformed("missing byte"))? as usize;
            if n > budget.max_string_chars {
                return Err(PreflightError::Resource {
                    resource: "add-in name characters",
                    requested: n as u64,
                    limit: budget.max_string_chars as u64,
                });
            }
            // Reserve UTF16 decoding plus UTF8 projection before either allocation.
            let projection = n
                .checked_mul(5)
                .and_then(|n| self.bytes.checked_add(n))
                .ok_or(PreflightError::Representation("add-in projection overflow"))?;
            if projection > budget.max_string_bytes {
                return Err(PreflightError::Resource {
                    resource: "add-in declaration bytes",
                    requested: projection as u64,
                    limit: budget.max_string_bytes as u64,
                });
            }
            self.bytes = projection;
            let name = text(&mut data, n)?;
            if word(&mut data)? != 2 || data != [0x1c, 0x17] {
                return Err(PreflightError::Representation(
                    "unsupported add-in EXTERNNAME formula",
                ));
            }
            names.push(name);
        }
        Ok(())
    }
    /// Resolve a stored add-in index to its source function spelling.
    pub(in crate::biff_overlay) fn resolve(&self, xti: (u16, u16, u16), slot: u32) -> Result<&str> {
        if xti.1 != 0xfffe || xti.2 != 0xfffe {
            return Err(PreflightError::Representation(
                "external NameX scope unsupported",
            ));
        }
        let Some(Book::AddIn(names)) = self.books.get(xti.0 as usize) else {
            return Err(PreflightError::Representation(
                "external workbook/UDF NameX unsupported",
            ));
        };
        let name = slot
            .checked_sub(1)
            .and_then(|i| names.get(i as usize))
            .ok_or(PreflightError::Malformed("NameX slot"))?;
        if arity(name).is_none() {
            return Err(PreflightError::Representation(
                "unsupported add-in function identity",
            ));
        }
        Ok(name)
    }
}
/// Return the supported argument contract for a reconstructed function token.
pub(in crate::biff_overlay) fn arity(name: &str) -> Option<(usize, usize)> {
    let name = if let Some(suffix) = name.strip_prefix("_xlfn.") {
        if !suffix.eq_ignore_ascii_case("IFERROR") {
            return None;
        }
        suffix
    } else {
        name
    };
    if name.eq_ignore_ascii_case("EDATE") || name.eq_ignore_ascii_case("IFERROR") {
        Some((2, 2))
    } else if name.eq_ignore_ascii_case("YEARFRAC") {
        Some((2, 3))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration(name: &str) -> Vec<u8> {
        let mut data = vec![0; 6];
        data.extend([name.len() as u8, 0]);
        data.extend(name.as_bytes());
        data.extend([2, 0, 0x1c, 0x17]);
        data
    }
    fn plan(name: &str) -> AddIns {
        let mut plan = AddIns::default();
        let budget = ResourceBudget::default();
        plan.observe(0x1ae, &[1, 0, 1, 4], budget).unwrap();
        plan.observe(0x1ae, &[1, 0, 1, 0x3a], budget).unwrap();
        plan.observe(0x23, &declaration(name), budget).unwrap();
        plan
    }
    #[test]
    fn exact_original_identity_and_checked_slots() {
        let identities = plan("edate");
        assert_eq!(identities.resolve((1, 0xfffe, 0xfffe), 1).unwrap(), "edate");
        for (xti, slot) in [
            ((0, 0xfffe, 0xfffe), 1),
            ((1, 0, 0), 1),
            ((1, 0xfffe, 0xfffe), 0),
            ((1, 0xfffe, 0xfffe), 2),
        ] {
            assert!(identities.resolve(xti, slot).is_err());
        }
        assert!(plan("UNKNOWN").resolve((1, 0xfffe, 0xfffe), 1).is_err());
        assert!(plan("_xlfn.EDATE").resolve((1, 0xfffe, 0xfffe), 1).is_err());
    }
    #[test]
    fn bounded_declarations_and_malformed_records_refuse_before_decode() {
        let mut plan = AddIns::default();
        let budget = ResourceBudget {
            max_metadata_entries: 0,
            ..ResourceBudget::default()
        };
        assert!(matches!(
            plan.observe(0x1ae, &[1, 0, 1, 0x3a], budget),
            Err(PreflightError::Resource { .. })
        ));
        let budget = ResourceBudget::default();
        plan.observe(0x1ae, &[1, 0, 1, 0x3a], budget).unwrap();
        assert!(plan.observe(0x23, &[0; 2], budget).is_err());
        let mut plan = AddIns::default();
        plan.observe(0x1ae, &[1, 0, 1, 0x3a], budget).unwrap();
        assert!(matches!(
            plan.observe(
                0x23,
                &declaration("IFERROR"),
                ResourceBudget {
                    max_string_bytes: 10,
                    ..budget
                }
            ),
            Err(PreflightError::Resource { .. })
        ));
    }
    #[test]
    fn original_namex_funcvar_reconstructs_and_rejects_bad_arity_or_value_use() {
        let plan = plan("IFERROR");
        let xtis = [(1, 0xfffe, 0xfffe)];
        let tokens = [
            0x59, 0, 0, 1, 0, 0, 0, 0x1e, 1, 0, 0x1e, 0, 0, 3, 0x1e, 7, 0, 0x42, 3, 255, 0,
        ];
        assert_eq!(
            super::super::formula_with_addins(
                &tokens,
                &[],
                &[],
                &xtis,
                &plan,
                ResourceBudget::default()
            )
            .unwrap(),
            Some("IFERROR(1+0,7)".into())
        );
        let mut wrong = tokens;
        wrong[18] = 2;
        assert!(
            super::super::formula_with_addins(
                &wrong,
                &[],
                &[],
                &xtis,
                &plan,
                ResourceBudget::default()
            )
            .is_err()
        );
        assert!(
            super::super::formula_with_addins(
                &tokens[..7],
                &[],
                &[],
                &xtis,
                &plan,
                ResourceBudget::default()
            )
            .is_err()
        );
    }
}
