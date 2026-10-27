//! Checked source-ordered numerical collection materialization for typed kernels.
use super::argument_role::ArgumentRole;
use super::tuple_signature::Signature;
use super::whole_collection::WholeCollection;
use crate::eval::host::{ArgumentCoercion, CallHost, OperandKind, OperandReader};
use crate::eval::values::{FunctionError, FunctionResult, OperandOrigin, VALUE_ERROR, Value};
use crate::model::CellError::{NA, Ref};
use crate::model::CellValue::{Blank, Boolean, Error, Number};
use crate::model::{CellAddress, CellError};

/// Numeric admission chosen independently from positional traversal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::eval::adapters) enum Admission {
    /// Count direct logical/numeric text, ignore them within arrays and references.
    Aggregate,
    /// Retain only actual numbers, preserving an excluded position as None.
    Numeric,
    /// Require every position to be numeric rather than silently changing a table.
    Strict,
    /// Retain numbers and treat actual blanks as zero, rejecting other values.
    BlankZero,
}

/// Read one position under its declared numeric admission rule.
fn member(
    host: &mut CallHost<'_, '_>,
    index: usize,
    offset: usize,
    admission: Admission,
    kind: OperandKind,
) -> FunctionResult<Option<f64>> {
    let operand = OperandReader::new(host).collection_cell(index, offset)?;
    if operand.missing {
        return Err(VALUE_ERROR.into());
    }
    match &operand.value {
        Error(error) => return Err((*error).into()),
        Number(value) => return Ok(Some(*value)),
        _ => {}
    }
    if admission == Admission::BlankZero {
        return if matches!(operand.value, Blank) {
            Ok(Some(0.0))
        } else {
            Err(VALUE_ERROR.into())
        };
    }
    if admission == Admission::Strict {
        return Err(VALUE_ERROR.into());
    }
    let direct = operand.origin == OperandOrigin::Direct || kind == OperandKind::Scalar;
    if admission == Admission::Aggregate && direct {
        return match operand.value {
            Blank => Ok(None),
            value => ArgumentCoercion::new(host)
                .coerce_scalar_number(&value)
                .map(Some),
        };
    }
    Ok(None)
}

/// Traverse checked collection cells in position order under one admission rule.
fn visit_members(
    host: &mut CallHost<'_, '_>,
    index: usize,
    len: usize,
    admission: Admission,
    mut consume: impl FnMut(Option<f64>) -> FunctionResult<()>,
) -> FunctionResult<()> {
    let kind = host.kind(index)?;
    for offset in 0..len {
        consume(member(host, index, offset, admission, kind)?)?;
    }
    Ok(())
}

/// Charge and collect one operand without erasing excluded pair positions.
pub(in crate::eval::adapters) fn positions(
    host: &mut CallHost<'_, '_>,
    role: &WholeCollection,
    admission: Admission,
) -> FunctionResult<Vec<Option<f64>>> {
    let index = role.argument();
    let len = role.len(host)?;
    host.check_values(len)?;
    host.charge_work((len as u64).saturating_mul(8))?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    let allocation_target = host.target();
    visit_members(host, index, len, admission, |value| {
        result
            .try_reserve(1)
            .map_err(|reservation_source| crate::eval::limits::EvaluationError::reservation(crate::eval::limits::CapabilityId::FunctionAdapter, Some(allocation_target), crate::eval::limits::EvaluationReasonError::AllocatingAFunctionResultOrScratchBuffer, reservation_source))?;
        // SAFETY: one additional slot was reserved immediately above.
        result.push(value);
        Ok(())
    })?;
    Ok(result)
}

/// Gather one or more samples, charging before copying their admitted members.
pub(in crate::eval::adapters) fn collect(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
    admission: Admission,
) -> FunctionResult<Vec<f64>> {
    let mut result = Vec::new();
    for role in roles {
        let index = role.argument();
        let len = OperandReader::new(host).collection_len(index)?;
        let total = result
            .len()
            .checked_add(len)
            .ok_or_else(|| host.size_overflow())?;
        host.check_values(total)?;
        host.charge_work((len as u64).saturating_mul(12))?;
        result
            .try_reserve(len)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        let allocation_target = host.target();
        visit_members(host, index, len, admission, |value| {
            if let Some(value) = value {
                result
                    .try_reserve(1)
                    .map_err(|reservation_source| crate::eval::limits::EvaluationError::reservation(crate::eval::limits::CapabilityId::FunctionAdapter, Some(allocation_target), crate::eval::limits::EvaluationReasonError::AllocatingAFunctionResultOrScratchBuffer, reservation_source))?;
                // SAFETY: one additional slot was reserved immediately above.
                result.push(value);
            }
            Ok(())
        })?;
    }
    Ok(result)
}

/// Preserve positional associations while excluding a pair containing a nonnumber.
pub(in crate::eval::adapters) fn pairs(
    host: &mut CallHost<'_, '_>,
    left: &WholeCollection,
    right: &WholeCollection,
    admission: Admission,
) -> FunctionResult<(Vec<f64>, Vec<f64>)> {
    let a_len = left.len(host)?;
    let b_len = right.len(host)?;
    if a_len != b_len {
        return Err(NA.into());
    }
    host.check_values(a_len.saturating_mul(4))?;
    let a = positions(host, left, admission)?;
    let b = positions(host, right, admission)?;
    let (mut x, mut y) = (Vec::new(), Vec::new());
    let capacity = a.len().min(b.len());
    x.try_reserve_exact(capacity)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    y.try_reserve_exact(capacity)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    for (a, b) in a.into_iter().zip(b) {
        if let (Some(a), Some(b)) = (a, b) {
            // SAFETY: capacity slots were reserved; zip emits at most min(a.len(), b.len()) pairs.
            x.push(a);
            // SAFETY: capacity slots were reserved; zip emits at most min(a.len(), b.len()) pairs.
            y.push(b);
        }
    }
    Ok((x, y))
}

/// Collect the original paired geometry without independently filtering either axis.
pub(in crate::eval::adapters) fn points(
    host: &mut CallHost<'_, '_>,
    values: &WholeCollection,
    timeline: &WholeCollection,
) -> FunctionResult<Vec<(f64, f64)>> {
    let a = OperandReader::new(host).describe(values.argument())?;
    let b = OperandReader::new(host).describe(timeline.argument())?;
    if a.sheets != 1 || b.sheets != 1 {
        return Err(VALUE_ERROR.into());
    }
    if a.rows != b.rows || a.columns != b.columns {
        return Err(NA.into());
    }
    let count = values.len(host)?;
    host.check_values(count.saturating_mul(12))?;
    host.charge_work((count as u64).saturating_mul(8))?;
    let mut points = Vec::new();
    points
        .try_reserve_exact(count)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    for offset in 0..count {
        let value = values.read(host, offset)?;
        let time = timeline.read(host, offset)?;
        if let Error(error) = value.value {
            return Err(error.into());
        }
        if let Error(error) = time.value {
            return Err(error.into());
        }
        let (Number(value), Number(time)) = (value.value, time.value) else {
            return Err(VALUE_ERROR.into());
        };
        // SAFETY: count slots reserved and the loop emits exactly count aligned pairs.
        points.push((time, value));
    }
    Ok(points)
}

/// A whole-collection operation whose result may have a data-dependent shape.
pub(in crate::eval::adapters) trait CollectionKernel {
    /// Typed sources admitted before collection traversal begins.
    type Signature: super::tuple_signature::Signature;

    /// Calculate the complete result after shared source binding and admission.
    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        address: CellAddress,
    ) -> FunctionResult<Value>;
}

/// Execute a typed collection kernel with shared signature admission.
pub(in crate::eval::adapters) fn execute<K: CollectionKernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    kernel: K,
) -> FunctionResult<Value> {
    let (_, arguments) = K::Signature::admit(host)?;
    kernel.apply(host, &arguments, address)
}

/// Fold checked collection members in argument order, preserving origin and omission.
pub(in crate::eval::adapters) fn fold<T>(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
    mut state: T,
    mut step: impl FnMut(
        &mut CallHost<'_, '_>,
        &mut T,
        crate::eval::host::ScalarOperand,
    ) -> FunctionResult<()>,
) -> FunctionResult<T> {
    for role in roles {
        let len = role.len(host)?;
        host.check_values(len)?;
        host.charge_work(len as u64)?;
        for offset in 0..len {
            let operand = role.read(host, offset)?;
            step(host, &mut state, operand)?;
        }
    }
    Ok(state)
}

/// Fold members like [`fold`], but read only stored cells of single-area references.
///
/// Absent cells read as blanks and cannot fail, so `skip` receives the number of
/// absent positions passed over before each stored member and after the last one.
/// Arrays, scalars, multi-area references, and external readers use dense reads,
/// reporting every member to `step`.
pub(in crate::eval::adapters) fn fold_stored<T>(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
    mut state: T,
    mut skip: impl FnMut(&mut CallHost<'_, '_>, &mut T, usize) -> FunctionResult<()>,
    mut step: impl FnMut(
        &mut CallHost<'_, '_>,
        &mut T,
        crate::eval::host::ScalarOperand,
    ) -> FunctionResult<()>,
) -> FunctionResult<T> {
    for role in roles {
        let len = role.len(host)?;
        let Some(positions) = OperandReader::new(host).stored_positions(role.argument())? else {
            host.check_values(len)?;
            host.charge_work(len as u64)?;
            for offset in 0..len {
                let operand = role.read(host, offset)?;
                step(host, &mut state, operand)?;
            }
            continue;
        };
        let columns = role.describe(host)?.columns;
        host.charge_work(positions.len() as u64)?;
        let mut next = 0usize;
        for (row, column) in positions {
            let offset = row
                .checked_mul(columns)
                .and_then(|start| start.checked_add(column))
                .ok_or_else(|| host.size_overflow())?;
            let gap = offset.checked_sub(next).ok_or(Ref)?;
            if gap > 0 {
                skip(host, &mut state, gap)?;
            }
            let operand = role.read_at(host, row, column)?;
            step(host, &mut state, operand)?;
            next = offset.checked_add(1).ok_or_else(|| host.size_overflow())?;
        }
        let gap = len.checked_sub(next).ok_or(Ref)?;
        if gap > 0 {
            skip(host, &mut state, gap)?;
        }
    }
    Ok(state)
}

/// Materialize statistical numeric inputs with explicit origin-sensitive admission.
pub(in crate::eval::adapters) fn statistics(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
    policy: poirot_functions::statistics::Collection,
) -> FunctionResult<Vec<f64>> {
    use poirot_functions::statistics::{Collection, Operand, Origin, Value as StatisticalValue};
    let mut values = Vec::new();
    for role in roles {
        let len = role.len(host)?;
        let maximum = values
            .len()
            .checked_add(len)
            .ok_or_else(|| host.size_overflow())?;
        host.check_values(maximum)?;
        host.charge_work((len as u64).saturating_mul(3))?;
        values
            .try_reserve(len)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        let kind = host.kind(role.argument())?;
        let area =
            policy == Collection::Mode && OperandReader::new(host).is_area(role.argument())?;
        for offset in 0..len {
            let operand = role.read(host, offset)?;
            let direct = operand.origin == OperandOrigin::Direct || kind == OperandKind::Scalar;
            let origin = if area {
                Origin::Array
            } else {
                match operand.origin {
                    OperandOrigin::Direct => Origin::Direct,
                    _ if kind == OperandKind::Scalar => Origin::SelectedScalar,
                    OperandOrigin::ArrayElement => Origin::Array,
                    OperandOrigin::ReferencedCell => Origin::Reference,
                }
            };
            let parse = policy == Collection::PercentRank
                || (direct
                    && matches!(
                        policy,
                        Collection::NumericAggregate | Collection::GeoMean | Collection::ExtremaA
                    ));
            let value = if operand.missing {
                StatisticalValue::Missing
            } else {
                match operand.value {
                    Number(number) => StatisticalValue::Number(number),
                    Boolean(boolean) => StatisticalValue::Boolean(boolean),
                    Blank => StatisticalValue::Blank,
                    Error(error) => StatisticalValue::Error(error),
                    value => {
                        let numeric = if parse {
                            match ArgumentCoercion::new(host).coerce_scalar_number(&value) {
                                Ok(number) => Some(number),
                                Err(FunctionError::Spreadsheet(VALUE_ERROR)) => None,
                                Err(other) => return Err(other),
                            }
                        } else {
                            None
                        };
                        StatisticalValue::Text { numeric }
                    }
                }
            };
            let admitted = poirot_functions::statistics::coerce_statistics_collection(
                policy,
                &[Operand { value, origin }],
            )?;
            host.check_values(
                values
                    .len()
                    .checked_add(admitted.len())
                    .ok_or_else(|| host.size_overflow())?,
            )?;
            values
                .try_reserve(admitted.len())
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            // SAFETY: admitted.len additional slots were reserved immediately above.
            values.extend(admitted);
        }
    }
    Ok(values)
}

/// Fold paired members with left admission before reading the corresponding right cell.
pub(in crate::eval::adapters) fn zip_fold<T, L>(
    host: &mut CallHost<'_, '_>,
    left: &WholeCollection,
    right: &WholeCollection,
    mismatch: CellError,
    mut state: T,
    mut admit_left: impl FnMut(
        &mut CallHost<'_, '_>,
        &mut T,
        crate::eval::host::ScalarOperand,
    ) -> FunctionResult<L>,
    mut step: impl FnMut(
        &mut CallHost<'_, '_>,
        &mut T,
        L,
        crate::eval::host::ScalarOperand,
    ) -> FunctionResult<()>,
) -> FunctionResult<T> {
    let len = left.len(host)?;
    if right.len(host)? != len {
        return Err(mismatch.into());
    }
    host.check_values(len)?;
    host.charge_work((len as u64).saturating_mul(2))?;
    for offset in 0..len {
        let operand = left.read(host, offset)?;
        let admitted = admit_left(host, &mut state, operand)?;
        let operand = right.read(host, offset)?;
        step(host, &mut state, admitted, operand)?;
    }
    Ok(state)
}

/// Fold original source channels without traversing or scalarizing their members.
pub(in crate::eval::adapters) fn fold_sources<T>(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
    mut state: T,
    mut step: impl FnMut(
        &mut CallHost<'_, '_>,
        &mut T,
        crate::eval::host::HostOperand,
    ) -> FunctionResult<()>,
) -> FunctionResult<T> {
    for role in roles {
        let operand = host.aggregate_argument_once(role.argument())?;
        step(host, &mut state, operand)?;
    }
    Ok(state)
}
