use super::dependencies::Dependencies;
use super::key::Key;
use crate::eval::coordinates::{CellMap, CellSet, EvaluationTarget};
use crate::eval::ports::{DependencyRead, ReadEvent};
use crate::model::query::RangeMembership;

/// Accumulates consumed dependencies and completed cells for incremental cache publication.
/// It keeps a separate trace per formula as evaluation enters and returns from
/// nested dependencies.
#[derive(Default)]
pub(in crate::eval) struct Reads {
    /// Traces suspended when execution switches to another consumer.
    pub traces: CellMap<Dependencies>,
    current: Option<(Key, Dependencies)>,
    /// Newly computed or source-restored results awaiting incremental admission.
    pub fresh_results: CellSet,
    /// Cells whose results may be retained after successful execution.
    pub completed: CellSet,
    /// Results whose old consumed trace was validated without executing their formula.
    pub validated: CellSet,
}

impl Reads {
    // Range consumers issue many reads without changing the consumer. Keep its
    // trace directly accessible rather than hashing its coordinates per value.
    fn trace(&mut self, key: Key) -> &mut Dependencies {
        if self
            .current
            .as_ref()
            .is_none_or(|(current, _)| *current != key)
        {
            self.flush();
            self.current = Some((key, self.traces.remove(&key).unwrap_or_default()));
        }
        &mut self
            .current
            .get_or_insert_with(|| (key, Dependencies::default()))
            .1
    }

    /// Keep ownership guards separate from consumed values while recording the same public event.
    pub fn spill_membership(&mut self, consumer: Option<EvaluationTarget>, range: RangeMembership) {
        if let Some(consumer) = consumer {
            let ranges = &mut self.trace((consumer.sheet, consumer.address)).spill_ranges;
            if !ranges.contains(&range) {
                ranges.push(range);
            }
        }
    }

    /// Mark geometry-sensitive reads that scalar equality cannot validate.
    pub fn mark_metadata(&mut self, consumer: Option<EvaluationTarget>) {
        if let Some(consumer) = consumer {
            self.trace((consumer.sheet, consumer.address))
                .metadata_reads = true;
        }
    }

    /// Mark a producer whose future footprint cannot be certified by scalar equality.
    pub fn mark_spill_sensitive(&mut self, key: Key) {
        self.trace(key).spill_sensitive = true;
    }

    /// Remove speculative consumers while preserving previously settled evidence.
    pub(in crate::eval) fn discard_consumers(&mut self, speculative: &CellSet) {
        self.flush();
        for key in speculative {
            self.traces.remove(key);
            self.completed.remove(key);
            self.fresh_results.remove(key);
            self.validated.remove(key);
        }
    }

    /// Return the active trace to the map before publishing the batch.
    pub fn flush(&mut self) {
        if let Some((key, trace)) = self.current.take() {
            self.traces.insert(key, trace);
        }
    }

    /// Observe a read without copying payloads retained by an external sink.
    pub(in crate::eval) fn record(&mut self, event: &ReadEvent) {
        let Some(consumer) = event.consumer else {
            return;
        };
        let trace = self.trace((consumer.sheet, consumer.address));
        match &event.read {
            DependencyRead::Cell(cell) => {
                trace.value_read((cell.sheet, cell.address));
                trace.cell((cell.sheet, cell.address));
            }
            DependencyRead::SubtotalMetadata(cell) => {
                trace.metadata_reads = true;
                trace.cell((cell.sheet, cell.address));
            }
            DependencyRead::SparseMembership { sheet, first, last } => {
                let (sheet, first, last) = (*sheet, *first, *last);
                if first == last {
                    // A point edge invalidates both present and absent coordinates,
                    // including generated publication, exactly like this guard.
                    trace.cell((sheet, first));
                    return;
                }
                let range = RangeMembership { sheet, first, last };
                if let Some(previous) = trace.ranges.last_mut() {
                    if previous.sheet == sheet
                        && previous.first.col == first.col
                        && previous.last.col == last.col
                        && first.row <= previous.last.row.saturating_add(1)
                        && last.row.saturating_add(1) >= previous.first.row
                    {
                        previous.first.row = previous.first.row.min(first.row);
                        previous.last.row = previous.last.row.max(last.row);
                        return;
                    }
                    if previous.sheet == sheet
                        && previous.first.row == first.row
                        && previous.last.row == last.row
                        && first.col <= previous.last.col.saturating_add(1)
                        && last.col.saturating_add(1) >= previous.first.col
                    {
                        previous.first.col = previous.first.col.min(first.col);
                        previous.last.col = previous.last.col.max(last.col);
                        return;
                    }
                }
                trace.ranges.push(range);
            }
            DependencyRead::OpaqueProvider(_) => trace.opaque = true,
            DependencyRead::Extension(invocation) => {
                use crate::eval::functions::extensions::ExtensionCacheability;
                match invocation.binding.registration.contract.cacheability {
                    ExtensionCacheability::Deterministic
                    | ExtensionCacheability::Versioned { .. } => {}
                    ExtensionCacheability::Volatile => trace.volatile = true,
                    ExtensionCacheability::Opaque => trace.opaque = true,
                }
                // The host exposes the entire captured context as one value.
                // A context-capable provider may consume any of its fields.
                if invocation
                    .binding
                    .registration
                    .contract
                    .capabilities
                    .context
                {
                    trace.context.as_of = true;
                    trace.context.seed = true;
                    trace.context.utc_offset = true;
                    trace.context.date_order = true;
                }
            }
            DependencyRead::Volatile(_) => trace.volatile = true,
            DependencyRead::Random(_) => trace.context.seed = true,
            DependencyRead::Context(policy) => {
                use crate::eval::functions::registry::DependencyPolicy;
                trace.context.as_of |= policy.contains(DependencyPolicy::TIME);
                trace.context.date_system |= policy.contains(DependencyPolicy::DATE_SYSTEM);
                trace.context.utc_offset |= policy.contains(DependencyPolicy::LOCALIZATION);
                trace.context.date_order |= policy.contains(DependencyPolicy::LOCALIZATION);
            }
            DependencyRead::ContextField(field) => match field {
                crate::eval::ports::ContextRead::AsOf => trace.context.as_of = true,
                crate::eval::ports::ContextRead::UtcOffset => trace.context.utc_offset = true,
                crate::eval::ports::ContextRead::DateOrder => trace.context.date_order = true,
                crate::eval::ports::ContextRead::DateSystem => trace.context.date_system = true,
            },
            DependencyRead::Formatting => trace.formatting = true,
            DependencyRead::NumberLocale => trace.number_locale = true,
            DependencyRead::TextPolicy => trace.text_policy = true,
            _ => {}
        }
    }
}

impl crate::eval::ports::DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.record(&event);
    }
}
