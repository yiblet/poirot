//! Track which inputs were read to produce each cached result.
//!
//! Missing cells and scanned ranges are dependencies too: inserting a value there
//! must invalidate a formula that previously observed a blank or an empty range.
use super::*;
use crate::eval::coordinate_hash::{CellMap, CellSet};
use crate::eval::{DependencyRead, ReadEvent};
use crate::model::query::RangeMembership;
use std::collections::{BTreeMap, BTreeSet};

/// Sheet index and cell address identifying a graph node.
pub(super) type Key = (usize, CellAddress);
/// Reads and execution properties that determine when one cached result expires.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) struct Dependencies {
    /// Individual consumed cells, including absent cells.
    pub cells: Vec<Key>,
    /// Membership guards that also detect newly inserted cells in scanned ranges.
    pub ranges: Vec<RangeMembership>,
    /// Consecutive point reads compressed into rectangles.
    pub point_runs: Vec<RangeMembership>,
    /// Actual value reads in execution order, compressed into forward point spans.
    /// Repeated spans are harmless: validation selects each cause's first occurrence.
    pub value_reads: ValueReadOrder,
    /// Metadata reads cannot be certified by comparing cell values alone.
    pub metadata_reads: bool,
    /// Geometry-only guards; literal value changes do not create new producers.
    pub spill_ranges: Vec<RangeMembership>,
    /// Execution consumed a provider whose dependencies cannot be fully observed.
    pub opaque: bool,
    /// Execution must be repeated in each new evaluation.
    pub volatile: bool,
    /// Execution context fields consumed while computing the result.
    pub context: crate::eval::context_dependencies::ContextMask,
    /// Whether formatting configuration affected the result.
    pub formatting: bool,
    /// Numeric input locale was consumed.
    #[serde(default)]
    pub number_locale: bool,
    /// Whether text semantics affected the result.
    pub text_policy: bool,
    /// Whether the result owns a spill region or belongs to another cell's spill.
    pub spill: crate::eval::spill_publication::SpillRole,
    /// A possible producer needs ownership validation even when its current result is scalar.
    pub spill_sensitive: bool,
}
impl Dependencies {
    fn value_read(&mut self, (sheet, at): Key) {
        if let Some(last) = self.value_reads.last_mut() {
            if last.contains(sheet, at) {
                return;
            }
            if last.sheet == sheet
                && last.first.col == last.last.col
                && at.col == last.last.col
                && Some(at.row) == last.last.row.checked_add(1)
            {
                last.last = at;
                return;
            }
            if last.sheet == sheet
                && last.first.row == last.last.row
                && at.row == last.last.row
                && Some(at.col) == last.last.col.checked_add(1)
            {
                last.last = at;
                return;
            }
        }
        self.value_reads.push(RangeMembership {
            sheet,
            first: at,
            last: at,
        });
    }

    /// Order dirty value dependencies by their first consumed read; other evidence forces execution.
    pub fn ordered_causes(
        &self,
        validity: &Validity,
        budget: &mut crate::eval::budget::BudgetMeter,
    ) -> crate::eval::EvalResult<Option<OrderedCauses>> {
        if self.metadata_reads {
            return Ok(None);
        }
        match validity {
            // A single cause needs no ordering evidence. Cached lookup indexes,
            // for example, can retain membership without replaying point reads.
            Validity::Cause { key, version } => {
                Ok(Some(OrderedCauses::One(Some((*key, *version)))))
            }
            Validity::Causes(causes) => {
                if matches!(self.value_reads, ValueReadOrder::Unavailable) {
                    return Ok(None);
                }
                if let ValueReadOrder::One(span) = &self.value_reads {
                    if (span.first.col != span.last.col && span.first.row != span.last.row)
                        || span.first.row > span.last.row
                        || span.first.col > span.last.col
                    {
                        return Ok(None);
                    }
                    // Establish complete evidence before evaluating anything: an
                    // unordered membership cause could change an old lazy branch.
                    for key in causes.keys() {
                        budget.charge(crate::eval::WorkKind::Comparison, 1)?;
                        if !span.contains(key.0, key.1) {
                            return Ok(None);
                        }
                    }
                    return Ok(Some(OrderedCauses::Span {
                        next: Some(span.first),
                        span: *span,
                        remaining: causes.len(),
                    }));
                }
                // Irregular multi-span traces need a complete safe ordering.
                // Index only dirty causes, not the potentially enormous scanned ranges.
                let mut rows = BTreeMap::new();
                let mut columns = BTreeMap::new();
                for (&key, &version) in causes {
                    budget.charge(crate::eval::WorkKind::Comparison, 1)?;
                    rows.insert((key.0, key.1.row, key.1.col), version);
                    columns.insert((key.0, key.1.col, key.1.row), version);
                }
                let mut ordered = Vec::new();
                if ordered.try_reserve(causes.len()).is_err() {
                    return Ok(None);
                }
                for span in self.value_reads.iter() {
                    budget.charge(crate::eval::WorkKind::Comparison, 1)?;
                    let vertical = span.first.col == span.last.col;
                    if !vertical && span.first.row != span.last.row {
                        return Ok(None);
                    }
                    let (first, last) = if vertical {
                        (
                            (span.sheet, span.first.col, span.first.row),
                            (span.sheet, span.last.col, span.last.row),
                        )
                    } else {
                        (
                            (span.sheet, span.first.row, span.first.col),
                            (span.sheet, span.last.row, span.last.col),
                        )
                    };
                    if first > last {
                        return Ok(None);
                    }
                    loop {
                        let index = if vertical { &columns } else { &rows };
                        // SAFETY: The local comparison above proves the inclusive range is ordered.
                        let next = index.range(first..=last).next().map(
                            |(&(sheet, axis, offset), &version)| {
                                let address = if vertical {
                                    CellAddress {
                                        row: offset,
                                        col: axis,
                                    }
                                } else {
                                    CellAddress {
                                        row: axis,
                                        col: offset,
                                    }
                                };
                                ((sheet, address), version)
                            },
                        );
                        let Some((key, version)) = next else {
                            break;
                        };
                        rows.remove(&(key.0, key.1.row, key.1.col));
                        columns.remove(&(key.0, key.1.col, key.1.row));
                        // SAFETY: Reserved causes.len() slots above; removing each selected key
                        // from both indexes means at most that many unique causes are pushed.
                        ordered.push((key, version));
                    }
                    if rows.is_empty() {
                        return Ok(Some(OrderedCauses::Many(ordered.into_iter())));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
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
/// Accumulates consumed dependencies and completed cells for incremental cache publication.
/// It keeps a separate trace per formula as evaluation enters and returns from
/// nested dependencies.
#[derive(Default)]
pub(in crate::eval) struct Reads {
    /// Traces suspended when execution switches to another consumer.
    pub traces: CellMap<Dependencies>,
    current: Option<(Key, Dependencies)>,
    /// Newly computed or source-restored results awaiting incremental admission.
    pub fresh_results: CellSet,
    /// Cells whose results may be retained after successful execution.
    pub completed: CellSet,
    /// Results whose old consumed trace was validated without executing their formula.
    pub validated: CellSet,
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
    /// Keep ownership guards separate from consumed values while recording the same public event.
    pub fn spill_membership(&mut self, consumer: Option<EvaluationTarget>, range: RangeMembership) {
        if let Some(consumer) = consumer {
            let ranges = &mut self.trace((consumer.sheet, consumer.address)).spill_ranges;
            if !ranges.contains(&range) {
                ranges.push(range);
            }
        }
    }
    /// Mark geometry-sensitive reads that scalar equality cannot validate.
    pub fn mark_metadata(&mut self, consumer: Option<EvaluationTarget>) {
        if let Some(consumer) = consumer {
            self.trace((consumer.sheet, consumer.address))
                .metadata_reads = true;
        }
    }
    /// Mark a producer whose future footprint cannot be certified by scalar equality.
    pub fn mark_spill_sensitive(&mut self, key: Key) {
        self.trace(key).spill_sensitive = true;
    }
    /// Remove speculative consumers while preserving previously settled evidence.
    pub(in crate::eval) fn discard_consumers(&mut self, speculative: &CellSet) {
        self.flush();
        for key in speculative {
            self.traces.remove(key);
            self.completed.remove(key);
            self.fresh_results.remove(key);
            self.validated.remove(key);
        }
    }
    /// Return the active trace to the map before publishing the batch.
    pub fn flush(&mut self) {
        if let Some((key, trace)) = self.current.take() {
            self.traces.insert(key, trace);
        }
    }
    /// Observe a read without copying payloads retained by an external sink.
    pub(in crate::eval) fn record(&mut self, event: &ReadEvent) {
        let Some(consumer) = event.consumer else {
            return;
        };
        let trace = self.trace((consumer.sheet, consumer.address));
        match &event.read {
            DependencyRead::Cell(cell) => {
                trace.value_read((cell.sheet, cell.address));
                trace.cell((cell.sheet, cell.address));
            }
            DependencyRead::SubtotalMetadata(cell) => {
                trace.metadata_reads = true;
                trace.cell((cell.sheet, cell.address));
            }
            DependencyRead::SparseMembership { sheet, first, last } => {
                let (sheet, first, last) = (*sheet, *first, *last);
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
                use crate::eval::extensions::ExtensionCacheability as C;
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
                use crate::eval::registry::DependencyPolicy as P;
                trace.context.as_of |= policy.contains(P::TIME);
                trace.context.date_system |= policy.contains(P::DATE_SYSTEM);
                trace.context.utc_offset |= policy.contains(P::LOCALIZATION);
                trace.context.date_order |= policy.contains(P::LOCALIZATION);
            }
            DependencyRead::ContextField(field) => match field {
                crate::eval::run::ContextRead::AsOf => trace.context.as_of = true,
                crate::eval::run::ContextRead::UtcOffset => trace.context.utc_offset = true,
                crate::eval::run::ContextRead::DateOrder => trace.context.date_order = true,
                crate::eval::run::ContextRead::DateSystem => trace.context.date_system = true,
            },
            DependencyRead::Formatting => trace.formatting = true,
            DependencyRead::NumberLocale => trace.number_locale = true,
            DependencyRead::TextPolicy => trace.text_policy = true,
            _ => {}
        }
    }
}
impl crate::eval::DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.record(&event);
    }
}
/// One retained result and the dependencies needed to validate its reuse.
pub(in crate::eval) struct Cached {
    /// Shared result storage, independent of the evaluation that produced it.
    pub value: Arc<CellValue>,
    /// Last publication at which the observable value changed.
    pub version: u64,
    /// Whether retained evidence is current, forced to execute, or awaiting direct causes.
    pub validity: Validity,
    /// Consumed reads, context flags, and spill ownership for this result.
    pub dependencies: Box<Dependencies>,
}
/// Immediate invalidation evidence, separate from the last retained value.
#[derive(Clone, Default)]
pub(in crate::eval) enum Validity {
    /// The retained result is available to ordinary cache readers.
    #[default]
    Clean,
    /// A source/context change requires normal execution.
    Forced,
    /// One dirty input can settle without speculating over an unordered set of reads.
    Cause { key: Key, version: u64 },
    /// Multiple dirty inputs are settled in the retained value-read order.
    Causes(CellMap<u64>),
}
impl Validity {
    /// Storage used by additional dirty causes, apart from the inline validity tag.
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Causes(causes) => causes.len().saturating_mul(64),
            _ => 0,
        }
    }
    /// Whether the retained result can be published without further validation.
    pub fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }
}
/// Ordered actual value reads; the common single span needs no heap allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) enum ValueReadOrder {
    /// No point value was read.
    #[default]
    Empty,
    /// One consecutive traversal or one cell.
    One(RangeMembership),
    /// Nonconsecutive traversals retained in execution order.
    Many(Vec<RangeMembership>),
    /// Optional evidence could not be allocated; normal execution remains available.
    Unavailable,
}
impl ValueReadOrder {
    fn last_mut(&mut self) -> Option<&mut RangeMembership> {
        match self {
            Self::One(span) => Some(span),
            Self::Many(spans) => spans.last_mut(),
            _ => None,
        }
    }
    fn push(&mut self, span: RangeMembership) {
        match self {
            Self::Empty => *self = Self::One(span),
            Self::One(first) => {
                let mut spans = Vec::new();
                if spans.try_reserve(2).is_err() {
                    *self = Self::Unavailable;
                    return;
                }
                // SAFETY: try_reserve(2) succeeded above, providing capacity for both spans.
                spans.push(*first);
                // SAFETY: Exactly one of the two reserved slots has been filled.
                spans.push(span);
                *self = Self::Many(spans);
            }
            Self::Many(spans) => {
                if spans.try_reserve(1).is_err() {
                    *self = Self::Unavailable;
                    return;
                }
                // SAFETY: try_reserve(1) succeeded immediately above.
                spans.push(span);
            }
            Self::Unavailable => {}
        }
    }
    /// Visit the retained spans without expanding their coordinates.
    pub fn iter(&self) -> std::slice::Iter<'_, RangeMembership> {
        match self {
            Self::One(span) => std::slice::from_ref(span).iter(),
            Self::Many(spans) => spans.iter(),
            _ => [].iter(),
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::One(_) => 1,
            Self::Many(spans) => spans.len(),
            _ => 0,
        }
    }
}

/// Dirty causes in consumed order, with lazy allocation-free single-span traversal.
pub(in crate::eval) enum OrderedCauses {
    /// One dirty input needs no temporary collection.
    One(Option<(Key, u64)>),
    /// A single ordered read span can yield dirty coordinates on demand.
    Span {
        span: RangeMembership,
        next: Option<CellAddress>,
        remaining: usize,
    },
    /// Irregular traces require sorting by the consumed read order.
    Many(std::vec::IntoIter<(Key, u64)>),
}
impl OrderedCauses {
    /// Find the next cause using a short cache borrow, released before recursive evaluation.
    pub fn next(
        &mut self,
        validity: &Validity,
        budget: &mut crate::eval::budget::BudgetMeter,
    ) -> crate::eval::EvalResult<Option<(Key, u64)>> {
        match self {
            Self::One(cause) => Ok(cause.take()),
            Self::Many(causes) => Ok(causes.next()),
            Self::Span {
                span,
                next,
                remaining,
            } => {
                if *remaining == 0 {
                    return Ok(None);
                }
                let Validity::Causes(causes) = validity else {
                    return Err(crate::eval::EvaluationFailure::StalePlan);
                };
                while let Some(address) = next.take() {
                    budget.charge(crate::eval::WorkKind::Comparison, 1)?;
                    if address != span.last {
                        *next = if span.first.col == span.last.col {
                            address.row.checked_add(1).map(|row| CellAddress {
                                row,
                                col: address.col,
                            })
                        } else {
                            address.col.checked_add(1).map(|col| CellAddress {
                                row: address.row,
                                col,
                            })
                        };
                    }
                    let key = (span.sheet, address);
                    if let Some(&version) = causes.get(&key) {
                        *remaining = remaining.saturating_sub(1);
                        return Ok(Some((key, version)));
                    }
                }
                Ok(None)
            }
        }
    }
}
/// Inline retained entries keyed directly by full worksheet coordinates.
type Entries = CellMap<Cached>;
/// Persistent overlay results with reverse edges for transitive invalidation.
#[derive(Default)]
pub(in crate::eval) struct Cache {
    /// Live results keyed without worksheet spatial indexes.
    pub entries: Entries,
    /// Context used by the latest evaluation.
    pub context: Option<ContextKey>,
    /// Estimated bytes charged against persistent retention limits.
    pub retained_bytes: usize,
    /// Spill anchors whose previously exported children must be cleared.
    pub spill_resets: BTreeSet<Key>,
    /// Derived exact lookup prefixes; intentionally absent from snapshots.
    pub lookup_prefixes: crate::eval::adapters::search::execution::LookupPrefixes,
    reverse: CellMap<CellSet>,
    always_dirty: CellSet,
    ranges: RangeIndex,
    spill_ranges: RangeIndex,
    ranges_dirty: bool,
    next_version: u64,
}
impl Cache {
    /// Admit a complete batch before publishing values, indexes, and spill cleanup together.
    pub fn commit(
        &mut self,
        staged: Vec<(Key, Cached)>,
        retention: crate::eval::run::MemoRetentionLimits,
        consumer: Option<EvaluationTarget>,
        spill_resets: impl IntoIterator<Item = Key>,
    ) -> Result<(), EvaluationFailure> {
        let mut cells = self.len();
        let mut bytes = self.retained_bytes;
        let mut overflow = false;
        for (key, entry) in &staged {
            if let Some(old) = self.retained(*key) {
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
                consumed: crate::eval::run::MemoRetentionUsage {
                    cells: self.len(),
                    bytes: self.retained_bytes,
                },
                requested: crate::eval::run::MemoRetentionUsage {
                    cells: cells.saturating_sub(self.len()),
                    bytes: bytes.saturating_sub(self.retained_bytes),
                },
                source: consumer,
            });
        }
        self.next_version
            .checked_add(staged.len() as u64)
            .ok_or(EvaluationFailure::StalePlan)?;
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
    /// Look up a current result without evaluating or exposing dirty evidence.
    pub fn get(&self, key: Key) -> Option<&Cached> {
        self.retained(key).filter(|entry| entry.validity.is_clean())
    }
    /// Read retained evidence, including results that must not be exposed as current.
    pub fn retained(&self, key: Key) -> Option<&Cached> {
        self.entries.get(&key)
    }
    /// Count only values whose validity has already been established.
    pub fn valid_len(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.validity.is_clean())
            .count()
    }
    /// Count retained results across all sheets.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Remove a result and its point edges; range indices are rebuilt separately.
    pub fn remove(&mut self, key: Key) {
        self.always_dirty.remove(&key);
        if let Some(entry) = self.entries.remove(&key) {
            self.ranges_dirty |= !entry.dependencies.ranges.is_empty()
                || !entry.dependencies.point_runs.is_empty()
                || !entry.dependencies.spill_ranges.is_empty();
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
    }
    /// Replace a result and register its point edges and always-dirty status.
    pub fn insert(&mut self, key: Key, mut entry: Cached) -> Result<(), crate::model::ModelError> {
        key.1.validate()?;
        let previous = self.retained(key);
        let version = match previous {
            Some(old) if crate::eval::same_result_identity(&old.value, &entry.value) => old.version,
            _ => self
                .next_version
                .checked_add(1)
                .ok_or_else(|| crate::model::ModelError("cache generation overflow".into()))?,
        };
        entry.version = version;
        entry.validity = Validity::Clean;
        let retained_bytes = self
            .retained_bytes
            .checked_sub(previous.map_or(0, Cached::retained_bytes))
            .and_then(|bytes| bytes.checked_add(entry.retained_bytes()))
            .ok_or_else(|| crate::model::ModelError("cache retention size overflow".into()))?;
        let same_dependencies = previous.is_some_and(|old| old.dependencies == entry.dependencies);
        self.next_version = self.next_version.max(version);
        // Value edits commonly leave the consumed graph unchanged. Keep its
        // reverse edges, range indexes, and retained entry in place.
        if same_dependencies {
            let retained = self
                .entries
                .get_mut(&key)
                .ok_or_else(|| crate::model::ModelError("retained result disappeared".into()))?;
            *retained = entry;
            self.retained_bytes = retained_bytes;
            return Ok(());
        }
        self.remove(key);
        self.ranges_dirty |= !entry.dependencies.ranges.is_empty()
            || !entry.dependencies.point_runs.is_empty()
            || !entry.dependencies.spill_ranges.is_empty();
        self.retained_bytes = retained_bytes;
        if entry.dependencies.opaque || entry.dependencies.volatile {
            self.always_dirty.insert(key);
        }
        for dependency in &entry.dependencies.cells {
            self.reverse.entry(*dependency).or_default().insert(key);
        }
        self.entries.insert(key, entry);
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
            self.spill_ranges.take_readers(key, &mut pending);
            self.remove(key);
        }
        self.lookup_prefixes.invalidate(&seen);
    }

    /// Force spatial membership consumers after an authored occupancy/formula edit.
    pub fn invalidate_geometry(&mut self, seeds: impl IntoIterator<Item = Key>) {
        let seeds: Vec<_> = seeds.into_iter().collect();
        for key in &seeds {
            self.remove(*key);
        }
        let geometry = seeds.iter().copied().collect();
        self.invalidate_context_with_geometry(seeds, &geometry);
    }
    /// Retain scalar evidence while propagating only immediate invalidation causes.
    pub fn invalidate_values(&mut self, seeds: impl IntoIterator<Item = Key>) {
        let seeds: Vec<_> = seeds.into_iter().collect();
        // Authored literals are sources, never generation certificates. Even a
        // retained numeric-coercion result must force its immediate readers.
        for key in &seeds {
            self.remove(*key);
        }
        self.invalidate_context(seeds);
    }
    fn invalidate_context(&mut self, seeds: impl IntoIterator<Item = Key>) {
        self.invalidate_context_with_geometry(seeds, &CellSet::default());
    }
    fn invalidate_context_with_geometry(
        &mut self,
        seeds: impl IntoIterator<Item = Key>,
        geometry_seeds: &CellSet,
    ) {
        let seeds: Vec<_> = seeds.into_iter().collect();
        if seeds.is_empty() {
            return;
        }
        if self.ranges_dirty {
            self.rebuild_ranges();
        }
        let mut pending = seeds.clone();
        let mut seen = CellSet::default();
        for key in &seeds {
            self.force(*key);
        }
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                continue;
            }
            let mut readers = Vec::new();
            if let Some(points) = self.reverse.get(&key) {
                readers.extend(points.iter().copied());
            }
            self.ranges.readers(key, &mut readers);
            readers.sort_unstable();
            readers.dedup();
            // Geometry changes force the coordinate/producer that consumed the
            // membership guard. Its value readers remain conditionally valid.
            let mut geometry = Vec::new();
            if geometry_seeds.contains(&key)
                || self.retained(key).is_some_and(|entry| {
                    entry.dependencies.spill_sensitive
                        || matches!(
                            entry.dependencies.spill,
                            crate::eval::spill_publication::SpillRole::Anchor { .. }
                        )
                })
            {
                self.spill_ranges.readers(key, &mut geometry);
            }
            for reader in geometry {
                if reader != key {
                    self.force(reader);
                    readers.push(reader);
                }
            }
            let version = self.retained(key).map(|entry| entry.version);
            for reader in readers {
                if !seen.contains(&reader) {
                    pending.push(reader);
                }
                let Some(entry) = self.entries.get_mut(&reader) else {
                    continue;
                };
                let prior_bytes = entry.validity.retained_bytes();
                // A spill child must resolve its owner again: the anchor's
                // top-left scalar cannot certify the child's contents or extent.
                if matches!(entry.dependencies.spill,
                    crate::eval::spill_publication::SpillRole::Child { anchor } if (reader.0, anchor) == key)
                {
                    entry.validity = Validity::Forced;
                }
                match (version, &mut entry.validity) {
                    (_, Validity::Forced) => {}
                    (None, _) => entry.validity = Validity::Forced,
                    (Some(version), Validity::Clean) => {
                        entry.validity = Validity::Cause { key, version }
                    }
                    (Some(_), Validity::Cause { key: prior, .. }) if *prior == key => {}
                    (
                        Some(version),
                        Validity::Cause {
                            key: prior,
                            version: old,
                        },
                    ) => {
                        let mut causes = CellMap::default();
                        causes.insert(*prior, *old);
                        causes.insert(key, version);
                        entry.validity = Validity::Causes(causes);
                    }
                    (Some(version), Validity::Causes(causes)) => {
                        causes.entry(key).or_insert(version);
                    }
                }
                self.retained_bytes = self
                    .retained_bytes
                    .saturating_sub(prior_bytes)
                    .saturating_add(entry.validity.retained_bytes());
            }
        }
        self.lookup_prefixes.invalidate(&seen);
    }
    fn force(&mut self, key: Key) {
        if let Some(entry) = self.entries.get_mut(&key) {
            let prior_bytes = entry.validity.retained_bytes();
            entry.validity = Validity::Forced;
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(prior_bytes)
                .saturating_add(entry.validity.retained_bytes());
        }
    }

    /// Invalidate volatile, opaque, and context-sensitive results for a new evaluation.
    pub fn begin(&mut self, context: ContextKey) {
        if self.context.as_ref() == Some(&context) {
            // The common unchanged-context path only needs volatile/opaque roots.
            // insert/remove maintain this derived index, including snapshot restore.
            let seeds = self.always_dirty.iter().copied().collect::<Vec<_>>();
            self.invalidate_context(seeds);
            return;
        }
        let seeds = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                let trace = &entry.dependencies;
                trace.opaque
                    || trace.volatile
                    || self.context.as_ref().is_none_or(|previous| {
                        !trace
                            .context
                            .changed_fields(previous.inputs, context.inputs, false, false)
                            .is_empty()
                            || (trace.formatting && previous.formatting != context.formatting)
                            || (trace.number_locale
                                && previous.number_locale != context.number_locale)
                    })
            })
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        self.invalidate_context(seeds);
        self.context = Some(context);
    }
    /// Invalidate cells that consumed text policy and all their dependents.
    pub fn invalidate_text_policy(&mut self) {
        let seeds = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.dependencies.text_policy)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        self.invalidate(seeds);
    }
    /// Rebuild the rectangle lookup after dependency traces have changed.
    pub fn rebuild_ranges(&mut self) {
        self.ranges = RangeIndex::new(&self.entries);
        self.spill_ranges = RangeIndex::build(&self.entries, RangePurpose::SpillGeometry);
        self.ranges_dirty = false;
    }
}
/// Which dependency rectangles an index resolves.
enum RangePurpose {
    /// Values consumed by formulas, including compressed point reads.
    Values,
    /// Potential ownership or future-growth guards.
    SpillGeometry,
}
/// Per-sheet lookup of consumers whose read rectangles contain a changed cell.
#[derive(Default)]
struct RangeIndex {
    sheets: BTreeMap<usize, RangeTree>,
}
/// Finds cached consumers whose read rectangles include an edited cell on one sheet.
/// Row-sorted entries and subtree bounds prune unrelated rectangles; matched entries
/// can be consumed as invalidation advances through the dependency graph.
#[derive(Default)]
struct RangeTree {
    entries: Vec<(RangeMembership, Key)>,
    max_end: Vec<Option<u32>>,
    first_col: u32,
    last_col: u32,
}
impl RangeIndex {
    #[cfg(test)]
    fn intersects(&self, key: Key) -> bool {
        self.sheets.get(&key.0).is_some_and(|tree| {
            key.1.col >= tree.first_col
                && key.1.col <= tree.last_col
                && tree.intersects(1, 0, tree.entries.len(), key)
        })
    }
    fn readers(&self, key: Key, out: &mut Vec<Key>) {
        if let Some(tree) = self.sheets.get(&key.0)
            && key.1.col >= tree.first_col
            && key.1.col <= tree.last_col
        {
            tree.readers(1, 0, tree.entries.len(), key, out);
        }
    }

    fn new(entries: &Entries) -> Self {
        Self::build(entries, RangePurpose::Values)
    }
    fn build(entries: &Entries, purpose: RangePurpose) -> Self {
        let mut index = Self::default();
        for (&(sheet, at), entry) in entries {
            let (ranges, points) = if !matches!(purpose, RangePurpose::Values) {
                (entry.dependencies.spill_ranges.as_slice(), &[][..])
            } else {
                (
                    entry.dependencies.ranges.as_slice(),
                    entry.dependencies.point_runs.as_slice(),
                )
            };
            for range in ranges.iter().chain(points) {
                index
                    .sheets
                    .entry(range.sheet)
                    .or_default()
                    .entries
                    .push((*range, (sheet, at)));
            }
        }
        for tree in index.sheets.values_mut() {
            (tree.first_col, tree.last_col) = tree
                .entries
                .iter()
                .fold((u32::MAX, 0), |(first, last), (range, _)| {
                    (first.min(range.first.col), last.max(range.last.col))
                });
            tree.entries
                .sort_unstable_by_key(|(range, _)| range.first.row);
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
    #[cfg(test)]
    fn intersects(&self, node: usize, lo: usize, hi: usize, key: Key) -> bool {
        let Some((range, _)) = self.entries.get(lo) else {
            return false;
        };
        if lo >= hi
            || hi > self.entries.len()
            || range.first.row > key.1.row
            || self
                .max_end
                .get(node)
                .copied()
                .flatten()
                .is_none_or(|end| end < key.1.row)
        {
            return false;
        }
        // SAFETY: The preceding guard establishes lo < hi <= entries.len().
        if hi - lo == 1 {
            return range.contains(key.0, key.1);
        }
        // SAFETY: lo < hi proves hi - lo is defined and the midpoint cannot exceed hi.
        let mid = lo + (hi - lo) / 2;
        let Some(left) = node.checked_mul(2) else {
            return false;
        };
        let Some(right) = left.checked_add(1) else {
            return false;
        };
        self.intersects(left, lo, mid, key) || self.intersects(right, mid, hi, key)
    }

    fn readers(&self, node: usize, lo: usize, hi: usize, key: Key, out: &mut Vec<Key>) {
        let Some((range, reader)) = self.entries.get(lo) else {
            return;
        };
        if lo >= hi
            || hi > self.entries.len()
            || range.first.row > key.1.row
            || self
                .max_end
                .get(node)
                .copied()
                .flatten()
                .is_none_or(|end| end < key.1.row)
        {
            return;
        }
        // SAFETY: The preceding guard establishes lo < hi <= entries.len().
        if hi - lo == 1 {
            if range.contains(key.0, key.1) {
                out.push(*reader);
            }
            return;
        }
        // SAFETY: lo < hi proves hi - lo is defined and the midpoint cannot exceed hi.
        let mid = lo + (hi - lo) / 2;
        let Some(left) = node.checked_mul(2) else {
            return;
        };
        let Some(right) = left.checked_add(1) else {
            return;
        };
        self.readers(left, lo, mid, key, out);
        self.readers(right, mid, hi, key, out);
    }

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
        224usize
            .saturating_add(self.validity.retained_bytes())
            .saturating_add(self.value.text_storage_bytes())
            .saturating_add(self.dependencies.cells.len().saturating_mul(48))
            .saturating_add(
                (self.dependencies.ranges.len()
                    + self.dependencies.point_runs.len()
                    + self.dependencies.spill_ranges.len()
                    + self.dependencies.value_reads.len())
                .saturating_mul(64),
            )
    }
}

#[cfg(test)]
mod replacement_tests {
    use super::*;

    fn entry(value: f64, source: Key) -> Cached {
        Cached {
            value: Arc::new(CellValue::Number(value)),
            version: 0,
            validity: Validity::Clean,
            dependencies: Box::new(Dependencies {
                cells: vec![source],
                ranges: vec![RangeMembership {
                    sheet: source.0,
                    first: source.1,
                    last: source.1,
                }],
                ..Default::default()
            }),
        }
    }

    #[test]
    fn unchanged_dependencies_preserve_indexes_and_update_result_versions() {
        let target = (0, CellAddress { row: 0, col: 1 });
        let source = (0, CellAddress { row: 0, col: 0 });
        let other = (0, CellAddress { row: 4, col: 0 });
        let mut cache = Cache::default();
        cache.insert(target, entry(1., source)).unwrap();
        cache.rebuild_ranges();
        let old_version = cache.get(target).unwrap().version;
        cache.insert(target, entry(2., source)).unwrap();
        assert!(!cache.ranges_dirty);
        assert!(cache.reverse.get(&source).unwrap().contains(&target));
        assert!(cache.get(target).unwrap().version > old_version);
        let version = cache.get(target).unwrap().version;
        cache.insert(target, entry(2., source)).unwrap();
        assert_eq!(cache.get(target).unwrap().version, version);
        cache.insert(target, entry(3., other)).unwrap();
        assert!(cache.ranges_dirty);
        assert!(!cache.reverse.contains_key(&source));
        assert!(cache.reverse.get(&other).unwrap().contains(&target));
        let mut volatile = entry(4., other);
        volatile.dependencies.volatile = true;
        cache.insert(target, volatile).unwrap();
        assert!(cache.always_dirty.contains(&target));
        cache.insert(target, entry(4., other)).unwrap();
        assert!(!cache.always_dirty.contains(&target));
        cache.invalidate_values([other]);
        assert!(cache.get(target).is_none());
        assert_eq!(
            cache.retained_bytes,
            cache.retained(target).unwrap().retained_bytes()
        );
    }
}

#[cfg(test)]
mod validity_accounting_tests {
    use super::*;

    #[test]
    fn dirty_storage_accounting_tracks_multiple_duplicate_and_forced_causes() {
        let first = (0, CellAddress { row: 0, col: 0 });
        let second = (0, CellAddress { row: 1, col: 0 });
        let target = (0, CellAddress { row: 0, col: 1 });
        let mut cache = Cache::default();
        for key in [first, second] {
            cache
                .insert(
                    key,
                    Cached {
                        value: Arc::new(1.0.into()),
                        version: 0,
                        validity: Validity::Clean,
                        dependencies: Box::default(),
                    },
                )
                .unwrap();
        }
        let reader = Cached {
            value: Arc::new(CellValue::Text("retained result".into())),
            version: 0,
            validity: Validity::Clean,
            dependencies: Box::new(Dependencies {
                cells: vec![first, second],
                ..Default::default()
            }),
        };
        cache.insert(target, reader).unwrap();
        let clean_bytes = cache.retained_bytes;
        let check = |cache: &Cache| {
            assert_eq!(
                cache.retained_bytes,
                [first, second, target]
                    .into_iter()
                    .map(|key| cache.retained(key).unwrap().retained_bytes())
                    .sum::<usize>()
            );
        };
        cache.invalidate_context([first]);
        check(&cache);
        assert_eq!(cache.retained_bytes, clean_bytes);
        cache.invalidate_context([second]);
        check(&cache);
        assert!(cache.retained_bytes > clean_bytes);
        let multiple_bytes = cache.retained_bytes;
        cache.invalidate_context([first, first, second]);
        check(&cache);
        assert_eq!(cache.retained_bytes, multiple_bytes);
        cache.force(target);
        check(&cache);
        assert_eq!(cache.retained_bytes, clean_bytes);
        cache.force(target);
        check(&cache);
        cache.invalidate_context([first, second]);
        check(&cache);
        assert_eq!(cache.retained_bytes, clean_bytes);
    }
}

#[cfg(test)]
mod retention_overflow_tests {
    use super::*;
    fn entry() -> Cached {
        Cached {
            version: 0,
            validity: Validity::Clean,
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
                crate::eval::run::MemoRetentionLimits::default(),
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
                        version: 0,
                        validity: Validity::Clean,
                        value: Arc::new(1.0.into()),
                        dependencies: Box::new(deps),
                    },
                )
                .unwrap();
        }
        let mut index = RangeIndex::new(&cache.entries);
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
                        version: 0,
                        validity: Validity::Clean,
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
                        version: 0,
                        validity: Validity::Clean,
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
                    let mut index = RangeIndex::new(&cache.entries);
                    let key = (sheet, CellAddress { row, col });
                    let mut retained = Vec::new();
                    index.readers(key, &mut retained);
                    assert_eq!(index.intersects(key), !retained.is_empty());
                    // Read-only certificates must survive repeated queries.
                    let mut repeated = Vec::new();
                    index.readers(key, &mut repeated);
                    assert_eq!(retained, repeated);
                    index.take_readers(key, &mut actual);
                    retained.sort_unstable();
                    actual.sort_unstable();
                    assert_eq!(retained, actual);
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
        use crate::eval::{DependencyReadSink, EvaluationRun, FormulaCompiler, WorkbookView};
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
        let policy = crate::eval::PreparationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
        let mut observed = Vec::new();
        for with_internal in [false, true] {
            let mut events = Events::default();
            let mut run = EvaluationRun::new(
                WorkbookView::new(&book, &plan, &policy).unwrap(),
                crate::eval::EvaluationInputs::default(),
                crate::eval::EvaluationBudgets::default(),
            )
            .unwrap()
            .with_dependency_sink(&mut events);
            if with_internal {
                run.session.overlay_reads = Some(Reads::default());
            }
            let value = run
                .evaluate_cell(crate::eval::EvaluationTarget {
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
            == Some(crate::eval::EvaluationTarget {
                sheet: 0,
                address: key(0, 0, 5).1
            })
            && e.read
                == DependencyRead::Cell(crate::eval::EvaluationTarget {
                    sheet: 0,
                    address: key(0, 1, 0).1
                })));
    }

    #[test]
    fn nested_consumers_keep_their_own_guards_and_outside_reads() {
        use crate::eval::DependencyReadSink;
        let parent = key(0, 10, 10);
        let child = key(0, 2, 1);
        let owner = key(0, 0, 0);
        let mut reads = Reads::default();
        let emit = |reads: &mut Reads, consumer: Key, read| {
            reads.read(ReadEvent {
                consumer: Some(crate::eval::EvaluationTarget {
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
            DependencyRead::Cell(crate::eval::EvaluationTarget {
                sheet: 0,
                address: owner.1,
            }),
        );
        emit(
            &mut reads,
            parent,
            DependencyRead::Cell(crate::eval::EvaluationTarget {
                sheet: 0,
                address: child.1,
            }),
        );
        emit(
            &mut reads,
            parent,
            DependencyRead::Cell(crate::eval::EvaluationTarget {
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
            DependencyRead::Cell(crate::eval::EvaluationTarget {
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

#[cfg(test)]
mod value_order_tests {
    use super::*;

    #[test]
    fn compressed_read_spans_preserve_first_occurrences_across_repeated_traversals() {
        let key = |sheet, row, col| (sheet, CellAddress { row, col });
        let sequence = [
            key(0, 5, 4),
            key(0, 6, 4),
            key(0, 7, 4),
            key(0, 6, 4),
            key(0, 0, 2),
            key(0, 0, 3),
            key(0, 0, 4),
            key(0, 5, 4),
            key(1, 0, 0),
            key(1, 1, 0),
            key(0, 0, 2),
            key(0, 3, 9),
        ];
        for mask in 1usize..(1 << sequence.len()) {
            let mut dependencies = Dependencies::default();
            for point in sequence {
                dependencies.value_read(point);
            }
            let mut causes = CellMap::default();
            for (position, point) in sequence.iter().enumerate() {
                if mask & (1 << position) != 0 {
                    causes.insert(*point, 1);
                }
            }
            let mut seen = CellSet::default();
            let expected = sequence
                .into_iter()
                .filter(|point| causes.contains_key(point) && seen.insert(*point))
                .collect::<Vec<_>>();
            let mut budget =
                crate::eval::budget::BudgetMeter::new(crate::eval::EvaluationBudgets::default())
                    .unwrap();
            let causes = Validity::Causes(causes);
            let actual = dependencies
                .ordered_causes(&causes, &mut budget)
                .unwrap()
                .unwrap();
            let mut actual = actual;
            let mut observed = Vec::new();
            while let Some((point, _)) = actual.next(&causes, &mut budget).unwrap() {
                observed.push(point);
            }
            let actual = observed;
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn one_span_yields_causes_lazily_and_stops_after_last_dirty_coordinate() {
        let first = (0, CellAddress { row: 0, col: 0 });
        let second = (0, CellAddress { row: 1, col: 0 });
        let dependencies = Dependencies {
            value_reads: ValueReadOrder::One(RangeMembership {
                sheet: 0,
                first: first.1,
                last: CellAddress {
                    row: 1_000_000,
                    col: 0,
                },
            }),
            ..Default::default()
        };
        let causes = Validity::Causes([(first, 7), (second, 9)].into_iter().collect());
        let mut budget = crate::eval::budget::BudgetMeter::new(crate::eval::EvaluationBudgets {
            run_work: 4,
            ..Default::default()
        })
        .unwrap();
        let mut ordered = dependencies
            .ordered_causes(&causes, &mut budget)
            .unwrap()
            .unwrap();
        assert_eq!(
            ordered.next(&causes, &mut budget).unwrap(),
            Some((first, 7))
        );
        assert_eq!(
            ordered.next(&causes, &mut budget).unwrap(),
            Some((second, 9))
        );
        assert_eq!(ordered.next(&causes, &mut budget).unwrap(), None);
    }

    #[test]
    fn metadata_and_membership_without_value_evidence_do_not_certify_multiple_causes() {
        let first = (0, CellAddress { row: 0, col: 0 });
        let second = (0, CellAddress { row: 1, col: 0 });
        let mut dependencies = Dependencies::default();
        dependencies.value_read(first);
        let causes = Validity::Causes([(first, 1), (second, 2)].into_iter().collect());
        let mut budget =
            crate::eval::budget::BudgetMeter::new(crate::eval::EvaluationBudgets::default())
                .unwrap();
        assert!(
            dependencies
                .ordered_causes(&causes, &mut budget)
                .unwrap()
                .is_none()
        );
        dependencies.value_read(second);
        dependencies.metadata_reads = true;
        assert!(
            dependencies
                .ordered_causes(&causes, &mut budget)
                .unwrap()
                .is_none()
        );
        let cause = Validity::Cause {
            key: first,
            version: 1,
        };
        assert!(
            dependencies
                .ordered_causes(&cause, &mut budget)
                .unwrap()
                .is_none()
        );
    }
}
