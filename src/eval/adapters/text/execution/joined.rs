use crate::eval::host::CallHost;
use crate::eval::values::{FunctionResult, VALUE_ERROR};

/// TEXTJOIN output and the number of members joined so far.
#[derive(Default)]
pub(super) struct Joined {
    pub(super) output: Vec<u16>,
    pub(super) count: usize,
}

impl Joined {
    /// Append one member, preceded by the next delimiter after the first member.
    pub(super) fn push(
        &mut self,
        host: &mut CallHost<'_, '_>,
        delimiters: &[Vec<u16>],
        text: &[u16],
    ) -> FunctionResult<()> {
        let separator = if self.count > 0 && !delimiters.is_empty() {
            // SAFETY: count is positive and the delimiter sequence is nonempty in this branch.
            delimiters
                .get((self.count - 1) % delimiters.len())
                .map_or(&[][..], Vec::as_slice)
        } else {
            &[]
        };
        let additional = separator
            .len()
            .checked_add(text.len())
            .ok_or_else(|| host.size_overflow())?;
        let size = self
            .output
            .len()
            .checked_add(additional)
            .ok_or_else(|| host.size_overflow())?;
        if size > 32767 {
            return Err(VALUE_ERROR.into());
        }
        host.check_text_capacity(size.saturating_mul(2))?;
        host.charge_work(additional as u64)?;
        self.output
            .try_reserve(additional)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        // SAFETY: all separator and text units were reserved immediately above.
        self.output.extend_from_slice(separator);
        // SAFETY: the local additional reservation includes every text unit.
        self.output.extend_from_slice(text);
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| host.size_overflow())?;
        Ok(())
    }
}
