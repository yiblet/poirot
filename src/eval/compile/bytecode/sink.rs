use super::high::High;

/// Bytecode lowering destination shared by sizing and emission passes.
pub(super) trait Sink<'ast> {
    fn emit(&mut self, op: High<'ast>);
}
