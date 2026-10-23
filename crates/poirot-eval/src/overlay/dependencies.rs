//! Track which inputs were read to produce each cached result.
//!
//! Missing cells and scanned ranges are dependencies too: inserting a value there
//! must invalidate a formula that previously observed a blank or an empty range.
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
    /// Whether the result owns a spill region or belongs to another cell's spill.
    pub spill: crate::spill_publication::SpillRole,
}
impl Dependencies {
    fn cell(&mut self, (sheet, at): Key) {
        // Membership already invalidates every cell in this rectangle, including
        // missing cells. Keep outside reads such as spill owners as point edges.
        if self
            .ranges
            .last()
            .is_some_and(|range| range.contains(sheet, at))
        {
            return;
        }
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
/// Reads and completed cells recorded during one calculation.
#[derive(Default)]
pub(crate) struct Reads {
    /// Traces suspended when execution switches to another consumer.
    pub traces: CellMap<Dependencies>,
    current: Option<(Key, Dependencies)>,
    /// Newly computed or source-restored results awaiting incremental admission.
    pub fresh_results: CellSet,
    /// Cells whose results may be retained after successful execution.
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
    /// Remove speculative consumers while preserving previously settled evidence.
    pub(crate) fn discard_consumers(&mut self, speculative: &CellSet) {
        self.flush();
        for key in speculative {
            self.traces.remove(key);
            self.completed.remove(key);
            self.fresh_results.remove(key);
        }
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
                if first == last {
                    // A point edge invalidates both present and absent coordinates,
                    // including generated publication, exactly like this guard.
                    trace.cell((sheet, first));
                    return;
                }
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
            DependencyRead::Random(_) => trace.context.seed = true,
            DependencyRead::Context(policy) => {
                use crate::registry::DependencyPolicy as P;
                trace.context.as_of |= policy.contains(P::TIME);
                trace.context.date_system |= policy.contains(P::DATE_SYSTEM);
                trace.context.utc_offset |= policy.contains(P::LOCALIZATION);
                trace.context.date_order |= policy.contains(P::LOCALIZATION);
            }
            DependencyRead::ContextField(field) => match field {
                crate::run::ContextRead::AsOf => trace.context.as_of = true,
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
    ranges_dirty: bool,
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
        for (key, entry) in staged {
            self.insert(key, entry)
                .map_err(|_| EvaluationFailure::StalePlan)?;
        }
        self.spill_resets.extend(spill_resets);
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
            self.ranges_dirty |=
                !entry.dependencies.ranges.is_empty() || !entry.dependencies.point_runs.is_empty();
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
        self.ranges_dirty |=
            !entry.dependencies.ranges.is_empty() || !entry.dependencies.point_runs.is_empty();
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
        // Publication only marks rectangle changes; build once when an edit
        // actually needs the index rather than after every output request.
        let mut pending: Vec<_> = seeds.into_iter().collect();
        if self.ranges_dirty && !pending.is_empty() {
            self.rebuild_ranges();
        }
        let mut seen = CellSet::default();
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                continue;
            }
            if let Some(readers) = self.reverse.remove(&key) {
                pending.extend(readers);
            }
            self.ranges.take_readers(key, &mut pending);
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
        self.ranges_dirty = false;
    }
}
/// Per-sheet lookup of consumers whose read rectangles contain a changed cell.
#[derive(Default)]
struct RangeIndex {
    sheets: BTreeMap<usize, RangeTree>,
}
/// Row-sorted read rectangles with live subtree row bounds and whole-sheet column bounds.
#[derive(Default)]
struct RangeTree {
    entries: Vec<(RangeMembership, Key)>,
    max_end: Vec<Option<u32>>,
    first_col: u32,
    last_col: u32,
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
            (tree.first_col, tree.last_col) = tree
                .entries
                .iter()
                .fold((u32::MAX, 0), |(first, last), (range, _)| {
                    (first.min(range.first.col), last.max(range.last.col))
                });
            tree.entries.sort_by_key(|(range, _)| range.first.row);
            tree.max_end.resize(tree.entries.len() * 4, None);
            tree.build(1, 0, tree.entries.len());
        }
        index
    }
    // A matched entry can be consumed: its consumer is now pending and will
    // propagate once through `seen`, even if other entries reach it again.
    // Unmatched entries remain live, including column misses and row-zero reads.
    // Invalidation removes every reached consumer, marking the index dirty; the
    // next nonempty pass rebuilds only surviving traces. Exclusive mutable access
    // keeps a pass synchronous, so no nested pass can observe a partial closure.
    fn take_readers(&mut self, key: Key, out: &mut Vec<Key>) {
        if let Some(tree) = self.sheets.get_mut(&key.0)
            && tree.first_col <= key.1.col
            && key.1.col <= tree.last_col
        {
            tree.query(1, 0, tree.entries.len(), key, out);
        }
    }
}
impl RangeTree {
    fn build(&mut self, node: usize, lo: usize, hi: usize) -> Option<u32> {
        if lo >= hi || hi > self.entries.len() || node >= self.max_end.len() {
            return None;
        }
        if hi - lo == 1 {
            // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
            self.max_end[node] = Some(self.entries[lo].0.last.row);
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
    fn query(&mut self, node: usize, lo: usize, hi: usize, key: Key, out: &mut Vec<Key>) {
        if lo >= hi || hi > self.entries.len() || node >= self.max_end.len()
            // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
            || self.entries[lo].0.first.row > key.1.row || self.max_end[node].is_none_or(|end| end < key.1.row)
        {
            return;
        }
        if hi - lo == 1 {
            // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
            if self.entries[lo].0.contains(key.0, key.1) {
                // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
                out.push(self.entries[lo].1);
                // SAFETY: The entry guard checks node < max_end.len(); no vector is resized here.
                self.max_end[node] = None;
            }
        } else {
            let mid = (lo + hi) / 2;
            self.query(node * 2, lo, mid, key, out);
            self.query(node * 2 + 1, mid, hi, key, out);
            let left = self.max_end.get(node * 2).copied().flatten();
            let right = self.max_end.get(node * 2 + 1).copied().flatten();
            // SAFETY: The entry guard checks node < max_end.len(); recursive calls do not resize it.
            self.max_end[node] = left.max(right);
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

#[cfg(test)]
mod range_index_tests {
    use super::*;

    #[test]
    fn consuming_matches_keeps_row_zero_and_column_misses_live() {
        let a = (0, CellAddress { row: 0, col: 0 });
        let c = (0, CellAddress { row: 0, col: 2 });
        let reader = (1, CellAddress { row: 0, col: 0 });
        let other = (1, CellAddress { row: 1, col: 0 });
        let mut cache = Cache::default();
        for (key, source) in [(reader, a), (reader, c), (other, c)] {
            let mut deps = cache
                .get(key)
                .map(|e| (*e.dependencies).clone())
                .unwrap_or_default();
            deps.ranges
                .push(RangeMembership::new(0, source.1, source.1).unwrap());
            cache
                .insert(
                    key,
                    Cached {
                        value: Arc::new(1.0.into()),
                        dependencies: Box::new(deps),
                    },
                )
                .unwrap();
        }
        let mut index = RangeIndex::new(&cache.sheets);
        let mut found = Vec::new();
        // B1 lies inside global column bounds, but matches no rectangle.
        index.take_readers((0, CellAddress { row: 0, col: 1 }), &mut found);
        assert!(found.is_empty());
        index.take_readers(a, &mut found);
        assert_eq!(found, [reader]);
        found.clear();
        index.take_readers(a, &mut found);
        assert!(found.is_empty());
        index.take_readers(c, &mut found);
        found.sort();
        assert_eq!(found, [reader, other]);
        found.clear();
        index.take_readers(c, &mut found);
        assert!(found.is_empty());
        assert!(
            index
                .sheets
                .values()
                .all(|t| t.max_end.get(1) == Some(&None))
        );

        // A real pass removes the first reader; the second survives and its
        // column-C entry must be reconstructed for a later independent pass.
        cache.invalidate([a, a, reader]);
        assert!(cache.get(reader).is_none());
        assert!(cache.get(other).is_some());
        cache.invalidate([c]);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn duplicate_points_ranges_and_cycles_preserve_transitive_closure() {
        let keys = (0..4)
            .map(|row| (0, CellAddress { row, col: 0 }))
            .collect::<Vec<_>>();
        let mut cache = Cache::default();
        // 0 -> 1 -> 2 -> 1 is a cycle, and 2 -> 3 closes the chain.
        // Each arrow has both a point edge and two separate rectangle entries.
        for (consumer, source) in [(1, 0), (2, 1), (1, 2), (3, 2)] {
            let mut deps = cache
                .get(keys[consumer])
                .map(|e| (*e.dependencies).clone())
                .unwrap_or_default();
            deps.cells.push(keys[source]);
            let range = RangeMembership::new(0, keys[source].1, keys[source].1).unwrap();
            deps.ranges.push(range);
            deps.point_runs.push(range);
            cache
                .insert(
                    keys[consumer],
                    Cached {
                        value: Arc::new(1.0.into()),
                        dependencies: Box::new(deps),
                    },
                )
                .unwrap();
        }
        cache.invalidate([keys[0], keys[1], keys[0]]);
        assert_eq!(cache.len(), 0);
        cache.invalidate([keys[0]]);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn indexed_readers_match_rectangle_membership_on_both_axes_and_sheets() {
        // Plain inclusive inequalities form the independent rectangle oracle.
        // Include column gaps within global bounds, points, overlapping rectangles,
        // transposes, a blank-dependency rectangle and a second source sheet.
        let rectangles = [
            (0, 0, 0, 5, 0),
            (0, 0, 3, 5, 3),
            (0, 3, 0, 3, 5),
            (0, 1, 1, 4, 4),
            (0, 7, 7, 7, 7),
            (1, 0, 0, 5, 0),
        ];
        let mut cache = Cache::default();
        for (i, &(sheet, top, left, bottom, right)) in rectangles.iter().enumerate() {
            let range = RangeMembership::new(
                sheet,
                CellAddress {
                    row: top,
                    col: left,
                },
                CellAddress {
                    row: bottom,
                    col: right,
                },
            )
            .unwrap();
            let mut dependencies = Dependencies::default();
            if i % 2 == 0 {
                dependencies.ranges.push(range);
            } else {
                dependencies.point_runs.push(range);
            }
            cache
                .insert(
                    (
                        2,
                        CellAddress {
                            row: i as u32,
                            col: 9,
                        },
                    ),
                    Cached {
                        value: Arc::new(CellValue::Number(i as f64)),
                        dependencies: Box::new(dependencies),
                    },
                )
                .unwrap();
        }
        for sheet in 0..4 {
            for row in 0..10 {
                for col in 0..10 {
                    let mut actual = Vec::new();
                    let mut index = RangeIndex::new(&cache.sheets);
                    index.take_readers((sheet, CellAddress { row, col }), &mut actual);
                    actual.sort();
                    let expected = rectangles
                        .iter()
                        .enumerate()
                        .filter_map(|(i, &(s, t, l, b, r))| {
                            (s == sheet && t <= row && row <= b && l <= col && col <= r).then_some(
                                (
                                    2,
                                    CellAddress {
                                        row: i as u32,
                                        col: 9,
                                    },
                                ),
                            )
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(actual, expected, "sheet={sheet}, row={row}, col={col}");
                }
            }
        }
        let mut actual = Vec::new();
        RangeIndex::default().take_readers((0, CellAddress { row: 0, col: 0 }), &mut actual);
        assert!(actual.is_empty());
    }
}

#[cfg(test)]
mod covered_point_tests {
    use super::*;

    fn key(sheet: usize, row: u32, col: u32) -> Key {
        (sheet, CellAddress { row, col })
    }

    fn membership(sheet: usize, top: u32, left: u32, bottom: u32, right: u32) -> RangeMembership {
        RangeMembership::new(sheet, key(sheet, top, left).1, key(sheet, bottom, right).1).unwrap()
    }

    #[test]
    fn suppressing_covered_reads_preserves_the_independent_dependency_union() {
        // The oracle records explicit points plus inclusive rectangle inequalities,
        // independently of the production compression and RangeMembership::contains.
        let mut deps = Dependencies::default();
        let mut points = Vec::new();
        let mut rectangles = Vec::new();
        for (sheet, top, left, bottom, right) in [
            (0, 1, 1, 4, 3),
            (0, 3, 2, 6, 5),
            (0, 1, 6, 4, 7),
            (1, 1, 1, 4, 3),
        ] {
            // Points before membership must remain safe; points after membership
            // include overlaps, adjacent coordinates, other sheets and outside owners.
            for at in [key(sheet, top, left), key(sheet, top, right + 1)] {
                deps.cell(at);
                points.push(at);
            }
            deps.ranges
                .push(membership(sheet, top, left, bottom, right));
            rectangles.push((sheet, top, left, bottom, right));
            let old_runs = deps.point_runs.len();
            let old_points = deps.cells.len();
            for row in top..=bottom {
                for col in left..=right {
                    deps.cell(key(sheet, row, col));
                }
            }
            assert_eq!(deps.point_runs.len(), old_runs);
            assert_eq!(deps.cells.len(), old_points);
            for at in [
                key(sheet, 0, 0),
                key(sheet, bottom + 1, right),
                key(2, top, left),
            ] {
                deps.cell(at);
                points.push(at);
            }
        }
        for sheet in 0..4 {
            for row in 0..9 {
                for col in 0..10 {
                    let at = key(sheet, row, col);
                    let expected = points.contains(&at)
                        || rectangles.iter().any(|&(s, t, l, b, r)| {
                            s == sheet && t <= row && row <= b && l <= col && col <= r
                        });
                    let actual = deps.cells.contains(&at)
                        || deps
                            .ranges
                            .iter()
                            .chain(&deps.point_runs)
                            .any(|r| r.contains(sheet, at.1));
                    assert_eq!(actual, expected, "{at:?}");
                }
            }
        }
    }

    #[test]
    fn older_membership_and_gaps_conservatively_keep_point_edges() {
        let mut deps = Dependencies::default();
        deps.ranges.push(membership(0, 1, 1, 3, 1));
        deps.ranges.push(membership(0, 1, 3, 3, 3));
        let old_range_point = key(0, 2, 1);
        let gap = key(0, 2, 2);
        deps.cell(old_range_point);
        assert_eq!(deps.cells, vec![old_range_point]);
        deps.cell(gap);
        // These points may compress together, but the gap must never be dropped.
        assert!(
            deps.cells.contains(&gap) || deps.point_runs.iter().any(|r| r.contains(gap.0, gap.1))
        );
        assert!(
            deps.cells.contains(&old_range_point)
                || deps
                    .point_runs
                    .iter()
                    .any(|r| r.contains(old_range_point.0, old_range_point.1))
        );
    }

    #[test]
    fn internal_compaction_keeps_external_events_and_work_identical() {
        use crate::{DependencyReadSink, EvaluationRun, FormulaCompiler, WorkbookView};
        /// Retain callbacks for an exact ordered comparison.
        #[derive(Default)]
        struct Events(Vec<ReadEvent>);
        impl DependencyReadSink for Events {
            fn read(&mut self, event: ReadEvent) {
                self.0.push(event);
            }
        }
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value(key(0, 0, 0).1, 3.).unwrap();
        sheet.set_formula(key(0, 1, 0).1, "C1*2").unwrap();
        sheet.set_value(key(0, 0, 2).1, 4.).unwrap();
        sheet.set_formula(key(0, 0, 5).1, "SUM(A1:A3)+C1").unwrap();
        let policy = crate::EvaluationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
        let mut observed = Vec::new();
        for with_internal in [false, true] {
            let mut events = Events::default();
            let mut run = EvaluationRun::new(
                WorkbookView::new(&book, &plan, &policy).unwrap(),
                crate::EvaluationInputs::default(),
                crate::EvaluationBudgets::default(),
            )
            .unwrap()
            .with_dependency_sink(&mut events);
            if with_internal {
                run.session.overlay_reads = Some(Reads::default());
            }
            let value = run
                .evaluate_cell(crate::EvaluationTarget {
                    sheet: 0,
                    address: key(0, 0, 5).1,
                })
                .unwrap();
            assert_eq!(value, CellValue::Number(15.));
            let usage = run.finish(Ok(())).execution;
            observed.push((events.0, usage));
        }
        assert_eq!(observed[0], observed[1]);
        assert!(
            observed[0]
                .0
                .iter()
                .any(|e| matches!(e.read, DependencyRead::SparseMembership { .. }))
        );
        assert!(observed[0].0.iter().any(|e| e.consumer
            == Some(crate::EvaluationTarget {
                sheet: 0,
                address: key(0, 0, 5).1
            })
            && e.read
                == DependencyRead::Cell(crate::EvaluationTarget {
                    sheet: 0,
                    address: key(0, 1, 0).1
                })));
    }

    #[test]
    fn nested_consumers_keep_their_own_guards_and_outside_reads() {
        use crate::DependencyReadSink;
        let parent = key(0, 10, 10);
        let child = key(0, 2, 1);
        let owner = key(0, 0, 0);
        let mut reads = Reads::default();
        let emit = |reads: &mut Reads, consumer: Key, read| {
            reads.read(ReadEvent {
                consumer: Some(crate::EvaluationTarget {
                    sheet: consumer.0,
                    address: consumer.1,
                }),
                read,
            });
        };
        emit(
            &mut reads,
            parent,
            DependencyRead::SparseMembership {
                sheet: 0,
                first: key(0, 1, 1).1,
                last: key(0, 4, 1).1,
            },
        );
        emit(
            &mut reads,
            child,
            DependencyRead::Cell(crate::EvaluationTarget {
                sheet: 0,
                address: owner.1,
            }),
        );
        emit(
            &mut reads,
            parent,
            DependencyRead::Cell(crate::EvaluationTarget {
                sheet: 0,
                address: child.1,
            }),
        );
        emit(
            &mut reads,
            parent,
            DependencyRead::Cell(crate::EvaluationTarget {
                sheet: 0,
                address: owner.1,
            }),
        );
        // Adjacent membership merges do not invalidate the already covered guard.
        emit(
            &mut reads,
            parent,
            DependencyRead::SparseMembership {
                sheet: 0,
                first: key(0, 5, 1).1,
                last: key(0, 6, 1).1,
            },
        );
        emit(
            &mut reads,
            parent,
            DependencyRead::Cell(crate::EvaluationTarget {
                sheet: 0,
                address: key(0, 6, 1).1,
            }),
        );
        reads.flush();
        let parent_trace = reads.traces.get(&parent).unwrap();
        assert_eq!(parent_trace.cells, vec![owner]);
        assert!(parent_trace.point_runs.is_empty());
        assert_eq!(parent_trace.ranges.len(), 1);
        assert!(parent_trace.ranges[0].contains(0, key(0, 6, 1).1));
        assert_eq!(reads.traces.get(&child).unwrap().cells, vec![owner]);
    }
}
