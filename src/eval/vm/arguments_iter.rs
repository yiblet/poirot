//! Source-order iteration over deferred argument handles.

use super::arguments_ref::ArgumentsRef;
use super::executable_ref::ExecutableRef;

/// Source-order iterator yielding deferred expression handles.
pub(in crate::eval) struct ArgumentsIter<'a> {
    arguments: ArgumentsRef<'a>,
    index: usize,
}

impl<'a> ArgumentsIter<'a> {
    /// Start iterating a call's arguments at the first source position.
    pub(super) fn new(arguments: ArgumentsRef<'a>) -> Self {
        Self {
            arguments,
            index: 0,
        }
    }
}

impl<'a> Iterator for ArgumentsIter<'a> {
    /// Deferred expression handle for the next source argument.
    type Item = ExecutableRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.arguments.get(self.index)?;
        self.index += 1;
        Some(value)
    }
}
