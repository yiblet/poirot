use super::prefix_axis::PrefixAxis;
use super::prefix_interval::PrefixInterval;
use super::prefix_key::PrefixKey;
use super::prefix_partition::PrefixPartition;
use super::search_prefix::SearchPrefix;
use crate::model::{CellAddress, Workbook};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Active lookup axes used to guard recursive prefix construction.
type AxisSet = HashSet<PrefixAxis, foldhash::fast::RandomState>;

/// Upper bound on retained prefix storage units: axes, keyed matches and query text units.
pub(in crate::eval) const MAX_ENTRIES: usize = 65_536;

/// Session cache that reuses exact-search prefixes across compatible lookup calls.
/// It bounds retained storage, guards recursive construction, and invalidates prefixes whose consumed cells or gaps changed.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(in crate::eval) struct LookupPrefixes {
    #[serde(skip)]
    pub(in crate::eval) enabled: bool,
    // Partition first by sheet and physical column/row, then by bounded range.
    pub(in crate::eval) axes: BTreeMap<PrefixPartition, BTreeMap<PrefixInterval, SearchPrefix>>,
    #[serde(skip)]
    pub(in crate::eval) active: AxisSet,
    #[serde(skip)]
    pub(in crate::eval) reserved: usize,
}

impl LookupPrefixes {
    /// Validate persisted evaluated search state and rebuild transient admission counts.
    pub(in crate::eval) fn restore(mut self, workbook: &Workbook) -> Result<Self, String> {
        let mut total = 0usize;
        for (&partition, ranges) in &mut self.axes {
            if workbook.sheet(partition.0).is_none() || ranges.is_empty() {
                return Err("lookup prefix worksheet or empty partition".into());
            }
            for (&interval, prefix) in ranges {
                let axis = PrefixAxis::from_location(partition, interval);
                axis.first.validate().map_err(|e| e.to_string())?;
                axis.last.validate().map_err(|e| e.to_string())?;
                if interval.0 > interval.1 || axis.location() != (partition, interval) {
                    return Err("lookup prefix axis geometry".into());
                }
                // SAFETY: Ordered u32 endpoints were checked above, so subtraction
                // is nonnegative and adding one remains representable in u64.
                let len = u64::from(interval.1) - u64::from(interval.0) + 1;
                if prefix.next as u64 > len
                    || prefix.first.values().any(|&index| index >= prefix.next)
                    || prefix
                        .text
                        .values()
                        .flatten()
                        .any(|&index| index >= prefix.next)
                    || prefix.first.keys().any(|key| match key {
                        PrefixKey::Number(bits) => {
                            let number = f64::from_bits(*bits);
                            !number.is_finite() || (number == 0. && *bits != 0)
                        }
                        PrefixKey::Boolean(_) => false,
                    })
                {
                    return Err("lookup prefix selection or numeric key".into());
                }
                prefix.text_storage = prefix
                    .text
                    .keys()
                    .try_fold(0usize, |total, query| {
                        total.checked_add(query.len())?.checked_add(1)
                    })
                    .ok_or("lookup prefix text storage overflow")?;
                total = total
                    .checked_add(1)
                    .and_then(|n| n.checked_add(prefix.first.len()))
                    .and_then(|n| n.checked_add(prefix.text_storage))
                    .filter(|n| *n <= MAX_ENTRIES)
                    .ok_or("lookup prefix storage limit")?;
            }
        }
        self.reserved = total;
        Ok(self)
    }

    /// Whether any exact-search evidence is retained.
    pub(in crate::eval) fn is_empty(&self) -> bool {
        self.axes.is_empty()
    }

    /// Invalidate changed present cells and gaps through their consumed axis intervals.
    /// Every indexed candidate lies inside its axis at an offset below `next`;
    /// an exhausted axis covers its whole interval. These guards therefore cover
    /// both stored candidates and absent gaps without per-cell reverse flags.
    pub(in crate::eval) fn invalidate(&mut self, changed: &crate::eval::coordinates::CellSet) {
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
                            affected.insert(PrefixAxis::from_location(partition, interval));
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
    pub(in crate::eval) fn invalidate_footprint(
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
                        affected.insert(PrefixAxis::from_location(partition, interval));
                    }
                }
            }
        }
        for axis in affected {
            self.remove(axis);
        }
        self.recount();
    }

    pub(in crate::eval) fn contains(&self, axis: PrefixAxis) -> bool {
        let (partition, interval) = axis.location();
        self.axes
            .get(&partition)
            .is_some_and(|ranges| ranges.contains_key(&interval))
    }

    pub(in crate::eval) fn remove(&mut self, axis: PrefixAxis) -> Option<SearchPrefix> {
        let (partition, interval) = axis.location();
        let ranges = self.axes.get_mut(&partition)?;
        let prefix = ranges.remove(&interval);
        if ranges.is_empty() {
            self.axes.remove(&partition);
        }
        prefix
    }

    pub(in crate::eval) fn insert(&mut self, axis: PrefixAxis, prefix: SearchPrefix) {
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
    pub(in crate::eval) fn admit(&mut self, limit: usize) {
        self.recount();
        if self.reserved > MAX_ENTRIES.min(limit) {
            self.axes.clear();
            self.reserved = 0;
        }
        self.enable();
    }

    /// Enable prefix reuse for eligible unobserved lookups in this session.
    pub(in crate::eval) fn enable(&mut self) {
        self.enabled = true;
    }
}
