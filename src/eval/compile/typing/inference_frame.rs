use poirot_parsing::formula::Expr;

/// One suspended expression in the shared depth-first preparation traversal.
pub(super) struct InferenceFrame<'a> {
    pub(super) expression: &'a Expr,
    pub(super) depth: usize,
    pub(super) slot: usize,
    pub(super) base: usize,
    pub(super) binding: Option<usize>,
}
