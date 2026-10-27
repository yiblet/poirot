/// Cell-versus-area provenance retained for reference coercion and intersection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum AreaKind {
    Cell,
    Area,
}
