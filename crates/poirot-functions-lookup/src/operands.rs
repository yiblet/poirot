use crate::{
    BudgetRequest, Context, Error, Failure, OperationalFailure, Scalar, Unsupported, Value, Work,
    Workbook,
};
/// Ask the host to admit categorized work before performing it.
pub(super) fn charge(w: &mut impl Workbook, work: Work, units: usize) -> Result<(), Failure> {
    w.preflight(BudgetRequest { work, units })
}
/// Multiply resource dimensions without allowing size overflow.
pub(super) fn checked_product(a: usize, b: usize) -> Result<usize, Failure> {
    a.checked_mul(b)
        .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))
}
/// Borrow eager values or defer indexed argument acquisition to the host.
pub(super) struct Arguments<'a> {
    /// Eager argument slice; absence selects host-provided deferred arguments.
    pub(super) ready: Option<&'a [Value]>,
    /// Validated number of argument slots.
    pub(super) count: usize,
}
impl Arguments<'_> {
    /// Return the validated argument-slot count.
    pub(super) fn len(&self) -> usize {
        self.count
    }
    /// Whether the call has no argument slots.
    pub(super) fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Materialize one argument on demand after host work admission.
    pub(super) fn get(
        &self,
        index: usize,
        c: Context,
        w: &mut impl Workbook,
    ) -> Result<Value, Failure> {
        if index >= self.count {
            return Err(Error::Value.into());
        }
        charge(w, Work::InputMaterialization, 1)?;
        if let Some(values) = self.ready {
            let value = values.get(index).ok_or(Error::Value)?;
            preflight_value(value, w, Work::InputMaterialization)?;
            Ok(value.clone())
        } else {
            match w.argument(index, c) {
                Err(Failure::Spreadsheet(e)) => Ok(Value::Scalar(Scalar::Error(e))),
                other => other,
            }
        }
    }
}
/// Reserve cloning/materialization work for a value and its nested storage.
pub(super) fn preflight_value(
    value: &Value,
    w: &mut impl Workbook,
    work: Work,
) -> Result<(), Failure> {
    match value {
        Value::Scalar(Scalar::Text(s)) => charge(w, work, s.len())?,
        Value::Reference(r) => {
            if let Some(name) = &r.workbook {
                charge(w, work, name.len())?;
            }
        }
        Value::Union(refs) => {
            charge(w, work, refs.len())?;
            for r in refs {
                if let Some(name) = &r.workbook {
                    charge(w, work, name.len())?;
                }
            }
        }
        Value::SignedReference(r) => {
            if let Some(name) = &r.workbook {
                charge(w, work, name.len())?;
            }
        }
        Value::SignedUnion(refs) => {
            charge(w, work, refs.len())?;
            for r in refs {
                if let Some(name) = &r.workbook {
                    charge(w, work, name.len())?;
                }
            }
        }
        Value::Array(a) => {
            charge(w, work, a.values.len())?;
            for scalar in &a.values {
                if let Scalar::Text(s) = scalar {
                    charge(w, work, s.len())?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
/// Select a scalar using caller coordinates and retained operand provenance.
pub(super) fn scalar(
    value: &Value,
    context: Context,
    workbook: &mut impl Workbook,
) -> Result<Scalar, Failure> {
    preflight_value(value, workbook, Work::InputMaterialization)?;
    let result = match value {
        Value::Scalar(v) => v.clone(),
        Value::Reference(r) => {
            let checked =
                crate::domain::CheckedReference::new(r).ok_or(Failure::Spreadsheet(Error::Ref))?;
            let selection = crate::geometry::scalar_selection(&checked, context)?;
            workbook.cell(r, selection.sheet, selection.row, selection.col)?
        }
        Value::SignedReference(r) => {
            if !r.valid() {
                return Err(Error::Ref.into());
            }
            let (row, col) = r.scalar_coordinates(context)?;
            r.read(row, col, workbook)?
        }
        Value::Array(_) => return Err(Failure::Unsupported(Unsupported::ScalarArrayIntersection)),
        Value::Union(_) | Value::SignedUnion(_) => return Err(Error::Value.into()),
    };
    match result {
        Scalar::Error(e) => Err(e.into()),
        v => Ok(v),
    }
}
/// Coerce an admitted operand to a lookup numeric value.
pub(super) fn number(v: Scalar, w: &mut impl Workbook) -> Result<f64, Failure> {
    match v {
        Scalar::Number(n) => Ok(n),
        Scalar::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
        Scalar::Blank => Ok(0.0),
        Scalar::Missing => Err(Error::Value.into()),
        Scalar::Text(s) => w.numeric_text(&s),
        Scalar::Error(e) => Err(e.into()),
    }
}
/// Apply the family's integer conversion and range checks.
pub(super) fn integer(v: Scalar, floor: bool, w: &mut impl Workbook) -> Result<i32, Failure> {
    let n = number(v, w)?;
    let n = if floor { n.floor() } else { n };
    if !n.is_finite() || n < i32::MIN as f64 || n > i32::MAX as f64 {
        return Err(Error::Num.into());
    }
    Ok(n as i32)
}
/// Convert an admitted operand to lookup text.
pub(super) fn text(v: &Scalar, w: &mut impl Workbook) -> Result<String, Failure> {
    match v {
        Scalar::Text(s) => {
            charge(w, Work::SearchText, s.len())?;
            Ok(s.clone())
        }
        Scalar::Number(n) => {
            charge(w, Work::SearchText, 32)?;
            w.number_text(*n)
        }
        Scalar::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
        Scalar::Blank | Scalar::Missing => Ok(String::new()),
        Scalar::Error(e) => Err((*e).into()),
    }
}
/// Coerce an admitted operand into a logical option.
pub(super) fn boolean(v: Scalar) -> Result<bool, Failure> {
    match v {
        Scalar::Bool(b) => Ok(b),
        Scalar::Number(n) => Ok(n != 0.0),
        Scalar::Blank | Scalar::Missing => Ok(false),
        Scalar::Text(s) if s.eq_ignore_ascii_case("TRUE") => Ok(true),
        Scalar::Text(s) if s.eq_ignore_ascii_case("FALSE") => Ok(false),
        Scalar::Error(e) => Err(e.into()),
        _ => Err(Error::Value.into()),
    }
}
/// Test the omitted-argument channel without collapsing it to blank.
pub(super) fn missing(v: &Value) -> bool {
    matches!(v, Value::Scalar(Scalar::Missing))
}
/// Construct a numeric output with the family's nonfinite checks.
pub(super) fn num(n: f64) -> Value {
    Value::Scalar(Scalar::Number(n))
}
