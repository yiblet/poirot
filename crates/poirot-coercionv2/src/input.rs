use crate::{CellValue, Origin};

/// Borrowed scalar plus source provenance. `None` represents omitted syntax,
/// never an empty cell. References/arrays must be selected by the host first.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
    value: Option<&'a CellValue>,
    origin: Origin,
}

impl<'a> Input<'a> {
    pub fn direct(value: &'a CellValue) -> Self {
        Self::new(value, Origin::Direct)
    }

    pub fn referenced(value: &'a CellValue) -> Self {
        Self::new(value, Origin::Reference)
    }

    pub fn array_member(value: &'a CellValue) -> Self {
        Self::new(value, Origin::Array)
    }

    pub fn new(value: &'a CellValue, origin: Origin) -> Self {
        Self {
            value: Some(value),
            origin,
        }
    }

    pub fn missing() -> Self {
        Self {
            value: None,
            origin: Origin::Direct,
        }
    }

    pub fn value(self) -> Option<&'a CellValue> {
        self.value
    }

    pub fn origin(self) -> Origin {
        self.origin
    }
}
