//! Borrowed Serde view of a workbook plus its sparse overlay state.
use super::borrowed_source::BorrowedSource;
use super::incremental::Overlay;
use super::overlay_error::OverlayError;
use super::snapshot_baseline::SnapshotBaseline;
use super::snapshot_payload::{SnapshotPayload, VERSION};
use super::snapshot_state::SnapshotState;
use super::source::Source;
use super::workspace_state::WorkspaceState;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::inputs::CellEdit;
use crate::eval::reuse::{Cache, Cached, Key};
use crate::model::{CellAddress, CellValue, Workbook};
use serde::{Deserialize, Serialize, Serializer, ser::Error as _};
use std::borrow::{Borrow, Cow};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Compatibility revision for persisted engine preparation and retained results.
/// Bump when wire layout or execution semantics invalidate prepared snapshots,
/// including changes that leave Serde field layouts intact. This is separate from
/// durable workbook-source versioning owned by the public file envelope.
pub const COMPATIBILITY_REVISION: u64 = 78;

/// Serde view of a baseline workbook and its sparse overlay state.
pub struct Snapshot<'a> {
    pub(super) state: SnapshotState<'a>,
}

impl<'a> Snapshot<'a> {
    /// Borrow overlay state without requiring the overlay itself to implement Serde.
    pub(super) fn borrowed<W: Borrow<Workbook>>(
        source: &'a Source<'_, W>,
        cache: Option<&'a RefCell<Cache>>,
    ) -> Self {
        Self {
            state: SnapshotState::Borrowed(BorrowedSource {
                workbook: source.workbook(),
                inputs: &source.inputs,
                compiler: &source.compiler,
                spill_candidates: &source.spill_candidates,
                policy: &source.policy,
                preparation: &source.preparation,
                cache,
            }),
        }
    }

    /// Capture owned serialized state that can outlive the source overlay.
    pub fn into_owned(self) -> Result<Snapshot<'static>, OverlayError> {
        let payload = match self.state {
            SnapshotState::Owned(payload) => payload,
            SnapshotState::Borrowed(overlay) => {
                let empty = Cache::default();
                let cache = overlay
                    .cache
                    .map(|c| c.try_borrow())
                    .transpose()
                    .map_err(|e| OverlayError::Snapshot(e.to_string()))?;
                Box::new(
                    SnapshotPayload::capture(
                        &overlay,
                        &*overlay
                            .compiler
                            .try_borrow()
                            .map_err(|e| OverlayError::Snapshot(e.to_string()))?,
                        cache.as_deref().unwrap_or(&empty),
                        SnapshotBaseline::Model(overlay.workbook.snapshot()),
                    )?
                    .into_owned(),
                )
            }
        };
        Ok(Snapshot {
            state: SnapshotState::Owned(payload),
        })
    }

    /// Materialize the effective authored source written beside prepared state.
    /// A borrowed overlay yields its own workbook when unedited, otherwise one clone with
    /// its edits applied; an owned payload is restored and validated first.
    pub(in crate::eval) fn effective_source(self) -> Result<Cow<'a, Workbook>, OverlayError> {
        match self.state {
            SnapshotState::Borrowed(overlay) if overlay.inputs.is_empty() => {
                Ok(Cow::Borrowed(overlay.workbook))
            }
            SnapshotState::Borrowed(overlay) => Ok(Cow::Owned(
                overlay
                    .workbook
                    .with_changes(overlay.inputs.workbook_changes())?,
            )),
            SnapshotState::Owned(_) => {
                let (baseline, state) = self.restore()?;
                Ok(Cow::Owned(
                    baseline.with_changes(state.inputs.workbook_changes())?,
                ))
            }
        }
    }

    /// Serialize preparation for a checkpoint whose source section holds the effective
    /// workbook. An unedited borrowed overlay omits its baseline, which equals that source.
    pub(in crate::eval) fn serialize_prepared<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        self.serialize_payload(serializer, true)
    }

    /// Serialize the payload, storing the baseline model unless `omit_unedited_baseline`
    /// is set and the borrowed overlay has no authored edits.
    fn serialize_payload<S: Serializer>(
        &self,
        serializer: S,
        omit_unedited_baseline: bool,
    ) -> Result<S::Ok, S::Error> {
        match &self.state {
            SnapshotState::Owned(payload) => payload.serialize(serializer),
            SnapshotState::Borrowed(overlay) => {
                let compiler = overlay.compiler.try_borrow().map_err(S::Error::custom)?;
                let empty = Cache::default();
                let cache = overlay
                    .cache
                    .map(|c| c.try_borrow())
                    .transpose()
                    .map_err(S::Error::custom)?;
                let baseline = if omit_unedited_baseline && overlay.inputs.is_empty() {
                    SnapshotBaseline::Source
                } else {
                    SnapshotBaseline::Model(overlay.workbook.snapshot())
                };
                SnapshotPayload::capture(
                    overlay,
                    &compiler,
                    cache.as_deref().unwrap_or(&empty),
                    baseline,
                )
                .map_err(S::Error::custom)?
                .serialize(serializer)
            }
        }
    }

    /// Restore a detached snapshot, which stores its own baseline workbook.
    pub(super) fn restore(self) -> Result<(Workbook, WorkspaceState), OverlayError> {
        let mut payload = self.into_payload()?;
        let SnapshotBaseline::Model(model) =
            std::mem::replace(&mut payload.baseline, SnapshotBaseline::Source)
        else {
            return Err(OverlayError::Snapshot(
                "snapshot baseline belongs to a checkpoint source section".into(),
            ));
        };
        let workbook = model.restore()?;
        let encoded = WorkspaceState::encode_baseline(&workbook)?;
        Self::rebuild(payload, workbook, encoded)
    }

    /// Restore checkpoint preparation beside its decoded effective `source` workbook.
    /// `encoded_source` must be the postcard model snapshot `source` was decoded from.
    /// An unedited checkpoint's source is its baseline; a stored baseline must reproduce
    /// `source` once the checkpoint's edits are applied.
    pub(in crate::eval) fn restore_with_source(
        self,
        source: Workbook,
        encoded_source: &[u8],
    ) -> Result<(Workbook, WorkspaceState), OverlayError> {
        let mut payload = self.into_payload()?;
        match std::mem::replace(&mut payload.baseline, SnapshotBaseline::Source) {
            SnapshotBaseline::Source => {
                if !payload.writes.is_empty()
                    || !payload.deleted.is_empty()
                    || !payload.style_only.is_empty()
                {
                    return Err(OverlayError::Snapshot(
                        "edited checkpoint lacks its baseline".into(),
                    ));
                }
                Self::rebuild(payload, source, encoded_source.to_vec())
            }
            SnapshotBaseline::Model(model) => {
                let workbook = model.restore()?;
                let encoded = WorkspaceState::encode_baseline(&workbook)?;
                let (baseline, state) = Self::rebuild(payload, workbook, encoded)?;
                let effective = baseline.with_changes(state.inputs.workbook_changes())?;
                if !source.snapshot().matches(&effective)
                    || source.file_metadata != effective.file_metadata
                {
                    return Err(OverlayError::Snapshot(
                        "source and prepared state disagree".into(),
                    ));
                }
                Ok((baseline, state))
            }
        }
    }

    /// Own the wire payload and check its version and parser policy.
    fn into_payload(self) -> Result<SnapshotPayload<'static>, OverlayError> {
        let SnapshotState::Owned(payload) = self.into_owned()?.state else {
            return Err(OverlayError::Snapshot(
                "snapshot ownership conversion failed".into(),
            ));
        };
        let payload = *payload;
        if payload.version != VERSION {
            return Err(OverlayError::Snapshot(
                "unsupported snapshot version".into(),
            ));
        }
        if payload.preparation.parse != payload.policy.parse {
            return Err(OverlayError::Snapshot("parser policy mismatch".into()));
        }
        Ok(payload)
    }

    /// Validate serialized identities against `workbook` and rebuild runtime caches and
    /// dependency indices. `baseline` is the encoded form of `workbook`.
    fn rebuild(
        payload: SnapshotPayload<'static>,
        workbook: Workbook,
        baseline: Vec<u8>,
    ) -> Result<(Workbook, WorkspaceState), OverlayError> {
        // Typed file decoding owns optional quotas. Restoration validates source,
        // ownership and cache evidence without imposing another hidden size cap.
        let policy = payload.policy.into_owned();
        let preparation = payload.preparation.into_owned();
        let compiler = payload
            .compiler
            .restore(&preparation)
            .map_err(OverlayError::Snapshot)?;
        let mut overlay = Overlay {
            source: Source {
                workbook,
                inputs: crate::eval::inputs::RunInputs::empty(),
                compiler: RefCell::new(compiler),
                spill_candidates: Default::default(),
                policy,
                preparation,
                extensions: None,
            },
            cache: RefCell::new(Cache::default()),
        };
        let mut previous_prefix: Option<(usize, CellAddress)> = None;
        for &(sheet, address) in &payload.inspected_prefixes {
            if previous_prefix.is_some_and(|(prior_sheet, prior)| {
                sheet < prior_sheet
                    || (sheet == prior_sheet
                        && (address.col <= prior.col || address.row >= prior.row))
            }) {
                return Err(OverlayError::Snapshot(
                    "noncanonical source discovery prefixes".into(),
                ));
            }
            previous_prefix = Some((sheet, address));
            overlay
                .source
                .validate_target(EvaluationTarget { sheet, address })?;
        }
        for (sheet, address, cell) in payload.writes {
            let target = EvaluationTarget { sheet, address };
            overlay.source.validate_target(target)?;
            if overlay
                .source
                .inputs
                .sheets
                .entry(sheet)
                .or_default()
                .set_cell(address, cell.into_owned(), false)?
            {
                return Err(OverlayError::Snapshot("duplicate overlay write".into()));
            }
        }
        for &(sheet, address) in payload.deleted.as_ref() {
            overlay
                .source
                .validate_target(EvaluationTarget { sheet, address })?;
            if overlay.source.inputs.cell(sheet, address).is_some() {
                return Err(OverlayError::Snapshot("write and removal overlap".into()));
            }
            overlay
                .source
                .inputs
                .sheets
                .entry(sheet)
                .or_default()
                .set(address, CellEdit::Delete)?;
        }
        for &(sheet, address) in payload.style_only.as_ref() {
            let cell = overlay.source.inputs.cell(sheet, address).ok_or_else(|| {
                OverlayError::Snapshot("style-only marker lacks an override".into())
            })?;
            let baseline = overlay
                .source
                .workbook
                .sheet(sheet)
                .and_then(|s| s.source_cell(address));
            if !match baseline {
                Some(base) => {
                    base.formula == cell.formula
                        && crate::eval::ports::same_result_identity(&base.value, &cell.value)
                }
                None => cell.formula.is_none() && cell.value == CellValue::Blank,
            } {
                return Err(OverlayError::Snapshot(
                    "style-only marker changes source content".into(),
                ));
            }
            if let Some(edit) = overlay
                .source
                .inputs
                .sheets
                .get_mut(&sheet)
                .and_then(|cells| cells.get_mut(&address))
            {
                // The validation above proved each marker contains a cell.
                let previous = std::mem::replace(edit, CellEdit::Delete);
                *edit = match previous {
                    CellEdit::Replace(cell) => CellEdit::StyleOnly(cell),
                    other => other,
                };
            }
        }
        {
            let cache = overlay.cache.get_mut();
            cache.context = payload.context.map(Cow::into_owned);
            cache.spill_resets = payload.spill_resets.into_owned();
            for &(sheet, address) in &cache.spill_resets {
                overlay
                    .source
                    .validate_target(EvaluationTarget { sheet, address })?;
            }
            cache.lookup_prefixes = payload
                .lookup_prefixes
                .into_owned()
                .restore(overlay.source.workbook())
                .map_err(OverlayError::Snapshot)?;
            if (!payload.cached.is_empty() || !cache.lookup_prefixes.is_empty())
                && cache.context.is_none()
            {
                return Err(OverlayError::Snapshot(
                    "cached results lack execution context".into(),
                ));
            }
            cache.entries.reserve(payload.cached.len());
            for entry in payload.cached {
                overlay.source.validate_target(EvaluationTarget {
                    sheet: entry.key.0,
                    address: entry.key.1,
                })?;
                for &(sheet, address) in &entry.dependencies.cells {
                    overlay
                        .source
                        .validate_target(EvaluationTarget { sheet, address })?;
                }
                for range in entry
                    .dependencies
                    .ranges
                    .iter()
                    .chain(&entry.dependencies.point_runs)
                    .chain(&entry.dependencies.spill_ranges)
                    .chain(entry.dependencies.value_reads.iter())
                {
                    overlay.source.validate_target(EvaluationTarget {
                        sheet: range.sheet,
                        address: range.first,
                    })?;
                    crate::model::query::RangeMembership::new(
                        range.sheet,
                        range.first,
                        range.last,
                    )?;
                }
                if cache.get(entry.key).is_some() {
                    return Err(OverlayError::Snapshot("duplicate cached cell".into()));
                }
                cache.insert(
                    entry.key,
                    Cached {
                        version: 0,
                        validity: crate::eval::reuse::Validity::Clean,
                        value: Arc::new(entry.value.restore()),
                        dependencies: Box::new(entry.dependencies.into_owned()),
                    },
                )?;
            }
            let mut footprints = BTreeMap::<Key, usize>::new();
            for (&(sheet, address), entry) in &cache.entries {
                if let Some(owner) = entry.dependencies.spill.owner(address) {
                    if overlay
                        .source
                        .effective()
                        .cell(sheet, owner)
                        .is_none_or(|cell| cell.formula.is_none())
                    {
                        return Err(OverlayError::Snapshot(
                            "spill producer lacks source formula".into(),
                        ));
                    }
                    let key = (sheet, owner);
                    let root = cache.get(key).ok_or_else(|| {
                        OverlayError::Snapshot("spill child lacks its producer".into())
                    })?;
                    let last = root.dependencies.spill.last().ok_or_else(|| {
                        OverlayError::Snapshot("spill producer lacks footprint bounds".into())
                    })?;
                    if root.dependencies.spill.owner(owner) != Some(owner)
                        || owner.row > address.row
                        || owner.col > address.col
                        || address.row > last.row
                        || address.col > last.col
                    {
                        return Err(OverlayError::Snapshot(
                            "invalid cached spill ownership".into(),
                        ));
                    }
                    *footprints.entry(key).or_default() += 1;
                }
            }
            for ((sheet, owner), count) in footprints {
                let last = cache
                    .get((sheet, owner))
                    .and_then(|entry| entry.dependencies.spill.last())
                    .ok_or_else(|| OverlayError::Snapshot("missing cached spill owner".into()))?;
                last.validate()?;
                let expected = (u64::from(last.row) - u64::from(owner.row) + 1)
                    * (u64::from(last.col) - u64::from(owner.col) + 1);
                if expected != count as u64 {
                    return Err(OverlayError::Snapshot(
                        "incomplete cached spill footprint".into(),
                    ));
                }
            }
            // Range indexes stay marked dirty and are built on the first invalidation.
        }
        let Source {
            workbook,
            inputs,
            compiler,
            policy,
            preparation,
            ..
        } = overlay.source;
        Ok((
            workbook,
            WorkspaceState {
                baseline,
                inputs,
                compiler: compiler.into_inner(),
                inspected_prefixes: payload.inspected_prefixes,
                cache: overlay.cache.into_inner(),
                policy,
                preparation,
            },
        ))
    }
}

impl Serialize for Snapshot<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_payload(serializer, false)
    }
}

impl<'de, 'a> Deserialize<'de> for Snapshot<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            state: SnapshotState::Owned(Box::new(SnapshotPayload::<'static>::deserialize(
                deserializer,
            )?)),
        })
    }
}
