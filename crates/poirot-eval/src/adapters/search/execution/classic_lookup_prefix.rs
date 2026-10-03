//! Bounded numeric/boolean exact-search prefixes, owned by one evaluation session.
//! Observed runs keep ordinary reads; no values survive into another request.
use super::*;
use plan::{LookupTable, LookupVector};
use selection::CandidateReader;
use std::collections::{HashMap, HashSet};

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
}
/// Bounded per-session exact-search prefix cache with recursion guards.
#[derive(Default)]
pub(crate) struct Prefixes {
    enabled: bool,
    axes: PrefixMap<Axis, Prefix>,
    active: AxisSet,
    reserved: usize,
}
impl Prefixes {
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
    if session.lookup_prefixes.active.contains(&axis) || session.lookup_prefixes.reserved >= cap {
        return None;
    }
    let existing = session.lookup_prefixes.axes.remove(&axis);
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
            return Ok(Some(found));
        }
        if prefix.exhausted {
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
        session.lookup_prefixes.axes.insert(axis, prefix);
    }
    // Dropped/failed prefixes keep their admission reservation. This bounds all
    // nested allocations conservatively without a second unbounded retention map.
    Some(result)
}
