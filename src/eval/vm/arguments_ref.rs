//! Deferred argument handles for one call.

use super::arguments_iter::ArgumentsIter;
use super::executable_ref::ExecutableRef;
#[cfg(test)]
use crate::eval::compile::bound::Expression;
use crate::eval::compile::bytecode::Compiled;

/// Deferred argument blocks from a call or independently compiled programs.
#[derive(Clone, Copy)]
pub(in crate::eval) enum ArgumentsRef<'a> {
    /// No arguments for scalar host helpers.
    Empty,
    /// Independently compiled arguments supplied through adapter integration APIs.
    Programs(&'a [Compiled]),
    /// Bound-tree operands in source order.
    #[cfg(test)]
    Tree(&'a [Expression]),
    /// Linked bytecode argument descriptors.
    Bytecode {
        /// Program owning the descriptors and blocks.
        program: &'a Compiled,
        /// First argument descriptor index.
        first: usize,
        /// Number of syntactic argument positions.
        count: usize,
    },
}

#[cfg(test)]
impl<'a> From<&'a [Expression]> for ArgumentsRef<'a> {
    fn from(value: &'a [Expression]) -> Self {
        Self::Tree(value)
    }
}

#[cfg(test)]
impl<'a> From<&'a Vec<Expression>> for ArgumentsRef<'a> {
    fn from(value: &'a Vec<Expression>) -> Self {
        Self::Tree(value)
    }
}

#[cfg(test)]
impl<'a, const N: usize> From<&'a [Expression; N]> for ArgumentsRef<'a> {
    fn from(value: &'a [Expression; N]) -> Self {
        Self::Tree(value)
    }
}

impl<'a> ArgumentsRef<'a> {
    /// Count syntactic argument positions, including omissions.
    pub(in crate::eval) fn len(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Programs(v) => v.len(),
            #[cfg(test)]
            Self::Tree(v) => v.len(),
            Self::Bytecode { count, .. } => count,
        }
    }

    /// Whether the call has no argument positions.
    pub(in crate::eval) fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Borrow one deferred argument by zero-based ordinal.
    pub(in crate::eval) fn get(self, ordinal: usize) -> Option<ExecutableRef<'a>> {
        match self {
            Self::Empty => None,
            Self::Programs(v) => v
                .get(ordinal)
                .map(|program| ExecutableRef::Bytecode { program, start: 0 }),
            #[cfg(test)]
            Self::Tree(v) => v.get(ordinal).map(ExecutableRef::Tree),
            Self::Bytecode {
                program,
                first,
                count,
            } => {
                if ordinal >= count {
                    return None;
                }
                let mut descriptor = first;
                for _ in 0..ordinal {
                    descriptor = program
                        .template
                        .resolve_label(crate::eval::compile::bytecode::Label(
                            program.template.code.get(descriptor)?.c,
                        ))?
                        .index();
                }
                Some(ExecutableRef::Bytecode {
                    program,
                    start: descriptor.checked_add(1)?,
                })
            }
        }
    }
}

impl<'a> IntoIterator for ArgumentsRef<'a> {
    /// Deferred expression handle for one source argument.
    type Item = ExecutableRef<'a>;

    /// Source-order iterator over deferred argument handles.
    type IntoIter = ArgumentsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        ArgumentsIter::new(self)
    }
}

impl<'a> From<&'a [Compiled]> for ArgumentsRef<'a> {
    fn from(programs: &'a [Compiled]) -> Self {
        Self::Programs(programs)
    }
}
