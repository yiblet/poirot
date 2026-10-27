//! Reference union, range and intersection operators.

use crate::eval::limits::{EvalResult, WorkKind};
use crate::eval::session::Session;
use crate::eval::values::{Area, AreaKind, ReferenceAreas, SignedArea, VALUE_ERROR, Value, err};
use crate::model::CellError::{Null, Ref};
use crate::model::CellValue::Error;
use poirot_parsing::formula::BinaryOp;

/// Applies reference operators to physical or signed reference operands.
pub(in crate::eval) struct ReferenceOperators<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ReferenceOperators<'s, 'a> {
    /// Borrow the session whose budget reference operators charge.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Execute physical reference union, range, or intersection operations.
    pub(in crate::eval) fn references(
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
                self.session
                    .state
                    .runtime
                    .budget
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
                    self.session
                        .state
                        .runtime
                        .budget
                        .charge(WorkKind::ReferencePair, 1)?;
                    if area.sheet != envelope.sheet {
                        return Ok(err(Ref));
                    }
                    envelope.r0 = envelope.r0.min(area.r0);
                    envelope.r1 = envelope.r1.max(area.r1);
                    envelope.c0 = envelope.c0.min(area.c0);
                    envelope.c1 = envelope.c1.max(area.c1);
                }
                self.session.state.runtime.budget.allocation(1)?;
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
                        self.session
                            .state
                            .runtime
                            .budget
                            .charge(WorkKind::ReferencePair, 1)?;
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
                            self.session
                                .state
                                .runtime
                                .budget
                                .check_allocation(output.len() + 1)?;
                            self.session
                                .state
                                .runtime
                                .budget
                                .charge(WorkKind::Materialization, 1)?;
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

    /// Project ordinary or signed references into signed areas.
    fn signed_areas(&mut self, value: Value) -> EvalResult<Option<Vec<SignedArea>>> {
        Ok(match value {
            Value::SignedRefs(areas) => Some(areas),
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                self.session.state.runtime.budget.allocation(areas.len())?;
                Some(areas.into_iter().map(SignedArea::from_unsigned).collect())
            }
            _ => None,
        })
    }

    /// Execute union, range, or intersection while retaining signed geometry.
    pub(in crate::eval) fn signed_references(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
    ) -> EvalResult<Value> {
        if let Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) = &left {
            return Ok(err(*error));
        }
        if let Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) = &right {
            return Ok(err(*error));
        }
        let (Some(left), Some(right)) = (self.signed_areas(left)?, self.signed_areas(right)?)
        else {
            return Ok(err(VALUE_ERROR));
        };
        match op {
            BinaryOp::Union => {
                self.session
                    .state
                    .runtime
                    .budget
                    .allocation(left.len().saturating_add(right.len()))?;
                Ok(Value::SignedRefs(left.into_iter().chain(right).collect()))
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
                for area in left.iter().skip(1).chain(right.iter().skip(1)) {
                    self.session
                        .state
                        .runtime
                        .budget
                        .charge(WorkKind::ReferencePair, 1)?;
                    if area.sheet != envelope.sheet {
                        return Ok(err(Ref));
                    }
                    envelope.r0 = envelope.r0.min(area.r0);
                    envelope.r1 = envelope.r1.max(area.r1);
                    envelope.c0 = envelope.c0.min(area.c0);
                    envelope.c1 = envelope.c1.max(area.c1);
                }
                self.session.state.runtime.budget.allocation(1)?;
                Ok(Value::SignedRefs(vec![envelope.source_bounds(
                    (
                        envelope.r0.min(right_first.r0),
                        envelope.r1.max(right_first.r1),
                    ),
                    (
                        envelope.c0.min(right_first.c0),
                        envelope.c1.max(right_first.c1),
                    ),
                )]))
            }
            BinaryOp::Intersection => {
                let mut output = Vec::new();
                for a in left {
                    for b in &right {
                        self.session
                            .state
                            .runtime
                            .budget
                            .charge(WorkKind::ReferencePair, 1)?;
                        if a.sheet == b.sheet
                            && let Some(area) = SignedArea::new(
                                AreaKind::Area,
                                a.sheet,
                                (a.r0.max(b.r0), a.r1.min(b.r1)),
                                (a.c0.max(b.c0), a.c1.min(b.c1)),
                            )
                        {
                            self.session
                                .state
                                .runtime
                                .budget
                                .check_allocation(output.len() + 1)?;
                            self.session
                                .state
                                .runtime
                                .budget
                                .charge(WorkKind::Materialization, 1)?;
                            output.push(a.source_bounds((area.r0, area.r1), (area.c0, area.c1)));
                        }
                    }
                }
                Ok(if output.is_empty() {
                    err(Null)
                } else {
                    Value::SignedRefs(output)
                })
            }
            _ => Ok(err(Ref)),
        }
    }
}
