use crate::CellError;
use std::fmt;

/// A candidate does not accept these inputs; only this failure permits fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// Wrong number of arguments for a candidate signature.
    Arity { expected: usize, actual: usize },
    /// A selected scalar does not meet the requested rule.
    Input {
        argument: Option<usize>,
        expected: &'static str,
    },
}

/// Resource exhausted while attempting a requested conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Cumulative work, including abandoned attempts.
    Work,
    /// Cumulative bytes admitted before allocating text output.
    TextBytes,
}

/// Mismatch, spreadsheet error, and operational failure stay distinguishable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoercionV2Failure {
    /// This candidate may be followed by a lower-priority candidate.
    Mismatch(Mismatch),
    /// A spreadsheet error is terminal unless explicitly requested as data.
    Worksheet(CellError),
    /// An operational failure must never select a different overload.
    Limit(Resource),
    /// A Unicode-scalar consumer cannot represent isolated UTF-16 surrogates.
    UnsupportedText,
}

impl CoercionV2Failure {
    pub(crate) fn mismatch(expected: &'static str) -> Self {
        Self::Mismatch(Mismatch::Input {
            argument: None,
            expected,
        })
    }

    pub(crate) fn at_argument(self, index: usize) -> Self {
        match self {
            Self::Mismatch(Mismatch::Input { expected, .. }) => Self::Mismatch(Mismatch::Input {
                argument: Some(index),
                expected,
            }),
            other => other,
        }
    }
}

impl fmt::Display for CoercionV2Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch(mismatch) => write!(f, "no matching input: {mismatch:?}"),
            Self::Worksheet(error) => error.fmt(f),
            Self::Limit(resource) => write!(f, "conversion resource exhausted: {resource:?}"),
            Self::UnsupportedText => f.write_str("Unicode scalar conversion requires valid UTF-16"),
        }
    }
}

impl std::error::Error for CoercionV2Failure {}

/// Caller-supplied resource limits; hosts can supply their remaining allowance.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub work: u64,
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
    pub work: u64,
    pub text_bytes: usize,
}

/// Explicit execution context, separate from immutable argument values.
/// Failed attempts retain their resource charges. No clock or locale is implicit.
#[derive(Debug)]
pub struct CoercionV2Context {
    limits: Limits,
    usage: Usage,
}

impl Default for CoercionV2Context {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl CoercionV2Context {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            usage: Usage::default(),
        }
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }

    pub(crate) fn charge(&mut self, work: u64) -> Result<(), CoercionV2Failure> {
        let used = self
            .usage
            .work
            .checked_add(work)
            .filter(|used| *used <= self.limits.work)
            .ok_or(CoercionV2Failure::Limit(Resource::Work))?;
        self.usage.work = used;
        Ok(())
    }

    pub(crate) fn admit_text(&mut self, bytes: usize) -> Result<(), CoercionV2Failure> {
        let used = self
            .usage
            .text_bytes
            .checked_add(bytes)
            .filter(|used| *used <= self.limits.text_bytes)
            .ok_or(CoercionV2Failure::Limit(Resource::TextBytes))?;
        self.usage.text_bytes = used;
        Ok(())
    }
}
