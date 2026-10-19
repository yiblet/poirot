//! Bounded numeric/boolean exact-search prefixes, retained by incremental overlays.
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
    next: usize,
    exhausted: bool,
    cells: Vec<CellAddress>,
    published_cells: usize,
}
/// Bounded exact-search prefix cache with session-local recursion guards.
#[derive(Default)]
pub(crate) struct Prefixes {
    enabled: bool,
    // Partition first by sheet and physical column/row, then by bounded range.
    axes: BTreeMap<Partition, BTreeMap<Interval, Prefix>>,
    active: AxisSet,
    reserved: usize,
    // Presence is the overlay-owned in-index flag; each set covers overlapping axes.
    members: crate::coordinate_hash::CellMap<AxisSet>,
    #[cfg(test)]
    membership_writes: usize,
}
impl Prefixes {
    /// Invalidate indexed cells by their flag/reverse memberships, and absent
    /// coordinates through sparse gap guards. No workbook cell is mutated.
    pub(crate) fn invalidate(&mut self, changed: &crate::coordinate_hash::CellSet) {
        let mut affected = AxisSet::default();
        for &key in changed {
            if let Some(axes) = self.members.get(&key) {
                affected.extend(axes.iter().copied());
            }
            // Only these two physical partitions can contain an inserted gap.
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
            if let Some(prefix) = self.remove(axis) {
                self.unregister(axis, &prefix);
            }
        }
        self.recount();
    }
    /// Publish only newly scanned cell memberships alongside a successful batch.
    pub(crate) fn publish(&mut self) {
        for (&partition, ranges) in &mut self.axes {
            for (&interval, prefix) in ranges {
                let axis = Axis::from_location(partition, interval);
                for &cell in prefix.cells.iter().skip(prefix.published_cells) {
                    self.members
                        .entry((axis.sheet, cell))
                        .or_default()
                        .insert(axis);
                    #[cfg(test)]
                    {
                        self.membership_writes += 1;
                    }
                }
                prefix.published_cells = prefix.cells.len();
            }
        }
        self.recount();
    }
    /// Forget only memberships actually published before this axis was removed.
    fn unregister(&mut self, axis: Axis, prefix: &Prefix) {
        for &cell in prefix.cells.iter().take(prefix.published_cells) {
            let key = (axis.sheet, cell);
            if let Some(axes) = self.members.get_mut(&key) {
                axes.remove(&axis);
                if axes.is_empty() {
                    self.members.remove(&key);
                }
            }
        }
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
        self.reserved = self
            .axes
            .values()
            .flat_map(|ranges| ranges.values())
            .map(|prefix| 1 + prefix.first.len() + prefix.cells.len())
            .sum();
    }
    /// Revalidate retained storage against this evaluation's collection limit.
    pub(crate) fn admit(&mut self, limit: usize) {
        self.recount();
        if self.reserved > MAX_ENTRIES.min(limit) {
            self.axes.clear();
            self.members.clear();
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
    source: TextSourceVersion,
) -> Option<FunctionResult<Option<usize>>> {
    if !session.lookup_prefixes.enabled
        || mode != SearchMode::Exact
        || session.array_result_policy == ArrayResultPolicy::DynamicSpill
        || session.dependency_sink.is_some()
        || session.cell_reader.is_some()
        || session.cell_observer.is_some()
        || session.builtin_invocation_observer.is_some()
        || session.execution_observer.is_some()
        || session.extensions.is_some()
    {
        return None;
    }
    let needle = Key::search(key)?;
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
    let cap = MAX_ENTRIES.min(session.budget.limits.materialized_values);
    if session.lookup_prefixes.active.contains(&axis)
        || (session.lookup_prefixes.reserved >= cap && !session.lookup_prefixes.contains(axis))
    {
        return None;
    }
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
        if let Some(&found) = prefix.first.get(&needle) {
            session.run_usage.lookup_prefix_hits += 1;
            return Ok(Some(found));
        }
        if prefix.exhausted {
            session.run_usage.lookup_prefix_hits += 1;
            return Ok(None);
        }
        let mut reader = sparse::LookupReader {
            session,
            table,
            vector,
            key,
            mode,
            is_match,
            source,
            forward: None,
        };
        let mut next = reader.next(prefix.next, vector.len)?;
        while let Some(index) = next {
            reader.session.run_usage.lookup_prefix_scans += 1;
            let value = reader.session.lookup_read(table, vector, index)?;
            if reader.session.lookup_prefixes.reserved < cap {
                reader.session.budget.allocation(1)?;
                reader.session.lookup_prefixes.reserved += 1;
                prefix.cells.push(match vector.axis {
                    SearchAxis::Vertical => CellAddress {
                        row: axis.first.row + index as u32,
                        col: axis.first.col,
                    },
                    SearchAxis::Horizontal => CellAddress {
                        row: axis.first.row,
                        col: axis.first.col + index as u32,
                    },
                });
            } else {
                retain = false;
            }

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
            prefix.next = index + 1;
            if lookup_compare(
                key,
                &value,
                mode,
                is_match,
                source,
                &mut reader.session.budget,
            )? == Some(Ordering::Equal)
            {
                return Ok(Some(index));
            }
            next = reader.next(prefix.next, vector.len)?;
        }
        prefix.exhausted = true;
        Ok(None)
    })();
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
            first: axis.first,
            last,
        });
    }
    session.lookup_prefixes.active.remove(&axis);
    if retain && result.is_ok() {
        session.lookup_prefixes.insert(axis, prefix);
    } else {
        session.lookup_prefixes.unregister(axis, &prefix);
    }
    // Dropped/failed prefixes keep their admission reservation. This bounds all
    // nested allocations conservatively without a second unbounded retention map.
    Some(result)
}

#[cfg(test)]
mod membership_tests {
    use super::*;

    #[test]
    fn membership_publication_skips_old_cells_and_only_adds_scanned_suffix() {
        let mut prefixes = Prefixes::default();
        let first = CellAddress { row: 0, col: 0 };
        let suffix = CellAddress { row: 1, col: 0 };
        let axis = Axis {
            sheet: 0,
            first,
            last: CellAddress { row: 9, col: 0 },
        };
        prefixes.insert(
            axis,
            Prefix {
                cells: vec![first],
                next: 1,
                ..Default::default()
            },
        );
        prefixes.publish();
        assert_eq!(prefixes.membership_writes, 1);
        let old_capacity = prefixes.members.capacity();
        prefixes.publish();
        assert_eq!(
            prefixes.membership_writes, 1,
            "unchanged indexes must perform no flag writes"
        );
        assert_eq!(prefixes.members.capacity(), old_capacity);
        let mut prefix = prefixes.remove(axis).unwrap();
        prefix.cells.push(suffix);
        prefix.next = 2;
        prefixes.insert(axis, prefix);
        prefixes.publish();
        assert_eq!(prefixes.membership_writes, 2);
        assert!(prefixes.members.get(&(0, suffix)).unwrap().contains(&axis));
        let changed = [(0, suffix)].into_iter().collect();
        prefixes.invalidate(&changed);
        assert!(prefixes.members.is_empty());
        assert!(!prefixes.contains(axis));
    }

    fn lookup(run: &mut Session<'_>, formula: &str) -> EvalResult<CellValue> {
        let parsed = poirot_parsing::formula::parse_formula(formula).unwrap();
        let bound = bound::FormulaPlan::bind(&parsed).unwrap();
        let at = "Z20".parse().unwrap();
        let value = run.expr(0, at, &bound.expression())?;
        run.scalar(at, value)
    }

    #[test]
    fn dropping_extended_error_nonfinite_or_capacity_prefix_clears_old_flags() {
        for suffix in [Error(NA), Number(f64::INFINITY), Number(2.)] {
            let mut book = Workbook::new();
            let sheet = book.add_sheet("S").unwrap();
            sheet.set_value("A1".parse().unwrap(), 1.).unwrap();
            sheet
                .set_value("A2".parse().unwrap(), suffix.clone())
                .unwrap();
            sheet.set_value("A3".parse().unwrap(), 3.).unwrap();
            let mut run = Session::with_context(&book, EvaluationInputs::default());
            run.lookup_prefixes.enable();
            assert_eq!(lookup(&mut run, "MATCH(1,A1:A3,0)").unwrap(), Number(1.));
            run.lookup_prefixes.publish();
            assert_eq!(run.lookup_prefixes.members.len(), 1);
            if suffix == Number(2.) {
                run.budget.limits.materialized_values = 4;
            }
            // POI numeric ingress retains infinity; comparison rejects it as NUM.
            let expected = if matches!(suffix, Number(n) if !n.is_finite()) {
                Error(Num)
            } else {
                Number(3.)
            };
            assert_eq!(lookup(&mut run, "MATCH(3,A1:A3,0)").unwrap(), expected);
            assert!(run.lookup_prefixes.members.is_empty(), "{suffix:?}");
            assert!(run.lookup_prefixes.axes.is_empty(), "{suffix:?}");
            run.lookup_prefixes.publish();
            assert!(run.lookup_prefixes.members.is_empty());
        }
    }

    #[test]
    fn budget_failure_while_extending_a_published_prefix_clears_old_flags() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value("A1".parse().unwrap(), 1.).unwrap();
        sheet.set_value("A2".parse().unwrap(), 2.).unwrap();
        let mut run = Session::with_context(&book, EvaluationInputs::default());
        run.lookup_prefixes.enable();
        assert_eq!(lookup(&mut run, "MATCH(1,A1:A2,0)").unwrap(), Number(1.));
        run.lookup_prefixes.publish();
        // The next uncached candidate exceeds the reduced memo admission limit.
        run.budget.limits.materialized_values = 1;
        assert!(lookup(&mut run, "MATCH(2,A1:A2,0)").is_err());
        assert!(run.lookup_prefixes.members.is_empty());
        assert!(run.lookup_prefixes.axes.is_empty());
    }

    #[test]
    fn indexed_cell_flag_tracks_all_overlaps_and_is_removed_with_their_indices() {
        let mut prefixes = Prefixes::default();
        let shared = CellAddress { row: 4, col: 0 };
        let vertical = Axis {
            sheet: 0,
            first: CellAddress { row: 0, col: 0 },
            last: CellAddress { row: 9, col: 0 },
        };
        let horizontal = Axis {
            sheet: 0,
            first: shared,
            last: CellAddress { row: 4, col: 9 },
        };
        let untouched = Axis {
            sheet: 1,
            ..vertical
        };
        for axis in [vertical, horizontal, untouched] {
            let cell = if axis.sheet == 1 {
                CellAddress { row: 4, col: 0 }
            } else {
                shared
            };
            prefixes.insert(
                axis,
                Prefix {
                    cells: vec![cell],
                    next: 5,
                    ..Default::default()
                },
            );
        }
        prefixes.publish();
        assert_eq!(prefixes.members.get(&(0, shared)).unwrap().len(), 2);
        // Discarding one published axis must preserve another axis's shared flag.
        let prefix = prefixes.remove(vertical).unwrap();
        prefixes.unregister(vertical, &prefix);
        assert_eq!(prefixes.members.get(&(0, shared)).unwrap().len(), 1);
        assert!(
            prefixes
                .members
                .get(&(0, shared))
                .unwrap()
                .contains(&horizontal)
        );
        let changed = [(0, shared)].into_iter().collect();
        prefixes.invalidate(&changed);
        assert!(!prefixes.members.contains_key(&(0, shared)));
        assert!(!prefixes.contains(vertical));
        assert!(!prefixes.contains(horizontal));
        assert!(prefixes.contains(untouched));
        assert_eq!(prefixes.members.get(&(1, shared)).unwrap().len(), 1);
        prefixes.admit(0);
        assert!(prefixes.members.is_empty());
        assert!(prefixes.axes.is_empty());
    }
}
