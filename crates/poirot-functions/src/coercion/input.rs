use crate::coercion::{CellValue, Origin};

/// Borrowed scalar plus source provenance. `None` represents omitted syntax,
/// never an empty cell. References/arrays must be selected by the host first.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
    value: Option<&'a CellValue>,
    origin: Origin,
}

impl<'a> Input<'a> {
    /// Borrow a literal or computed scalar supplied directly to the call.
    pub fn direct(value: &'a CellValue) -> Self {
        Self::new(value, Origin::Direct)
    }

    /// Borrow a scalar already selected from a worksheet reference.
    pub fn referenced(value: &'a CellValue) -> Self {
        Self::new(value, Origin::Reference)
    }

    /// Borrow a scalar already selected from a materialized array.
    pub fn array_member(value: &'a CellValue) -> Self {
        Self::new(value, Origin::Array)
    }

    /// Borrow a scalar with explicit provenance; this does not resolve references.
    pub fn new(value: &'a CellValue, origin: Origin) -> Self {
        Self {
            value: Some(value),
            origin,
        }
    }

    /// Represent an omitted argument slot, distinct from a present blank cell.
    pub fn missing() -> Self {
        Self {
            value: None,
            origin: Origin::Direct,
        }
    }

    /// Return the borrowed scalar, or `None` only for omitted syntax.
    pub fn value(self) -> Option<&'a CellValue> {
        self.value
    }

    /// Return the source category used by origin-sensitive conversion rules.
    /// Omitted inputs carry [`Origin::Direct`].
    pub fn origin(self) -> Origin {
        self.origin
    }
}
