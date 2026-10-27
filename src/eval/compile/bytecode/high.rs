use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};

// Borrowed high-level IR. N-ary operations remain intact until the stack sink.
// Unresolved source labels are scoped by syntax depth. A fixed table maps them
// to unique body-local IDs in the final dense label vector.
/// Temporary high-level instruction stream lowered into fixed-width executable bytecode.
pub(super) enum High<'a> {
    Enter(usize, &'a Expr),
    Leave,
    Atom(&'a Expr),
    Truncated,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Array(usize, usize),
    ArrayAllocate(usize, usize),
    Scalarize,
    Invoke(&'a str, usize, usize),
    Argument(usize, usize, usize, &'a Expr),
    YieldArgument,
    Goto(usize),
    Label(usize),
}
