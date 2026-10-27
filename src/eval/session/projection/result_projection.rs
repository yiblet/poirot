//! Final worksheet results and legacy scalar selection.

use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::session::cells::CellEvaluator;
use crate::eval::session::composition::Session;
use crate::eval::spill;
use crate::eval::spill::SpillPublisher;
use crate::eval::values::{
    AreaKind, ReferenceAreas, VALUE_ERROR, Value, convert_legacy_missing_blank,
};
use crate::eval::vm::RectangularTraversal;
use crate::model::CellError::Ref;
use crate::model::CellValue::Error;
use crate::model::{CellAddress, CellValue};

/// Projects evaluated values into worksheet results and legacy scalars.
pub(in crate::eval) struct ResultProjection<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ResultProjection<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Project a worksheet result with its checked footprint for later atomic admission.
    pub(in crate::eval) fn publish_worksheet_result(
        &mut self,
        sheet: usize,
        address: CellAddress,
        mut value: Value,
    ) -> EvalResult<spill::ComputedCell> {
        let area_reference = match &value {
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                matches!(areas.as_slice(), [area] if area.kind == AreaKind::Area)
            }
            Value::SignedRefs(areas) => {
                matches!(areas.as_slice(), [area] if area.kind == AreaKind::Area && area.positive_shape().is_some())
            }
            _ => false,
        };
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && area_reference
        {
            let shape =
                crate::eval::vm::Shape::inspect(&value).ok_or(EvaluationFailure::StalePlan)?;
            value = Value::Array(RectangularTraversal::new(self.session).array(
                address,
                shape,
                |session, row, column| {
                    RectangularTraversal::new(session).element(&value, shape, row, column)
                },
            )?);
        }
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let Value::Array(rows) = &mut value
        {
            for member in rows.values.iter_mut().flatten() {
                // TODO(coercion-non-argument): [LOW] Each published spill member becomes a worksheet value; inline array members retain blanks.
                poirot_functions::coercion::convert_worksheet_formula_result(member);
            }
        }
        let mut result = if self.session.policies.array_result_policy
            == ArrayResultPolicy::DynamicSpill
            && let Value::Array(rows) = value
        {
            SpillPublisher::new(self.session).stage(sheet, address, rows)?
        } else {
            spill::ComputedCell::scalar(self.finish_expression(sheet, address, value)?)
        };
        // TODO(coercion-non-argument): [LOW] Completed worksheet formulas expose numeric zero for a remaining blank; reads and intermediate selections preserve identity.
        poirot_functions::coercion::convert_worksheet_formula_result(&mut result.value);
        Ok(result)
    }

    /// Finish an ad hoc expression, rejecting Missing without publishing worksheet spills.
    pub(in crate::eval) fn finish_expression(
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
    pub(in crate::eval) fn select_legacy_scalar(
        &mut self,
        at: CellAddress,
        value: Value,
    ) -> EvalResult<CellValue> {
        match value {
            // TODO(coercion-non-argument): [MEDIUM] Generic scalar projection drops omission identity rather than leaving the consumer to choose a default.
            Value::Missing => Ok(convert_legacy_missing_blank()),
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
                    CellEvaluator::new(self.session).cell(first.sheet, at)
                } else {
                    self.session.state.runtime.budget.allocation(1)?;
                    self.select_legacy_scalar(at, Value::Refs(ReferenceAreas::One(*first)))
                }
            }
            Value::SignedRefs(areas) => {
                let mut found = None;
                for area in areas {
                    self.session
                        .state
                        .runtime
                        .budget
                        .charge(WorkKind::ReferencePair, 1)?;
                    let plan = area.implicit(at);
                    if let Some(plan) = plan
                        && found.replace(plan).is_some()
                    {
                        return Ok(Error(VALUE_ERROR));
                    }
                }
                match found {
                    Some(plan) => CellEvaluator::new(self.session).signed_read(plan),
                    None => Ok(Error(VALUE_ERROR)),
                }
            }
            Value::Refs(areas) => {
                let mut found = None;
                for area in areas {
                    self.session
                        .state
                        .runtime
                        .budget
                        .charge(WorkKind::ReferencePair, 1)?;
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
                    Some((sheet, address)) => CellEvaluator::new(self.session).cell(sheet, address),
                    None => Ok(Error(VALUE_ERROR)),
                }
            }
        }
    }
}
