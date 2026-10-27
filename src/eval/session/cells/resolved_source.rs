//! Authored cell content resolved through scenario overrides.

use crate::eval::config::ArrayResultPolicy;
use crate::eval::session::composition::Session;
use crate::model::CellAddress;
use crate::model::CellValue::Blank;

#[cfg(test)]
thread_local! {
    /// Count source resolution attempts independently on each test thread.
    pub(in crate::eval) static SOURCE_RESOLUTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Authored cell content resolved through scenario overrides before calculation.
/// Its incoming-spill flag tells cell evaluation whether a blank or saved child
/// may need an array producer to establish its current value.
#[derive(Clone, Copy)]
pub(in crate::eval) struct ResolvedSource<'a> {
    /// Authored content stays immutable while the execution publishes derived values.
    pub(in crate::eval) cell: Option<&'a crate::model::Cell>,
    /// Dynamic spill resolution is relevant to blank coordinates and saved children.
    pub(in crate::eval) accepts_spill: bool,
}

impl<'a> ResolvedSource<'a> {
    /// Resolve immutable source and saved ownership once for an ordinary or range read.
    pub(in crate::eval) fn resolve(
        session: &Session<'a>,
        sheet: usize,
        address: CellAddress,
    ) -> Option<Self> {
        #[cfg(test)]
        SOURCE_RESOLUTIONS.with(|count| count.set(count.get().saturating_add(1)));
        if address.validate().is_err() {
            return None;
        }
        let worksheet = session.sources.workbook.sheet(sheet)?;
        let cell = crate::eval::inputs::EffectiveWorkbook::new(
            session.sources.workbook,
            session.sources.inputs,
        )
        .cell(sheet, address);
        Some(ResolvedSource::new(
            worksheet,
            address,
            cell,
            session.policies.array_result_policy,
        ))
    }

    /// Classify a borrowed authored cell using the ordinary saved-spill eligibility rule.
    pub(in crate::eval) fn from_cell(
        session: &Session<'a>,
        sheet: usize,
        address: CellAddress,
        cell: Option<&'a crate::model::Cell>,
    ) -> Option<Self> {
        if address.validate().is_err() {
            return None;
        }
        let worksheet = session.sources.workbook.sheet(sheet)?;
        Some(ResolvedSource::new(
            worksheet,
            address,
            cell,
            session.policies.array_result_policy,
        ))
    }

    /// Classify incoming ownership from already selected immutable worksheet content.
    fn new(
        worksheet: &crate::model::Worksheet,
        address: CellAddress,
        cell: Option<&'a crate::model::Cell>,
        policy: ArrayResultPolicy,
    ) -> Self {
        Self {
            cell,
            accepts_spill: policy == ArrayResultPolicy::DynamicSpill
                && (cell.is_none_or(|cell| cell.formula.is_none() && cell.value == Blank)
                    || worksheet.spill_owner(address).is_some()),
        }
    }
}
