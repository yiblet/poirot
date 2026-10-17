//! Requested coordinate and collection reads preserve source order and per-read charges.
use super::*;

impl CallHost<'_, '_> {
    /// Inspect whether one resolved operand has area-like provenance.
    pub(crate) fn is_area(&mut self, index: usize) -> FunctionResult<bool> {
        self.prepare(index)?;
        Ok(match &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            Value::Array(_) => true,
            // SAFETY: Short-circuit evaluation checks the vector length is exactly one before reading element zero.
            Value::SignedRefs(a) => a.len() == 1 && a[0].kind == AreaKind::Area,
            // SAFETY: Short-circuit evaluation checks the vector length is exactly one before reading element zero.
            Value::Refs(areas) => areas.len() == 1 && areas[0].kind == AreaKind::Area,
            Value::SheetSpan(areas) => areas.first().is_some_and(|a| a.kind == AreaKind::Area),
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => false,
        })
    }
    /// Inspect operand shape, channel, sheet count, and omission without member reads.
    pub(crate) fn describe(&mut self, index: usize) -> FunctionResult<OperandDescription> {
        let (rows, columns) = self.shape(index)?;
        let is_area = self.is_area(index)?;
        let operand = &self.memo.get(&index).ok_or(VALUE_ERROR)?;
        Ok(OperandDescription {
            rows,
            columns,
            kind: operand.kind(),
            is_area,
            sheets: match &operand.value {
                Value::SheetSpan(areas) => areas.len(),
                _ => 1,
            },
            missing: operand.missing,
        })
    }
    /// Source coordinates of an AreaEval operand, without traversing its cells.
    /// Unknown computed-array coordinates remain unknown.
    pub(crate) fn operand_anchor(&mut self, index: usize) -> FunctionResult<Option<CellAddress>> {
        self.prepare(index)?;
        Ok(match &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            Value::Array(view) => view.anchor,
            // SAFETY: The match guard checks areas.len() == 1 before reading any first-area fields.
            Value::Refs(areas) if areas.len() == 1 && areas[0].kind == AreaKind::Area => {
                Some(CellAddress {
                    // SAFETY: The match guard checks areas.len() == 1 before reading any first-area fields.
                    row: areas[0].r0,
                    // SAFETY: The match guard checks areas.len() == 1 before reading any first-area fields.
                    col: areas[0].c0,
                })
            }
            _ => None,
        })
    }
    /// Read one logical member at zero-based offsets, preserving origin and omission.
    pub(crate) fn read_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.prepare(index)?;
        let operand = &self.memo.get(&index).ok_or(VALUE_ERROR)?;
        let (value, origin, missing) = match &operand.value {
            Value::Missing if row == 0 && column == 0 => {
                (coerce_legacy_missing_blank(), OperandOrigin::Direct, true)
            }
            Value::Missing => return Err(Ref.into()),
            Value::SelectedScalar(value, origin) if row == 0 && column == 0 => (
                Self::copy_cell(&mut self.session.budget, value)?,
                *origin,
                operand.missing,
            ),
            Value::Scalar(value) if row == 0 && column == 0 => (
                Self::copy_cell(&mut self.session.budget, value)?,
                OperandOrigin::Direct,
                operand.missing,
            ),
            Value::Scalar(_) | Value::SelectedScalar(_, _) => return Err(Ref.into()),
            Value::Array(rows) => {
                let value = rows.get(row).and_then(|row| row.get(column)).ok_or(Ref)?;
                (
                    Self::copy_cell(&mut self.session.budget, value)?,
                    OperandOrigin::ArrayElement,
                    operand
                        .array_missing
                        .get(row)
                        .and_then(|flags| flags.get(column))
                        .copied()
                        .unwrap_or(false),
                )
            }
            Value::Refs(areas) | Value::SheetSpan(areas) if areas.len() == 1 => {
                // SAFETY: Both reference match arms explicitly require areas.len() == 1.
                let area = areas[0];
                let height = area
                    .r1
                    .checked_sub(area.r0)
                    .and_then(|size| size.checked_add(1))
                    .ok_or(Ref)?;
                let width = area
                    .c1
                    .checked_sub(area.c0)
                    .and_then(|size| size.checked_add(1))
                    .ok_or(Ref)?;
                if row as u64 >= u64::from(height) || column as u64 >= u64::from(width) {
                    return Err(Ref.into());
                }
                (
                    self.session.cell(
                        area.sheet,
                        CellAddress {
                            row: area.r0 + row as u32,
                            col: area.c0 + column as u32,
                        },
                    )?,
                    OperandOrigin::ReferencedCell,
                    false,
                )
            }
            Value::SignedRefs(areas) if areas.len() == 1 => {
                // SAFETY: Both reference match arms explicitly require areas.len() == 1.
                let plan = areas[0].read(row as u64, column as u64).ok_or(Ref)?;
                (
                    self.session.signed_read(plan)?,
                    OperandOrigin::ReferencedCell,
                    false,
                )
            }
            Value::SignedRefs(_) => return Err(VALUE_ERROR.into()),
            Value::SheetSpan(_) => {
                return Err(self.unsupported(
                    CapabilityId::FunctionAdapter,
                    "this coordinate port has no verified multi-sheet selection policy",
                ));
            }
            Value::Refs(_) => return Err(VALUE_ERROR.into()),
        };
        Ok(ScalarOperand {
            value,
            origin,
            missing,
        })
    }
    /// Criteria resize reference ranges from their anchor; arrays retain original bounds.
    pub(crate) fn read_relative_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.prepare(index)?;
        if let Value::SignedRefs(areas) = &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            if areas.len() != 1 {
                return Err(VALUE_ERROR.into());
            }
            // SAFETY: Each reference branch returns unless areas.len() == 1 before reading element zero.
            let area = areas[0];
            self.session.budget.charge(WorkKind::ReferencePair, 1)?;
            let plan = area.relative(row, column).ok_or(Ref)?;
            return Ok(ScalarOperand {
                value: self.session.signed_read(plan)?,
                origin: OperandOrigin::ReferencedCell,
                missing: false,
            });
        }
        if let Value::Refs(areas) = &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            if areas.len() != 1 {
                return Err(VALUE_ERROR.into());
            }
            // SAFETY: Each reference branch returns unless areas.len() == 1 before reading element zero.
            let area = areas[0];
            self.session.budget.charge(WorkKind::ReferencePair, 1)?;
            let plan = resolution::CellReadPlan::relative(area, row, column)?;
            return Ok(ScalarOperand {
                value: self.session.cell(plan.sheet, plan.address)?,
                origin: OperandOrigin::ReferencedCell,
                missing: false,
            });
        }
        self.read_cell(index, row, column)
    }
    /// Inspect flattened collection length without traversing its cells.
    pub(crate) fn collection_len(&mut self, index: usize) -> FunctionResult<usize> {
        self.prepare(index)?;
        let count = match &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => 1,
            Value::Array(rows) => rows
                .iter()
                .try_fold(0u64, |count, row| count.checked_add(row.len() as u64))
                .ok_or(Num)?,
            Value::SignedRefs(areas) => {
                let mut count = 0u64;
                for area in areas {
                    self.session.budget.charge(WorkKind::ReferencePair, 1)?;
                    count = count.checked_add(area.size()).ok_or(Num)?;
                }
                count
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut count = 0u64;
                for area in areas {
                    self.session.budget.charge(WorkKind::ReferencePair, 1)?;
                    count = count.checked_add(area.size()).ok_or(Num)?;
                }
                count
            }
        };
        usize::try_from(count).map_err(|_| {
            self.unsupported(
                CapabilityId::FunctionAdapter,
                "collection length does not fit this platform",
            )
        })
    }
    /// Read one flattened member in source sheet-first, row-major order.
    pub(crate) fn collection_cell(
        &mut self,
        index: usize,
        offset: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.prepare(index)?;
        match &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => {
                self.read_cell(index, 0, offset)
            }
            Value::Array(rows) => {
                let columns = rows.first().map_or(0, Vec::len);
                if columns == 0 {
                    return Err(VALUE_ERROR.into());
                }
                self.read_cell(index, offset / columns, offset % columns)
            }
            Value::SignedRefs(areas) => {
                let mut remaining = offset as u64;
                for area in areas {
                    self.session.budget.charge(WorkKind::ReferencePair, 1)?;
                    if remaining < area.size() {
                        let plan = area
                            .read(remaining / area.width(), remaining % area.width())
                            .ok_or(Ref)?;
                        return Ok(ScalarOperand {
                            value: self.session.signed_read(plan)?,
                            origin: OperandOrigin::ReferencedCell,
                            missing: false,
                        });
                    }
                    remaining -= area.size();
                }
                Err(Ref.into())
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut remaining = offset as u64;
                for area in areas {
                    self.session.budget.charge(WorkKind::ReferencePair, 1)?;
                    if remaining < area.size() {
                        let width = u64::from(area.c1 - area.c0 + 1);
                        let address = CellAddress {
                            row: area.r0 + (remaining / width) as u32,
                            col: area.c0 + (remaining % width) as u32,
                        };
                        return Ok(ScalarOperand {
                            value: self.session.cell(area.sheet, address)?,
                            origin: OperandOrigin::ReferencedCell,
                            missing: false,
                        });
                    }
                    remaining -= area.size();
                }
                Err(Ref.into())
            }
        }
    }
    /// Resolve rectangular dimensions without reading member values.
    pub(crate) fn shape(&mut self, index: usize) -> FunctionResult<(usize, usize)> {
        self.prepare(index)?;
        match &self.memo.get(&index).ok_or(VALUE_ERROR)?.value {
            Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => Ok((1, 1)),
            Value::Array(rows) => {
                let columns = rows.first().ok_or(VALUE_ERROR)?.len();
                if columns == 0 || rows.iter().any(|row| row.len() != columns) {
                    return Err(VALUE_ERROR.into());
                }
                Ok((rows.len(), columns))
            }
            Value::SheetSpan(areas) | Value::Refs(areas)
                if !areas.is_empty()
                    && (areas.len() == 1
                        || matches!(
                            self.memo.get(&index).ok_or(VALUE_ERROR)?.value,
                            Value::SheetSpan(_)
                        )) =>
            {
                // SAFETY: The relevant match guards require a nonempty area vector or exactly one area.
                let area = areas[0];
                if area.r1 >= MAX_ROWS || area.c1 >= MAX_COLUMNS {
                    return Err(Ref.into());
                }
                let rows = area
                    .r1
                    .checked_sub(area.r0)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Ref)?;
                let columns = area
                    .c1
                    .checked_sub(area.c0)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Ref)?;
                Ok((rows as usize, columns as usize))
            }
            Value::SignedRefs(areas) if areas.len() == 1 => {
                // SAFETY: The relevant match guards require a nonempty area vector or exactly one area.
                areas[0].positive_shape().ok_or_else(|| {
                    self.unsupported(
                        CapabilityId::FunctionAdapter,
                        "shape port cannot represent zero/negative Java source dimensions",
                    )
                })
            }
            Value::SignedRefs(_) | Value::Refs(_) | Value::SheetSpan(_) => Err(VALUE_ERROR.into()),
        }
    }
}
