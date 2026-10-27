use super::opcode::Opcode;

/// Fixed-width stack instruction. Control-flow targets are label-table indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Instruction {
    /// Operation selecting the interpretation of operand words.
    pub opcode: Opcode,
    /// Operation flags, currently emitted as zero.
    pub flags: u8,
    /// Reserved storage, currently emitted as zero.
    pub reserved: u16,
    /// First operand word; interpretation depends on the opcode.
    pub a: u32,
    /// Second operand word; interpretation depends on the opcode.
    pub b: u32,
    /// Third operand word, including label IDs for control-flow instructions.
    pub c: u32,
}

impl Instruction {
    pub(super) fn new(opcode: Opcode, a: u32, b: u32) -> Self {
        Self {
            opcode,
            flags: 0,
            reserved: 0,
            a,
            b,
            c: 0,
        }
    }
}
