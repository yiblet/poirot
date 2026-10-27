use crate::coercion::CoercionError;

/// Resource exhausted while attempting a requested conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Cumulative work, including abandoned attempts.
    Work,
    /// Cumulative bytes admitted before allocating text output.
    TextBytes,
}

/// Cumulative conversion ceilings; hosts can supply their remaining allowance.
/// The default uses the maximum representable counters, without an application quota.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum admitted work units, including attempts that later fail.
    pub work: u64,
    /// Maximum cumulative bytes reserved for owned text conversions.
    pub text_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            work: u64::MAX,
            text_bytes: usize::MAX,
        }
    }
}

/// Work actually performed and conservative text-allocation reservations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Work units admitted so far; rejected charges do not change this counter.
    pub work: u64,
    /// Admitted text-byte reservations, retained even if an attempt is abandoned.
    pub text_bytes: usize,
}

/// Conversion limits and accumulated work for a function call.
/// Failed attempts retain their admitted resource charges. No clock or locale is implicit.
/// Default construction starts at zero with [`Limits::default()`].
/// Work units are policy estimates, not elapsed time; text bytes are reservations,
/// not a measurement of live heap use. This context does not bound the function
/// algorithm itself or native regex execution.
#[derive(Debug)]
pub struct CoercionContext {
    limits: Limits,
    usage: Usage,
}

impl Default for CoercionContext {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl CoercionContext {
    /// Start a conversion budget with zero usage and the supplied cumulative ceilings.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            usage: Usage::default(),
        }
    }

    /// Read admitted work and text reservations without resetting the budget.
    pub fn usage(&self) -> Usage {
        self.usage
    }

    pub(crate) fn charge(&mut self, work: u64) -> Result<(), CoercionError> {
        let used = self
            .usage
            .work
            .checked_add(work)
            .filter(|used| *used <= self.limits.work)
            .ok_or(CoercionError::Limit(Resource::Work))?;
        self.usage.work = used;
        Ok(())
    }

    pub(crate) fn admit_text(&mut self, bytes: usize) -> Result<(), CoercionError> {
        let used = self
            .usage
            .text_bytes
            .checked_add(bytes)
            .filter(|used| *used <= self.limits.text_bytes)
            .ok_or(CoercionError::Limit(Resource::TextBytes))?;
        self.usage.text_bytes = used;
        Ok(())
    }
}
