use crate::eval::session::ExpressionDemand;

/// Argument roles are fixed by the consumer before inspecting or projecting a value.
#[derive(Clone, Copy)]
pub(super) enum ArgumentDemand {
    Scalar,
    Inherited(ExpressionDemand),
    ArrayPrefix(usize),
    ArraySlots(&'static [usize]),
}

impl ArgumentDemand {
    pub(super) fn at(self, index: usize) -> ExpressionDemand {
        match self {
            Self::Inherited(demand) => demand,
            Self::ArrayPrefix(count) if index < count => ExpressionDemand::Array,
            Self::ArraySlots(slots) if slots.contains(&index) => ExpressionDemand::Array,
            _ => ExpressionDemand::Scalar,
        }
    }
}
