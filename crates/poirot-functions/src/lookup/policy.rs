use crate::lookup::operands::Arguments;
use crate::lookup::{Context, LookupError, LookupValueError, Value, Workbook, evaluate_arguments};
/// Search-family function selected by the evaluator's registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Search {
    /// LOOKUP vector or array form.
    Lookup,
    /// XLOOKUP with optional match and search modes.
    XLookup,
    /// XMATCH returning a position.
    XMatch,
}
/// Reference construction or explicit scalar intersection handled by this family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceFunction {
    /// Resolve a text-generated reference through the host.
    Indirect,
}
/// Typed family dispatch identity used to prepare a lookup call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    /// Dispatch a search-family operation.
    Search(Search),
    /// Dispatch a reference-family operation.
    Reference(ReferenceFunction),
}
/// Extra host dependency capabilities required by the selected operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyPolicy {
    /// Track only the values/references actually consumed.
    Ordinary,
    /// Track runtime-selected reference resolution.
    DynamicReference,
}
/// Possible dependency category, reference-return behavior, and scheduling volatility
/// for a lookup/reference function. This describes possible effects, not consumed reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionPolicy {
    /// Dependency contract the adapter must honor.
    pub dependency: DependencyPolicy,
    /// Whether output may preserve reference geometry.
    pub retains_reference: bool,
    /// Whether reuse must account for volatile behavior.
    pub volatile: bool,
}
impl Function {
    /// Return the dependency and reference contract of this function.
    pub fn policy(self) -> FunctionPolicy {
        use ReferenceFunction::Indirect;
        let dependency = match self {
            Self::Reference(Indirect) => DependencyPolicy::DynamicReference,
            _ => DependencyPolicy::Ordinary,
        };
        FunctionPolicy {
            dependency,
            retains_reference: matches!(
                self,
                Self::Reference(Indirect) | Self::Search(Search::XLookup)
            ),
            volatile: matches!(self, Self::Reference(Indirect)),
        }
    }
}
/// An arity-checked lookup call that can execute against eager values or deferred host arguments.
/// Preparing it leaves references unresolved and ignored operands unevaluated.
#[derive(Clone, Debug, PartialEq)]
pub struct CallPlan {
    function: Function,
    count: usize,
}
impl CallPlan {
    /// Validate a typed call's arity and capture its execution policy.
    pub fn prepare(function: Function, count: usize) -> Result<Self, LookupError> {
        use ReferenceFunction::Indirect;
        let valid = match function {
            Function::Reference(Indirect) => count >= 1,
            Function::Search(Search::Lookup) => (2..=3).contains(&count),
            Function::Search(Search::XLookup) => count >= 3,
            Function::Search(Search::XMatch) => count >= 2,
        };
        if valid {
            Ok(Self { function, count })
        } else {
            Err(LookupValueError::Value.into())
        }
    }
    /// Inspect the prepared call's host requirements.
    pub fn policy(&self) -> FunctionPolicy {
        self.function.policy()
    }
    /// Execute the admitted call with the default family semantics.
    pub fn execute(
        &self,
        context: Context,
        workbook: &mut impl Workbook,
    ) -> Result<Value, LookupError> {
        evaluate_arguments(
            self.function,
            &Arguments {
                ready: None,
                count: self.count,
            },
            context,
            workbook,
        )
    }
}
/// How an exact lookup miss may select another candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMode {
    /// Require an exact match.
    Exact,
    /// Allow the next smaller candidate.
    Smaller,
    /// Allow the next larger candidate.
    Larger,
    /// Use wildcard matching for text.
    Wildcard,
}
/// Traversal direction and ordering assumption for lookup search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    /// Scan candidates from first to last.
    Forward,
    /// Scan candidates from last to first.
    Backward,
    /// Binary search data assumed ascending.
    BinaryAscending,
    /// Binary search data assumed descending.
    BinaryDescending,
}
