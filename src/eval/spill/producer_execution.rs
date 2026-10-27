//! Private producer computation and admission during spill settlement.
use super::attempted_footprint::AttemptedFootprint;
use super::computed_cell::ComputedCell;
use super::held::Held;
use super::settlement::unsupported;
use super::spill_publisher::SpillPublisher;
use crate::eval::config::ProducerPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::ports::{CellCompletion, CellProvenance, same_result_identity};
use crate::eval::reuse::SpillRole;
use crate::eval::session::Session;
use crate::eval::session::{CellEvaluator, FormulaExecution, ResultProjection};
use crate::eval::values::{Area, AreaKind, ArrayView};
use crate::model::CellValue::{Blank, Error};
use crate::model::{CellAddress, CellError};

/// Computes and admits private array producers for the settlement engine.
///
/// A producer's payload is held privately until the engine admits it; retained
/// overlay results and caller-trusted saved spills restore whole footprints instead.
pub(super) struct ProducerExecution<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ProducerExecution<'s, 'a> {
    /// Borrow the session for one producer step.
    pub(super) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Compute a private raw payload; dependency reads resolve within the current attempt.
    pub(super) fn compute(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let key = (sheet, at);
        if self.session.state.runtime.memo.cache.contains_key(&key)
            || self
                .session
                .state
                .runtime
                .preparation
                .held
                .contains_key(&key)
        {
            return Ok(());
        }
        if self
            .session
            .state
            .runtime
            .preparation
            .computing
            .contains(&key)
        {
            return Err(unsupported(
                sheet,
                at,
                "circular unpublished producer dependency",
            ));
        }
        if self.session.ports.cell_reader.is_some() {
            return Err(unsupported(
                sheet,
                at,
                "dynamic spill ownership is incompatible with an external scalar cell cache",
            ));
        }
        let conditional = self
            .session
            .reuse
            .overlay_cache
            .and_then(|cache| {
                cache.try_borrow().ok().and_then(|cache| {
                    cache.retained(key).map(|entry| {
                        matches!(
                            entry.validity,
                            crate::eval::reuse::Validity::Cause { .. }
                                | crate::eval::reuse::Validity::Causes(_)
                        )
                    })
                })
            })
            .unwrap_or(false);
        if conditional {
            // Dirty evidence can itself read a potential child of this producer.
            // Guard validation just like execution against unpublished recursion.
            self.session.state.runtime.preparation.computing.insert(key);
            let validated = CellEvaluator::new(self.session).revalidate_cached(key);
            self.session
                .state
                .runtime
                .preparation
                .computing
                .remove(&key);
            if validated?.is_some()
                && let Some(reads) = &mut self.session.reuse.overlay_reads
            {
                reads.fresh_results.insert(key);
            }
        }
        self.restore_cached(sheet, at)?;
        if self
            .session
            .state
            .runtime
            .preparation
            .held
            .contains_key(&key)
        {
            return Ok(());
        }
        let retained = self.session.reuse.overlay_cache.and_then(|cache| {
            cache
                .try_borrow()
                .ok()?
                .retained(key)
                .filter(|entry| {
                    entry.validity.is_clean()
                        || self
                            .session
                            .reuse
                            .overlay_reads
                            .as_ref()
                            .is_some_and(|reads| reads.validated.contains(&key))
                })
                .filter(|entry| entry.dependencies.spill.owner(at).is_none())
                .map(|entry| entry.value.as_ref().clone())
        });
        if let Some(value) = retained {
            self.session.state.runtime.preparation.held.insert(
                key,
                Held {
                    result: ComputedCell::scalar(value),
                    source_cached: true,
                    retained: true,
                },
            );
            if self.session.state.runtime.budget.profiling {
                self.session.state.runtime.run_usage.external_cache_hits = self
                    .session
                    .state
                    .runtime
                    .run_usage
                    .external_cache_hits
                    .saturating_add(1);
            }
            if let Some(reads) = &mut self.session.reuse.overlay_reads {
                reads.completed.insert(key);
            }
            return Ok(());
        }
        if self.session.policies.producer_policy == ProducerPolicy::TrustSaved {
            let result = self.restore_saved(sheet, at);
            if let Err(failure) = &result {
                let consumer = self.session.state.runtime.stack.consumer;
                let _ = SpillPublisher::new(self.session).complete(CellCompletion {
                    consumer,
                    target: EvaluationTarget { sheet, address: at },
                    provenance: CellProvenance::ExternalCache,
                    result: Err(failure),
                });
            }
            return result;
        }
        let prepared = self
            .session
            .sources
            .source_formula(sheet, at)
            .ok_or(EvaluationFailure::StalePlan)?;
        let target = EvaluationTarget { sheet, address: at };
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(target);
                failure
            })?;
        self.session.state.runtime.budget.enter_depth()?;
        self.session.state.runtime.budget.push_cell(target);
        self.session.state.runtime.preparation.computing.insert(key);
        self.session.state.runtime.stack.visiting.insert(key);
        let previous = self.session.state.runtime.stack.consumer.replace(target);
        let demand = self.session.policies.root_demand();
        let previous_demand = std::mem::replace(&mut self.session.state.expression_demand, demand);
        if self.session.state.runtime.budget.profiling {
            self.session.state.runtime.run_usage.formula_executions = self
                .session
                .state
                .runtime
                .run_usage
                .formula_executions
                .saturating_add(1);
        }
        let result = (|| {
            self.session
                .state
                .runtime
                .budget
                .text(prepared.source().len())?;
            let value =
                FormulaExecution::new(self.session).prepared_expression(sheet, at, &prepared)?;
            ResultProjection::new(self.session).publish_worksheet_result(sheet, at, value)
        })();
        self.session.state.expression_demand = previous_demand;
        self.session.state.runtime.stack.consumer = previous;
        self.session.state.runtime.stack.visiting.remove(&key);
        self.session
            .state
            .runtime
            .preparation
            .computing
            .remove(&key);
        self.session.state.runtime.budget.pop_cell();
        self.session.state.runtime.budget.leave_depth();
        self.session
            .state
            .runtime
            .preparation
            .producer_reads
            .forget(key);
        if let Err(failure) = &result {
            // An operational failure is final; unlike a held successful payload,
            // it must retain the normal failed-cell completion notification.
            let _ = SpillPublisher::new(self.session).complete(CellCompletion {
                consumer: previous,
                target,
                provenance: CellProvenance::Evaluated,
                result: Err(failure),
            });
        }
        let result = result?;
        self.session.state.runtime.preparation.held.insert(
            key,
            Held {
                result,
                source_cached: false,
                retained: false,
            },
        );
        Ok(())
    }

    /// Admit one private attempt while preserving raw geometry for ownership arbitration.
    pub(super) fn finalize(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let Some(mut held) = self
            .session
            .state
            .runtime
            .preparation
            .held
            .remove(&(sheet, at))
        else {
            return Ok(());
        };
        if let Some(rows) = &held.result.rows {
            let last_row = u32::try_from(rows.len().saturating_sub(1))
                .ok()
                .and_then(|n| at.row.checked_add(n))
                .ok_or(EvaluationFailure::StalePlan)?;
            let last_col = rows
                .first()
                .and_then(|r| u32::try_from(r.len().saturating_sub(1)).ok())
                .and_then(|n| at.col.checked_add(n))
                .ok_or(EvaluationFailure::StalePlan)?;
            let runtime = &mut *self.session.state.runtime;
            if let Some(active) = runtime.preparation.settlement.active_mut() {
                let settled_collision = (at.row..=last_row).any(|row| {
                    (at.col..=last_col).any(|col| {
                        let address = CellAddress { row, col };
                        runtime
                            .preparation
                            .roles
                            .get(&(sheet, address))
                            .is_some_and(|role| role.owner(address) != Some(at))
                            && !active.admitted.contains(&(sheet, address))
                    })
                });
                active.footprints.insert(
                    (sheet, at),
                    AttemptedFootprint {
                        area: Area {
                            kind: AreaKind::Area,
                            sheet,
                            r0: at.row,
                            c0: at.col,
                            r1: last_row,
                            c1: last_col,
                        },
                        settled_collision,
                    },
                );
                if active.blocked.contains(&(sheet, at)) || settled_collision {
                    held.result.rows = None;
                    held.result.value = Error(CellError::Spill);
                }
            }
        }
        let admission = SpillPublisher::new(self.session).admit(sheet, at, held.result);
        if let Err(failure) = &admission {
            let consumer = self.session.state.runtime.stack.consumer;
            let _ = SpillPublisher::new(self.session).complete(CellCompletion {
                consumer,
                target: EvaluationTarget { sheet, address: at },
                provenance: if held.source_cached {
                    CellProvenance::ExternalCache
                } else {
                    CellProvenance::Evaluated
                },
                result: Err(failure),
            });
        }
        let value = admission?;
        let last = self
            .session
            .state
            .runtime
            .preparation
            .roles
            .get(&(sheet, at))
            .and_then(|role| match role {
                SpillRole::Anchor { last } => Some(*last),
                SpillRole::Child { .. } | SpillRole::None => None,
            })
            .unwrap_or(at);
        // New geometry can enter a previously absent indexed gap even when the
        // producer anchor lies outside the search axis. Valid retained units use
        // restore_cached instead and preserve their existing prefix indexes.
        if !held.retained {
            self.session
                .state
                .runtime
                .lookup_prefixes
                .invalidate_footprint(sheet, at, last);
        }
        {
            let consumer = self.session.state.runtime.stack.consumer;
            SpillPublisher::new(self.session).complete(CellCompletion {
                consumer,
                target: EvaluationTarget { sheet, address: at },
                provenance: if held.source_cached {
                    CellProvenance::ExternalCache
                } else {
                    CellProvenance::Evaluated
                },
                result: Ok(&value),
            })?;
        }
        if let Some(reads) = &mut self.session.reuse.overlay_reads {
            reads.completed.insert((sheet, at));
            if !held.retained {
                reads.fresh_results.insert((sheet, at));
            }
        }
        Ok(())
    }

    /// Restore an entire validated footprint together. A cached scalar child alone
    /// cannot establish spill ownership or reserve the rest of its producer's cells.
    fn restore_cached(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let Some(cache) = self.session.reuse.overlay_cache else {
            return Ok(());
        };
        let restored = {
            let cache = cache
                .try_borrow()
                .map_err(|_| EvaluationFailure::StalePlan)?;
            let validated = |key| {
                self.session
                    .reuse
                    .overlay_reads
                    .as_ref()
                    .is_some_and(|reads| reads.validated.contains(&key))
            };
            let Some(owner) = cache
                .retained((sheet, at))
                .filter(|entry| entry.validity.is_clean() || validated((sheet, at)))
                .and_then(|c| c.dependencies.spill.owner(at))
            else {
                return Ok(());
            };
            if self
                .session
                .state
                .runtime
                .memo
                .cache
                .contains_key(&(sheet, owner))
            {
                return Ok(());
            }
            let owner_validated = validated((sheet, owner));
            let Some(last) = cache
                .retained((sheet, owner))
                .filter(|entry| entry.validity.is_clean() || owner_validated)
                .and_then(|c| c.dependencies.spill.last())
            else {
                return Ok(());
            };
            let height = last
                .row
                .checked_sub(owner.row)
                .and_then(|height| height.checked_add(1))
                .ok_or(EvaluationFailure::StalePlan)? as usize;
            let width = last
                .col
                .checked_sub(owner.col)
                .and_then(|width| width.checked_add(1))
                .ok_or(EvaluationFailure::StalePlan)? as usize;
            self.session
                .state
                .runtime
                .budget
                .check_allocation(height.saturating_mul(width))?;
            let mut rows = Vec::with_capacity(height);
            for row in owner.row..=last.row {
                let mut values = Vec::with_capacity(width);
                for col in owner.col..=last.col {
                    let Some(entry) = cache
                        .retained((sheet, CellAddress { row, col }))
                        .filter(|entry| entry.validity.is_clean() || owner_validated)
                    else {
                        return Ok(());
                    };
                    let address = CellAddress { row, col };
                    if entry.dependencies.spill.owner(address) != Some(owner)
                        || (address == owner && entry.dependencies.spill.last() != Some(last))
                    {
                        return Ok(());
                    }
                    self.session
                        .state
                        .runtime
                        .budget
                        .text(entry.value.text_storage_bytes())?;
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
        self.session.state.runtime.preparation.held.insert(
            (sheet, restored.0),
            Held {
                result: ComputedCell {
                    value,
                    rows: Some(ArrayView::unknown(restored.1)),
                },
                source_cached: true,
                retained: true,
            },
        );
        if self.session.state.runtime.budget.profiling {
            self.session.state.runtime.run_usage.external_cache_hits = self
                .session
                .state
                .runtime
                .run_usage
                .external_cache_hits
                .saturating_add(1);
        }
        Ok(())
    }

    /// Restore caller-trusted source values without evaluating the producer expression.
    fn restore_saved(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let worksheet = self
            .session
            .sources
            .workbook
            .sheet(sheet)
            .ok_or(EvaluationFailure::StalePlan)?;
        let effective = crate::eval::inputs::EffectiveWorkbook::new(
            self.session.sources.workbook,
            self.session.sources.inputs,
        );
        let cell = effective
            .cell(sheet, at)
            .ok_or(EvaluationFailure::StalePlan)?;
        let baseline = worksheet.source_cell(at);
        let unchanged_anchor = baseline.is_some_and(|base| {
            base.formula == cell.formula && same_result_identity(&base.value, &cell.value)
        }) && !self.session.sources.inputs.is_some_and(|inputs| {
            matches!(
                inputs.edit(sheet, at),
                Some(crate::eval::inputs::CellEdit::Replace(_))
            )
        });
        self.session
            .state
            .runtime
            .budget
            .check_text(cell.value.text_storage_bytes())
            .map_err(|mut failure| {
                failure.set_target(EvaluationTarget { sheet, address: at });
                failure
            })?;
        let value = cell.value.clone();
        let mut rows = None;
        if unchanged_anchor {
            let declared = worksheet.spill_extent(at);
            let mut owned = false;
            if declared.is_none() {
                for (_, owner) in worksheet.spill_children() {
                    self.session
                        .state
                        .runtime
                        .budget
                        .charge(WorkKind::SparseScan, 1)?;
                    if owner == at {
                        owned = true;
                        break;
                    }
                }
            }
            if declared.is_some() || owned {
                self.session.state.runtime.budget.charge(
                    WorkKind::SparseScan,
                    worksheet.spill_children().count() as u64,
                )?;
                let last = worksheet.complete_spill_extent(at).ok_or_else(|| {
                    unsupported(
                        sheet,
                        at,
                        "caller-trusted saved spill is incomplete or lacks declared geometry",
                    )
                })?;
                let height = last
                    .row
                    .checked_sub(at.row)
                    .and_then(|n| n.checked_add(1))
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or(EvaluationFailure::StalePlan)?;
                let width = last
                    .col
                    .checked_sub(at.col)
                    .and_then(|n| n.checked_add(1))
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or(EvaluationFailure::StalePlan)?;
                self.session
                    .state
                    .runtime
                    .budget
                    .check_allocation(height.saturating_mul(width))?;
                let mut saved = Vec::new();
                saved
                    .try_reserve_exact(height)
                    .map_err(|_| EvaluationFailure::ResourceLimit {
                        capability: CapabilityId::FunctionAdapter,
                        source: Some(EvaluationTarget { sheet, address: at }),
                        detail: "saved spill row allocation failed".into(),
                    })?;
                for row in at.row..=last.row {
                    let mut values = Vec::new();
                    values.try_reserve_exact(width).map_err(|_| {
                        EvaluationFailure::ResourceLimit {
                            capability: CapabilityId::FunctionAdapter,
                            source: Some(EvaluationTarget { sheet, address: at }),
                            detail: "saved spill value allocation failed".into(),
                        }
                    })?;
                    for col in at.col..=last.col {
                        let address = CellAddress { row, col };
                        self.session
                            .state
                            .runtime
                            .budget
                            .charge(WorkKind::SparseScan, 1)?;
                        let source = worksheet.source_cell(address);
                        let effective = effective.cell(sheet, address);
                        let explicit_content = self.session.sources.inputs.is_some_and(|inputs| {
                            matches!(
                                inputs.edit(sheet, address),
                                Some(crate::eval::inputs::CellEdit::Replace(_))
                            )
                        });
                        if address != at
                            && (explicit_content
                                || !match (source, effective) {
                                    (None, None) => true,
                                    (Some(base), Some(current)) => {
                                        base.formula == current.formula
                                            && same_result_identity(&base.value, &current.value)
                                    }
                                    // A style-only override can materialize an absent blank source.
                                    (None, Some(current)) => {
                                        current.formula.is_none() && current.value == Blank
                                    }
                                    _ => false,
                                })
                        {
                            return Err(unsupported(
                                sheet,
                                at,
                                "caller-trusted saved spill has an edited child",
                            ));
                        }
                        let cached = worksheet.get(address).ok_or(EvaluationFailure::StalePlan)?;
                        self.session
                            .state
                            .runtime
                            .budget
                            .text(cached.value.text_storage_bytes())?;
                        // SAFETY: width slots were reserved before this exactly-width row loop.
                        values.push(cached.value.clone());
                    }
                    // SAFETY: height slots were reserved before this exactly-height row loop.
                    saved.push(values);
                }
                rows = Some(ArrayView::unknown(saved));
            }
        }
        let key = (sheet, at);
        let previous = self
            .session
            .state
            .runtime
            .stack
            .consumer
            .replace(EvaluationTarget { sheet, address: at });
        self.session.state.runtime.stack.visiting.insert(key);
        let staged = if let Some(saved) = rows {
            SpillPublisher::new(self.session).stage(sheet, at, saved)
        } else {
            Ok(ComputedCell::scalar(value))
        };
        self.session.state.runtime.stack.visiting.remove(&key);
        self.session.state.runtime.stack.consumer = previous;
        let result = staged?;
        self.session.state.runtime.preparation.held.insert(
            key,
            Held {
                result,
                source_cached: true,
                retained: false,
            },
        );
        Ok(())
    }
}
