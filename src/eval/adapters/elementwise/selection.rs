//! Completed selector masks with owned deferred branches and condition-bounded shapes.
use super::argument_role::ArgumentRole;
use super::broadcast::Broadcast;
use super::demand::Eager;
use super::fixed_scalar::FixedScalar;
use super::prepared::Prepared;
use super::with_demand::{Deferred, WithDemand};
use crate::eval::config::BuiltinSemanticsPolicy;
use crate::eval::host::{ArgumentCoercion, CallHost, PolicyReads, SelectionAccess};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::values::core_projection::semantic;
use crate::eval::values::{VALUE_ERROR, Value, err};
use crate::eval::vm::Shape;
use crate::model::CellError::NA;
use crate::model::CellValue::{Error, Number};
use crate::model::{CellError, CellValue};

/// Function-specific selection and omitted-result policy; traversal stays shared.
pub(in crate::eval::adapters) trait Selector<const N: usize> {
    /// Choose one of the ordered branch roles, or propagate a condition error.
    fn select(&self, value: &CellValue) -> Result<usize, CellError>;

    /// Supply the result for an absent trailing branch or explicit omission.
    fn omitted(&self, absent: bool) -> Value;
}

/// Prepare one selected branch, preserving explicit omission and its value channel.
fn prepare<S: Selector<N>, const N: usize>(
    host: &mut CallHost<'_, '_>,
    branch: &Deferred<Broadcast>,
    selector: &S,
) -> EvalResult<Prepared<Broadcast>> {
    let ordinal = branch.argument();
    if ordinal >= host.len() {
        Ok(Prepared::new(selector.omitted(true)))
    } else if SelectionAccess::new(host).is_missing(ordinal) {
        Ok(Prepared::new(selector.omitted(false)))
    } else {
        branch.prepare(host)
    }
}

/// Resolve a completed selector before preparing any branch; read only its mask.
/// Non-singleton condition axes bound output; shorter branch axes produce #N/A.
/// Whole-array expressions may still compute every member when prepared.
pub(in crate::eval::adapters) fn execute<S: Selector<N>, const N: usize>(
    host: &mut CallHost<'_, '_>,
    condition: WithDemand<FixedScalar, Eager>,
    branches: [Deferred<Broadcast>; N],
    selector: S,
) -> EvalResult<Value> {
    let ordinal = condition.role().argument();
    let condition =
        if PolicyReads::new(host).builtin_policy() == BuiltinSemanticsPolicy::ExcelDocumentation {
            let value = SelectionAccess::new(host).argument(ordinal)?.1;
            if SelectionAccess::new(host).retains_array(&value) {
                return array(host, value, branches, selector);
            }
            host.select_legacy_scalar(value)?
        } else {
            host.scalar_argument(ordinal)?
        };
    match selector.select(&condition) {
        Ok(index) => prepare(
            host,
            branches.get(index).ok_or(EvaluationFailure::StalePlan)?,
            &selector,
        )
        .map(Prepared::into_value),
        Err(error) => Ok(err(error)),
    }
}

/// Own the mask and each demanded value once, preserving source branch order.
fn array<S: Selector<N>, const N: usize>(
    host: &mut CallHost<'_, '_>,
    condition: Value,
    branches: [Deferred<Broadcast>; N],
    selector: S,
) -> EvalResult<Value> {
    let condition_shape = Shape::inspect(&condition).ok_or_else(|| {
        host.capability_failure(
            CapabilityId::FunctionAdapter,
            "selection requires one rectangular condition",
        )
    })?;
    let mut needed = [false; N];
    let mask = SelectionAccess::new(host).array(condition_shape, |reader, row, column| {
        let value = reader.member(&condition, condition_shape, row, column)?;
        Ok(match selector.select(&value) {
            Ok(index) => {
                *needed.get_mut(index).ok_or(EvaluationFailure::StalePlan)? = true;
                Number(index as f64)
            }
            Err(error) => Error(error),
        })
    })?;
    let mut prepared: [Option<(Prepared<Broadcast>, Shape)>; N] = std::array::from_fn(|_| None);
    let mut output = condition_shape;
    for (index, branch) in branches.iter().enumerate() {
        if !needed
            .get(index)
            .copied()
            .ok_or(EvaluationFailure::StalePlan)?
        {
            continue;
        }
        let value = prepare(host, branch, &selector)?;
        let shape = value.shape().ok_or_else(|| {
            host.capability_failure(
                CapabilityId::FunctionAdapter,
                "selection requires one rectangular selected branch",
            )
        })?;
        if condition_shape.rows == 1 {
            output.rows = output.rows.max(shape.rows);
        }
        if condition_shape.columns == 1 {
            output.columns = output.columns.max(shape.columns);
        }
        *prepared
            .get_mut(index)
            .ok_or(EvaluationFailure::StalePlan)? = Some((value, shape));
    }
    SelectionAccess::new(host).array(output, |reader, row, column| {
        let index = match reader.member(&mask, condition_shape, row, column)? {
            Number(index) => index as usize,
            Error(error) => return Ok(Error(error)),
            _ => return Err(EvaluationFailure::StalePlan),
        };
        let (value, shape) = prepared
            .get(index)
            .and_then(Option::as_ref)
            .ok_or(EvaluationFailure::StalePlan)?;
        if (shape.rows != 1 && row >= shape.rows) || (shape.columns != 1 && column >= shape.columns)
        {
            return Ok(Error(NA));
        }
        value.read(reader, *shape, row, column)
    })
}

/// Candidate roles determine whether probes are indices, tests, or expression cases.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) enum CandidateRoles {
    Positions,
    Conditions,
    Cases,
}

/// Describes how a variadic logical call interleaves tests and deferred result branches.
/// The shared selector uses these source positions to stop at the first selected branch without forcing later results.
pub(in crate::eval::adapters) struct VariadicLayout {
    pub first: usize,
    pub end: usize,
    pub step: usize,
    pub default: Option<usize>,
    pub candidates: CandidateRoles,
    pub results: std::ops::Range<usize>,
}

/// Pure family decision, with charged numeric and equality services left deferred.
pub(in crate::eval::adapters) enum SelectionTest {
    Match(Result<bool, CellError>),
    Index,
    Equal,
}

/// Domain rules for ordered first-match or positional branch selection.
pub(in crate::eval::adapters) trait VariadicSelector {
    /// Describe candidate order, optional expression, and deferred result roles.
    fn layout(&self, count: usize) -> VariadicLayout;

    /// Admit a candidate without reading workbook data or performing charged work.
    fn test(&self, value: &CellValue, expression: Option<&CellValue>) -> SelectionTest;

    /// Preserve a scalar result's explicit omission channel.
    fn omitted(&self) -> Value {
        Value::Missing
    }

    /// Map a failed expression before any candidate member is read.
    fn expression_error(&self, error: CellError) -> CellError {
        error
    }

    /// Apply family policy to a charged equality result.
    fn comparison(&self, result: Result<bool, CellError>) -> Result<bool, CellError> {
        result
    }

    /// Validate a positional index after evaluator-owned numeric admission.
    fn position(&self, _index: f64, _count: usize) -> Result<usize, CellError> {
        Err(VALUE_ERROR)
    }
}

/// Bind and transfer one deferred rectangular probe without a second observation.
fn deferred_probe(host: &mut CallHost<'_, '_>, ordinal: usize) -> EvalResult<Value> {
    Deferred::<Broadcast>::bind(ordinal, 0, true)
        .prepare(host)
        .map(Prepared::into_value)
}

/// Read each demanded probe once, retaining reference geometry until needed.
fn probe(
    host: &mut CallHost<'_, '_>,
    ordinal: usize,
    candidates: CandidateRoles,
) -> EvalResult<Value> {
    if matches!(candidates, CandidateRoles::Positions) {
        let value = deferred_probe(host, ordinal)?;
        return if SelectionAccess::new(host).retains_array(&value) {
            Ok(value)
        } else {
            host.select_legacy_scalar(value).map(Into::into)
        };
    }
    host.probe_argument(ordinal)
}

/// Resolve a domain decision using charged coercion and comparison services.
fn decide<S: VariadicSelector>(
    host: &mut CallHost<'_, '_>,
    selector: &S,
    candidate: &CellValue,
    expression: Option<&CellValue>,
    ordinal: usize,
) -> EvalResult<crate::eval::vm::Selection> {
    use crate::eval::vm::Selection;
    let result = match selector.test(candidate, expression) {
        SelectionTest::Match(result) => result,
        SelectionTest::Equal => selector.comparison(semantic(ArgumentCoercion::new(host).equal(
            expression.ok_or(EvaluationFailure::StalePlan)?,
            candidate,
            crate::eval::host::NumericPolicy::PinnedPoi,
        ))?),
        SelectionTest::Index => {
            return Ok(
                match semantic(SelectionAccess::new(host).number(candidate))? {
                    Ok(index) => match selector.position(index, host.len()) {
                        Ok(branch) => Selection::Branch(branch),
                        Err(error) => Selection::Error(error),
                    },
                    Err(error) => Selection::Error(error),
                },
            );
        }
    };
    Ok(match result {
        Ok(true) => Selection::Branch(ordinal.checked_add(1).ok_or(EvaluationFailure::StalePlan)?),
        Ok(false) => Selection::Pending,
        Err(error) => Selection::Error(error),
    })
}

/// Execute ordered probes, late mask expansion, and selected deferred results.
pub(in crate::eval::adapters) fn variadic<S: VariadicSelector>(
    host: &mut CallHost<'_, '_>,
    selector: S,
) -> EvalResult<Value> {
    use crate::eval::vm::{Selection, broadcast};
    let layout = selector.layout(host.len());
    if layout.step == 0 {
        return Err(EvaluationFailure::StalePlan);
    }
    let expression = if matches!(layout.candidates, CandidateRoles::Cases) {
        Some(probe(host, 0, CandidateRoles::Cases)?)
    } else {
        None
    };
    if let Some(Value::Scalar(Error(error))) = &expression {
        return Ok(err(selector.expression_error(*error)));
    }
    let expression_shape = expression
        .as_ref()
        .map(|value| {
            SelectionAccess::new(host).shape(value, "selection requires a rectangular expression")
        })
        .transpose()?;
    let mut output = expression_shape.unwrap_or(Shape {
        rows: 1,
        columns: 1,
    });
    let mut selected = None;
    if expression
        .as_ref()
        .is_some_and(|value| SelectionAccess::new(host).retains_array(value))
    {
        let count = output
            .rows
            .checked_mul(output.columns)
            .ok_or(EvaluationFailure::StalePlan)?;
        selected = Some(SelectionAccess::new(host).buffer(
            count,
            Selection::Pending,
            "selection allocation failed",
        )?);
    }
    // SAFETY: layout.step is checked nonzero above.
    for ordinal in (layout.first..layout.end).step_by(layout.step) {
        if let Some(mask) = &selected {
            host.charge_work(mask.len() as u64)?;
            if !mask.iter().any(|slot| matches!(slot, Selection::Pending)) {
                break;
            }
        }
        let candidate = if selected.is_some() {
            deferred_probe(host, ordinal)?
        } else {
            probe(host, ordinal, layout.candidates)?
        };
        if selected.is_none() && !SelectionAccess::new(host).retains_array(&candidate) {
            let candidate = host.select_legacy_scalar(candidate)?;
            let expression = match expression.as_ref() {
                Some(Value::Scalar(value)) => Some(value),
                None => None,
                _ => return Err(EvaluationFailure::StalePlan),
            };
            match decide(host, &selector, &candidate, expression, ordinal)? {
                Selection::Branch(branch) => {
                    if SelectionAccess::new(host).is_missing(branch) {
                        return Ok(selector.omitted());
                    }
                    return Deferred::<FixedScalar>::bind(branch, 0, true)
                        .prepare(host)
                        .map(Prepared::into_value);
                }
                Selection::Error(error) => return Ok(err(error)),
                Selection::Pending => continue,
            }
        }
        if matches!(candidate, Value::Missing) {
            return Err(host.capability_failure(
                CapabilityId::FunctionAdapter,
                "pinned MissingArgEval scalar coercion throws a source exception",
            ));
        }
        let shape = SelectionAccess::new(host)
            .shape(&candidate, "selection requires rectangular candidates")?;
        let Some(expanded) = broadcast(output, shape) else {
            return Ok(err(VALUE_ERROR));
        };
        let mask = match selected.take() {
            Some(mask) => mask,
            None => SelectionAccess::new(host).buffer(
                1,
                Selection::Pending,
                "selection allocation failed",
            )?,
        };
        let mut mask = SelectionAccess::new(host).expand(mask, output, expanded)?;
        output = expanded;
        for (index, slot) in mask.iter_mut().enumerate() {
            if !matches!(slot, Selection::Pending) {
                continue;
            }
            let row = index
                .checked_div(output.columns)
                .ok_or(EvaluationFailure::StalePlan)?;
            let column = index
                .checked_rem(output.columns)
                .ok_or(EvaluationFailure::StalePlan)?;
            let expression_member = match (&expression, expression_shape) {
                (Some(value), Some(shape)) => {
                    Some(SelectionAccess::new(host).member(value, shape, row, column)?)
                }
                _ => None,
            };
            // Expression errors terminate before reading the candidate member.
            if let Some(Error(error)) = expression_member {
                *slot = Selection::Error(selector.expression_error(error));
                continue;
            }
            let member = SelectionAccess::new(host).member(&candidate, shape, row, column)?;
            *slot = decide(
                host,
                &selector,
                &member,
                expression_member.as_ref(),
                ordinal,
            )?;
        }
        selected = Some(mask);
    }
    match selected {
        None => match layout.default {
            Some(branch) if SelectionAccess::new(host).is_missing(branch) => Ok(selector.omitted()),
            Some(branch) => Deferred::<FixedScalar>::bind(branch, 0, true)
                .prepare(host)
                .map(Prepared::into_value),
            None => Ok(err(NA)),
        },
        Some(mut mask) => {
            if let Some(default) = layout.default {
                host.charge_work(mask.len() as u64)?;
                for slot in &mut mask {
                    if matches!(slot, Selection::Pending) {
                        *slot = Selection::Branch(default);
                    }
                }
            }
            let step = if matches!(layout.candidates, CandidateRoles::Positions) {
                1
            } else {
                2
            };
            // SAFETY: step is assigned only the nonzero literals 1 and 2 above.
            SelectionAccess::new(host).array_results(
                mask,
                output,
                layout.results.step_by(step).chain(layout.default),
            )
        }
    }
}
