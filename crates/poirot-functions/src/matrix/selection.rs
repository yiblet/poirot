//! Independently authored rectangular selection, sorting and grouping algorithms.
use crate::matrix::{Axis, Budget, Error, Matrix, Result};
use crate::scalar::CellError;
use std::cmp::Ordering;

/// Direction of a sorting key, independent of row or column orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// Smaller keys precede larger keys.
    Ascending,
    /// Larger keys precede smaller keys.
    Descending,
}

fn length<T>(input: &Matrix<T>, axis: Axis) -> usize {
    match axis {
        Axis::Rows => input.rows(),
        Axis::Columns => input.cols(),
    }
}
fn append<T>(values: &mut Vec<T>, value: T) -> Result<()> {
    values
        .try_reserve(1)
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: The successful local reservation admits exactly this one additional member.
    values.push(value);
    Ok(())
}
fn storage<T>(count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    Ok(values)
}
/// Copy selected zero-based rows or columns, preserving their requested order.
/// Empty selections report the spreadsheet's unsupported empty-array error.
pub fn select<T: Clone>(
    input: &Matrix<T>,
    axis: Axis,
    indices: &[usize],
    budget: Budget,
) -> Result<Matrix<T>> {
    if indices.is_empty() {
        return Err(Error::Formula(CellError::Calc));
    }
    let n = length(input, axis);
    if indices.iter().any(|&i| i >= n) {
        return Err(Error::Formula(CellError::Value));
    }
    let (rows, cols) = match axis {
        Axis::Rows => (indices.len(), input.cols()),
        Axis::Columns => (input.rows(), indices.len()),
    };
    let count = rows.checked_mul(cols).ok_or(Error::SizeOverflow)?;
    budget.check(Some(count), Some(count))?;
    let mut values = storage(count)?;
    for row in 0..rows {
        for col in 0..cols {
            let r = if axis == Axis::Rows {
                *indices.get(row).ok_or(Error::Formula(CellError::Value))?
            } else {
                row
            };
            let c = if axis == Axis::Columns {
                *indices.get(col).ok_or(Error::Formula(CellError::Value))?
            } else {
                col
            };
            append(&mut values, input.get(r, c)?.clone())?;
        }
    }
    Matrix::new(rows, cols, values)
}
/// Keep rows or columns whose Boolean mask is true, without changing member values.
pub fn filter<T: Clone>(
    input: &Matrix<T>,
    axis: Axis,
    include: &[bool],
    budget: Budget,
) -> Result<Matrix<T>> {
    if include.len() != length(input, axis) {
        return Err(Error::Formula(CellError::Value));
    }
    budget.check(Some(input.values().len()), Some(input.values().len()))?;
    let mut indices = storage(include.len())?;
    for (index, keep) in include.iter().enumerate() {
        if *keep {
            append(&mut indices, index)?;
        }
    }
    select(input, axis, &indices, budget)
}

/// Conservative work admission for comparison sorting plus output copying.
pub fn selection_work(cells: usize, records: usize, keys: usize) -> Option<usize> {
    let levels = usize::BITS.checked_sub(records.leading_zeros())? as usize;
    records
        .checked_mul(levels)?
        .checked_mul(keys)?
        .checked_add(cells)
}
fn oriented<T>(input: &Matrix<T>, axis: Axis) -> Result<Matrix<&T>> {
    let (rows, cols) = match axis {
        Axis::Rows => (input.rows(), input.cols()),
        Axis::Columns => (input.cols(), input.rows()),
    };
    let mut values = storage(input.values().len())?;
    for row in 0..rows {
        for col in 0..cols {
            append(
                &mut values,
                match axis {
                    Axis::Rows => input.get(row, col)?,
                    Axis::Columns => input.get(col, row)?,
                },
            )?;
        }
    }
    Matrix::new(rows, cols, values)
}
fn compare_rows<T>(
    a: &[&T],
    b: &[&T],
    compare: &impl Fn(&T, &T) -> Ordering,
    orders: Option<&[SortOrder]>,
) -> Ordering {
    for (index, (a, b)) in a.iter().zip(b).enumerate() {
        let order = compare(a, b);
        let order = if orders.and_then(|o| o.get(index)) == Some(&SortOrder::Descending) {
            order.reverse()
        } else {
            order
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}
/// Sort records by one or more corresponding key columns, retaining input order on ties.
/// Keys are always a record-by-key matrix, even when output records are columns.
pub fn sort<T: Clone, K>(
    input: &Matrix<T>,
    axis: Axis,
    keys: &Matrix<K>,
    orders: &[SortOrder],
    compare: impl Fn(&K, &K) -> Ordering,
    budget: Budget,
) -> Result<Matrix<T>> {
    let n = length(input, axis);
    if keys.rows() != n || keys.cols() != orders.len() {
        return Err(Error::Formula(CellError::Value));
    }
    budget.check(
        Some(input.values().len().max(keys.values().len())),
        selection_work(input.values().len(), n, keys.cols()),
    )?;
    let oriented = oriented(keys, Axis::Rows)?;
    let mut records = storage(n)?;
    // SAFETY: Matrix::new rejects zero-width matrices; this explicit check also proves the chunk width locally.
    if oriented.cols() == 0 {
        return Err(Error::Formula(CellError::Value));
    }
    // SAFETY: The preceding local guard establishes a nonzero chunk width.
    records
        .try_reserve(n)
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: The nonzero width guard above and local reservation admit all n unchanged chunks.
    records.extend(oriented.values().chunks(oriented.cols()).enumerate());
    ordered(&mut records, |(ai, a), (bi, b)| {
        compare_rows(a, b, &compare, Some(orders)).then(ai.cmp(bi))
    })?;
    let mut indices = storage(n)?;
    indices
        .try_reserve(records.len())
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: The unchanged source iterator emits exactly the locally reserved number of ordinals.
    indices.extend(records.into_iter().map(|(index, _)| index));
    select(input, axis, &indices, budget)
}
/// Group equal rows or columns in O(record count * log(record count)) comparisons.
/// The caller supplies a consistent total comparison; output follows first occurrence order.
pub fn unique<T: Clone>(
    input: &Matrix<T>,
    axis: Axis,
    exactly_once: bool,
    compare: impl Fn(&T, &T) -> Ordering,
    budget: Budget,
) -> Result<Matrix<T>> {
    let n = length(input, axis);
    let key_width = if axis == Axis::Rows {
        input.cols()
    } else {
        input.rows()
    };
    budget.check(
        Some(input.values().len()),
        selection_work(input.values().len(), n, key_width),
    )?;
    let partition = partition(input, axis, &compare, budget)?;
    let mut indices = storage(n)?;
    for group in &partition.groups {
        if (!exactly_once || group.len() == 1)
            && let Some(index) = partition.order.get(group.start)
        {
            append(&mut indices, *index)?;
        }
    }
    // SAFETY: usize ordering is a locally fixed total order; no caller comparator reaches this sort.
    indices.sort_unstable();
    select(input, axis, &indices, budget)
}

// A bounded bottom-up merge avoids the standard sort's comparator-consistency
// panic. Every loop advances independently of comparison results; a broken
// client comparator cannot make the kernel loop forever or index unchecked.
fn ordered<T: Copy>(values: &mut [T], compare: impl Fn(&T, &T) -> Ordering) -> Result<()> {
    let n = values.len();
    let mut scratch = storage(n)?;
    scratch
        .try_reserve(values.len())
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: The successful local reservation admits this unchanged slice length.
    scratch.extend_from_slice(values);
    let mut width = 1usize;
    while width < n {
        let mut start = 0usize;
        while start < n {
            let middle = start.saturating_add(width).min(n);
            let end = middle.saturating_add(width).min(n);
            let (mut left, mut right) = (start, middle);
            for destination in start..end {
                let take_left = if right >= end {
                    true
                } else if left >= middle {
                    false
                } else {
                    let a = values.get(left).ok_or(Error::Formula(CellError::Value))?;
                    let b = values.get(right).ok_or(Error::Formula(CellError::Value))?;
                    compare(a, b) != Ordering::Greater
                };
                let source = if take_left {
                    let source = left;
                    left = left.saturating_add(1);
                    source
                } else {
                    let source = right;
                    right = right.saturating_add(1);
                    source
                };
                let value = *values.get(source).ok_or(Error::Formula(CellError::Value))?;
                *scratch
                    .get_mut(destination)
                    .ok_or(Error::Formula(CellError::Value))? = value;
            }
            start = end;
        }
        if scratch.len() != values.len() {
            return Err(Error::Formula(CellError::Value));
        }
        // SAFETY: The immediately preceding local check establishes equal slice lengths.
        values.copy_from_slice(&scratch);
        width = width.saturating_mul(2);
    }
    Ok(())
}

/// Groups records with equal complete keys after a stable sort.
/// The permutation preserves original record identities, and each group is a span into that permutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    /// Permutation from sorted position to original source record.
    pub order: Vec<usize>,
    /// Nonempty ranges into the permutation, in key order.
    pub groups: Vec<std::ops::Range<usize>>,
}
/// Partition complete row or column keys with bounded O(n log n) comparisons.
pub fn partition<T>(
    input: &Matrix<T>,
    axis: Axis,
    compare: impl Fn(&T, &T) -> Ordering,
    budget: Budget,
) -> Result<Partition> {
    let n = length(input, axis);
    let width = if axis == Axis::Rows {
        input.cols()
    } else {
        input.rows()
    };
    budget.check(
        Some(input.values().len()),
        selection_work(input.values().len(), n, width),
    )?;
    let keys = oriented(input, axis)?;
    if keys.cols() == 0 {
        return Err(Error::Formula(CellError::Value));
    }
    let mut records = storage(n)?;
    // SAFETY: The preceding local guard establishes a nonzero chunk width.
    records
        .try_reserve(n)
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: The local nonzero width guard and Matrix dimensions make exactly n chunks; n slots were reserved here.
    records.extend(keys.values().chunks(keys.cols()).enumerate());
    ordered(&mut records, |(ai, a), (bi, b)| {
        compare_rows(a, b, &compare, None).then(ai.cmp(bi))
    })?;
    let mut groups = storage(n)?;
    let mut start = 0usize;
    for group in
        records.chunk_by(|(_, a), (_, b)| compare_rows(a, b, &compare, None) == Ordering::Equal)
    {
        let end = start.checked_add(group.len()).ok_or(Error::SizeOverflow)?;
        append(&mut groups, start..end)?;
        start = end;
    }
    let mut order = storage(n)?;
    order
        .try_reserve(records.len())
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: The unchanged source iterator emits exactly the locally reserved number of ordinals.
    order.extend(records.into_iter().map(|(index, _)| index));
    Ok(Partition { order, groups })
}

/// A node in a grouping hierarchy, linking a key prefix to its original source records.
/// Its member span indexes the sorted permutation rather than copying the records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// Number of grouping fields represented; zero denotes the grand total.
    pub depth: usize,
    /// Parent group, absent only for the grand total.
    pub parent: Option<usize>,
    /// Member span in the supplied partition's permutation.
    pub members: std::ops::Range<usize>,
}
/// Construct a preorder hierarchy in O(records * fields), retaining source membership.
pub fn hierarchy<T>(
    keys: &Matrix<T>,
    partition: &Partition,
    compare: impl Fn(&T, &T) -> Ordering,
    budget: Budget,
) -> Result<Vec<Group>> {
    let n = keys.rows();
    let width = keys.cols();
    let count = n
        .checked_mul(width)
        .and_then(|v| v.checked_add(1))
        .ok_or(Error::SizeOverflow)?;
    budget.check(Some(count), Some(count))?;
    if partition.order.len() != n || partition.order.iter().any(|&r| r >= n) {
        return Err(Error::Formula(CellError::Value));
    }
    let mut seen = storage(n)?;
    seen.try_reserve(n).map_err(|_| Error::AllocationFailure)?;
    // SAFETY: This empty vector has locally reserved n slots; resizing to n needs no further capacity.
    seen.resize(n, false);
    for &row in &partition.order {
        let previous = seen.get_mut(row).ok_or(Error::Formula(CellError::Value))?;
        if *previous {
            return Err(Error::Formula(CellError::Value));
        }
        *previous = true;
    }
    let mut groups = storage(count)?;
    append(
        &mut groups,
        Group {
            depth: 0,
            parent: None,
            members: 0..n,
        },
    )?;
    let levels = width.checked_add(1).ok_or(Error::SizeOverflow)?;
    let mut active = storage(levels)?;
    active
        .try_reserve(levels)
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: This empty vector has locally reserved levels slots.
    active.resize(levels, 0usize);
    let mut previous = None;
    for (position, &row) in partition.order.iter().enumerate() {
        let mut common = 0;
        if let Some(previous) = previous {
            for field in 0..width {
                if compare(keys.get(row, field)?, keys.get(previous, field)?) != Ordering::Equal {
                    break;
                }
                common += 1;
            }
        }
        for depth in 1..=width {
            if depth > common {
                // SAFETY: This loop starts at one, so subtracting one cannot underflow.
                let parent = *active
                    .get(depth - 1)
                    .ok_or(Error::Formula(CellError::Value))?;
                let index = groups.len();
                append(
                    &mut groups,
                    Group {
                        depth,
                        parent: Some(parent),
                        members: position..position.saturating_add(1),
                    },
                )?;
                *active
                    .get_mut(depth)
                    .ok_or(Error::Formula(CellError::Value))? = index;
            } else {
                let index = *active.get(depth).ok_or(Error::Formula(CellError::Value))?;
                groups
                    .get_mut(index)
                    .ok_or(Error::Formula(CellError::Value))?
                    .members
                    .end = position + 1;
            }
        }
        previous = Some(row);
    }
    Ok(groups)
}

/// Whether grouping keys form parent-child levels or independent table columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldRelationship {
    /// Later keys are sorted within earlier keys.
    Hierarchy,
    /// Complete key tuples are sorted without parent constraints.
    Table,
}
/// Sort group siblings and place selected totals before or after their children.
pub fn group_layout(
    groups: &[Group],
    fields: usize,
    relationship: FieldRelationship,
    total_depth: i64,
    compare: impl Fn(usize, usize) -> Ordering,
    budget: Budget,
) -> Result<Vec<usize>> {
    budget.check(
        Some(groups.len().saturating_mul(2)),
        selection_work(groups.len(), groups.len(), 1),
    )?;
    if groups
        .first()
        .is_none_or(|g| g.depth != 0 || g.parent.is_some())
        || groups
            .iter()
            .skip(1)
            .enumerate()
            .any(|(i, g)| g.parent.is_none_or(|p| p > i) || g.depth > fields)
    {
        return Err(Error::Formula(CellError::Value));
    }
    for group in groups.iter().skip(1) {
        let parent = groups
            .get(group.parent.ok_or(Error::Formula(CellError::Value))?)
            .ok_or(Error::Formula(CellError::Value))?;
        if parent.depth.checked_add(1) != Some(group.depth)
            || group.members.is_empty()
            || group.members.start < parent.members.start
            || group.members.end > parent.members.end
        {
            return Err(Error::Formula(CellError::Value));
        }
    }
    let mut children: Vec<Vec<usize>> = storage(groups.len())?;
    children
        .try_reserve(groups.len())
        .map_err(|_| Error::AllocationFailure)?;
    // SAFETY: This empty vector has locally reserved one slot per unchanged group; Vec::new cannot panic.
    children.resize_with(groups.len(), Vec::new);
    for (index, group) in groups.iter().enumerate().skip(1) {
        if relationship == FieldRelationship::Table && group.depth != fields {
            continue;
        }
        let parent = if relationship == FieldRelationship::Table {
            0
        } else {
            group.parent.ok_or(Error::Formula(CellError::Value))?
        };
        let siblings = children
            .get_mut(parent)
            .ok_or(Error::Formula(CellError::Value))?;
        siblings
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailure)?;
        append(siblings, index)?;
    }
    for siblings in &mut children {
        ordered(siblings, |a, b| compare(*a, *b).then(a.cmp(b)))?;
    }
    let mut output = storage(groups.len())?;
    let mut stack = storage(groups.len().saturating_mul(2))?;
    append(&mut stack, (0usize, false))?;
    while let Some((index, exiting)) = stack.pop() {
        let group = groups.get(index).ok_or(Error::Formula(CellError::Value))?;
        let visible = group.depth == fields
            || (total_depth != 0 && group.depth < total_depth.unsigned_abs() as usize);
        if exiting {
            if visible {
                append(&mut output, index)?;
            }
            continue;
        }
        if total_depth < 0 || group.depth == fields {
            if visible {
                append(&mut output, index)?;
            }
        } else {
            append(&mut stack, (index, true))?;
        }
        let siblings = children
            .get(index)
            .ok_or(Error::Formula(CellError::Value))?;
        for &child in siblings.iter().rev() {
            append(&mut stack, (child, false))?;
        }
    }
    Ok(output)
}
