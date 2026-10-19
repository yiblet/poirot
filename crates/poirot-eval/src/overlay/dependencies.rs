//! Consumed-read graph. Point reverse edges and a rectangle index cover absent cells too.
use super::*;
use crate::coordinate_hash::{CellMap, CellSet};
use crate::{DependencyRead, ReadEvent};
use poirot_model::query::RangeMembership;
use std::collections::{BTreeMap, BTreeSet};

/// Sheet index and cell address identifying a graph node.
pub(super) type Key = (usize, CellAddress);
/// Reads and execution properties that determine when one cached result expires.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Dependencies {
    /// Individual consumed cells, including absent cells.
    pub cells: Vec<Key>,
    /// Membership guards that also detect newly inserted cells in scanned ranges.
    pub ranges: Vec<RangeMembership>,
    /// Consecutive point reads compressed into rectangles.
    pub point_runs: Vec<RangeMembership>,
    /// Execution consumed a provider whose dependencies cannot be fully observed.
    pub opaque: bool,
    /// Execution must be repeated in each new evaluation.
    pub volatile: bool,
    /// Execution context fields consumed while computing the result.
    pub context: crate::context_dependencies::ContextMask,
    /// Whether formatting configuration affected the result.
    pub formatting: bool,
    /// Numeric input locale was consumed.
    #[serde(default)]
    pub number_locale: bool,
    /// Whether text semantics affected the result.
    pub text_policy: bool,
    /// Legal committed ownership and extent; independent of dependency recording.
    pub spill: crate::spill_publication::SpillRole,
}
impl Dependencies {
    fn cell(&mut self, (sheet, at): Key) {
        if let Some(run) = self.point_runs.last_mut() {
            if run.contains(sheet, at) {
                return;
            }
            if run.sheet == sheet
                && run.first.col == run.last.col
                && at.col == run.last.col
                && at.row == run.last.row + 1
            {
                run.last = at;
                return;
            }
            if run.sheet == sheet
                && run.first.row == run.last.row
                && at.row == run.last.row
                && at.col == run.last.col + 1
            {
                run.last = at;
                return;
            }
        }
        if let Some(&(previous_sheet, previous)) = self.cells.last() {
            if previous_sheet == sheet
                && ((previous.col == at.col && previous.row + 1 == at.row)
                    || (previous.row == at.row && previous.col + 1 == at.col))
            {
                self.cells.pop();
                self.point_runs.push(RangeMembership {
                    sheet,
                    first: previous,
                    last: at,
                });
                return;
            }
            if (previous_sheet, previous) == (sheet, at) {
                return;
            }
        }
        self.cells.push((sheet, at));
    }
}
/// Collects dependency traces and completion markers for one evaluation batch.
#[derive(Default)]
pub(crate) struct Reads {
    /// Traces suspended when execution switches to another consumer.
    pub traces: CellMap<Dependencies>,
    current: Option<(Key, Dependencies)>,
    /// Cells actually executed rather than restored from the persistent cache.
    pub evaluated: CellSet,
    /// Cells eligible for publication after successful execution.
    pub completed: CellSet,
}
impl Reads {
    // Range consumers issue many reads without changing the consumer. Keep its
    // trace directly accessible rather than hashing its coordinates per value.
    fn trace(&mut self, key: Key) -> &mut Dependencies {
        if self
            .current
            .as_ref()
            .is_none_or(|(current, _)| *current != key)
        {
            self.flush();
            self.current = Some((key, self.traces.remove(&key).unwrap_or_default()));
        }
        &mut self
            .current
            .get_or_insert_with(|| (key, Dependencies::default()))
            .1
    }
    /// Return the active trace to the map before publishing the batch.
    pub fn flush(&mut self) {
        if let Some((key, trace)) = self.current.take() {
            self.traces.insert(key, trace);
        }
    }
}
impl crate::DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        let Some(consumer) = event.consumer else {
            return;
        };
        let trace = self.trace((consumer.sheet, consumer.address));
        match event.read {
            DependencyRead::Cell(cell) | DependencyRead::SubtotalMetadata(cell) => {
                trace.cell((cell.sheet, cell.address));
            }
            DependencyRead::SparseMembership { sheet, first, last } => {
                let range = RangeMembership { sheet, first, last };
                if let Some(previous) = trace.ranges.last_mut() {
                    if previous.sheet == sheet
                        && previous.first.col == first.col
                        && previous.last.col == last.col
                        && first.row <= previous.last.row.saturating_add(1)
                        && last.row.saturating_add(1) >= previous.first.row
                    {
                        previous.first.row = previous.first.row.min(first.row);
                        previous.last.row = previous.last.row.max(last.row);
                        return;
                    }
                    if previous.sheet == sheet
                        && previous.first.row == first.row
                        && previous.last.row == last.row
                        && first.col <= previous.last.col.saturating_add(1)
                        && last.col.saturating_add(1) >= previous.first.col
                    {
                        previous.first.col = previous.first.col.min(first.col);
                        previous.last.col = previous.last.col.max(last.col);
                        return;
                    }
                }
                trace.ranges.push(range);
            }
            DependencyRead::OpaqueProvider(_) => trace.opaque = true,
            DependencyRead::Extension(invocation) => {
                use crate::extensions::ExtensionCacheability as C;
                match invocation.binding.registration.contract.cacheability {
                    C::Deterministic | C::Versioned { .. } => {}
                    C::Volatile => trace.volatile = true,
                    C::Opaque => trace.opaque = true,
                }
                // The host exposes the entire captured context as one value.
                // A context-capable provider may consume any of its fields.
                if invocation
                    .binding
                    .registration
                    .contract
                    .capabilities
                    .context
                {
                    trace.context.as_of = true;
                    trace.context.seed = true;
                    trace.context.utc_offset = true;
                    trace.context.date_order = true;
                }
            }
            DependencyRead::Volatile(_) => trace.volatile = true,
            DependencyRead::Random(_) => {
                trace.context.seed = true;
                trace.volatile = true;
            }
            DependencyRead::Context(policy) => {
                use crate::registry::DependencyPolicy as P;
                trace.context.as_of |= policy.contains(P::TIME);
                trace.context.utc_offset |= policy.contains(P::LOCALIZATION);
                trace.context.date_order |= policy.contains(P::LOCALIZATION);
                trace.volatile |= policy.contains(P::TIME);
            }
            DependencyRead::ContextField(field) => match field {
                crate::run::ContextRead::AsOf => {
                    trace.context.as_of = true;
                    trace.volatile = true;
                }
                crate::run::ContextRead::UtcOffset => trace.context.utc_offset = true,
                crate::run::ContextRead::DateOrder => trace.context.date_order = true,
                crate::run::ContextRead::DateSystem => trace.context.date_system = true,
            },
            DependencyRead::Formatting => trace.formatting = true,
            DependencyRead::NumberLocale => trace.number_locale = true,
            DependencyRead::TextPolicy => trace.text_policy = true,
            _ => {}
        }
    }
}
/// One retained result and the dependencies needed to validate its reuse.
pub(crate) struct Cached {
    /// Shared result storage, independent of the evaluation that produced it.
    pub value: Arc<CellValue>,
    /// Consumed reads, context flags, and spill ownership for this result.
    pub dependencies: Box<Dependencies>,
}
/// Persistent overlay results with reverse edges for transitive invalidation.
#[derive(Default)]
pub(crate) struct Cache {
    /// Sparse result pages, allocated only for sheets with retained results.
    pub sheets: BTreeMap<usize, CellStore<Cached, 16, 2>>,
    /// Context used by the latest evaluation.
    pub context: Option<ContextKey>,
    /// Estimated bytes charged against persistent retention limits.
    pub retained_bytes: usize,
    /// Spill anchors whose previously exported children must be cleared.
    pub spill_resets: BTreeSet<Key>,
    /// Derived exact lookup prefixes; intentionally absent from snapshots.
    pub lookup_prefixes: crate::adapters::search::execution::LookupPrefixes,
    reverse: CellMap<CellSet>,
    always_dirty: CellSet,
    ranges: RangeIndex,
}
impl Cache {
    /// Admit a complete batch before publishing values, indexes, and spill cleanup together.
    pub fn commit(
        &mut self,
        staged: Vec<(Key, Cached)>,
        retention: crate::run::MemoRetentionLimits,
        consumer: Option<EvaluationTarget>,
        spill_resets: impl IntoIterator<Item = Key>,
    ) -> Result<(), EvaluationFailure> {
        let mut cells = self.len();
        let mut bytes = self.retained_bytes;
        let mut overflow = false;
        for (key, entry) in &staged {
            if let Some(old) = self.get(*key) {
                let remaining = bytes.checked_sub(old.retained_bytes());
                overflow |= remaining.is_none();
                bytes = remaining.unwrap_or(usize::MAX);
            } else {
                let next = cells.checked_add(1);
                overflow |= next.is_none();
                cells = next.unwrap_or(usize::MAX);
            }
            let next = bytes.checked_add(entry.retained_bytes());
            overflow |= next.is_none();
            bytes = next.unwrap_or(usize::MAX);
        }
        if overflow || cells > retention.cells || bytes > retention.bytes {
            return Err(EvaluationFailure::MemoRetentionLimit {
                limits: retention,
                consumed: crate::run::MemoRetentionUsage {
                    cells: self.len(),
                    bytes: self.retained_bytes,
                },
                requested: crate::run::MemoRetentionUsage {
                    cells: cells.saturating_sub(self.len()),
                    bytes: bytes.saturating_sub(self.retained_bytes),
                },
                source: consumer,
            });
        }
        for (key, _) in &staged {
            key.1.validate().map_err(|_| EvaluationFailure::StalePlan)?;
        }
        let changed = !staged.is_empty();
        for (key, entry) in staged {
            self.insert(key, entry)
                .map_err(|_| EvaluationFailure::StalePlan)?;
        }
        self.spill_resets.extend(spill_resets);
        if changed {
            self.rebuild_ranges();
        }
        Ok(())
    }
    /// Look up a retained result without evaluating it.
    pub fn get(&self, key: Key) -> Option<&Cached> {
        self.sheets.get(&key.0)?.get(&key.1)
    }
    /// Count retained results across all sparse sheets.
    pub fn len(&self) -> usize {
        self.sheets.values().map(|s| s.cells().len()).sum()
    }
    /// Remove a result and its point edges; range indices are rebuilt separately.
    pub fn remove(&mut self, key: Key) {
        self.always_dirty.remove(&key);
        if let Some(entry) = self.sheets.get_mut(&key.0).and_then(|s| s.remove(&key.1)) {
            self.retained_bytes -= entry.retained_bytes();
            for dependency in &entry.dependencies.cells {
                if let Some(readers) = self.reverse.get_mut(dependency) {
                    readers.remove(&key);
                    if readers.is_empty() {
                        self.reverse.remove(dependency);
                    }
                }
            }
        }
        if self
            .sheets
            .get(&key.0)
            .is_some_and(|s| s.cells().is_empty())
        {
            self.sheets.remove(&key.0);
        }
    }
    /// Replace a result and register its point edges and always-dirty status.
    pub fn insert(&mut self, key: Key, entry: Cached) -> Result<(), poirot_model::ModelError> {
        key.1.validate()?;
        let retained_bytes = self
            .retained_bytes
            .checked_sub(self.get(key).map_or(0, Cached::retained_bytes))
            .and_then(|bytes| bytes.checked_add(entry.retained_bytes()))
            .ok_or_else(|| poirot_model::ModelError("cache retention size overflow".into()))?;
        self.remove(key);
        self.retained_bytes = retained_bytes;
        if entry.dependencies.opaque || entry.dependencies.volatile {
            self.always_dirty.insert(key);
        }
        for dependency in &entry.dependencies.cells {
            self.reverse.entry(*dependency).or_default().insert(key);
        }
        self.sheets.entry(key.0).or_default().set(key.1, entry)?;
        Ok(())
    }
    /// Iterative traversal handles arbitrary chain lengths and cycles without stack recursion.
    pub fn invalidate(&mut self, seeds: impl IntoIterator<Item = Key>) {
        let mut pending: Vec<_> = seeds.into_iter().collect();
        let mut seen = CellSet::default();
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                continue;
            }
            if let Some(readers) = self.reverse.remove(&key) {
                pending.extend(readers);
            }
            self.ranges.readers(key, &mut pending);
            self.remove(key);
        }
        self.lookup_prefixes.invalidate(&seen);
    }

    /// Invalidate volatile, opaque, and context-sensitive results for a new evaluation.
    pub fn begin(&mut self, context: ContextKey) {
        if self.context.as_ref() == Some(&context) {
            // The common unchanged-context path only needs volatile/opaque roots.
            // insert/remove maintain this derived index, including snapshot restore.
            let seeds = self.always_dirty.iter().copied().collect::<Vec<_>>();
            self.invalidate(seeds);
            return;
        }
        let seeds = self
            .sheets
            .iter()
            .flat_map(|(sheet, cells)| {
                cells
                    .cells()
                    .into_iter()
                    .filter(|(_, entry)| {
                        let trace = &entry.dependencies;
                        trace.opaque
                            || trace.volatile
                            || self.context.as_ref().is_none_or(|previous| {
                                !trace
                                    .context
                                    .changed_fields(previous.inputs, context.inputs, false, false)
                                    .is_empty()
                                    || (trace.formatting
                                        && previous.formatting != context.formatting)
                                    || (trace.number_locale
                                        && previous.number_locale != context.number_locale)
                            })
                    })
                    .map(move |(at, _)| (*sheet, *at))
            })
            .collect::<Vec<_>>();
        self.invalidate(seeds);
        self.context = Some(context);
    }
    /// Invalidate cells that consumed text policy and all their dependents.
    pub fn invalidate_text_policy(&mut self) {
        let seeds = self
            .sheets
            .iter()
            .flat_map(|(sheet, cells)| {
                cells
                    .cells()
                    .into_iter()
                    .filter(|(_, entry)| entry.dependencies.text_policy)
                    .map(move |(at, _)| (*sheet, *at))
            })
            .collect::<Vec<_>>();
        self.invalidate(seeds);
    }
    /// Rebuild the rectangle lookup after dependency traces have changed.
    pub fn rebuild_ranges(&mut self) {
        self.ranges = RangeIndex::new(&self.sheets);
    }
}
/// Per-sheet lookup of consumers whose read rectangles contain a changed cell.
#[derive(Default)]
struct RangeIndex {
    sheets: BTreeMap<usize, RangeTree>,
}
/// Row-sorted read rectangles with subtree row bounds for pruning queries.
#[derive(Default)]
struct RangeTree {
    entries: Vec<(RangeMembership, Key)>,
    max_end: Vec<u32>,
}
impl RangeIndex {
    fn new(sheets: &BTreeMap<usize, CellStore<Cached, 16, 2>>) -> Self {
        let mut index = Self::default();
        for (sheet, cells) in sheets {
            for (at, entry) in cells.cells() {
                for range in entry
                    .dependencies
                    .ranges
                    .iter()
                    .chain(&entry.dependencies.point_runs)
                {
                    index
                        .sheets
                        .entry(range.sheet)
                        .or_default()
                        .entries
                        .push((*range, (*sheet, *at)));
                }
            }
        }
        for tree in index.sheets.values_mut() {
            tree.entries.sort_by_key(|(range, _)| range.first.row);
            tree.max_end.resize(tree.entries.len() * 4, 0);
            tree.build(1, 0, tree.entries.len());
        }
        index
    }
    fn readers(&self, key: Key, out: &mut Vec<Key>) {
        if let Some(tree) = self.sheets.get(&key.0) {
            tree.query(1, 0, tree.entries.len(), key, out);
        }
    }
}
impl RangeTree {
    fn build(&mut self, node: usize, lo: usize, hi: usize) -> u32 {
        if lo >= hi || hi > self.entries.len() || node >= self.max_end.len() {
            return 0;
        }
        if hi - lo == 1 {
            // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
            self.max_end[node] = self.entries[lo].0.last.row;
        } else {
            let mid = (lo + hi) / 2;
            // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
            self.max_end[node] =
                self.build(node * 2, lo, mid)
                    .max(self.build(node * 2 + 1, mid, hi));
        }
        // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
        self.max_end[node]
    }
    fn query(&self, node: usize, lo: usize, hi: usize, key: Key, out: &mut Vec<Key>) {
        if lo >= hi || hi > self.entries.len() || node >= self.max_end.len()
            // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
            || self.entries[lo].0.first.row > key.1.row || self.max_end[node] < key.1.row
        {
            return;
        }
        if hi - lo == 1 {
            // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
            if self.entries[lo].0.contains(key.0, key.1) {
                // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
                out.push(self.entries[lo].1);
            }
        } else {
            let mid = (lo + hi) / 2;
            self.query(node * 2, lo, mid, key, out);
            self.query(node * 2 + 1, mid, hi, key, out);
        }
    }
}

impl Cached {
    /// Estimate result and dependency storage for retention accounting.
    pub fn retained_bytes(&self) -> usize {
        128usize
            .saturating_add(self.value.text_storage_bytes())
            .saturating_add(self.dependencies.cells.len().saturating_mul(48))
            .saturating_add(
                (self.dependencies.ranges.len() + self.dependencies.point_runs.len())
                    .saturating_mul(64),
            )
    }
}

#[cfg(test)]
mod retention_overflow_tests {
    use super::*;
    fn entry() -> Cached {
        Cached {
            value: Arc::new(CellValue::Number(1.)),
            dependencies: Box::default(),
        }
    }
    #[test]
    fn unlimited_policy_still_refuses_intrinsic_cache_size_overflow_atomically() {
        // Exercise the counter boundary without allocating machine-sized storage.
        let mut cache = Cache {
            retained_bytes: usize::MAX,
            ..Default::default()
        };
        let key = (0, CellAddress { row: 0, col: 0 });
        assert!(matches!(
            cache.commit(
                vec![(key, entry())],
                crate::run::MemoRetentionLimits::default(),
                None,
                []
            ),
            Err(EvaluationFailure::MemoRetentionLimit { .. })
        ));
        assert!(cache.insert(key, entry()).is_err());
        assert_eq!(cache.retained_bytes, usize::MAX);
        assert_eq!(cache.len(), 0);
    }
}
