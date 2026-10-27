//! Consumed-read attribution and forwarding to dependency sinks.

use crate::eval::config::ArrayResultPolicy;
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::session::composition::Session;
use crate::eval::spill::{SpillPublisher, SpillQuery};
use crate::model::CellAddress;

/// Attributes consumed reads to the active cell and forwards them to installed sinks.
pub(in crate::eval) struct ReadRecorder<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ReadRecorder<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Construct dependency-only payloads only when a tracker or external sink needs them.
    pub(in crate::eval) fn read_lazy(&mut self, read: impl FnOnce() -> DependencyRead) {
        if self.session.reuse.overlay_reads.is_some()
            || self.session.ports.dependency_sink.is_some()
            || SpillQuery::new(self.session).settling()
        {
            self.read(read());
        }
    }

    /// Record spill geometry without claiming every coordinate's value was consumed.
    pub(in crate::eval) fn read_spill_membership(
        &mut self,
        sheet: usize,
        first: CellAddress,
        last: CellAddress,
    ) {
        if self.session.reuse.overlay_reads.is_none()
            && self.session.ports.dependency_sink.is_none()
            && !SpillQuery::new(self.session).settling()
        {
            return;
        }
        let read = DependencyRead::SparseMembership { sheet, first, last };
        SpillPublisher::new(self.session).record_effect(&read);
        if let Some(reads) = &mut self.session.reuse.overlay_reads {
            reads.spill_membership(
                self.session.state.runtime.stack.consumer,
                crate::model::query::RangeMembership { sheet, first, last },
            );
        }
        if let Some(sink) = &mut self.session.ports.dependency_sink {
            sink.read(ReadEvent {
                consumer: self.session.state.runtime.stack.consumer,
                read,
            });
        }
    }

    /// Attribute a consumed read to the active cell and forward it to installed sinks.
    pub(in crate::eval) fn read(&mut self, read: DependencyRead) {
        SpillPublisher::new(self.session).record_effect(&read);
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && let DependencyRead::SparseMembership { sheet, last, .. } = &read
            && let Some(reads) = &mut self.session.reuse.overlay_reads
        {
            // Sparse consumers may observe an empty interval without reading
            // individual blank coordinates. Retain their ownership-query guard
            // even when an indexed lookup supplied the values from another caller.
            reads.spill_membership(
                self.session.state.runtime.stack.consumer,
                crate::model::query::RangeMembership {
                    sheet: *sheet,
                    first: CellAddress { row: 0, col: 0 },
                    last: *last,
                },
            );
        }
        record_read(
            &mut self.session.reuse.overlay_reads,
            &mut self.session.ports.dependency_sink,
            ReadEvent {
                consumer: self.session.state.runtime.stack.consumer,
                read,
            },
        );
    }
}

/// Forward a read while allowing the memo and sink fields to be borrowed independently.
fn record_read(
    reads: &mut Option<crate::eval::reuse::Reads>,
    sink: &mut Option<&mut dyn DependencyReadSink>,
    event: ReadEvent,
) {
    if let Some(reads) = reads {
        reads.record(&event);
    }
    if let Some(sink) = sink {
        sink.read(event);
    }
}
