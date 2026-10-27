//! Reads performed for the active cell: dependency recording, function arguments,
//! sparse range members, eager operand folding and sheet or defined-name resolution.
//!
//! Depends on: the parent session state and its cells and projection executors, vm
//! (deferred argument expressions), spill (discovery guards), ports (read events),
//! inputs (effective workbook), reuse (read recording), config (consumed context
//! fields), host (operands), limits and values.

/// Function argument evaluation and matrix materialization.
mod argument_reader;
/// Eager operand member folding.
mod item_folder;
/// Sheet qualifier and defined-name resolution.
mod name_resolver;
/// Consumed-read attribution and forwarding.
mod read_recorder;
/// A sparse range member and its inspected source.
mod sparse_member;
/// Sparse range membership collection.
mod sparse_reader;
#[cfg(test)]
mod tests;

pub(in crate::eval) use argument_reader::ArgumentReader;
pub(in crate::eval) use item_folder::ItemFolder;
pub(in crate::eval) use name_resolver::NameResolver;
pub(in crate::eval) use read_recorder::ReadRecorder;
pub(in crate::eval) use sparse_member::SparseMember;
pub(in crate::eval) use sparse_reader::SparseReader;
