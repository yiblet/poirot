//! Rectangular array publication and shared atomic memo admission.
use super::*;

/// Legal committed spill roles shared by local and retained result publication.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum SpillRole {
    /// A scalar result without a generated footprint.
    #[default]
    None,
    /// Producer whose complete rectangular footprint ends at this address.
    Anchor { last: CellAddress },
    /// Generated result owned by another source cell on the same sheet.
    Child { anchor: CellAddress },
}
impl SpillRole {
    /// Resolve ownership relative to the address holding this record.
    pub(crate) fn owner(self, address: CellAddress) -> Option<CellAddress> {
        match self {
            Self::None => None,
            Self::Anchor { .. } => Some(address),
            Self::Child { anchor } => Some(anchor),
        }
    }
    /// Return the committed extent only for a producer.
    pub(crate) fn last(self) -> Option<CellAddress> {
        match self {
            Self::Anchor { last } => Some(last),
            _ => None,
        }
    }
}
/// Derive one committed role from the engine's shared footprint ownership map.
pub(crate) fn spill_role(owners: &CellMap<CellAddress>, key: (usize, CellAddress)) -> SpillRole {
    match owners.get(&key) {
        None => SpillRole::None,
        Some(&anchor) if anchor != key.1 => SpillRole::Child { anchor },
        Some(&anchor) => SpillRole::Anchor {
            last: owners
                .iter()
                .filter(|((sheet, _), owner)| *sheet == key.0 && **owner == anchor)
                .fold(anchor, |last, ((_, at), _)| CellAddress {
                    row: last.row.max(at.row),
                    col: last.col.max(at.col),
                }),
        },
    }
}

impl Session<'_> {
    /// Restore an entire validated footprint together. A cached scalar child alone
    /// cannot establish spill ownership or reserve the rest of its producer's cells.
    pub(super) fn restore_cached_spill(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let Some(cache) = self.overlay_cache else {
            return Ok(());
        };
        let restored = {
            let cache = cache
                .try_borrow()
                .map_err(|_| EvaluationFailure::StalePlan)?;
            let Some(owner) = cache
                .get((sheet, at))
                .and_then(|c| c.dependencies.spill.owner(at))
            else {
                return Ok(());
            };
            if self.cache.contains_key(&(sheet, owner)) {
                return Ok(());
            }
            let Some(last) = cache
                .get((sheet, owner))
                .and_then(|c| c.dependencies.spill.last())
            else {
                return Ok(());
            };
            let height = (last.row - owner.row + 1) as usize;
            let width = (last.col - owner.col + 1) as usize;
            self.budget.check_allocation(height.saturating_mul(width))?;
            let mut rows = Vec::with_capacity(height);
            for row in owner.row..=last.row {
                let mut values = Vec::with_capacity(width);
                for col in owner.col..=last.col {
                    let Some(entry) = cache.get((sheet, CellAddress { row, col })) else {
                        return Ok(());
                    };
                    self.budget.text(entry.value.text_storage_bytes())?;
                    values.push(entry.value.as_ref().clone());
                }
                rows.push(values);
            }
            (owner, rows)
        };
        let value = restored
            .1
            .first()
            .and_then(|row| row.first())
            .ok_or(EvaluationFailure::StalePlan)?
            .clone();
        self.pending_spills
            .insert((sheet, restored.0), ArrayView::unknown(restored.1));
        self.admit_cell(sheet, restored.0, &value)?;
        self.run_usage.external_cache_hits += 1;
        Ok(())
    }
    /// Stage a producer's spill after checking blockers and ownership.
    pub(super) fn stage_spill(
        &mut self,
        sheet: usize,
        at: CellAddress,
        rows: ArrayView,
    ) -> EvalResult<CellValue> {
        if !self.visiting.contains(&(sheet, at)) {
            return Err(self.spill_unsupported(sheet,at,"dynamic array publication requires a source formula cell; ad hoc probes are unsupported"));
        }
        let width = rows.first().map_or(0, Vec::len);
        if width == 0 || rows.iter().any(|row| row.len() != width) {
            return Ok(Error(VALUE_ERROR));
        }
        let height = rows.len();
        let bound = self.source_spill_shape(sheet, at);
        if bound.is_none_or(|(r, c)| height as u64 > r || width as u64 > c) {
            return Err(self.spill_unsupported(sheet,at,"dynamic producer has no supported static footprint bound (names, extensions and computed references are unsupported)"));
        }
        let last = at
            .row
            .checked_add(u32::try_from(height - 1).unwrap_or(u32::MAX))
            .zip(
                at.col
                    .checked_add(u32::try_from(width - 1).unwrap_or(u32::MAX)),
            )
            .map(|(row, col)| CellAddress { row, col });
        if last.is_none_or(|last| last.validate().is_err()) {
            return Ok(Error(CellError::Spill));
        }
        let count = height
            .checked_mul(width)
            .ok_or_else(|| self.spill_unsupported(sheet, at, "array shape overflow"))?;
        self.budget.check_allocation(count)?;
        // SAFETY: The preceding last.is_none_or guard returns on None before this unwrap; last is not reassigned.
        let last = last.unwrap();
        if self
            .spill_read_ranges
            .get(&(sheet, at))
            .is_some_and(|ranges| {
                ranges.iter().any(|area| {
                    area.sheet == sheet
                        && area.r0 <= last.row
                        && area.r1 >= at.row
                        && area.c0 <= last.col
                        && area.c1 >= at.col
                })
            })
        {
            return Err(self.spill_unsupported(
                sheet,
                at,
                "array result reads its own unpublished sparse footprint",
            ));
        }
        self.read(DependencyRead::SparseMembership {
            sheet,
            first: at,
            last,
        });
        let effective = run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs);
        for r in 0..height {
            for c in 0..width {
                self.budget.charge(WorkKind::ReferencePair, 1)?;
                let child = CellAddress {
                    row: at.row + r as u32,
                    col: at.col + c as u32,
                };
                if self
                    .blank_reads
                    .get(&(sheet, at))
                    .is_some_and(|reads| reads.contains(&(sheet, child)))
                {
                    return Err(self.spill_unsupported(
                        sheet,
                        at,
                        "array result reads its own unpublished footprint",
                    ));
                }
                if child != at
                    && (effective
                        .cell(sheet, child)
                        .is_some_and(|cell| cell.formula.is_some() || cell.value != Blank)
                        || self
                            .spill_owners
                            .get(&(sheet, child))
                            .is_some_and(|owner| *owner != at))
                {
                    return Ok(Error(CellError::Spill));
                }
            }
        }
        let first = rows
            .first()
            .and_then(|row| row.first())
            .ok_or(EvaluationFailure::StalePlan)?
            .clone();
        self.pending_spills.insert((sheet, at), rows);
        Ok(first)
    }
    /// Attribute an unsupported spill capability to its producer cell.
    pub(super) fn spill_unsupported(
        &self,
        sheet: usize,
        address: CellAddress,
        detail: &str,
    ) -> EvaluationFailure {
        EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::FunctionAdapter,
            source: Some(EvaluationTarget { sheet, address }),
            detail: detail.into(),
        }
    }
    /// Admit a completed cell and all staged spill children before making them visible.
    pub(super) fn admit_cell(
        &mut self,
        sheet: usize,
        at: CellAddress,
        value: &CellValue,
    ) -> EvalResult<()> {
        // Stage all payloads before admission; no children are visible on any failure.
        let rows = self.pending_spills.remove(&(sheet, at));
        let count = rows
            .as_ref()
            .map_or(1, |rows| rows.iter().map(Vec::len).sum());
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
                replaced.cells += 1;
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
                        CellAddress {
                            row: at.row + r as u32,
                            col: at.col + c as u32,
                        },
                        value,
                    )?;
                }
            }
        } else {
            inspect(at, value)?;
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
        if let Some(limits) = self.memo_limits
            && (overflow || next.cells > limits.cells || next.bytes > limits.bytes)
        {
            return Err(EvaluationFailure::MemoRetentionLimit {
                limits,
                consumed: self.memo_usage,
                requested,
                source: Some(EvaluationTarget { sheet, address: at }),
            });
        }
        self.memo_usage = next;
        if let Some(rows) = rows {
            for (r, row) in rows.into_iter().enumerate() {
                for (c, value) in row.into_iter().enumerate() {
                    let child = CellAddress {
                        row: at.row + r as u32,
                        col: at.col + c as u32,
                    };
                    self.cache.insert((sheet, child), value);
                    self.spill_owners.insert((sheet, child), at);
                }
            }
        } else {
            self.cache.insert((sheet, at), value.clone());
        }
        self.budget.usage.completed_cells += count as u64;
        Ok(())
    }
    /// Discover a producer whose footprint may supply this cell.
    pub(super) fn discover_spill(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        self.discover_spills_in(
            Area {
                kind: AreaKind::Cell,
                sheet,
                r0: at.row,
                r1: at.row,
                c0: at.col,
                c1: at.col,
            },
            Some(at),
        )
    }
    fn source_spill_shape(&self, sheet: usize, at: CellAddress) -> Option<(u64, u64)> {
        let prepared = self
            .plan
            .and_then(|plan| plan.formula(EvaluationTarget { sheet, address: at }))
            .cloned()
            .or_else(|| {
                run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs)
                    .cell(sheet, at)
                    .and_then(|cell| cell.formula.as_deref())
                    .map(|source| self.prepare_source(source))
            })?;
        match prepared.as_ref() {
            PreparedFormula::Ready(formula) => formula.spill.shape,
            _ => None,
        }
    }
    /// Resolve competing producers by ascending source address (row, then column),
    /// independent of request order. Only successfully admitted footprints reserve
    /// cells; a blocked or smaller-than-bound earlier producer does not win by name.
    pub(super) fn arbitrate_spill(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        if self.array_result_policy != ArrayResultPolicy::DynamicSpill
            || self.visiting.contains(&(sheet, at))
        {
            return Ok(());
        }
        let Some((rows, cols)) = self.source_spill_shape(sheet, at) else {
            return Ok(());
        };
        if rows == 0 || cols == 0 {
            return Ok(());
        }
        let area = Area {
            kind: AreaKind::Area,
            sheet,
            r0: at.row,
            c0: at.col,
            r1: (u64::from(at.row).saturating_add(rows - 1))
                .min(u64::from(poirot_model::MAX_ROWS - 1)) as u32,
            c1: (u64::from(at.col).saturating_add(cols - 1))
                .min(u64::from(poirot_model::MAX_COLUMNS - 1)) as u32,
        };
        // Every producer first admits earlier intersecting source producers.
        // Strict address ordering also makes transitive overlap discovery terminate.
        let target = EvaluationTarget { sheet, address: at };
        self.budget.enter_depth().map_err(|mut failure| {
            failure.set_target(target);
            failure
        })?;
        self.budget.push_cell(target);
        let previous = self.consumer.replace(target);
        self.visiting.insert((sheet, at));
        let result = self.discover_spills_before(area, Some(at), Some(at));
        self.visiting.remove(&(sheet, at));
        if result.is_err() {
            self.blank_reads.remove(&(sheet, at));
            self.spill_read_ranges.remove(&(sheet, at));
        }
        self.consumer = previous;
        self.budget.pop_cell();
        self.budget.leave_depth();
        result
    }
    /// Discover producers relevant to a requested rectangle or point.
    pub(super) fn discover_spills_in(
        &mut self,
        area: Area,
        point: Option<CellAddress>,
    ) -> EvalResult<()> {
        self.discover_spills_before(area, point, None)
    }
    fn discover_spills_before(
        &mut self,
        area: Area,
        point: Option<CellAddress>,
        before: Option<CellAddress>,
    ) -> EvalResult<()> {
        let sheet = area.sheet;
        if self.array_result_policy != ArrayResultPolicy::DynamicSpill {
            return Ok(());
        }
        // Inspect compiled syntax only. Static geometry excludes unrelated producers
        // before execution, including volatile/erroring formulas outside the target.
        // Discovery depends on the source membership of all possible earlier
        // anchors, not just the eventual overlap. New producers can begin outside
        // a consumer's range and spill into it.
        self.read(DependencyRead::SparseMembership {
            sheet,
            first: CellAddress { row: 0, col: 0 },
            last: CellAddress {
                row: area.r1,
                col: area.c1,
            },
        });
        let mut candidates = Vec::new();
        if self.workbook.sheet(sheet).is_some() {
            let effective = run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs);
            let membership = poirot_model::query::RangeMembership::new(
                sheet,
                CellAddress { row: 0, col: 0 },
                CellAddress {
                    row: area.r1,
                    col: area.c1,
                },
            )
            .map_err(|_| EvaluationFailure::StalePlan)?;
            for (&anchor, cell) in effective
                .stored(membership)
                .map_err(|_| EvaluationFailure::StalePlan)?
            {
                self.budget.charge(WorkKind::ReferencePair, 1)?;
                if Some(anchor) == point
                    || before.is_some_and(|before| anchor >= before)
                    || cell.formula.is_none()
                    || self.cache.contains_key(&(sheet, anchor))
                {
                    continue;
                }
                let prepared = self
                    .plan
                    .and_then(|plan| {
                        plan.formula(EvaluationTarget {
                            sheet,
                            address: anchor,
                        })
                    })
                    .cloned()
                    .unwrap_or_else(|| self.prepare_source(cell.formula.as_deref().unwrap_or("")));
                let (shape, unknown) = match prepared.as_ref() {
                    PreparedFormula::Ready(formula) => (
                        formula.spill.shape,
                        formula.spill.unbounded || {
                            let p = formula.bytecode();
                            p.calls.iter().any(|c| {
                                matches!(c.target, crate::bytecode::CallTarget::Extension(_))
                            })
                        },
                    ),
                    _ => (None, false),
                };
                let covers = shape.is_some_and(|(rows, cols)| {
                    u64::from(anchor.row).saturating_add(rows) > u64::from(area.r0)
                        && u64::from(anchor.col).saturating_add(cols) > u64::from(area.c0)
                });
                if unknown && !covers {
                    return Err(self.spill_unsupported(sheet,anchor,"child-first discovery cannot bound this dynamic producer (names, extensions and computed references are unsupported)"));
                }
                if self.visiting.contains(&(sheet, anchor)) {
                    if before.is_some() && covers {
                        return Err(self.spill_unsupported(sheet,anchor,"source-ordered spill arbitration encountered a circular overlapping producer dependency"));
                    }
                    continue;
                }
                if let Some((rows, cols)) = shape
                    && u64::from(anchor.row).saturating_add(rows) > u64::from(area.r0)
                    && u64::from(anchor.col).saturating_add(cols) > u64::from(area.c0)
                {
                    self.budget
                        .check_allocation(candidates.len().saturating_add(1))?;
                    candidates.push(anchor);
                }
            }
        }
        candidates.sort();
        for anchor in candidates {
            self.budget.charge(WorkKind::ReferencePair, 1)?;
            self.cell(sheet, anchor)?;
            if point.is_some_and(|at| self.spill_owners.contains_key(&(sheet, at))) {
                break;
            }
        }
        Ok(())
    }
}
// Only statically bounded producers are discoverable before their anchor.
/// Infer fixed spill dimensions from temporary syntax when supported.
pub(crate) fn spill_shape(expr: &poirot_parsing::formula::Expr) -> Option<(u64, u64)> {
    use poirot_parsing::formula::Expr as E;
    match expr {
        E::Array(rows) => Some((rows.len() as u64, rows.first()?.len() as u64)),
        E::Unary { op, operand } if *op != UnaryOp::ImplicitIntersection => operand_shape(operand),
        E::Binary { op, left, right }
            if !matches!(
                op,
                BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection
            ) =>
        {
            let a = operand_shape(left)?;
            let b = operand_shape(right)?;
            Some((a.0.max(b.0), a.1.max(b.1)))
        }
        E::Function { name, args } => match name.to_ascii_uppercase().as_str() {
            "TRANSPOSE" => {
                let (r, c) = operand_shape(args.first()?)?;
                Some((c, r))
            }
            "MINVERSE" | "ISERROR" | "ISERR" | "ISNA" | "ISNUMBER" | "ISTEXT" | "ISBLANK"
            | "ISLOGICAL" | "ISNONTEXT" => operand_shape(args.first()?),
            "MMULT" => Some((
                operand_shape(args.first()?)?.0,
                operand_shape(args.get(1)?)?.1,
            )),
            "FREQUENCY" => {
                let (r, c) = operand_shape(args.get(1)?)?;
                Some((r.checked_mul(c)?.checked_add(1)?, 1))
            }
            "IF" | "IFS" | "SWITCH" | "CHOOSE" => args
                .iter()
                .filter_map(operand_shape)
                .reduce(|a, b| (a.0.max(b.0), a.1.max(b.1))),
            "IFERROR" | "IFNA" => args
                .iter()
                .filter_map(operand_shape)
                .reduce(|a, b| (a.0.max(b.0), a.1.max(b.1))),
            _ => None,
        },
        _ => None,
    }
}
fn operand_shape(expr: &poirot_parsing::formula::Expr) -> Option<(u64, u64)> {
    use poirot_parsing::formula::Expr as E;
    match expr {
        E::Cell(_) => Some((1, 1)),
        E::Binary {
            op: BinaryOp::Range,
            left,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (E::Cell(a), E::Cell(b)) => Some((
                u64::from(a.row.abs_diff(b.row)) + 1,
                u64::from(a.column.abs_diff(b.column)) + 1,
            )),
            _ => None,
        },
        E::Unary {
            op: poirot_parsing::formula::UnaryOp::ImplicitIntersection,
            ..
        } => Some((1, 1)),
        E::Function { name, .. }
            if registry::resolve_function(name).is_some_and(|descriptor| {
                !matches!(
                    descriptor.handler,
                    FunctionId::Matrix(
                        registry::MatrixFunction::Transpose
                            | registry::MatrixFunction::Minverse
                            | registry::MatrixFunction::Mmult
                            | registry::MatrixFunction::Frequency
                            | registry::MatrixFunction::Trend
                    ) | FunctionId::Logical(
                        registry::LogicalFunction::If
                            | registry::LogicalFunction::IfError
                            | registry::LogicalFunction::IfNa
                            | registry::LogicalFunction::Choose
                            | registry::LogicalFunction::Ifs
                            | registry::LogicalFunction::Switch
                    ) | FunctionId::Information(
                        registry::InformationFunction::IsError
                            | registry::InformationFunction::IsErr
                            | registry::InformationFunction::IsNa
                            | registry::InformationFunction::IsNumber
                            | registry::InformationFunction::IsText
                            | registry::InformationFunction::IsBlank
                            | registry::InformationFunction::IsLogical
                            | registry::InformationFunction::IsNonText
                    ) | FunctionId::Reference(
                        registry::ReferenceFunction::Indirect
                            | registry::ReferenceFunction::Offset
                            | registry::ReferenceFunction::Index
                    )
                )
            }) =>
        {
            Some((1, 1))
        }
        _ => spill_shape(expr).or_else(|| {
            matches!(expr, E::Number(_) | E::String(_) | E::Bool(_) | E::Error(_)).then_some((1, 1))
        }),
    }
}

/// Whether syntax may produce an array without statically bounded dimensions.
pub(crate) fn unbounded_producer(expr: &poirot_parsing::formula::Expr) -> bool {
    use poirot_parsing::formula::Expr as E;
    match expr {
        E::Name(_) | E::QualifiedName { .. } => true,
        E::Unary {
            op: UnaryOp::ImplicitIntersection,
            ..
        } => false,
        E::Unary { .. } => spill_shape(expr).is_none(),
        E::Binary { op, .. }
            if !matches!(
                op,
                BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection
            ) =>
        {
            spill_shape(expr).is_none()
        }
        E::Function { name, args } => match name.to_ascii_uppercase().as_str() {
            "TRANSPOSE" | "MMULT" | "MINVERSE" | "FREQUENCY" => spill_shape(expr).is_none(),
            "ISERROR" | "ISERR" | "ISNA" | "ISNUMBER" | "ISTEXT" | "ISBLANK" | "ISLOGICAL"
            | "ISNONTEXT" => args.iter().any(unbounded_producer),
            "TREND" => true,
            "IF" | "IFS" | "SWITCH" | "CHOOSE" => args.iter().any(unbounded_producer),
            "IFERROR" | "IFNA" => args.iter().any(unbounded_producer),
            _ => false,
        },
        _ => false,
    }
}
