/// Optional snapshot loading quotas checked before admission; integer maxima mean no quota.
/// The default has no size quotas and retains a 256-level recursive decoder safety ceiling.
/// Save builders independently use [`Self::for_writing`] to preserve export defaults.
///
/// ```
/// use poirot::eval::artifact::{SnapshotSaveBuilder, SnapshotLoadBuilder, SnapshotLimits, SnapshotError};
/// use poirot::model::Workbook;
/// let book = Workbook::new();
/// let bytes = SnapshotSaveBuilder::workbook(&book).to_buffer()?;
/// let limits = SnapshotLimits { max_file_bytes: 0, ..Default::default() };
/// assert!(matches!(SnapshotLoadBuilder::default().limits(limits).from_bytes(&bytes),
///     Err(SnapshotError::Quota(_))));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SnapshotLimits {
    /// Maximum complete file bytes, including envelope and optional prepared section.
    pub max_file_bytes: usize,
    /// Maximum number of typed Serde values across both sections, including scalar fields.
    pub max_decoded_values: usize,
    /// Maximum aggregate string/byte payload copied by decoding, across both sections.
    pub max_decoded_bytes: usize,
    /// Maximum nested typed values, including enums, structs and collection elements.
    pub max_depth: usize,
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: usize::MAX,
            max_decoded_values: usize::MAX,
            max_decoded_bytes: usize::MAX,
            max_depth: 256,
        }
    }
}

impl SnapshotLimits {
    /// Historical snapshot output quotas, retained independently of unlimited loading defaults.
    pub fn for_writing() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            max_decoded_values: 8_000_000,
            max_decoded_bytes: 64 * 1024 * 1024,
            max_depth: 256,
        }
    }
}

impl SnapshotLimits {
    /// Set the maximum file bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_file_bytes(mut self, maximum: usize) -> Self {
        self.max_file_bytes = maximum;
        self
    }

    /// Remove the file bytes quota. Structural and representation checks remain.
    pub fn without_file_bytes_limit(mut self) -> Self {
        self.max_file_bytes = usize::MAX;
        self
    }

    /// Inspect the file bytes quota; `None` means no configured maximum.
    pub fn file_bytes_limit(&self) -> Option<usize> {
        (self.max_file_bytes != usize::MAX).then_some(self.max_file_bytes)
    }

    /// Set the maximum decoded values before decoding; zero admits none. Replaces only this setting.
    pub fn max_decoded_values(mut self, maximum: usize) -> Self {
        self.max_decoded_values = maximum;
        self
    }

    /// Remove the decoded values quota. Structural and representation checks remain.
    pub fn without_decoded_values_limit(mut self) -> Self {
        self.max_decoded_values = usize::MAX;
        self
    }

    /// Inspect the decoded values quota; `None` means no configured maximum.
    pub fn decoded_values_limit(&self) -> Option<usize> {
        (self.max_decoded_values != usize::MAX).then_some(self.max_decoded_values)
    }

    /// Set the maximum decoded bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_decoded_bytes(mut self, maximum: usize) -> Self {
        self.max_decoded_bytes = maximum;
        self
    }

    /// Remove the decoded bytes quota. Structural and representation checks remain.
    pub fn without_decoded_bytes_limit(mut self) -> Self {
        self.max_decoded_bytes = usize::MAX;
        self
    }

    /// Inspect the decoded bytes quota; `None` means no configured maximum.
    pub fn decoded_bytes_limit(&self) -> Option<usize> {
        (self.max_decoded_bytes != usize::MAX).then_some(self.max_decoded_bytes)
    }
}
