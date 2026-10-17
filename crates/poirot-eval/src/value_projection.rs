//! Final and legacy scalar selection, reference algebra, arithmetic, and sparse items.
use super::*;

impl<'a> Session<'a> {
    /// Publish a final dynamic array or project a supported final result to a scalar.
    pub(super) fn result_scalar(
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
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let Value::Array(rows) = value
        {
            return self.stage_spill(sheet, address, rows);
        }
        self.scalar(address, value)
    }
    // Accepted legacy scalar consumers retain their blank-compatible behavior;
    // new helper ports inspect Missing identity before requesting this conversion.
    /// Apply legacy scalar selection, dereferencing only the selected reference cell.
    pub(super) fn scalar(&mut self, at: CellAddress, value: Value) -> EvalResult<CellValue> {
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
                    self.scalar(at, Value::Refs(vec![*first]))
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
                // SAFETY: The range branch returns unless both vectors have exactly one element before comparing or copying their first areas.
                if left.len() != 1 || right.len() != 1 || left[0].sheet != right[0].sheet {
                    return Ok(err(Ref));
                }
                self.budget.allocation(1)?;
                // SAFETY: The range branch returns unless both vectors have exactly one element before comparing or copying their first areas.
                let (left, right) = (left[0], right[0]);
                Ok(Value::Refs(vec![Area {
                    kind: AreaKind::Area,
                    sheet: left.sheet,
                    r0: left.r0.min(right.r0),
                    r1: left.r1.max(right.r1),
                    c0: left.c0.min(right.c0),
                    c1: left.c1.max(right.c1),
                }]))
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
                    Value::Refs(output)
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
            let c = if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209 {
                core_projection::source_compare(&l, &r, &mut self.budget)?
            } else {
                compare(&l, &r, &mut self.budget)?
            };
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
            BinaryOp::Add => {
                if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209 {
                    source_arithmetic::add(l, r, &mut self.budget)?
                } else {
                    l + r
                }
            }
            BinaryOp::Subtract => {
                if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209 {
                    source_arithmetic::subtract(l, r, &mut self.budget)?
                } else {
                    l - r
                }
            }
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
        let result = if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209
            && op != BinaryOp::Subtract
            && result == 0.0
        {
            0.0
        } else {
            result
        };
        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
        Ok(self.numeric_policy.coerce_checked_numeric_result(result))
    }
    // A reference or array supplies only stored cells; aggregate functions ignore blanks.
    /// Collect aggregation members, retaining reference origin and sparse traversal behavior.
    pub(super) fn items(&mut self, value: Value) -> EvalResult<Vec<(CellValue, bool)>> {
        match value {
            Value::Missing => {
                self.budget.allocation(1)?;
                Ok(vec![(Blank, false)])
            }
            Value::Scalar(value) | Value::SelectedScalar(value, _) => {
                self.budget.allocation(1)?;
                Ok(vec![(value, false)])
            }
            Value::Array(rows) => {
                let count = rows.iter().map(Vec::len).sum();
                self.budget.allocation(count)?;
                Ok(rows
                    .into_iter()
                    .flatten()
                    .map(|value| (value, true))
                    .collect())
            }
            Value::SignedRefs(areas) => {
                let mut values = Vec::new();
                for area in areas {
                    let Some(physical) = area.physical() else {
                        continue;
                    };
                    let addresses = self.sparse_addresses(physical)?;
                    self.budget
                        .check_allocation(values.len().saturating_add(addresses.len()))?;
                    self.budget
                        .charge(WorkKind::Materialization, addresses.len() as u64)?;
                    for address in addresses {
                        values.push((self.cell(area.sheet, address)?, true));
                    }
                }
                Ok(values)
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let mut values = Vec::new();
                for area in areas {
                    let addresses = self.sparse_addresses(area)?;
                    self.budget
                        .check_allocation(values.len().saturating_add(addresses.len()))?;
                    self.budget
                        .charge(WorkKind::Materialization, addresses.len() as u64)?;
                    for address in addresses {
                        values.push((self.cell(area.sheet, address)?, true));
                    }
                }
                Ok(values)
            }
        }
    }
}
