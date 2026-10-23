//! Bounded typed exact-search prefixes and text selections retained by incremental overlays.
//! Observed runs keep ordinary reads; retained axes expire when scanned cells change.
use super::*;
use plan::{LookupTable, LookupVector};
use selection::CandidateReader;
use std::collections::{BTreeMap, HashMap, HashSet};

// Only bounded finite-number/boolean keys and coordinates use this hasher.
// Equality still compares the complete key; arbitrary strings never enter it.
/// Fast map restricted to bounded numeric/boolean lookup-prefix keys.
type PrefixMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>;
/// Active lookup axes used to guard recursive prefix construction.
type AxisSet = HashSet<Axis, foldhash::fast::RandomState>;

/// Physical row or column interval identifying a reusable search prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Axis {
    sheet: usize,
    first: CellAddress,
    last: CellAddress,
}
/// Sheet, orientation (vertical first), and fixed column or row.
type Partition = (usize, bool, u32);
/// Inclusive physical endpoints within a lookup partition.
type Interval = (u32, u32);
impl Axis {
    fn location(self) -> (Partition, Interval) {
        if self.first.col == self.last.col {
            (
                (self.sheet, true, self.first.col),
                (self.first.row, self.last.row),
            )
        } else {
            (
                (self.sheet, false, self.first.row),
                (self.first.col, self.last.col),
            )
        }
    }
    fn from_location((sheet, vertical, fixed): Partition, (first, last): Interval) -> Self {
        let address = |n| {
            if vertical {
                CellAddress { row: n, col: fixed }
            } else {
                CellAddress { row: fixed, col: n }
            }
        };
        Self {
            sheet,
            first: address(first),
            last: address(last),
        }
    }
}
/// Finite numeric or boolean exact-match key with normalized signed zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Number(u64),
    Boolean(bool),
}
impl Key {
    fn candidate(value: &CellValue) -> Option<Self> {
        match value {
            Number(n) if n.is_finite() => {
                Some(Self::Number(if *n == 0. { 0 } else { n.to_bits() }))
            }
            Boolean(b) => Some(Self::Boolean(*b)),
            _ => None,
        }
    }
    fn search(value: &CellValue) -> Option<Self> {
        if *value == Blank {
            Some(Self::Number(0))
        } else {
            Self::candidate(value)
        }
    }
}
/// First matching positions already scanned along one logical lookup axis.
#[derive(Default)]
struct Prefix {
    first: PrefixMap<Key, usize>,
    // Original UTF-16 query identity preserves every ordinary comparison rule.
    // Arbitrary text uses the standard randomized hasher, never PrefixMap.
    text: HashMap<Vec<u16>, Option<usize>>,
    // Retained entries plus owned units, so admission recount is constant per axis.
    text_storage: usize,
    next: usize,
    exhausted: bool,
}
/// Bounded exact-search prefix cache with session-local recursion guards.
#[derive(Default)]
pub(crate) struct Prefixes {
    enabled: bool,
    // Partition first by sheet and physical column/row, then by bounded range.
    axes: BTreeMap<Partition, BTreeMap<Interval, Prefix>>,
    active: AxisSet,
    reserved: usize,
}
impl Prefixes {
    /// Invalidate changed present cells and gaps through their consumed axis intervals.
    /// Every indexed candidate lies inside its axis at an offset below `next`;
    /// an exhausted axis covers its whole interval. These guards therefore cover
    /// both stored candidates and absent gaps without per-cell reverse flags.
    pub(crate) fn invalidate(&mut self, changed: &crate::coordinate_hash::CellSet) {
        let mut affected = AxisSet::default();
        for &key in changed {
            // Every consumed candidate and sparse gap belongs to one of these
            // two physical partitions; separate per-cell flags add no coverage.
            for (partition, position) in [
                ((key.0, true, key.1.col), key.1.row),
                ((key.0, false, key.1.row), key.1.col),
            ] {
                if let Some(ranges) = self.axes.get(&partition) {
                    for (&interval, prefix) in ranges.range(..=(position, u32::MAX)) {
                        if position <= interval.1
                            && (prefix.exhausted
                                || ((position - interval.0) as usize) < prefix.next)
                        {
                            affected.insert(Axis::from_location(partition, interval));
                        }
                    }
                }
            }
        }
        for axis in affected {
            self.remove(axis);
        }
        self.recount();
    }
    /// Expire consumed search membership intersecting a newly finalized footprint.
    /// Dependency-valid restored footprints do not call this admission boundary.
    pub(crate) fn invalidate_footprint(
        &mut self,
        sheet: usize,
        first: CellAddress,
        last: CellAddress,
    ) {
        let mut affected = AxisSet::default();
        for (vertical, fixed_first, fixed_last, start, end) in [
            (true, first.col, last.col, first.row, last.row),
            (false, first.row, last.row, first.col, last.col),
        ] {
            for (&partition, ranges) in self
                .axes
                .range((sheet, vertical, fixed_first)..=(sheet, vertical, fixed_last))
            {
                for (&interval, prefix) in ranges.range(..=(end, u32::MAX)) {
                    if start <= interval.1
                        && (prefix.exhausted
                            || (start.saturating_sub(interval.0) as usize) < prefix.next)
                    {
                        affected.insert(Axis::from_location(partition, interval));
                    }
                }
            }
        }
        for axis in affected {
            self.remove(axis);
        }
        self.recount();
    }
    fn contains(&self, axis: Axis) -> bool {
        let (partition, interval) = axis.location();
        self.axes
            .get(&partition)
            .is_some_and(|ranges| ranges.contains_key(&interval))
    }
    fn remove(&mut self, axis: Axis) -> Option<Prefix> {
        let (partition, interval) = axis.location();
        let ranges = self.axes.get_mut(&partition)?;
        let prefix = ranges.remove(&interval);
        if ranges.is_empty() {
            self.axes.remove(&partition);
        }
        prefix
    }
    fn insert(&mut self, axis: Axis, prefix: Prefix) {
        let (partition, interval) = axis.location();
        self.axes
            .entry(partition)
            .or_default()
            .insert(interval, prefix);
    }
    fn recount(&mut self) {
        self.reserved =
            self.axes
                .values()
                .flat_map(|ranges| ranges.values())
                .fold(0usize, |total, prefix| {
                    total
                        .saturating_add(1)
                        .saturating_add(prefix.first.len())
                        .saturating_add(prefix.text_storage)
                });
    }
    /// Revalidate retained storage against this evaluation's collection limit.
    pub(crate) fn admit(&mut self, limit: usize) {
        self.recount();
        if self.reserved > MAX_ENTRIES.min(limit) {
            self.axes.clear();
            self.reserved = 0;
        }
        self.enable();
    }
    /// Enable prefix reuse for eligible unobserved lookups in this session.
    pub(crate) fn enable(&mut self) {
        self.enabled = true;
    }
}
const MAX_ENTRIES: usize = 65_536;

/// None means fall back to the ordinary selector; Some carries an actual result.
pub(super) fn select(
    session: &mut Session<'_>,
    table: &LookupTable,
    vector: LookupVector,
    key: &CellValue,
    mode: SearchMode,
    is_match: bool,
) -> Option<FunctionResult<Option<usize>>> {
    if !session.lookup_prefixes.enabled
        || mode != SearchMode::Exact
        || session.dependency_sink.is_some()
        || session.cell_reader.is_some()
        || session.cell_observer.is_some()
        || session.builtin_invocation_observer.is_some()
        || session.execution_observer.is_some()
        || session.extensions.is_some()
    {
        return None;
    }
    let needle = Key::search(key);
    let text = key.text_ref();
    if needle.is_none() && text.is_none() {
        return None;
    }
    let LookupTable::Reference(area) = table else {
        return None;
    };
    let (first, last) = match vector.axis {
        SearchAxis::Vertical => (
            CellAddress {
                row: area.r0,
                col: area.c0 + vector.fixed as u32,
            },
            CellAddress {
                row: area.r1,
                col: area.c0 + vector.fixed as u32,
            },
        ),
        SearchAxis::Horizontal => (
            CellAddress {
                row: area.r0 + vector.fixed as u32,
                col: area.c0,
            },
            CellAddress {
                row: area.r0 + vector.fixed as u32,
                col: area.c1,
            },
        ),
    };
    let axis = Axis {
        sheet: area.sheet,
        first,
        last,
    };
    let search_area = Area {
        sheet: axis.sheet,
        r0: first.row,
        r1: last.row,
        c0: first.col,
        c1: last.col,
        ..*area
    };
    if let Err(failure) = session.resolve_spills_in(search_area) {
        return Some(Err(failure.into()));
    }
    if session.array_result_policy == ArrayResultPolicy::DynamicSpill
        && !session.ready_area(search_area)
    {
        return None;
    }

    let cap = MAX_ENTRIES.min(session.budget.limits.materialized_values);
    if session.lookup_prefixes.active.contains(&axis)
        || (session.lookup_prefixes.reserved >= cap && !session.lookup_prefixes.contains(axis))
    {
        return None;
    }
    // Bound query storage before cloning. One owned unit reserves two bytes;
    // together with entry/cell counts this also bounds total retained text bytes.
    let text = if let Some(text) = text {
        let units = text.units().count();
        if units > cap {
            return None;
        }
        let mut query = Vec::new();
        if query.try_reserve_exact(units).is_err() {
            return None;
        }
        // SAFETY: units counts this immutable text iterator; the reservation
        // above covers every code unit copied by extend.
        query.extend(text.units());
        Some(query)
    } else {
        None
    };
    let existing = session.lookup_prefixes.remove(axis);
    let mut prefix = match existing {
        Some(prefix) => prefix,
        None => {
            if let Err(failure) = session.budget.allocation(1) {
                return Some(Err(failure.into()));
            }
            session.lookup_prefixes.reserved += 1;
            Prefix::default()
        }
    };
    session.lookup_prefixes.active.insert(axis);
    let mut retain = true;
    let result = (|| {
        session.budget.charge(WorkKind::Comparison, 1)?;
        if let Some(found) = text.as_ref().and_then(|query| prefix.text.get(query)) {
            if session.budget.profiling {
                session.run_usage.lookup_prefix_hits =
                    session.run_usage.lookup_prefix_hits.saturating_add(1);
            }
            return Ok(*found);
        }
        if let Some(needle) = needle {
            if let Some(&found) = prefix.first.get(&needle) {
                if session.budget.profiling {
                    session.run_usage.lookup_prefix_hits =
                        session.run_usage.lookup_prefix_hits.saturating_add(1);
                }
                return Ok(Some(found));
            }
            if prefix.exhausted {
                if session.budget.profiling {
                    session.run_usage.lookup_prefix_hits =
                        session.run_usage.lookup_prefix_hits.saturating_add(1);
                }
                return Ok(None);
            }
        }
        let mut reader = sparse::LookupReader {
            session,
            table,
            vector,
            key,
            mode,
            is_match,
            forward: None,
        };
        let mut next = reader.next(if text.is_some() { 0 } else { prefix.next }, vector.len)?;
        while let Some(index) = next {
            if reader.session.budget.profiling {
                reader.session.run_usage.lookup_prefix_scans = reader
                    .session
                    .run_usage
                    .lookup_prefix_scans
                    .saturating_add(1);
            }
            let value = reader.session.lookup_read(table, vector, index)?;
            // A recursive provider can temporarily yield #REF before completing.
            // Do not retain a prefix that skipped any spreadsheet error/nonfinite.
            if matches!(value, Error(_)) || matches!(value,Number(n) if !n.is_finite()) {
                retain = false;
            }
            if let Some(candidate) = Key::candidate(&value)
                && let std::collections::hash_map::Entry::Vacant(entry) =
                    prefix.first.entry(candidate)
            {
                if reader.session.lookup_prefixes.reserved < cap {
                    reader.session.budget.allocation(1)?;
                    reader.session.lookup_prefixes.reserved += 1;
                    entry.insert(index);
                } else {
                    retain = false;
                }
            }
            let following = index.checked_add(1).ok_or(EvaluationFailure::StalePlan)?;
            prefix.next = prefix.next.max(following);
            if lookup_compare(key, &value, mode, is_match, &mut reader.session.budget)?
                == Some(Ordering::Equal)
            {
                return Ok(Some(index));
            }
            next = reader.next(following, vector.len)?;
        }
        prefix.exhausted = true;
        Ok(None)
    })();
    if retain && let (Some(query), Ok(selected)) = (text, &result) {
        let requested = query.len().saturating_add(1);
        if !prefix.text.contains_key(&query)
            && requested <= cap.saturating_sub(session.lookup_prefixes.reserved)
            && prefix.text.try_reserve(1).is_ok()
        {
            // SAFETY: requested is positive, and the local remaining-cap check
            // proves reserved + requested <= cap <= usize::MAX.
            session.lookup_prefixes.reserved += requested;
            prefix.text_storage = prefix.text_storage.saturating_add(requested);
            prefix.text.insert(query, *selected);
        }
    }
    // A reused prefix carries selection values from other consumers. Attribute
    // its consumed prefix to this consumer too, so any changed candidate dirties
    // every lookup that reused it. Session sources cannot change during execution.
    if session.overlay_reads.is_some()
        && let Ok(selected) = &result
    {
        let last = selected.map_or(axis.last, |index| match vector.axis {
            SearchAxis::Vertical => CellAddress {
                row: axis.first.row + index as u32,
                col: axis.first.col,
            },
            SearchAxis::Horizontal => CellAddress {
                row: axis.first.row,
                col: axis.first.col + index as u32,
            },
        });
        session.read(DependencyRead::SparseMembership {
            sheet: axis.sheet,
            first: if session.array_result_policy == ArrayResultPolicy::DynamicSpill {
                CellAddress { row: 0, col: 0 }
            } else {
                axis.first
            },
            last,
        });
    }
    session.lookup_prefixes.active.remove(&axis);
    if retain && result.is_ok() {
        session.lookup_prefixes.insert(axis, prefix);
    }
    // Dropped/failed prefixes keep their admission reservation. This bounds all
    // nested allocations conservatively without a second unbounded retention map.
    Some(result)
}

#[cfg(test)]
mod interval_tests {
    use super::*;

    fn lookup(run: &mut Session<'_>, formula: &str) -> EvalResult<CellValue> {
        let parsed = poirot_parsing::formula::parse_formula(formula).unwrap();
        let bound = bound::FormulaPlan::bind(&parsed).unwrap();
        let at = "Z20".parse().unwrap();
        let value = run.expr(0, at, &bound.expression())?;
        run.scalar(at, value)
    }

    #[test]
    fn error_nonfinite_and_capacity_failures_discard_extended_prefixes() {
        for suffix in [Error(NA), Number(f64::INFINITY), Number(2.)] {
            let mut book = Workbook::new();
            let sheet = book.add_sheet("S").unwrap();
            sheet.set_value("A1".parse().unwrap(), 1.).unwrap();
            sheet
                .set_value("A2".parse().unwrap(), suffix.clone())
                .unwrap();
            sheet.set_value("A3".parse().unwrap(), 3.).unwrap();
            let mut run = Session::with_context(&book, EvaluationInputs::default());
            run.array_result_policy = ArrayResultPolicy::ScalarIntersection;
            run.lookup_prefixes.enable();
            assert_eq!(lookup(&mut run, "MATCH(1,A1:A3,0)").unwrap(), Number(1.));
            assert_eq!(run.lookup_prefixes.axes.len(), 1);
            if suffix == Number(2.) {
                run.budget.limits.materialized_values = 3;
            }
            // POI numeric ingress retains infinity; comparison rejects it as NUM.
            let expected = if matches!(suffix, Number(n) if !n.is_finite()) {
                Error(Num)
            } else {
                Number(3.)
            };
            assert_eq!(lookup(&mut run, "MATCH(3,A1:A3,0)").unwrap(), expected);
            assert!(run.lookup_prefixes.axes.is_empty(), "{suffix:?}");
        }
    }

    #[test]
    fn budget_failure_discards_the_extended_prefix() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value("A1".parse().unwrap(), 1.).unwrap();
        sheet
            .set_value("A2".parse().unwrap(), "a long unread literal")
            .unwrap();
        let mut run = Session::with_context(&book, EvaluationInputs::default());
        run.lookup_prefixes.enable();
        assert_eq!(lookup(&mut run, "MATCH(1,A1:A2,0)").unwrap(), Number(1.));
        // The retained first candidate succeeds; extending to the next candidate
        // refuses its text before the newly extended prefix can be published.
        run.budget.limits.text_bytes = 8;
        let failure = lookup(&mut run, "MATCH(2,A1:A2,0)").unwrap_err();
        assert_eq!(
            failure.budget().unwrap().target,
            Some(EvaluationTarget {
                sheet: 0,
                address: "A2".parse().unwrap()
            })
        );
        assert!(run.lookup_prefixes.axes.is_empty());
    }

    #[test]
    fn interval_guards_cover_present_cells_gaps_and_overlapping_axes() {
        let vertical = Axis {
            sheet: 0,
            first: CellAddress { row: 2, col: 4 },
            last: CellAddress { row: 9, col: 4 },
        };
        let horizontal = Axis {
            sheet: 0,
            first: CellAddress { row: 4, col: 2 },
            last: CellAddress { row: 4, col: 9 },
        };
        for row in 0..12 {
            for col in 0..12 {
                for exhausted in [false, true] {
                    let mut prefixes = Prefixes::default();
                    for axis in [vertical, horizontal] {
                        prefixes.insert(
                            axis,
                            Prefix {
                                next: 4,
                                exhausted,
                                ..Default::default()
                            },
                        );
                    }
                    let changed = CellAddress { row, col };
                    prefixes.invalidate(&[(0, changed)].into_iter().collect());
                    for axis in [vertical, horizontal] {
                        let in_range = row >= axis.first.row
                            && row <= axis.last.row
                            && col >= axis.first.col
                            && col <= axis.last.col;
                        let offset = if axis.first.col == axis.last.col {
                            row.saturating_sub(axis.first.row)
                        } else {
                            col.saturating_sub(axis.first.col)
                        };
                        assert_eq!(
                            prefixes.contains(axis),
                            !(in_range && (exhausted || offset < 4))
                        );
                    }
                }
            }
        }
    }
}
