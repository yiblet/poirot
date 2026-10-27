use super::borrowed_source::BorrowedSource;
use super::cached_value::CachedValue;
use super::overlay_error::OverlayError;
use super::snapshot_baseline::SnapshotBaseline;
use crate::eval::compile::compiler::FormulaCompiler;
use crate::eval::config::{ContextKey, EvaluationPolicy};
use crate::eval::inputs::CellEdit;
use crate::eval::reuse::{Cache, Key};
use crate::model::{Cell, CellAddress};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Wire version of the serialized payload layout.
pub(super) const VERSION: u32 = 82;

/// Versioned wire payload; runtime providers are reattached during restoration.
#[derive(Serialize, Deserialize)]
pub(super) struct SnapshotPayload<'a> {
    pub(super) version: u32,
    pub(super) baseline: SnapshotBaseline<'a>,
    pub(super) policy: Cow<'a, EvaluationPolicy>,
    pub(super) preparation: Cow<'a, crate::eval::compile::compiler::PreparationPolicy>,
    pub(super) compiler: crate::eval::compile::compiler::cache_snapshot::CacheSnapshot<'a>,
    pub(super) writes: Vec<(usize, CellAddress, Cow<'a, Cell>)>,
    pub(super) deleted: Cow<'a, std::collections::BTreeSet<Key>>,
    pub(super) style_only: Cow<'a, std::collections::BTreeSet<Key>>,
    pub(super) cached: Vec<CachedValue<'a>>,
    pub(super) context: Option<Cow<'a, ContextKey>>,
    pub(super) spill_resets: Cow<'a, std::collections::BTreeSet<Key>>,
    pub(super) inspected_prefixes: Vec<Key>,
    pub(super) lookup_prefixes: Cow<'a, crate::eval::reuse::LookupPrefixes>,
}

impl<'a> SnapshotPayload<'a> {
    /// Capture borrowed overlay state; `baseline` says where restoration finds the workbook.
    pub(super) fn capture(
        overlay: &BorrowedSource<'a>,
        compiler: &'a FormulaCompiler,
        cache: &'a Cache,
        baseline: SnapshotBaseline<'a>,
    ) -> Result<Self, OverlayError> {
        let mut cached = cache
            .entries
            .iter()
            .filter(|(_, entry)| entry.validity.is_clean())
            .map(|(key, entry)| CachedValue {
                key: *key,
                value: crate::model::snapshot::Value::capture(&entry.value),
                dependencies: Cow::Borrowed(&entry.dependencies),
            })
            .collect::<Vec<_>>();
        cached.sort_unstable_by_key(|entry| entry.key);
        Ok(Self {
            version: VERSION,
            baseline,
            policy: Cow::Borrowed(overlay.policy),
            preparation: Cow::Borrowed(overlay.preparation),
            compiler: crate::eval::compile::compiler::cache_snapshot::CacheSnapshot::capture(
                compiler,
                overlay.preparation,
            )
            .map_err(|error| OverlayError::Snapshot(error.into()))?,
            // Preserve the existing wire layout while deriving its disjoint views
            // from one runtime collection. Only serialization allocates marker sets.
            writes: overlay
                .inputs
                .edits()
                .filter_map(|(sheet, at, edit)| {
                    edit.cell().map(|cell| (sheet, at, Cow::Borrowed(cell)))
                })
                .collect(),
            deleted: Cow::Owned(
                overlay
                    .inputs
                    .edits()
                    .filter_map(|(sheet, at, edit)| {
                        matches!(edit, CellEdit::Delete).then_some((sheet, at))
                    })
                    .collect(),
            ),
            style_only: Cow::Owned(
                overlay
                    .inputs
                    .edits()
                    .filter_map(|(sheet, at, edit)| edit.is_style_only().then_some((sheet, at)))
                    .collect(),
            ),
            cached,
            context: cache.context.as_ref().map(Cow::Borrowed),
            spill_resets: Cow::Borrowed(&cache.spill_resets),
            lookup_prefixes: Cow::Borrowed(&cache.lookup_prefixes),
            inspected_prefixes: crate::eval::spill::inspected_prefixes(
                &*overlay
                    .spill_candidates
                    .try_borrow()
                    .map_err(|e| OverlayError::Snapshot(e.into()))?,
            ),
        })
    }

    /// Copy borrowed payloads so the snapshot can outlive its overlay.
    pub(super) fn into_owned(self) -> SnapshotPayload<'static> {
        SnapshotPayload {
            version: self.version,
            baseline: match self.baseline {
                SnapshotBaseline::Model(model) => SnapshotBaseline::Model(model.into_owned()),
                SnapshotBaseline::Source => SnapshotBaseline::Source,
            },
            policy: Cow::Owned(self.policy.into_owned()),
            preparation: Cow::Owned(self.preparation.into_owned()),
            compiler: self.compiler.into_owned(),
            writes: self
                .writes
                .into_iter()
                .map(|(sheet, at, cell)| (sheet, at, Cow::Owned(cell.into_owned())))
                .collect(),
            deleted: Cow::Owned(self.deleted.into_owned()),
            style_only: Cow::Owned(self.style_only.into_owned()),
            cached: self
                .cached
                .into_iter()
                .map(|c| CachedValue {
                    key: c.key,
                    value: c.value.into_owned(),
                    dependencies: Cow::Owned(c.dependencies.into_owned()),
                })
                .collect(),
            context: self.context.map(|c| Cow::Owned(c.into_owned())),
            spill_resets: Cow::Owned(self.spill_resets.into_owned()),
            inspected_prefixes: self.inspected_prefixes,
            lookup_prefixes: Cow::Owned(self.lookup_prefixes.into_owned()),
        }
    }
}
