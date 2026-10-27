//! Budgeted draws from the current cell's reproducible random stream.
use super::call_host::CallHost;
use crate::eval::limits::{CapabilityId, EvaluationError};
use crate::eval::ports::DependencyRead;
use crate::eval::session::ReadRecorder;
use crate::eval::values::FunctionError;

/// Draws from the calling cell's random stream, charging its budget and recording the access.
pub(in crate::eval) struct RandomDraws<'h, 'call, 'workbook> {
    /// The call whose cell owns the stream and whose budget admits each draw.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> RandomDraws<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's draws.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Consume and record a budgeted draw from the current cell's reproducible stream.
    pub(in crate::eval) fn draw(
        &mut self,
        interval: Option<crate::eval::config::random::Interval>,
    ) -> Result<f64, FunctionError> {
        let work = if interval.is_some() {
            crate::eval::config::random::INTEGER_WORK
        } else {
            1
        };
        self.host.check_values(1)?;
        if !self
            .host
            .session
            .state
            .runtime
            .effects
            .random_streams
            .contains_key(&(self.host.sheet, self.host.address))
        {
            self.host.session.state.runtime.budget.check_allocation(
                self.host
                    .session
                    .state
                    .runtime
                    .effects
                    .random_streams
                    .len()
                    .saturating_add(1),
            )?;
            if self.host.session.state.runtime.budget.profiling {
                self.host.session.state.runtime.budget.check_allocation(
                    self.host
                        .session
                        .state
                        .runtime
                        .budget
                        .usage
                        .random_accesses
                        .len()
                        .saturating_add(1),
                )?;
            }
            // Bounds describe both entire collections; work charges only the two
            // new records. Previous slots are neither copied nor rematerialized.
            let records = 1 + u64::from(self.host.session.state.runtime.budget.profiling);
            self.host
                .session
                .state
                .runtime
                .budget
                .charge(crate::eval::limits::WorkKind::Materialization, records)?;
        }
        self.host.charge_work(work + 1)?;
        let seed = self.host.session.policies.context.randomness_seed;
        // Each stream owns a stable trace slot: distinct cells never rescan
        // earlier stream records. Both collections were bounded before insertion.
        let runtime = &mut *self.host.session.state.runtime;
        let profiling = runtime.budget.profiling;
        let accesses = &mut runtime.budget.usage.random_accesses;
        let (stream, usage_slot) = runtime
            .effects
            .random_streams
            .entry((self.host.sheet, self.host.address))
            .or_insert_with(|| {
                let slot = accesses.len();
                if profiling {
                    accesses.push(crate::eval::limits::RandomAccess {
                        seed,
                        sheet: self.host.sheet as u64,
                        row: self.host.address.row,
                        column: self.host.address.col,
                        version: crate::eval::config::random::STREAM_VERSION,
                        draws: 0,
                        words: 0,
                    });
                }
                (
                    crate::eval::config::random::Stream::new(
                        seed,
                        self.host.sheet as u64,
                        self.host.address.row,
                        self.host.address.col,
                    ),
                    slot,
                )
            });
        let (value, words) = match interval {
            None => (Some(stream.unit()), 1),
            Some(interval) => match stream.integer(interval) {
                Some((value, words)) => (Some(value), words),
                None => (None, crate::eval::config::random::INTEGER_WORK),
            },
        };
        if profiling {
            let access = accesses
                .get_mut(*usage_slot)
                .ok_or(EvaluationError::StalePlan)?;
            access.draws = access.draws.saturating_add(1);
            access.words = access.words.saturating_add(words);
        }
        ReadRecorder::new(self.host.session).read(DependencyRead::Random(
            crate::eval::limits::RandomAccess {
                seed,
                sheet: self.host.sheet as u64,
                row: self.host.address.row,
                column: self.host.address.col,
                version: crate::eval::config::random::STREAM_VERSION,
                draws: 1,
                words,
            },
        ));
        value.ok_or_else(|| {
            FunctionError::Operational(self.host.capability_failure(
                CapabilityId::FunctionAdapter,
                crate::eval::limits::EvaluationReasonError::BoundedIntegerEntropyRejectionExhausted,
            ))
        })
    }
}
