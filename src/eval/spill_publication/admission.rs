//! Atomic memo admission with only the storage and resource capabilities it needs.
use super::*;

/// Publishes computed cells and spill children after checking their storage budgets.
/// Its restricted borrows let ownership settlement admit results atomically without
/// reading workbook sources or executing another formula.
pub(super) struct Admission<'a> {
    pub(super) budget: &'a mut BudgetMeter,
    pub(super) cache: &'a mut run::CellMemo,
    pub(super) preparation: &'a mut Preparation,
    pub(super) admitted: &'a mut Option<Vec<(usize, CellAddress)>>,
    pub(super) memo_usage: &'a mut run::MemoRetentionUsage,
    pub(super) consumer: Option<EvaluationTarget>,
    pub(super) limits: Option<run::MemoRetentionLimits>,
    pub(super) journal: bool,
}
impl Admission<'_> {
    /// Admit a completed cell and all staged spill children before making them visible.
    pub(super) fn admit(
        &mut self,
        sheet: usize,
        at: CellAddress,
        result: ComputedCell,
    ) -> EvalResult<CellValue> {
        // Check the complete payload before admission; no children are visible on any failure.
        let ComputedCell { value, rows } = result;
        let extent = rows
            .as_ref()
            .map(|rows| {
                let height = u32::try_from(rows.len()).map_err(|_| EvaluationFailure::StalePlan)?;
                let width = rows.first().map_or(0, Vec::len);
                if height == 0 || width == 0 || rows.iter().any(|row| row.len() != width) {
                    return Err(EvaluationFailure::StalePlan);
                }
                let width = u32::try_from(width).map_err(|_| EvaluationFailure::StalePlan)?;
                let row_offset = height.checked_sub(1).ok_or(EvaluationFailure::StalePlan)?;
                let col_offset = width.checked_sub(1).ok_or(EvaluationFailure::StalePlan)?;
                let last = CellAddress {
                    row: at
                        .row
                        .checked_add(row_offset)
                        .ok_or(EvaluationFailure::StalePlan)?,
                    col: at
                        .col
                        .checked_add(col_offset)
                        .ok_or(EvaluationFailure::StalePlan)?,
                };
                last.validate().map_err(|_| EvaluationFailure::StalePlan)?;
                Ok(last)
            })
            .transpose()?;
        let count = rows.as_ref().map_or(Ok(1), |rows| {
            rows.iter().try_fold(0usize, |count, row| {
                count
                    .checked_add(row.len())
                    .ok_or(EvaluationFailure::StalePlan)
            })
        })?;
        self.budget
            .check_allocation(self.cache.len().saturating_add(count))?;
        self.budget
            .charge(WorkKind::Materialization, count as u64)?;
        let mut requested = run::MemoRetentionUsage::default();
        let mut replaced = run::MemoRetentionUsage::default();
        let mut overflow = false;
        let mut inspect = |address: CellAddress, value: &CellValue| -> EvalResult<()> {
            let payload = value.text_ref().map_or(0, |text| text.storage_bytes());
            if payload != 0 {
                self.budget.text(payload)?;
            }
            requested.cells = requested.cells.saturating_add(1);
            let bytes = requested
                .bytes
                .checked_add(128)
                .and_then(|n| n.checked_add(payload));
            overflow |= bytes.is_none();
            requested.bytes = bytes.unwrap_or(usize::MAX);
            if let Some(old) = self.cache.get(&(sheet, address)) {
                replaced.cells = replaced.cells.saturating_add(1);
                replaced.bytes = replaced
                    .bytes
                    .saturating_add(128)
                    .saturating_add(old.text_ref().map_or(0, |text| text.storage_bytes()));
            }
            Ok(())
        };
        if let Some(rows) = &rows {
            for (r, row) in rows.iter().enumerate() {
                for (c, value) in row.iter().enumerate() {
                    inspect(
                        // SAFETY: The local extent validation proves all rectangular
                        // row/column offsets and their additions fit u32 and the grid.
                        CellAddress {
                            row: at.row + r as u32,
                            col: at.col + c as u32,
                        },
                        value,
                    )?;
                }
            }
        } else {
            inspect(at, &value)?;
        }
        let next_cells = self
            .memo_usage
            .cells
            .saturating_sub(replaced.cells)
            .checked_add(requested.cells);
        let next_bytes = self
            .memo_usage
            .bytes
            .saturating_sub(replaced.bytes)
            .checked_add(requested.bytes);
        overflow |= next_cells.is_none() || next_bytes.is_none();
        let next = run::MemoRetentionUsage {
            cells: next_cells.unwrap_or(usize::MAX),
            bytes: next_bytes.unwrap_or(usize::MAX),
        };
        if let Some(limits) = self.limits
            && (overflow || next.cells > limits.cells || next.bytes > limits.bytes)
        {
            return Err(EvaluationFailure::MemoRetentionLimit {
                limits,
                consumed: *self.memo_usage,
                requested,
                source: Some(EvaluationTarget { sheet, address: at }),
            });
        }
        if rows.is_some() {
            self.budget
                .check_allocation(self.preparation.roles.len().saturating_add(count))?;
            self.preparation
                .roles
                .try_reserve(count)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::eval::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "spill role allocation failed".into(),
                })?;
            if self.journal {
                self.budget
                    .check_allocation(self.preparation.journal.len().saturating_add(count))?;
                self.preparation.journal.try_reserve(count).map_err(|_| {
                    EvaluationFailure::Resource {
                        kind: crate::eval::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        source: self.consumer,
                        detail: "spill journal allocation failed".into(),
                    }
                })?;
            }
        }
        self.cache
            .try_reserve(count)
            .map_err(|_| self.allocation_failure("cell memo allocation failed"))?;
        if let Some(admitted) = self.admitted.as_mut() {
            admitted
                .try_reserve(count)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::eval::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "admitted cell journal allocation failed".into(),
                })?;
        }
        if let Some(active) = self.preparation.settlement.active_mut() {
            active
                .admitted
                .try_reserve(count)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::eval::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "spill admission rollback allocation failed".into(),
                })?;
            active
                .consumers
                .try_reserve(count)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::eval::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "spill admission consumer allocation failed".into(),
                })?;
        }
        *self.memo_usage = next;
        if let Some(rows) = rows {
            let last = extent.ok_or(EvaluationFailure::StalePlan)?;
            for (r, row) in rows.into_iter().enumerate() {
                for (c, value) in row.into_iter().enumerate() {
                    // SAFETY: The local extent validation bounds these enumeration
                    // offsets by the checked last coordinate before any admission.
                    let child = CellAddress {
                        row: at.row + r as u32,
                        col: at.col + c as u32,
                    };
                    self.journal_spill_admission((sheet, child))?;
                    // SAFETY: The memo reserved count slots before this exactly-count loop;
                    // coordinate hashing and equality use only integer operations.
                    self.cache.insert((sheet, child), value);
                    if let Some(admitted) = &mut self.admitted {
                        // SAFETY: The admission journal reserved count slots before this loop.
                        admitted.push((sheet, child));
                    }
                    // SAFETY: The role map reserved count slots before this exactly-count loop;
                    // all keys use integer hashing and equality.
                    self.preparation.roles.insert(
                        (sheet, child),
                        if child == at {
                            SpillRole::Anchor { last }
                        } else {
                            SpillRole::Child { anchor: at }
                        },
                    );
                    if self.journal {
                        // SAFETY: count is the checked sum of all row lengths and
                        // the journal reserved count slots before this rectangular loop.
                        self.preparation.journal.push((sheet, child));
                    }
                }
            }
        } else {
            self.journal_spill_admission((sheet, at))?;
            // SAFETY: The memo reserved count (one) slots above. Integer keys cannot
            // panic while hashing. Cloning an existing scalar/text value preserves
            // its already representable payload length.
            self.cache.insert((sheet, at), value.clone());
            if let Some(admitted) = &mut self.admitted {
                // SAFETY: The admission journal reserved count (one) slots above.
                admitted.push((sheet, at));
            }
        }
        if self.budget.profiling {
            self.budget.usage.completed_cells = self
                .budget
                .usage
                .completed_cells
                .saturating_add(count as u64);
        }
        Ok(value)
    }
    /// Journal only this attempt's writes; settled cells cannot be replaced speculatively.
    fn journal_spill_admission(&mut self, key: (usize, CellAddress)) -> EvalResult<()> {
        if let Some(active) = self.preparation.settlement.active_mut() {
            if self.cache.contains_key(&key) && !active.admitted.contains(&key) {
                return Err(EvaluationFailure::StalePlan);
            }
            active
                .admitted
                .try_reserve(1)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::eval::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "spill rollback journal allocation failed".into(),
                })?;
            active
                .consumers
                .try_reserve(1)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::eval::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "spill consumer journal allocation failed".into(),
                })?;
            // SAFETY: One slot was reserved locally; integer keys cannot panic while hashing.
            active.admitted.insert(key);
            // SAFETY: One slot was reserved locally; integer keys cannot panic while hashing.
            active.consumers.insert(key);
        }
        Ok(())
    }
    /// Attribute fallible publication storage to the currently executing consumer.
    fn allocation_failure(&self, detail: &str) -> EvaluationFailure {
        EvaluationFailure::Resource {
            kind: crate::eval::ResourceFailureKind::Allocation,
            capability: CapabilityId::FunctionAdapter,
            source: self.consumer,
            detail: detail.into(),
        }
    }
}
