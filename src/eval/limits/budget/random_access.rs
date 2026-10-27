/// One cell's versioned random-stream consumption: seed, zero-based coordinates,
/// logical draws, and generator words consumed (including integer rejection samples).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomAccess {
    /// Captured evaluation seed.
    pub seed: u64,
    /// Zero-based workbook sheet index.
    pub sheet: u64,
    /// Zero-based formula row.
    pub row: u32,
    /// Zero-based formula column.
    pub column: u32,
    /// Stream algorithm and seed encoding version.
    pub version: u32,
    /// Number of logical random requests.
    pub draws: u64,
    /// Generator words consumed, including rejection samples.
    pub words: u64,
}
