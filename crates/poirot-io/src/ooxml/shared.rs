//! Verified serialization of manually supplied shared-formula provenance.
use crate::IoError;
use crate::data::SheetSource;
use crate::{Cell, CellAddress, SharedFormulaId};
use poirot_parsing::formula::{FormulaLocation, TranslationPlan, TranslationPolicy};
use std::collections::BTreeMap;

fn location(address: CellAddress) -> FormulaLocation {
    FormulaLocation {
        row: address.row,
        column: address.col,
    }
}
/// Proven serialization role for a live shared-formula member.
pub(super) enum SharedCell {
    /// Current deterministic master and the bounds of actual surviving members.
    Master {
        index: u32,
        first: CellAddress,
        last: CellAddress,
    },
    /// Follower whose complete source was checked against the current master.
    Follower { index: u32 },
}
/// Immutable per-sheet proof allowing shared follower text to be omitted safely.
#[derive(Default)]
pub(super) struct SharedExport {
    members: BTreeMap<CellAddress, SharedCell>,
}
impl SharedExport {
    /// Verify authoritative sources and disjoint geometry once before both write passes.
    pub(super) fn new(sheet: &impl SheetSource) -> Result<Self, IoError> {
        let spills = super::spill::bounds(sheet)?;
        let mut groups =
            BTreeMap::<(SharedFormulaId, CellAddress), Vec<(CellAddress, &Cell)>>::new();
        for (address, cell) in sheet.cells() {
            if let Some(shared) = cell.shared_formula {
                groups
                    .entry((shared.group, shared.origin))
                    .or_default()
                    .push((*address, cell));
            }
        }
        let mut result = Self::default();
        let mut ranges = spills.into_iter().collect::<Vec<_>>();
        for (index, ((_, origin), members)) in groups.into_iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let Some(&(master, master_cell)) = members.first() else {
                continue;
            };
            let Some(source) = master_cell.formula.as_deref() else {
                continue;
            };
            if members.len() < 2
                || origin.validate().is_err()
                || members.iter().any(|(address, cell)| {
                    sheet.spill_owner(*address).is_some()
                        || cell.formula.as_deref().is_none_or(|formula| {
                            !TranslationPlan::new(
                                source,
                                location(master),
                                location(*address),
                                TranslationPolicy::default(),
                            )
                            .is_ok_and(|plan| plan.replacement() == formula)
                        })
                })
            {
                continue;
            }
            let (mut first, mut last) = (master, master);
            for (address, _) in &members {
                first.row = first.row.min(address.row);
                first.col = first.col.min(address.col);
                last.row = last.row.max(address.row);
                last.col = last.col.max(address.col);
            }
            ranges.push((first, last));
            for (address, _) in members {
                let role = if address == master {
                    SharedCell::Master { index, first, last }
                } else {
                    SharedCell::Follower { index }
                };
                result.members.insert(address, role);
            }
        }
        // Import may tolerate overlapping declared rectangles by actual membership.
        // Output must not create ambiguous shared rectangles or claim array caches.
        // Conservative sheet-wide fallback preserves every authoritative formula.
        if super::spill::validate_disjoint_ranges(&mut ranges).is_err() {
            result.members.clear();
        }
        Ok(result)
    }
    /// Borrow the verified shared role, or use ordinary source serialization.
    pub(super) fn get(&self, address: CellAddress) -> Option<&SharedCell> {
        self.members.get(&address)
    }
}
