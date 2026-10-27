//! Worksheet boundaries and substream traversal shared by preflight and decoding.
use super::preflight_error::PreflightError;
use super::record::Record;

/// Records of one worksheet substream, skipping nested chart/object substreams.
#[derive(Clone)]
pub(in crate::xls) struct Substream<'r, 'a> {
    /// Complete checked workbook-stream record inventory.
    records: &'r [Record<'a>],
    /// Index of the next record to inspect.
    next: usize,
    /// BOF nesting depth; worksheet records are read only at depth one.
    depth: usize,
    /// Records read so far, including skipped nested records but not the closing EOF.
    visited: usize,
}

impl<'r, 'a> Substream<'r, 'a> {
    /// Start at a worksheet's record index, skipping its BOF record when present.
    pub(in crate::xls) fn new(records: &'r [Record<'a>], start: usize) -> Self {
        let bof = records.get(start).is_some_and(|r| r.id == 0x809);
        Self {
            records,
            next: start.saturating_add(usize::from(bof)),
            depth: 1,
            visited: 0,
        }
    }

    /// Return the next top-level worksheet record and its index, ending at the matching EOF.
    pub(in crate::xls) fn next_indexed(&mut self) -> Option<(usize, Record<'a>)> {
        loop {
            let index = self.next;
            let record = *self.records.get(index)?;
            self.next = index.checked_add(1)?;
            match record.id {
                0x809 => self.depth = self.depth.saturating_add(1),
                0xa => {
                    self.depth = self.depth.saturating_sub(1);
                    if self.depth == 0 {
                        self.next = self.records.len();
                        return None;
                    }
                }
                _ if self.depth == 1 => {
                    self.visited = self.visited.saturating_add(1);
                    return Some((index, record));
                }
                _ => {}
            }
            self.visited = self.visited.saturating_add(1);
        }
    }

    /// Continue after a record consumed by a multi-record decoder.
    pub(in crate::xls) fn skip_to(&mut self, last_consumed: usize) {
        let next = self.next.max(last_consumed.saturating_add(1));
        self.visited = self.visited.saturating_add(next.saturating_sub(self.next));
        self.next = next;
    }

    /// Records read so far, the traversal work charged by preflight.
    pub(in crate::xls) fn visited(&self) -> usize {
        self.visited
    }
}

impl<'a> Iterator for Substream<'_, 'a> {
    type Item = Record<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_indexed().map(|(_, record)| record)
    }
}

/// Locate the first record of each worksheet substream, in BoundSheet order.
///
/// Declared BoundSheet offsets (`lbPlyPos`) are used when every one names a
/// record after the workbook globals EOF. Otherwise worksheets are taken in
/// stream order: each top-level BOF after the globals, as some writers leave
/// `lbPlyPos` unset. Duplicate declared offsets are returned as given; callers
/// charge each traversal. Preflight and decoding both use this function, so
/// they traverse the same records.
pub(super) fn sheet_starts(
    rs: &[Record<'_>],
    positions: &[usize],
) -> Result<Vec<usize>, PreflightError> {
    if positions.is_empty() {
        return Ok(Vec::new());
    }
    let globals_end = rs
        .iter()
        .position(|r| r.id == 0xa)
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::SheetBoundary,
        ))?;
    let declared = positions
        .iter()
        .map(|position| {
            rs.binary_search_by_key(position, |r| r.at)
                .ok()
                .filter(|&i| i > globals_end)
        })
        .collect::<Option<Vec<_>>>();
    if let Some(declared) = declared {
        return Ok(declared);
    }
    let mut starts = Vec::new();
    let mut depth = 0usize;
    for (index, record) in rs.iter().enumerate().skip(globals_end.saturating_add(1)) {
        if starts.len() == positions.len() {
            break;
        }
        match record.id {
            0x809 => {
                if depth == 0 {
                    starts.push(index);
                }
                depth = depth.saturating_add(1);
            }
            0xa => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    if starts.len() == positions.len() {
        Ok(starts)
    } else {
        Err(PreflightError::Malformed(
            crate::BiffStructureError::SheetBoundary,
        ))
    }
}
