//! Demand-driven spill settlement for reads, ranges and spill references.
use super::anchor::source_anchor;
use super::settled_area::SettledArea;
use super::settlement;
use super::spill_access::SpillAccess;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::limits::EvalResult;
use crate::eval::session;
use crate::eval::session::Session;
use crate::eval::session::{CellEvaluator, ReadRecorder};
use crate::eval::values::{Area, AreaKind, ReferenceAreas, Value, err};
use crate::model::CellError::Ref;
use crate::model::CellValue::Error;
use crate::model::{CellAddress, CellError};

/// Settles the spill producers a read demands before the read observes ownership.
///
/// Every settlement attaches a fresh `SpillAccess` port to the engine; producers it
/// computes may demand further areas recursively through another `SpillDemand`.
pub(in crate::eval) struct SpillDemand<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> SpillDemand<'s, 'a> {
    /// Borrow the session for one demanded read.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Resolve one anchor to its finalized footprint without materializing referenced values.
    pub(in crate::eval) fn reference(&mut self, value: Value) -> EvalResult<Value> {
        let area = match source_anchor(&value) {
            Ok(area) => area,
            Err(error) => return Ok(err(error)),
        };
        Ok(match self.extent(area)? {
            Ok(area) => {
                self.session.state.runtime.budget.allocation(1)?;
                Value::Refs(ReferenceAreas::One(area))
            }
            Err(error) => err(error),
        })
    }

    /// Resolve finalized ownership and extent for an admitted physical anchor.
    pub(in crate::eval) fn extent(&mut self, area: Area) -> EvalResult<Result<Area, CellError>> {
        let anchor = CellAddress {
            row: area.r0,
            col: area.c0,
        };
        // Spill references consume extent/ownership, not merely the anchor's
        // scalar value. Scalar dependency equality cannot certify this read.
        if let Some(reads) = &mut self.session.reuse.overlay_reads {
            reads.mark_metadata(self.session.state.runtime.stack.consumer);
        }
        let value = CellEvaluator::new(self.session).cell(area.sheet, anchor)?;
        let Some(last) = self
            .session
            .state
            .runtime
            .preparation
            .roles
            .get(&(area.sheet, anchor))
            .and_then(|role| role.last())
        else {
            return Ok(Err(match value {
                Error(error) => error,
                _ => Ref,
            }));
        };
        Ok(Ok(Area {
            kind: AreaKind::Area,
            sheet: area.sheet,
            r0: anchor.row,
            r1: last.row,
            c0: anchor.col,
            c1: last.col,
        }))
    }

    /// Resolve a demanded area and retain a future-growth membership guard.
    pub(in crate::eval) fn prepare(&mut self, area: Area) -> EvalResult<()> {
        self.resolve(area)?;
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill {
            ReadRecorder::new(self.session).read_spill_membership(
                area.sheet,
                CellAddress { row: 0, col: 0 },
                CellAddress {
                    row: area.r1,
                    col: area.c1,
                },
            );
        }
        Ok(())
    }

    /// Settle a point read using its already resolved immutable source and ownership eligibility.
    pub(in crate::eval) fn resolve_cell(
        &mut self,
        sheet: usize,
        address: CellAddress,
        source: session::ResolvedSource<'a>,
    ) -> EvalResult<()> {
        self.settle(
            Area {
                kind: AreaKind::Cell,
                sheet,
                r0: address.row,
                r1: address.row,
                c0: address.col,
                c1: address.col,
            },
            Some(source),
        )
    }

    /// Complete a range's preparation before retaining its bounded member-read proof.
    pub(in crate::eval) fn resolve(&mut self, area: Area) -> EvalResult<()> {
        self.settle(area, None)?;
        // Point reads preserve the enclosing operand. Nested private attempts may
        // complete provisional work, but cannot publish proof across rollback.
        if area.kind == AreaKind::Area
            && self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && self
                .session
                .state
                .runtime
                .preparation
                .settlement
                .active()
                .is_none()
        {
            self.session.state.runtime.preparation.ready = Some(SettledArea(area));
        }
        Ok(())
    }

    /// Attach the recursive evaluator adapter to the spill settlement engine.
    fn settle(
        &mut self,
        area: Area,
        source: Option<session::ResolvedSource<'a>>,
    ) -> EvalResult<()> {
        settlement::Settlement::new(&mut SpillAccess::new(self.session)).settle(area, source)
    }
}
