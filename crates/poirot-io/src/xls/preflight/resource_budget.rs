//! Caller-configured XLS admission quotas.

/// Limits on BIFF/CFB inspection and backend allocation claims before XLS decoding.
/// These complement [`crate::ReadOptions`] input limits; they are not Excel validity limits.
/// Defaults impose no quotas. Integer maxima preserve the legacy unlimited sentinel.
/// Each load starts fresh; checks precede the corresponding backend claim/decoding.
#[derive(Debug, Clone, Copy)]
pub struct ResourceBudget {
    /// Maximum admitted workbook stream byte size.
    pub max_stream_bytes: usize,
    /// Maximum source metadata entries inspected before backend decoding.
    pub max_metadata_entries: usize,
    /// Maximum BIFF record count.
    pub max_records: usize,
    /// Maximum shared-string entries.
    pub max_strings: usize,
    /// Maximum character claim for one shared string.
    pub max_string_chars: usize,
    /// Maximum combined shared-string storage claim.
    pub max_string_bytes: usize,
    /// Maximum continuation records admitted for string reconstruction.
    pub max_continuations: usize,
    /// Maximum decoded cell-text storage claim.
    pub max_cell_text_bytes: u64,
    /// Maximum decoded cells (values, formulas and text) across all sheet traversals.
    pub max_cells: usize,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_stream_bytes: usize::MAX,
            max_metadata_entries: usize::MAX,
            max_records: usize::MAX,
            max_strings: usize::MAX,
            max_string_chars: usize::MAX,
            max_string_bytes: usize::MAX,
            max_continuations: usize::MAX,
            max_cell_text_bytes: u64::MAX,
            max_cells: usize::MAX,
        }
    }
}

impl ResourceBudget {
    /// Set the maximum stream bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_stream_bytes(mut self, maximum: usize) -> Self {
        self.max_stream_bytes = maximum;
        self
    }

    /// Remove the stream bytes quota. Structural and representation checks remain.
    pub fn without_stream_bytes_limit(mut self) -> Self {
        self.max_stream_bytes = usize::MAX;
        self
    }

    /// Inspect the stream bytes quota; `None` means no configured maximum.
    pub fn stream_bytes_limit(&self) -> Option<usize> {
        (self.max_stream_bytes != usize::MAX).then_some(self.max_stream_bytes)
    }

    /// Set the maximum metadata entries before decoding; zero admits none. Replaces only this setting.
    pub fn max_metadata_entries(mut self, maximum: usize) -> Self {
        self.max_metadata_entries = maximum;
        self
    }

    /// Remove the metadata entries quota. Structural and representation checks remain.
    pub fn without_metadata_entries_limit(mut self) -> Self {
        self.max_metadata_entries = usize::MAX;
        self
    }

    /// Inspect the metadata entries quota; `None` means no configured maximum.
    pub fn metadata_entries_limit(&self) -> Option<usize> {
        (self.max_metadata_entries != usize::MAX).then_some(self.max_metadata_entries)
    }

    /// Set the maximum records before decoding; zero admits none. Replaces only this setting.
    pub fn max_records(mut self, maximum: usize) -> Self {
        self.max_records = maximum;
        self
    }

    /// Remove the records quota. Structural and representation checks remain.
    pub fn without_records_limit(mut self) -> Self {
        self.max_records = usize::MAX;
        self
    }

    /// Inspect the records quota; `None` means no configured maximum.
    pub fn records_limit(&self) -> Option<usize> {
        (self.max_records != usize::MAX).then_some(self.max_records)
    }

    /// Set the maximum strings before decoding; zero admits none. Replaces only this setting.
    pub fn max_strings(mut self, maximum: usize) -> Self {
        self.max_strings = maximum;
        self
    }

    /// Remove the strings quota. Structural and representation checks remain.
    pub fn without_strings_limit(mut self) -> Self {
        self.max_strings = usize::MAX;
        self
    }

    /// Inspect the strings quota; `None` means no configured maximum.
    pub fn strings_limit(&self) -> Option<usize> {
        (self.max_strings != usize::MAX).then_some(self.max_strings)
    }

    /// Set the maximum string chars before decoding; zero admits none. Replaces only this setting.
    pub fn max_string_chars(mut self, maximum: usize) -> Self {
        self.max_string_chars = maximum;
        self
    }

    /// Remove the string chars quota. Structural and representation checks remain.
    pub fn without_string_chars_limit(mut self) -> Self {
        self.max_string_chars = usize::MAX;
        self
    }

    /// Inspect the string chars quota; `None` means no configured maximum.
    pub fn string_chars_limit(&self) -> Option<usize> {
        (self.max_string_chars != usize::MAX).then_some(self.max_string_chars)
    }

    /// Set the maximum string bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_string_bytes(mut self, maximum: usize) -> Self {
        self.max_string_bytes = maximum;
        self
    }

    /// Remove the string bytes quota. Structural and representation checks remain.
    pub fn without_string_bytes_limit(mut self) -> Self {
        self.max_string_bytes = usize::MAX;
        self
    }

    /// Inspect the string bytes quota; `None` means no configured maximum.
    pub fn string_bytes_limit(&self) -> Option<usize> {
        (self.max_string_bytes != usize::MAX).then_some(self.max_string_bytes)
    }

    /// Set the maximum continuations before decoding; zero admits none. Replaces only this setting.
    pub fn max_continuations(mut self, maximum: usize) -> Self {
        self.max_continuations = maximum;
        self
    }

    /// Remove the continuations quota. Structural and representation checks remain.
    pub fn without_continuations_limit(mut self) -> Self {
        self.max_continuations = usize::MAX;
        self
    }

    /// Inspect the continuations quota; `None` means no configured maximum.
    pub fn continuations_limit(&self) -> Option<usize> {
        (self.max_continuations != usize::MAX).then_some(self.max_continuations)
    }

    /// Set the maximum cell text bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_cell_text_bytes(mut self, maximum: u64) -> Self {
        self.max_cell_text_bytes = maximum;
        self
    }

    /// Remove the cell text bytes quota. Structural and representation checks remain.
    pub fn without_cell_text_bytes_limit(mut self) -> Self {
        self.max_cell_text_bytes = u64::MAX;
        self
    }

    /// Inspect the cell text bytes quota; `None` means no configured maximum.
    pub fn cell_text_bytes_limit(&self) -> Option<u64> {
        (self.max_cell_text_bytes != u64::MAX).then_some(self.max_cell_text_bytes)
    }

    /// Set the maximum decoded cells before decoding; zero admits none. Replaces only this setting.
    pub fn max_cells(mut self, maximum: usize) -> Self {
        self.max_cells = maximum;
        self
    }

    /// Remove the decoded cells quota. Structural and representation checks remain.
    pub fn without_cells_limit(mut self) -> Self {
        self.max_cells = usize::MAX;
        self
    }

    /// Inspect the decoded cells quota; `None` means no configured maximum.
    pub fn cells_limit(&self) -> Option<usize> {
        (self.max_cells != usize::MAX).then_some(self.max_cells)
    }
}
