use super::array_view::ArrayView;
use super::operand_origin::OperandOrigin;
use super::reference_areas::ReferenceAreas;
use super::signed_reference::SignedArea;
use crate::model::{CellError, CellValue};
use CellError::Num;
use CellValue::{Blank, Error, Number};

/// Internal scalar/array/reference channels shared by tree, bytecode, and builtin adapters.
#[derive(Clone)]
pub(in crate::eval) enum Value {
    SignedRefs(Vec<SignedArea>),
    /// Selected helper MissingArgEval, distinct from an actual blank cell.
    Missing,
    Scalar(CellValue),
    /// Materialized selection retaining its original argument-admission provenance.
    SelectedScalar(CellValue, OperandOrigin),
    Refs(ReferenceAreas),
    /// One true ThreeDEval/RefEval, distinct from a RefListEval union.
    SheetSpan(ReferenceAreas),
    Array(ArrayView),
}

pub(in crate::eval) fn err(e: CellError) -> Value {
    Error(e).into()
}

/// Explicit legacy projection for consumers that intentionally collapse omission.
/// Host operand APIs must retain the separate missing flag alongside this value.
// TODO(coercion-non-argument): [MEDIUM] Collapsing omission to blank can change predicates and defaults unless the missing flag is retained.
pub(in crate::eval) fn convert_legacy_missing_blank() -> CellValue {
    Blank
}

// TODO(coercion-non-argument): [HIGH] Numeric result publication turns nonfinite results into NUM; argument admission must select its own policy.
pub(in crate::eval) fn finite(n: f64) -> CellValue {
    if n.is_finite() { Number(n) } else { Error(Num) }
}

pub(in crate::eval) const VALUE_ERROR: CellError = CellError::Value;

impl From<CellValue> for Value {
    fn from(v: CellValue) -> Self {
        Self::Scalar(v)
    }
}
