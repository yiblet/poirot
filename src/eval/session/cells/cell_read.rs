//! A finished cell attempt and its completion delivery.

use super::bookkeeping::record_overlay_completion;
use super::cell_attempt::CellAttempt;
use super::completion_context::CompletionContext;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvalResult, EvaluationError};
use crate::eval::ports::{CellCompletion, CellProvenance};
use crate::model::CellValue;
use crate::model::CellValue::Blank;

/// A finished attempt and its value or operational failure, ready for completion delivery.
pub(super) struct CellRead {
    pub(super) attempt: CellAttempt,
    pub(super) result: EvalResult<CellValue>,
}

impl CellRead {
    /// Record completion eligibility and notify observers without masking the original failure.
    pub(super) fn complete(self, context: CompletionContext<'_, '_>) -> EvalResult<CellValue> {
        let CellRead {
            attempt:
                CellAttempt {
                    target: EvaluationTarget { sheet, address },
                    consumer,
                    provenance,
                    completion_candidate,
                },
            mut result,
        } = self;
        if context.settling_spills
            && context.array_result_policy == ArrayResultPolicy::DynamicSpill
            && matches!(result, Ok(Blank))
            && let Some(consumer) = consumer
        {
            // Private ownership arbitration tracks blank observations, including
            // memo hits: a producer can later grow into an ancestor's footprint.
            let tracked = (|| {
                // A dependency's blank can participate in an ancestor's future
                // footprint as well as its immediate caller's footprint.
                for owner in context
                    .visiting
                    .iter()
                    .copied()
                    .filter(|owner| *owner != (consumer.sheet, consumer.address))
                    .chain(std::iter::once((consumer.sheet, consumer.address)))
                {
                    context.preparation.producer_reads.record_blank(
                        owner,
                        (sheet, address),
                        context.budget,
                    )?;
                }
                Ok::<_, EvaluationError>(())
            })();
            if let Err(failure) = tracked {
                result = Err(failure);
            }
        }
        if let Some(reads) = context.reads
            && result.is_ok()
            && provenance != CellProvenance::SessionMemo
            && (provenance == CellProvenance::ExternalCache
                || completion_candidate
                || (context.array_result_policy == ArrayResultPolicy::DynamicSpill
                    && context.preparation.has_admitted_role((sheet, address))))
            && let Err(failure) =
                record_overlay_completion(reads, EvaluationTarget { sheet, address }, provenance)
        {
            result = Err(failure);
        }
        {
            let notification = context.preparation.complete_cell(
                context.budget,
                context.consumer,
                context.observer,
                CellCompletion {
                    consumer,
                    target: EvaluationTarget { sheet, address },
                    provenance,
                    result: result.as_ref(),
                },
            );
            // Notification remains required on abort, but cannot mask its original cause.
            if result.is_ok() {
                notification?;
            }
        }
        result
    }
}
