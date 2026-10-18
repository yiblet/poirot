//! Evidence-based categories for failed checks; these never change the oracle.
use crate::fods::xml::model::{FormulaCase, InputValue};
use poirot_formula::Expr;
use poirot_model::CellValue;
use serde::{Deserialize, Serialize};

/// Stable failure taxonomy separating evaluator gaps from source/harness limitations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapKind {
    /// Consumed function has no supported implementation.
    MissingFunction,
    /// Consumed defined name cannot be resolved.
    NameResolution,
    /// Evaluation returns a different spreadsheet error than expected.
    WrongErrorType,
    /// Evaluation returns an error where a value was expected.
    UnexpectedError,
    /// Evaluation returns a value where an error was expected.
    ExpectedError,
    /// Numeric result differs beyond the admitted comparison contract.
    NumericMismatch,
    /// Text result differs from the source expectation.
    TextMismatch,
    /// Boolean result differs from the source expectation.
    BooleanMismatch,
    /// Result channel differs from the expected scalar type.
    ValueTypeMismatch,
    /// Source check evaluates to a failing predicate.
    PredicateMismatch,
    /// Source formula cannot be parsed or translated faithfully.
    UnsupportedFormulaSyntax,
    /// Native workbook settings/features lack a supported execution contract.
    UnsupportedWorkbookContext,
    /// Source requires unsupported matrix/array execution context.
    UnsupportedArrayContext,
    /// Execution exceeds an explicit bounded resource policy.
    ResourceLimit,
    /// Runtime requires an unavailable evaluator capability.
    UnsupportedCapability,
    /// Pinned source behavior raises an operational exception.
    SourceException,
    /// Source assertion form is outside the runner's supported checks.
    UnsupportedCheck,
    /// Runner infrastructure failed independently of formula semantics.
    HarnessError,
}

impl GapKind {
    /// Return the stable human-readable category label.
    pub fn label(self) -> &'static str {
        match self {
            Self::MissingFunction => "missing_function",
            Self::NameResolution => "name_resolution",
            Self::WrongErrorType => "wrong_error_type",
            Self::UnexpectedError => "unexpected_error",
            Self::ExpectedError => "expected_error",
            Self::NumericMismatch => "numeric_mismatch",
            Self::TextMismatch => "text_mismatch",
            Self::BooleanMismatch => "boolean_mismatch",
            Self::ValueTypeMismatch => "value_type_mismatch",
            Self::PredicateMismatch => "predicate_mismatch",
            Self::UnsupportedFormulaSyntax => "unsupported_formula_syntax",
            Self::UnsupportedWorkbookContext => "unsupported_workbook_context",
            Self::UnsupportedArrayContext => "unsupported_array_context",
            Self::ResourceLimit => "resource_limit",
            Self::UnsupportedCapability => "unsupported_capability",
            Self::SourceException => "source_exception",
            Self::UnsupportedCheck => "unsupported_check",
            Self::HarnessError => "harness_error",
        }
    }
}

/// Find syntactically present unregistered functions; unselected branches may appear here.
pub fn missing_functions(expression: &Expr) -> Vec<String> {
    fn visit(expression: &Expr, missing: &mut std::collections::BTreeSet<String>) {
        match expression {
            Expr::Function { name, args } => {
                if poirot_eval::registry::resolve_function(name).is_none() {
                    missing.insert(name.to_string());
                }
                for argument in args {
                    visit(argument, missing);
                }
            }
            Expr::Unary { operand, .. } => visit(operand, missing),
            Expr::Binary { left, right, .. } => {
                visit(left, missing);
                visit(right, missing);
            }
            Expr::Array(rows) => {
                for item in rows.iter().flatten() {
                    visit(item, missing);
                }
            }
            _ => {}
        }
    }
    let mut missing = std::collections::BTreeSet::new();
    visit(expression, &mut missing);
    missing.into_iter().collect()
}

/// Only a missing root invocation proves the cause of a #NAME? result.
/// Missing calls in unexecuted branches are not evidence of this failure.
pub fn missing_root_function(expression: &Expr) -> Vec<String> {
    match expression {
        Expr::Function { name, .. } if poirot_eval::registry::resolve_function(name).is_none() => {
            vec![name.to_string()]
        }
        _ => Vec::new(),
    }
}

/// Numeric Expected cells can encode error codes/flags rather than result values.
/// Retain a generic predicate failure when the intended result cannot be inferred.
pub fn expected_is_error_flag(case: &FormulaCase) -> bool {
    let formula = case.correct_formula.to_ascii_uppercase();
    ["ERROR.TYPE(", "ISERROR(", "ISERR("]
        .iter()
        .any(|name| formula.contains(name))
        && matches!(
            case.expected.value,
            InputValue::Number { .. } | InputValue::Boolean { .. }
        )
        && case.expected.formula.is_none()
}

/// Project a native expected cell's typed saved value into the canonical model.
pub fn literal_expected(value: &InputValue) -> CellValue {
    match value {
        InputValue::Number { value, .. } => CellValue::Number(*value),
        InputValue::Boolean { value } => CellValue::Boolean(*value),
        InputValue::Text { value } => CellValue::Text(value.clone()),
        InputValue::Blank => CellValue::Blank,
    }
}

/// Classify a typed actual-versus-expected result difference.
pub fn mismatch(actual: &CellValue, expected: Option<&CellValue>) -> GapKind {
    if expected == Some(actual) {
        return GapKind::PredicateMismatch;
    }
    match (actual, expected) {
        (CellValue::Error(a), Some(CellValue::Error(e))) if a != e => GapKind::WrongErrorType,
        (_, Some(CellValue::Error(_))) if !matches!(actual, CellValue::Error(_)) => {
            GapKind::ExpectedError
        }
        (CellValue::Error(_), Some(e)) if !matches!(e, CellValue::Error(_)) => {
            GapKind::UnexpectedError
        }
        (CellValue::Number(_), Some(CellValue::Number(_))) => GapKind::NumericMismatch,
        (CellValue::Boolean(_), Some(CellValue::Boolean(_))) => GapKind::BooleanMismatch,
        (a, Some(e)) if a.text_ref().is_some() && e.text_ref().is_some() => GapKind::TextMismatch,
        (a, Some(e)) if a != e => GapKind::ValueTypeMismatch,
        _ => GapKind::PredicateMismatch,
    }
}
