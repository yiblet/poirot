//! Caller-implemented ports of one evaluation and the events delivered through them:
//! completed cells, consumed dependency reads, and validated results from an external
//! cache. Retention of delivered events belongs to the caller.
//!
//! Depends on: functions (builtin and extension identities recorded as reads),
//! coordinates (targets) and limits (failures and random access records).

/// Validated results supplied from a cache shared across runs.
mod cached_cell_reader;

/// A borrowed observation of one completed cell call.
mod cell_completion;

/// Caller-owned observer of completed cell calls.
mod cell_observer;

/// Origin of a completed cell value within the current run.
mod cell_provenance;

/// Individual context fields consumed by executed operations.
mod context_read;

/// A dependency or setting observed during execution.
mod dependency_read;

/// Caller-owned consumption port for observed reads.
mod dependency_read_sink;

/// An observed read and its consuming cell.
mod read_event;

/// Bitwise result identity used by incremental consumers.
mod result_identity;

pub use cached_cell_reader::CachedCellReader;
pub use cell_completion::CellCompletion;
pub use cell_observer::CellObserver;
pub use cell_provenance::CellProvenance;
pub use context_read::ContextRead;
pub use dependency_read::DependencyRead;
pub use dependency_read_sink::DependencyReadSink;
pub use read_event::ReadEvent;
pub use result_identity::same_result_identity;
