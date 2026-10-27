/// Index into a compiled body's label table, distinct from an instruction offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Label(pub u32);
