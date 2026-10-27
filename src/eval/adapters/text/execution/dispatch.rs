use super::join_kernel::JoinKernel;
use super::number_kernel::NumberKernel;
use super::scalar_kernel::ScalarKernel;
use crate::eval::adapters;
use crate::eval::adapters::elementwise::{self, collections};
use crate::eval::config::BuiltinSemanticsPolicy;
use crate::eval::functions::registry::{FunctionId, TextFunction};
use crate::eval::host::{CallHost, PolicyReads};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::session::Session;
use crate::eval::values::{VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellAddress;
use poirot_functions::text_units::SourceVersion as TextSourceVersion;

/// Dispatch text builtins under the selected text and compatibility policies.
pub(in crate::eval) fn execute(
    mut host: CallHost<'_, '_>,
    at: CellAddress,
    name: TextFunction,
    length: usize,
) -> EvalResult<Value> {
    if matches!(
        name,
        TextFunction::RegexTest | TextFunction::RegexExtract | TextFunction::RegexReplace
    ) {
        return adapters::text::execute_regex(name, host, at, length);
    }
    if matches!(
        name,
        TextFunction::TextBefore
            | TextFunction::TextAfter
            | TextFunction::TextSplit
            | TextFunction::ArrayToText
            | TextFunction::ValueToText
    ) {
        return adapters::text::execute_modern(name, host, at, length);
    }
    // CODE's documented numeric contract is independent of text indexing policy.
    if name == TextFunction::Code
        && PolicyReads::new(&mut host).builtin_policy_snapshot()
            == BuiltinSemanticsPolicy::ExcelDocumentation
    {
        return adapters::text::execute_units(host, name, length);
    }
    if matches!(
        name,
        TextFunction::Len
            | TextFunction::Left
            | TextFunction::Right
            | TextFunction::Mid
            | TextFunction::Replace
            | TextFunction::Concat
            | TextFunction::Find
            | TextFunction::Search
            | TextFunction::Lower
            | TextFunction::Upper
            | TextFunction::Rept
            | TextFunction::Code
            | TextFunction::Proper
    ) {
        PolicyReads::new(&mut host).text_policy();
        {
            if PolicyReads::new(&mut host).text_policy_snapshot().source
                == TextSourceVersion::ExcelCompatibilityV2Unverified
            {
                return Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: None,
                    detail: "unverified Excel text policy".into(),
                });
            }
            return adapters::text::execute_units(host, name, length);
        }
    }

    if name == TextFunction::TextJoin {
        return super::super::outcome::finish(collections::execute(&mut host, at, JoinKernel));
    }
    if !FunctionId::Text(name).descriptor().arity.accepts(length) {
        return Ok(err(VALUE_ERROR));
    }
    if name == TextFunction::NumberValue {
        return super::super::outcome::finish(elementwise::execute(&mut host, at, NumberKernel));
    }
    super::super::outcome::finish(elementwise::execute(&mut host, at, ScalarKernel(name)))
}

/// Prepare the text family's argument demand before executing its handler.
pub(in crate::eval) fn execute_function(
    session: &mut Session<'_>,
    s: usize,
    at: CellAddress,
    name: TextFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let host = if matches!(name, TextFunction::Concat | TextFunction::TextJoin) {
        CallHost::new(session, s, at, args)
    } else {
        CallHost::new_array(session, s, at, args, args.len())
    };
    execute(host, at, name, args.len())
}
