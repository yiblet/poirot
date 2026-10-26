//! Borrowed Serde view of a workbook plus its sparse overlay state.
use super::*;
use crate::run_overlay::CellEdit;
use serde::{Deserialize, Serialize, ser::Error as _};
use std::borrow::Cow;
const VERSION: u32 = 76;
/// Compatibility revision for persisted engine preparation and retained results.
/// Bump when wire layout or execution semantics invalidate prepared snapshots,
/// including changes that leave Serde field layouts intact. This is separate from
/// durable workbook-source versioning owned by the public file envelope.
pub const COMPATIBILITY_REVISION: u64 = 74;

/// Serde view of a baseline workbook and its sparse overlay state.
pub struct Snapshot<'a> {
    state: State<'a>,
}
/// Borrowed runtime view or independently owned wire payload.
enum State<'a> {
    Borrowed(BorrowedSource<'a>),
    Owned(Box<Payload<'static>>),
}
/// Type-erased borrowed source; snapshots do not depend on the workbook handle type.
struct BorrowedSource<'a> {
    workbook: &'a Workbook,
    inputs: &'a crate::run_overlay::RunInputs,
    compiler: &'a RefCell<FormulaCompiler>,
    policy: &'a OverlayPolicy,
    preparation: &'a crate::EvaluationPolicy,
    cache: Option<&'a RefCell<Cache>>,
}

/// Validated detached source and optional incremental results, awaiting a baseline handle.
pub struct WorkspaceState {
    baseline: poirot_model::snapshot::Snapshot<'static>,
    inputs: crate::run_overlay::RunInputs,
    compiler: FormulaCompiler,
    cache: Cache,
    policy: OverlayPolicy,
    preparation: crate::EvaluationPolicy,
}
impl WorkspaceState {
    /// Materialize effective authored source for independent persistence/recovery.
    /// Compiled programs and retained computed results are not promoted to inputs.
    /// The supplied baseline must match exactly, just as for runtime attachment.
    pub fn source_workbook(&self, workbook: &Workbook) -> Result<Workbook, OverlayError> {
        if !self.baseline.matches(workbook) {
            return Err(OverlayError::Snapshot(
                "baseline differs from detached state".into(),
            ));
        }
        let mut result = workbook.clone();
        let deleted = self.inputs.edits().filter_map(|(sheet, address, edit)| {
            matches!(edit, CellEdit::Delete)
                .then_some(WorkbookChange::RemoveCell { sheet, address })
        });
        let writes = self
            .inputs
            .edits()
            .filter_map(|(sheet, address, edit)| match edit {
                CellEdit::Delete => None,
                CellEdit::StyleOnly(cell) => Some(WorkbookChange::SetStyle {
                    sheet,
                    address,
                    style: cell.style.clone(),
                }),
                CellEdit::Replace(cell) => Some(WorkbookChange::SetCell {
                    sheet,
                    address,
                    cell: cell.as_ref().clone(),
                }),
            });
        result.apply(deleted.chain(writes))?;
        Ok(result)
    }

    /// Attach to an exactly matching baseline without extensions.
    pub fn attach<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.attach_registry(workbook, None)
    }
    /// Attach to an exactly matching baseline and extension registry.
    pub fn attach_with_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: &'a crate::extensions::ExtensionRegistry,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.attach_registry(workbook, Some(Registry::Borrowed(extensions)))
    }
    /// Reattach immutable shared providers for ordinary worker-thread transfer.
    pub fn attach_with_shared_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: Arc<crate::extensions::ExtensionRegistry>,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        self.attach_registry(workbook, Some(Registry::Shared(extensions)))
    }
    fn attach_registry<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        extensions: Option<Registry<'a>>,
    ) -> Result<Overlay<'a, W>, OverlayError> {
        if !self.baseline.matches(workbook.borrow()) {
            return Err(OverlayError::Snapshot(
                "baseline differs from detached state".into(),
            ));
        }
        let registry = extensions.as_ref().map(Registry::get).map_or_else(
            crate::RegistryIdentity::default,
            crate::RegistryIdentity::with_extensions,
        );
        if self.preparation.registry != registry {
            return Err(OverlayError::RegistryMismatch);
        }
        Ok(Overlay {
            source: Source {
                workbook,
                inputs: self.inputs,
                compiler: RefCell::new(self.compiler),
                spill_candidates: Default::default(),
                policy: self.policy,
                preparation: self.preparation,
                extensions,
            },
            cache: RefCell::new(self.cache),
        })
    }
    /// Attach as direct evaluation, reusing compilation but discarding incremental results.
    pub fn into_evaluation<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(self.attach(workbook)?.source, context)
    }
    /// Attach direct evaluation with the exact runtime extension registry.
    pub fn into_evaluation_with_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
        extensions: &'a crate::extensions::ExtensionRegistry,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(
            self.attach_with_extensions(workbook, extensions)?.source,
            context,
        )
    }
    /// Restore direct evaluation with shared providers and a fresh context.
    pub fn into_evaluation_with_shared_extensions<'a, W: Borrow<Workbook>>(
        self,
        workbook: W,
        context: EvaluationContext,
        extensions: Arc<crate::extensions::ExtensionRegistry>,
    ) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(
            self.attach_with_shared_extensions(workbook, extensions)?
                .source,
            context,
        )
    }
}
/// Versioned wire payload; runtime providers are reattached during restoration.
#[derive(Serialize, Deserialize)]
struct Payload<'a> {
    version: u32,
    model: poirot_model::snapshot::Snapshot<'a>,
    policy: Cow<'a, OverlayPolicy>,
    preparation: Cow<'a, crate::EvaluationPolicy>,
    compiler: crate::compiler::cache_snapshot::Cache<'a>,
    writes: Vec<(usize, CellAddress, Cow<'a, Cell>)>,
    deleted: Cow<'a, std::collections::BTreeSet<Key>>,
    style_only: Cow<'a, std::collections::BTreeSet<Key>>,
    cached: Vec<CachedValue<'a>>,
    context: Option<Cow<'a, ContextKey>>,
    spill_resets: Cow<'a, std::collections::BTreeSet<Key>>,
}
/// Serializable result paired with its consumed dependencies.
#[derive(Serialize, Deserialize)]
struct CachedValue<'a> {
    key: Key,
    value: poirot_model::snapshot::Value<'a>,
    dependencies: Cow<'a, Dependencies>,
}
impl<'a> Payload<'a> {
    fn capture(
        overlay: &BorrowedSource<'a>,
        compiler: &'a FormulaCompiler,
        cache: &'a Cache,
    ) -> Result<Self, OverlayError> {
        let mut cached = cache
            .entries
            .iter()
            .filter(|(_, entry)| entry.validity.is_clean())
            .map(|(key, entry)| CachedValue {
                key: *key,
                value: poirot_model::snapshot::Value::capture(&entry.value),
                dependencies: Cow::Borrowed(&entry.dependencies),
            })
            .collect::<Vec<_>>();
        cached.sort_unstable_by_key(|entry| entry.key);
        Ok(Self {
            version: VERSION,
            model: overlay.workbook.snapshot(),
            policy: Cow::Borrowed(overlay.policy),
            preparation: Cow::Borrowed(overlay.preparation),
            compiler: crate::compiler::cache_snapshot::Cache::capture(
                compiler,
                overlay.preparation,
            )
            .map_err(OverlayError::Snapshot)?,
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
        })
    }
    fn into_owned(self) -> Payload<'static> {
        Payload {
            version: self.version,
            model: self.model.into_owned(),
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
        }
    }
}
impl<'a> Snapshot<'a> {
    /// Borrow overlay state without requiring the overlay itself to implement Serde.
    pub(super) fn borrowed<W: Borrow<Workbook>>(
        source: &'a Source<'_, W>,
        cache: Option<&'a RefCell<Cache>>,
    ) -> Self {
        Self {
            state: State::Borrowed(BorrowedSource {
                workbook: source.workbook(),
                inputs: &source.inputs,
                compiler: &source.compiler,
                policy: &source.policy,
                preparation: &source.preparation,
                cache,
            }),
        }
    }
    /// Capture owned serialized state that can outlive the source overlay.
    pub fn into_owned(self) -> Result<Snapshot<'static>, OverlayError> {
        let payload = match self.state {
            State::Owned(payload) => payload,
            State::Borrowed(overlay) => {
                let empty = Cache::default();
                let cache = overlay
                    .cache
                    .map(|c| c.try_borrow())
                    .transpose()
                    .map_err(|e| OverlayError::Snapshot(e.to_string()))?;
                Box::new(
                    Payload::capture(
                        &overlay,
                        &*overlay
                            .compiler
                            .try_borrow()
                            .map_err(|e| OverlayError::Snapshot(e.to_string()))?,
                        cache.as_deref().unwrap_or(&empty),
                    )?
                    .into_owned(),
                )
            }
        };
        Ok(Snapshot {
            state: State::Owned(payload),
        })
    }
    /// Validate serialized identities and rebuild runtime caches and dependency indices.
    pub(super) fn restore(self) -> Result<(Workbook, WorkspaceState), OverlayError> {
        let State::Owned(payload) = self.into_owned()?.state else {
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
        // Typed file decoding owns optional quotas. Restoration validates source,
        // ownership and cache evidence without imposing another hidden size cap.
        let workbook = payload.model.restore()?;
        let policy = payload.policy.into_owned();
        let preparation = payload.preparation.into_owned();
        let compiler = payload
            .compiler
            .restore(&preparation)
            .map_err(OverlayError::Snapshot)?;
        let mut overlay = Overlay {
            source: Source {
                workbook,
                inputs: crate::run_overlay::RunInputs::empty(),
                compiler: RefCell::new(compiler),
                spill_candidates: Default::default(),
                policy,
                preparation,
                extensions: None,
            },
            cache: RefCell::new(Cache::default()),
        };
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
                        && crate::same_result_identity(&base.value, &cell.value)
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
            if !payload.cached.is_empty() && cache.context.is_none() {
                return Err(OverlayError::Snapshot(
                    "cached results lack execution context".into(),
                ));
            }
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
                    poirot_model::query::RangeMembership::new(
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
                        validity: super::dependencies::Validity::Clean,
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
            cache.rebuild_ranges();
        }
        let Source {
            workbook,
            inputs,
            compiler,
            policy,
            preparation,
            ..
        } = overlay.source;
        let baseline = workbook.snapshot().into_owned();
        Ok((
            workbook,
            WorkspaceState {
                baseline,
                inputs,
                compiler: compiler.into_inner(),
                cache: overlay.cache.into_inner(),
                policy,
                preparation,
            },
        ))
    }
}
impl Serialize for Snapshot<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.state {
            State::Owned(payload) => payload.serialize(serializer),
            State::Borrowed(overlay) => {
                let compiler = overlay.compiler.try_borrow().map_err(S::Error::custom)?;
                let empty = Cache::default();
                let cache = overlay
                    .cache
                    .map(|c| c.try_borrow())
                    .transpose()
                    .map_err(S::Error::custom)?;
                Payload::capture(overlay, &compiler, cache.as_deref().unwrap_or(&empty))
                    .map_err(S::Error::custom)?
                    .serialize(serializer)
            }
        }
    }
}
impl<'de, 'a> Deserialize<'de> for Snapshot<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            state: State::Owned(Box::new(Payload::<'static>::deserialize(deserializer)?)),
        })
    }
}

#[cfg(test)]
mod style_marker_validation {
    use super::*;

    #[test]
    fn malformed_style_only_markers_cannot_authorize_content_or_missing_writes() {
        for malformed in 0..3 {
            let mut book = Workbook::new();
            book.add_sheet("S")
                .unwrap()
                .set_value(CellAddress { row: 0, col: 0 }, 1.0)
                .unwrap();
            let mut overlay = Overlay::with_workbook(&book).build().unwrap();
            let key = (0, CellAddress { row: 0, col: 0 });
            overlay
                .set_value(
                    EvaluationTarget {
                        sheet: key.0,
                        address: key.1,
                    },
                    2.0,
                )
                .unwrap();
            let mut snapshot = overlay.snapshot().into_owned().unwrap();
            let State::Owned(payload) = &mut snapshot.state else {
                panic!("owned payload")
            };
            match malformed {
                0 => {
                    payload.style_only.to_mut().insert(key);
                }
                1 => {
                    payload
                        .style_only
                        .to_mut()
                        .insert((0, CellAddress { row: 1, col: 0 }));
                }
                _ => {
                    payload.writes.clear();
                    payload.writes.push((0, key.1, Cow::Owned(Cell::new(1.0))));
                    payload.style_only.to_mut().insert(key);
                    payload.deleted.to_mut().insert(key);
                }
            }
            assert!(snapshot.restore().is_err());
        }
    }
}
