use crate::eval::host::{CallHost, OperandReader, ScalarOperand};
use crate::eval::values::{Area, FunctionResult};

/// A prepared collection used for repeated coordinate reads within one call.
pub(in crate::eval::adapters) struct CoordinateCollection {
    argument: usize,
    area: Option<Area>,
}

impl CoordinateCollection {
    /// Capture a prepared argument's physical area when one can be read directly.
    pub(super) fn bind(host: &mut CallHost<'_, '_>, argument: usize) -> FunctionResult<Self> {
        let area = OperandReader::new(host).physical_area(argument)?;
        Ok(Self { argument, area })
    }

    /// Return the argument that supplies the positions and values.
    pub(super) fn argument(&self) -> usize {
        self.argument
    }

    /// Read a collection member with its original value channel and provenance.
    pub(in crate::eval::adapters) fn read_at(
        &self,
        host: &mut CallHost<'_, '_>,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        let mut reader = OperandReader::new(host);
        match self.area {
            Some(area) => reader.read_area_cell(area, row, column),
            None => reader.read_cell(self.argument, row, column),
        }
    }
}
