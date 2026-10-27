use poirot_parsing::formula::BinaryOp;

/// Stack operations and lazy argument descriptors understood by the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Opcode {
    /// Admission in AST preorder. `a` is the original expression depth.
    Enter,
    /// Leave the current expression admission frame.
    Leave,
    /// Push f64 bits from low `a` and high `b` words.
    Number,
    /// Push UTF-8 text at operand byte offset `a`, length `b`.
    Text,
    /// Push boolean `a != 0`.
    Boolean,
    /// Push error spelling at operand byte offset `a`, length `b`.
    Error,
    /// Push an omitted-argument marker.
    Missing,
    /// Push a reference value from per-instance slot `a`, without dereferencing.
    Reference,
    /// Resolve name spelling at operand byte offset `a`, length `b`.
    Name,
    /// `a,b` identify operand bytes: length-prefixed sheet, then remaining name.
    QualifiedName,
    /// Consume one value. `a`: plus=0, minus=1, percent=2, implicit intersection=3.
    Unary,
    /// Consume left/right values, preserving reference operators. `a` follows
    /// Add,Subtract,Multiply,Divide,Power,Concat,Equal,NotEqual,Less,LessEqual,
    /// Greater,GreaterEqual,Range,Union,Intersection in order from zero.
    Binary,
    /// Consume `a` rows * `b` columns of values in row-major order.
    Array,
    /// Admit `a` array elements before evaluation; `b,c` are rows and columns.
    ArrayAllocate,
    /// Scalarize the preceding array element before evaluating its successor.
    Scalarize,
    /// `a`: call binding index; `b`: arity; `c`: continuation label.
    /// Arguments start at ip+2; following `Argument.c` walks the descriptors.
    Invoke,
    /// Inline argument descriptor: `a`: argument ordinal; `c`: next descriptor/end label.
    Argument,
    /// Return the argument's reference/value to the requesting family adapter.
    YieldArgument,
    /// Continue at label `c`, resolved through the enclosing template's label table.
    Jump,
    /// Add scalar or broadcast array operands using their runtime types.
    Add,
    /// Subtract scalar or broadcast array operands using their runtime types.
    Subtract,
    /// Multiply scalar or broadcast array operands using their runtime types.
    Multiply,
    /// Divide scalar or broadcast array operands using their runtime types.
    Divide,
    /// Exponentiate scalar or broadcast array operands using their runtime types.
    Power,
    /// Concatenate scalar or broadcast array operands using their runtime types.
    Concat,
    /// Compare scalar or broadcast array operands for equality.
    Equal,
    /// Compare scalar or broadcast array operands for inequality.
    NotEqual,
    /// Compare scalar or broadcast array operands with less-than ordering.
    Less,
    /// Compare scalar or broadcast array operands with less-or-equal ordering.
    LessEqual,
    /// Compare scalar or broadcast array operands with greater-than ordering.
    Greater,
    /// Compare scalar or broadcast array operands with greater-or-equal ordering.
    GreaterEqual,
    /// Conditional over deferred branch labels, with the same operand layout as `Invoke`.
    Conditional,
}

impl Opcode {
    /// Whether the instruction owns deferred argument blocks and a continuation label.
    pub(in crate::eval) fn is_call(self) -> bool {
        matches!(self, Self::Invoke | Self::Conditional)
    }

    /// Scalar operation lifted by this stable, shape-polymorphic instruction.
    pub(in crate::eval) fn binary_operation(self) -> Option<BinaryOp> {
        Some(match self {
            Self::Add => BinaryOp::Add,
            Self::Subtract => BinaryOp::Subtract,
            Self::Multiply => BinaryOp::Multiply,
            Self::Divide => BinaryOp::Divide,
            Self::Power => BinaryOp::Power,
            Self::Concat => BinaryOp::Concat,
            Self::Equal => BinaryOp::Equal,
            Self::NotEqual => BinaryOp::NotEqual,
            Self::Less => BinaryOp::Less,
            Self::LessEqual => BinaryOp::LessEqual,
            Self::Greater => BinaryOp::Greater,
            Self::GreaterEqual => BinaryOp::GreaterEqual,
            _ => return None,
        })
    }

    /// Select a primitive from syntax independently of inferred or observed types.
    pub(super) fn for_binary(operation: BinaryOp) -> Self {
        match operation {
            BinaryOp::Add => Self::Add,
            BinaryOp::Subtract => Self::Subtract,
            BinaryOp::Multiply => Self::Multiply,
            BinaryOp::Divide => Self::Divide,
            BinaryOp::Power => Self::Power,
            BinaryOp::Concat => Self::Concat,
            BinaryOp::Equal => Self::Equal,
            BinaryOp::NotEqual => Self::NotEqual,
            BinaryOp::Less => Self::Less,
            BinaryOp::LessEqual => Self::LessEqual,
            BinaryOp::Greater => Self::Greater,
            BinaryOp::GreaterEqual => Self::GreaterEqual,
            BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection => Self::Binary,
        }
    }
}
