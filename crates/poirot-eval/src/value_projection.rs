//! Final and legacy scalar selection, reference algebra, arithmetic, and sparse items.
use super::*;

impl<'a> Session<'a> {
    /// Project a worksheet result with its checked footprint for later atomic admission.
    pub(super) fn publish_worksheet_result(
        &mut self,
        sheet: usize,
        address: CellAddress,
        mut value: Value,
    ) -> EvalResult<spill_publication::ComputedCell> {
        let area_reference = match &value {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                matches!(areas.as_slice(), [area] if area.kind == AreaKind::Area)
            }
            Value::SignedRefs(areas) => {
                matches!(areas.as_slice(), [area] if area.kind == AreaKind::Area && area.positive_shape().is_some())
            }
            _ => false,
        };
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill && area_reference {
            let shape = crate::array_operator::Shape::inspect(&value)
                .ok_or(EvaluationFailure::StalePlan)?;
            value = Value::Array(self.rectangular_array(
                address,
                shape,
                |session, row, column| session.rectangular_element(&value, shape, row, column),
            )?);
        }
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let Value::Array(rows) = &mut value
        {
            for member in rows.values.iter_mut().flatten() {
                // TODO(coercion-non-argument): [LOW] Each published spill member becomes a worksheet value; inline array members retain blanks.
                poirot_coercionv2::coerce_worksheet_formula_result(member);
            }
        }
        let mut result = if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let Value::Array(rows) = value
        {
            self.stage_spill(sheet, address, rows)?
        } else {
            spill_publication::ComputedCell::scalar(self.finish_expression(sheet, address, value)?)
        };
        // TODO(coercion-non-argument): [LOW] Completed worksheet formulas expose numeric zero for a remaining blank; reads and intermediate selections preserve identity.
        poirot_coercionv2::coerce_worksheet_formula_result(&mut result.value);
        Ok(result)
    }

    /// Finish an ad hoc expression, rejecting Missing without publishing worksheet spills.
    pub(super) fn finish_expression(
        &mut self,
        sheet: usize,
        address: CellAddress,
        value: Value,
    ) -> EvalResult<CellValue> {
        if matches!(value, Value::Missing) {
            // WorkbookEvaluator preserves MissingArgEval; HSSFFormulaEvaluator
            // rejects this final class. It must not become a fabricated zero.
            return Err(EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::FunctionAdapter,
                source: Some(EvaluationTarget { sheet, address }),
                detail: "pinned MissingArgEval final result throws a source exception".into(),
            });
        }
        self.select_legacy_scalar(address, value)
    }
    // Accepted legacy scalar consumers retain their blank-compatible behavior;
    // new helper ports inspect Missing identity before requesting this conversion.
    /// Select a legacy scalar, converting omissions to blanks and intersecting references implicitly.
    pub(super) fn select_legacy_scalar(
        &mut self,
        at: CellAddress,
        value: Value,
    ) -> EvalResult<CellValue> {
        match value {
            // TODO(coercion-non-argument): [MEDIUM] Generic scalar projection drops omission identity rather than leaving the consumer to choose a default.
            Value::Missing => Ok(coerce_legacy_missing_blank()),
            Value::Scalar(value) | Value::SelectedScalar(value, _) => Ok(value),
            Value::Array(rows) => Ok(rows
                .into_iter()
                .next()
                .and_then(|row| row.into_iter().next())
                .unwrap_or(Error(VALUE_ERROR))),
            Value::SheetSpan(areas) => {
                let Some(first) = areas.first() else {
                    return Ok(Error(Ref));
                };
                if first.kind == AreaKind::Area && first.contains(at) {
                    self.cell(first.sheet, at)
                } else {
                    self.budget.allocation(1)?;
                    self.select_legacy_scalar(at, Value::Refs(ReferenceAreas::One(*first)))
                }
            }
            Value::SignedRefs(areas) => {
                let mut found = None;
                for area in areas {
                    self.budget.charge(WorkKind::ReferencePair, 1)?;
                    let plan = area.implicit(at);
                    if let Some(plan) = plan
                        && found.replace(plan).is_some()
                    {
                        return Ok(Error(VALUE_ERROR));
                    }
                }
                match found {
                    Some(plan) => self.signed_read(plan),
                    None => Ok(Error(VALUE_ERROR)),
                }
            }
            Value::Refs(areas) => {
                let mut found = None;
                for area in areas {
                    self.budget.charge(WorkKind::ReferencePair, 1)?;
                    let address = if area.size() == 1 {
                        CellAddress {
                            row: area.r0,
                            col: area.c0,
                        }
                    } else if area.c0 == area.c1 && (area.r0..=area.r1).contains(&at.row) {
                        CellAddress {
                            row: at.row,
                            col: area.c0,
                        }
                    } else if area.r0 == area.r1 && (area.c0..=area.c1).contains(&at.col) {
                        CellAddress {
                            row: area.r0,
                            col: at.col,
                        }
                    } else {
                        continue;
                    };
                    if found.replace((area.sheet, address)).is_some() {
                        return Ok(Error(VALUE_ERROR));
                    }
                }
                match found {
                    Some((sheet, address)) => self.cell(sheet, address),
                    None => Ok(Error(VALUE_ERROR)),
                }
            }
        }
    }
    /// Execute physical reference union, range, or intersection operations.
    pub(super) fn references(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
    ) -> EvalResult<Value> {
        if matches!(left, Value::SignedRefs(_)) || matches!(right, Value::SignedRefs(_)) {
            return self.signed_references(op, left, right);
        }
        let (left, right) = match (left, right) {
            (
                Value::Refs(left) | Value::SheetSpan(left),
                Value::Refs(right) | Value::SheetSpan(right),
            ) => (left, right),
            (Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _), _)
            | (_, Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _)) => {
                return Ok(err(error));
            }
            _ => return Ok(err(VALUE_ERROR)),
        };
        match op {
            BinaryOp::Union => {
                self.budget
                    .allocation(left.len().saturating_add(right.len()))?;
                Ok(Value::Refs(left.into_iter().chain(right).collect()))
            }
            BinaryOp::Range => {
                let (Some(mut envelope), Some(right_first)) =
                    (left.first().copied(), right.first().copied())
                else {
                    return Ok(err(Ref));
                };
                if right_first.sheet != envelope.sheet {
                    return Ok(err(Ref));
                }
                // Each operand contributes its full bounds, including every union
                // member. No cell values are needed to construct this reference.
                for area in left.iter().skip(1).chain(right.iter().skip(1)) {
                    self.budget.charge(WorkKind::ReferencePair, 1)?;
                    if area.sheet != envelope.sheet {
                        return Ok(err(Ref));
                    }
                    envelope.r0 = envelope.r0.min(area.r0);
                    envelope.r1 = envelope.r1.max(area.r1);
                    envelope.c0 = envelope.c0.min(area.c0);
                    envelope.c1 = envelope.c1.max(area.c1);
                }
                self.budget.allocation(1)?;
                Ok(Value::Refs(ReferenceAreas::One(Area {
                    kind: AreaKind::Area,
                    sheet: envelope.sheet,
                    r0: envelope.r0.min(right_first.r0),
                    r1: envelope.r1.max(right_first.r1),
                    c0: envelope.c0.min(right_first.c0),
                    c1: envelope.c1.max(right_first.c1),
                })))
            }
            BinaryOp::Intersection => {
                let mut output = Vec::new();
                for left in left {
                    for right in &right {
                        self.budget.charge(WorkKind::ReferencePair, 1)?;
                        let intersection = Area {
                            kind: AreaKind::Area,
                            sheet: left.sheet,
                            r0: left.r0.max(right.r0),
                            r1: left.r1.min(right.r1),
                            c0: left.c0.max(right.c0),
                            c1: left.c1.min(right.c1),
                        };
                        if left.sheet == right.sheet
                            && intersection.r0 <= intersection.r1
                            && intersection.c0 <= intersection.c1
                        {
                            self.budget.check_allocation(output.len() + 1)?;
                            self.budget.charge(WorkKind::Materialization, 1)?;
                            output.push(intersection);
                        }
                    }
                }
                Ok(if output.is_empty() {
                    err(Null)
                } else {
                    Value::Refs(output.into())
                })
            }
            _ => Ok(err(Ref)),
        }
    }
    /// Execute a binary expression with reference operators and lazy operand projection.
    pub(super) fn binary(
        &mut self,
        op: BinaryOp,
        l: CellValue,
        r: CellValue,
    ) -> FunctionResult<CellValue> {
        if let Error(e) = l {
            return Err(e.into());
        }
        if let Error(e) = r {
            return Err(e.into());
        }
        if op == BinaryOp::Concat {
            return self.text_concat_operator(l, r);
        }
        if matches!(
            op,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            let c = core_projection::source_compare(&l, &r, &mut self.budget)?;
            return Ok(Boolean(match op {
                BinaryOp::Equal => c.is_eq(),
                BinaryOp::NotEqual => !c.is_eq(),
                BinaryOp::Less => c.is_lt(),
                BinaryOp::LessEqual => !c.is_gt(),
                BinaryOp::Greater => c.is_gt(),
                _ => !c.is_lt(),
            }));
        }
        let l = self.coercev2_operator_number(&l)?;
        let r = self.coercev2_operator_number(&r)?;
        let result = match op {
            BinaryOp::Add => source_arithmetic::add(l, r, &mut self.budget)?,
            BinaryOp::Subtract => source_arithmetic::subtract(l, r, &mut self.budget)?,
            BinaryOp::Multiply => l * r,
            BinaryOp::Divide => {
                if r == 0. {
                    return Err(Div0.into());
                }
                l / r
            }
            BinaryOp::Power => {
                if l == 0. && r == 0. {
                    return Err(Num.into());
                }
                if l == 0. && r < 0. {
                    return Err(Div0.into());
                }
                l.powf(r)
            }
            _ => return Err(VALUE_ERROR.into()),
        };
        let result = if op != BinaryOp::Subtract && result == 0.0 {
            0.0
        } else {
            result
        };
        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
        Ok(numeric_outcome::coerce_checked_numeric_result(result))
    }
    /// Fold eager members after admitting each complete batch and preserving read order.
    pub(super) fn fold_items<T>(
        &mut self,
        operand: crate::host::HostOperand,
        mut state: T,
        mut reserve_storage: impl FnMut(&mut T, usize) -> Result<(), std::collections::TryReserveError>,
        mut consume: impl FnMut(&mut T, crate::host::ScalarOperand) -> EvalResult<()>,
    ) -> EvalResult<T> {
        use crate::host::{OperandOrigin, ScalarOperand};
        let (value, missing, array_missing) = operand.into_parts();
        let mut length = 0usize;
        // Keep admission and fallible reservation local to each projected growth.
        macro_rules! reserve {
            ($additional:expr) => {{
                let additional = $additional;
                let count =
                    length
                        .checked_add(additional)
                        .ok_or_else(|| EvaluationFailure::Resource {
                            kind: crate::ResourceFailureKind::SizeOverflow,
                            capability: CapabilityId::FunctionAdapter,
                            source: self.consumer,
                            detail: "aggregate member count overflow".into(),
                        })?;
                self.budget.check_allocation(count)?;
                self.budget
                    .charge(WorkKind::Materialization, additional as u64)?;
                reserve_storage(&mut state, additional).map_err(|_| {
                    EvaluationFailure::Resource {
                        kind: crate::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        source: self.consumer,
                        detail: "aggregate member allocation failed".into(),
                    }
                })?;
                count
            }};
        }
        match value {
            Value::Missing => {
                reserve!(1);
                consume(
                    &mut state,
                    ScalarOperand {
                        value: Blank,
                        origin: OperandOrigin::Direct,
                        missing: true,
                    },
                )?;
            }
            Value::Scalar(value) => {
                reserve!(1);
                consume(
                    &mut state,
                    ScalarOperand {
                        value,
                        origin: OperandOrigin::Direct,
                        missing,
                    },
                )?;
            }
            Value::SelectedScalar(value, origin) => {
                reserve!(1);
                consume(
                    &mut state,
                    ScalarOperand {
                        value,
                        origin,
                        missing,
                    },
                )?;
            }
            Value::Array(rows) => {
                let count = rows
                    .iter()
                    .try_fold(0usize, |count, row| count.checked_add(row.len()))
                    .ok_or_else(|| EvaluationFailure::Resource {
                        kind: crate::ResourceFailureKind::SizeOverflow,
                        capability: CapabilityId::FunctionAdapter,
                        source: self.consumer,
                        detail: "aggregate member count overflow".into(),
                    })?;
                reserve!(count);
                for (row, values) in rows.into_iter().enumerate() {
                    for (column, value) in values.into_iter().enumerate() {
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ArrayElement,
                                missing: missing
                                    || array_missing
                                        .get(row)
                                        .and_then(|flags| flags.get(column))
                                        .copied()
                                        .unwrap_or(false),
                            },
                        )?;
                    }
                }
            }
            Value::SignedRefs(areas) => {
                for area in areas {
                    let Some(physical) = area.physical() else {
                        continue;
                    };
                    let members = self.sparse_members(physical)?;
                    length = reserve!(members.len());
                    for member in members {
                        let value = self.sparse_member_value(area.sheet, member)?;
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ReferencedCell,
                                missing: false,
                            },
                        )?;
                    }
                }
            }
            Value::Refs(areas) => {
                for area in areas {
                    let members = self.sparse_members(area)?;
                    length = reserve!(members.len());
                    for member in members {
                        let value = self.sparse_member_value(area.sheet, member)?;
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ReferencedCell,
                                missing: false,
                            },
                        )?;
                    }
                }
            }
            Value::SheetSpan(areas) => {
                // Sheet spans retain source-sheet order. Their function-specific
                // semantic error precedence is handled by the ordered consumer.
                for area in areas {
                    let members = self.sparse_members(area)?;
                    length = reserve!(members.len());
                    for member in members {
                        let value = self.sparse_member_value(area.sheet, member)?;
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ReferencedCell,
                                missing: false,
                            },
                        )?;
                    }
                }
            }
        }
        Ok(state)
    }
    /// Visit stored sheet-span members in sheet order until the consumer stops.
    pub(super) fn fold_ordered_sheet_span<T>(
        &mut self,
        areas: reference_areas::ReferenceAreas,
        mut state: T,
        mut consume: impl FnMut(
            &mut T,
            crate::host::ScalarOperand,
        ) -> EvalResult<std::ops::ControlFlow<()>>,
    ) -> EvalResult<T> {
        let mut length = 0usize;
        for area in areas {
            let members = self.sparse_members(area)?;
            length =
                length
                    .checked_add(members.len())
                    .ok_or_else(|| EvaluationFailure::Resource {
                        kind: crate::ResourceFailureKind::SizeOverflow,
                        capability: CapabilityId::FunctionAdapter,
                        source: self.consumer,
                        detail: "aggregate member count overflow".into(),
                    })?;
            self.budget.check_allocation(length)?;
            self.budget
                .charge(WorkKind::Materialization, members.len() as u64)?;
            for member in members {
                let value = self.sparse_member_value(area.sheet, member)?;
                let decision = consume(
                    &mut state,
                    crate::host::ScalarOperand {
                        value,
                        origin: crate::host::OperandOrigin::ReferencedCell,
                        missing: false,
                    },
                )?;
                if decision.is_break() {
                    return Ok(state);
                }
            }
        }
        Ok(state)
    }
    /// Collect projected members while reserving the complete batch before its cell reads.
    #[cfg(test)]
    pub(super) fn items_map<T>(
        &mut self,
        value: Value,
        mut project: impl FnMut(CellValue, bool) -> T,
    ) -> EvalResult<Vec<T>> {
        let selected_scalar = matches!(value, Value::SelectedScalar(_, _));
        self.fold_items(
            crate::host::HostOperand::from_value(value),
            Vec::new(),
            |values, additional| values.try_reserve(additional),
            |values, member| {
                if values.len() >= values.capacity() {
                    return Err(EvaluationFailure::StalePlan);
                }
                // SAFETY: The local capacity check proves this one push cannot grow the vector.
                values.push(project(
                    member.value,
                    !selected_scalar && member.origin != crate::host::OperandOrigin::Direct,
                ));
                Ok(())
            },
        )
    }
    /// Read captured authored content while generated-only members resolve their current source.
    fn sparse_member_value(
        &mut self,
        sheet: usize,
        member: functions::SparseMember<'a>,
    ) -> EvalResult<CellValue> {
        match member.cell {
            Some(cell) => {
                let source = self.source_from_cell(sheet, member.address, Some(cell));
                self.range_cell_from_source(sheet, member.address, source)
            }
            None => self.range_cell(sheet, member.address),
        }
    }
    /// Retain owned sparse members for host traversal contract tests.
    #[cfg(test)]
    pub(super) fn items(&mut self, value: Value) -> EvalResult<Vec<(CellValue, bool)>> {
        self.items_map(value, |value, indirect| (value, indirect))
    }
}

impl Session<'_> {
    /// Evaluate the concatenation operator through checked text services.
    pub(crate) fn text_concat_operator(
        &mut self,
        left: CellValue,
        right: CellValue,
    ) -> FunctionResult<CellValue> {
        let mut host =
            host::CallHost::new(self, 0, CellAddress { row: 0, col: 0 }, ArgumentsRef::Empty);
        adapters::text::execution::concat_operator(&mut host, left, right)
    }
}
