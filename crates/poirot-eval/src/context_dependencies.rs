//! Consumed context fields used to validate retained canonical evaluation results.
use crate::ContextFingerprint;

/// Conservative direct context fields actually consumed by a cell.
/// Downstream cells depend on result versions, not an inherited blanket mask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ContextMask {
    /// Captured clock was consumed.
    pub as_of: bool,
    /// Random seed was consumed.
    pub seed: bool,
    /// Fixed local UTC offset was consumed.
    pub utc_offset: bool,
    /// Numeric date ordering was consumed.
    pub date_order: bool,
    /// Workbook date system was consumed.
    pub date_system: bool,
}
/// A context category reported when a previously consumed setting changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContextField {
    /// Captured clock changed.
    AsOf,
    /// Random seed changed.
    Seed,
    /// Fixed UTC offset changed.
    UtcOffset,
    /// Numeric date ordering changed.
    DateOrder,
    /// Workbook date system changed.
    DateSystem,
}
impl ContextMask {
    /// Report changed fields that this cell actually consumed.
    pub fn changed_fields(
        self,
        previous: ContextFingerprint,
        current: ContextFingerprint,
        previous_1904: bool,
        current_1904: bool,
    ) -> Vec<ContextField> {
        let mut fields = Vec::new();
        if self.as_of && previous.as_of_time != current.as_of_time {
            fields.push(ContextField::AsOf);
        }
        if self.seed && previous.randomness_seed != current.randomness_seed {
            fields.push(ContextField::Seed);
        }
        if self.utc_offset
            && previous.localization.utc_offset_seconds != current.localization.utc_offset_seconds
        {
            fields.push(ContextField::UtcOffset);
        }
        if self.date_order && previous.localization.date_order != current.localization.date_order {
            fields.push(ContextField::DateOrder);
        }
        if self.date_system && previous_1904 != current_1904 {
            fields.push(ContextField::DateSystem);
        }
        fields
    }
}
