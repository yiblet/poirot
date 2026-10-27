//! Unit tests for bytecode execution, call admission, branch selection and rectangular traversal.

/// Capacity overflow of branch selection as an attributed resource failure.
mod branch_selector;
/// Bytecode VM parity with the tree oracle, stack reuse and unwinding.
mod bytecode_vm;
/// Observer refusals and policy reads during builtin call admission.
mod call_admission;
/// Capacity overflow of rectangular traversal as an attributed resource failure.
mod rectangular_traversal;
