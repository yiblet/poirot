use super::dispatch::r1c1;
use crate::eval::host::{
    ArgumentCoercion, CallHost, DynamicReferences, NumericPolicy, SelectionAccess,
};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::values::{
    Area, AreaKind, FunctionFailure, FunctionResult, Value, convert_legacy_missing_blank,
    signed_reference,
};
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::lookup as helper;
use poirot_parsing::formula::BinaryOp;

/// Connects workbook-independent lookup algorithms to the evaluator's lazy arguments and reads.
/// It translates reference results and retains evaluator failures that the lookup error type cannot carry.
pub(super) struct Port<'a, 'w> {
    pub(super) host: CallHost<'a, 'w>,
    pub(super) failure: Option<EvaluationFailure>,
}

pub(super) fn error(error: CellError) -> Result<helper::Error, helper::Failure> {
    Ok(match error {
        CellError::Value => helper::Error::Value,
        CellError::Ref => helper::Error::Ref,
        CellError::NA => helper::Error::Na,
        CellError::Num => helper::Error::Num,
        CellError::Div0 => helper::Error::Div0,
        CellError::Name => helper::Error::Name,
        CellError::Null => helper::Error::Null,
        _ => {
            return Err(helper::Failure::Unsupported(
                helper::Unsupported::DeferredArguments,
            ));
        }
    })
}

pub(super) fn cell_error(error: helper::Error) -> CellError {
    match error {
        helper::Error::Value => CellError::Value,
        helper::Error::Ref => CellError::Ref,
        helper::Error::Na => CellError::NA,
        helper::Error::Num => CellError::Num,
        helper::Error::Div0 => CellError::Div0,
        helper::Error::Name => CellError::Name,
        helper::Error::Null => CellError::Null,
    }
}

impl Port<'_, '_> {
    /// Resolve local sheet text while recording namespace dependencies for INDIRECT.
    pub(super) fn sheet_index(&mut self, name: &str) -> Result<Option<u32>, helper::Failure> {
        let result = self.host.check_text(name.len());
        self.operational(result)?;
        let mut owned = String::new();
        let result = owned
            .try_reserve_exact(name.len())
            .map_err(|_| self.host.allocation_failure());
        self.operational(result)?;
        // SAFETY: name.len() bytes were reserved immediately above for this empty string.
        owned.push_str(name);
        let result = DynamicReferences::new(&mut self.host).resolve_sheet(owned);
        self.operational(result).map(|v| v.map(|s| s as u32))
    }

    pub(super) fn operational<T>(&mut self, result: EvalResult<T>) -> Result<T, helper::Failure> {
        result.map_err(|failure| {
            self.failure = Some(failure);
            helper::Failure::Operational(helper::OperationalFailure::HostUnavailable)
        })
    }

    pub(super) fn function<T>(&mut self, result: FunctionResult<T>) -> Result<T, helper::Failure> {
        match result {
            Ok(value) => Ok(value),
            Err(FunctionFailure::Spreadsheet(e)) => Err(helper::Failure::Spreadsheet(error(e)?)),
            Err(FunctionFailure::Operational(e)) => self.operational(Err(e)),
        }
    }

    pub(super) fn scalar(&mut self, value: CellValue) -> Result<helper::Scalar, helper::Failure> {
        Ok(match value {
            Number(n) => helper::Scalar::Number(n),
            Text(s) => helper::Scalar::Text(s),
            CellValue::TextUtf16(text) => {
                let result = ArgumentCoercion::new(&mut self.host)
                    .scalar_text_view(crate::model::TextRef::Utf16(text.units()));
                helper::Scalar::Text(self.function(result)?.into_owned())
            }
            Boolean(b) => helper::Scalar::Bool(b),
            Blank => helper::Scalar::Blank,
            Error(e) => helper::Scalar::Error(error(e)?),
        })
    }

    pub(super) fn convert(&mut self, value: Value) -> Result<helper::Value, helper::Failure> {
        Ok(match value {
            Value::SignedRefs(areas) => {
                self.host.check_values(areas.len()).map_err(|failure| {
                    self.failure = Some(failure);
                    helper::Failure::Operational(helper::OperationalFailure::HostUnavailable)
                })?;
                let mut refs = Vec::with_capacity(areas.len());
                for area in areas {
                    refs.push(helper::SignedReference {
                        kind: match area.kind {
                            AreaKind::Cell => helper::ReferenceKind::Cell,
                            AreaKind::Area => helper::ReferenceKind::Area,
                        },
                        workbook: None,
                        first_sheet: area.sheet as u32,
                        last_sheet: area.sheet as u32,
                        first_row: area.r0,
                        last_row: area.r1,
                        first_col: area.c0,
                        last_col: area.c1,
                    });
                }
                if refs.len() == 1 {
                    // SAFETY: The single-reference branches check refs.len() == 1; the area branch requires first() to succeed before reading last(), and neither vector is shortened before access.
                    helper::Value::SignedReference(refs.pop().unwrap())
                } else {
                    helper::Value::SignedUnion(refs)
                }
            }
            Value::Missing => helper::Value::Scalar(helper::Scalar::Missing),
            Value::Scalar(v) => helper::Value::Scalar(self.scalar(v)?),
            Value::SelectedScalar(v, origin) => helper::Value::SelectedScalar(
                self.scalar(v)?,
                match origin {
                    crate::eval::values::OperandOrigin::Direct => {
                        poirot_functions::coercion::Origin::Direct
                    }
                    crate::eval::values::OperandOrigin::ReferencedCell => {
                        poirot_functions::coercion::Origin::Reference
                    }
                    crate::eval::values::OperandOrigin::ArrayElement => {
                        poirot_functions::coercion::Origin::Array
                    }
                },
            ),
            Value::SheetSpan(areas) => {
                let first = areas
                    .first()
                    .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
                // SAFETY: The single-reference branches check refs.len() == 1; the area branch requires first() to succeed before reading last(), and neither vector is shortened before access.
                let last = areas.last().unwrap();
                helper::Value::Reference(helper::Reference::new(
                    match first.kind {
                        AreaKind::Cell => helper::ReferenceKind::Cell,
                        AreaKind::Area => helper::ReferenceKind::Area,
                    },
                    None,
                    first.sheet as u32..=last.sheet as u32,
                    first.r0..=first.r1,
                    first.c0..=first.c1,
                )?)
            }
            Value::Refs(areas) => {
                let result = self.host.check_values(areas.len());
                self.operational(result)?;
                let refs: Vec<_> = areas
                    .into_iter()
                    .map(|a| {
                        helper::Reference::new(
                            match a.kind {
                                AreaKind::Cell => helper::ReferenceKind::Cell,
                                AreaKind::Area => helper::ReferenceKind::Area,
                            },
                            None,
                            a.sheet as u32..=a.sheet as u32,
                            a.r0..=a.r1,
                            a.c0..=a.c1,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if refs.len() == 1 {
                    // SAFETY: The single-reference branches check refs.len() == 1; the area branch requires first() to succeed before reading last(), and neither vector is shortened before access.
                    helper::Value::Reference(refs.into_iter().next().unwrap())
                } else {
                    helper::Value::Union(refs)
                }
            }
            Value::Array(rows) => {
                let columns = rows.first().map_or(0, Vec::len);
                if rows.iter().any(|r| r.len() != columns) {
                    return Err(helper::Failure::Spreadsheet(helper::Error::Value));
                }
                let result = self.host.check_values(rows.len().saturating_mul(columns));
                self.operational(result)?;
                let count = rows.len();
                let values = rows
                    .into_iter()
                    .flatten()
                    .map(|v| self.scalar(v))
                    .collect::<Result<Vec<_>, _>>()?;
                helper::Value::Array(helper::Array::new(count, columns, values)?)
            }
        })
    }

    pub(super) fn output(&mut self, value: helper::Value) -> EvalResult<Value> {
        fn scalar(value: helper::Scalar) -> CellValue {
            match value {
                helper::Scalar::Number(n) => Number(n),
                helper::Scalar::Text(s) => Text(s),
                helper::Scalar::Bool(b) => Boolean(b),
                helper::Scalar::Blank => Blank,
                // TODO(coercion-non-argument): [MEDIUM] Missing members in lookup result arrays become blank and can lose omission-sensitive behavior.
                helper::Scalar::Missing => convert_legacy_missing_blank(),
                helper::Scalar::Error(e) => Error(cell_error(e)),
            }
        }
        Ok(match value {
            helper::Value::Scalar(helper::Scalar::Missing)
            | helper::Value::SelectedScalar(helper::Scalar::Missing, _) => Value::Missing,
            helper::Value::Scalar(v) => scalar(v).into(),
            helper::Value::SelectedScalar(v, origin) => Value::SelectedScalar(
                scalar(v),
                match origin {
                    poirot_functions::coercion::Origin::Direct => {
                        crate::eval::values::OperandOrigin::Direct
                    }
                    poirot_functions::coercion::Origin::Reference => {
                        crate::eval::values::OperandOrigin::ReferencedCell
                    }
                    poirot_functions::coercion::Origin::Array => {
                        crate::eval::values::OperandOrigin::ArrayElement
                    }
                },
            ),
            helper::Value::Array(a) => {
                self.host.check_values(a.values().len())?;
                let (row_count, columns) = (a.rows(), a.cols());
                let mut values = a.into_values().into_iter();
                let mut rows = Vec::with_capacity(row_count);
                for _ in 0..row_count {
                    rows.push(values.by_ref().take(columns).map(scalar).collect());
                }
                Value::Array(crate::eval::values::ArrayView::unknown(rows))
            }
            helper::Value::Reference(r) => self.reference_output(vec![r])?,
            helper::Value::Union(r) => self.reference_output(r)?,
            helper::Value::SignedReference(r) => self.signed_output(vec![r])?,
            helper::Value::SignedUnion(r) => self.signed_output(r)?,
        })
    }

    pub(super) fn signed_output(
        &mut self,
        refs: Vec<helper::SignedReference>,
    ) -> EvalResult<Value> {
        self.host.check_values(refs.len())?;
        if refs
            .iter()
            .any(|r| !r.valid() || r.workbook.is_some() || r.first_sheet != r.last_sheet)
        {
            return Err(
                self.unsupported("signed OFFSET requires a valid local single-sheet identity")
            );
        }
        Ok(Value::SignedRefs(
            refs.into_iter()
                .map(|r| signed_reference::SignedArea {
                    kind: AreaKind::Area,
                    sheet: r.first_sheet as usize,
                    r0: r.first_row,
                    r1: r.last_row,
                    c0: r.first_col,
                    c1: r.last_col,
                })
                .collect(),
        ))
    }

    pub(super) fn reference_output(&mut self, refs: Vec<helper::Reference>) -> EvalResult<Value> {
        self.host.check_values(refs.len())?;
        if refs
            .iter()
            .any(|r| r.workbook().is_some() || r.first_sheet() != r.last_sheet())
        {
            return Err(self
                .unsupported("lookup reference output requires unavailable external/3D identity"));
        }
        Ok(Value::Refs(
            refs.into_iter()
                .map(|r| Area {
                    kind: match r.kind() {
                        helper::ReferenceKind::Cell => AreaKind::Cell,
                        helper::ReferenceKind::Area => AreaKind::Area,
                    },
                    sheet: r.first_sheet() as usize,
                    r0: r.first_row(),
                    r1: r.last_row(),
                    c0: r.first_col(),
                    c1: r.last_col(),
                })
                .collect(),
        ))
    }

    pub(super) fn unsupported(&self, detail: impl Into<String>) -> EvaluationFailure {
        self.host
            .capability_failure(CapabilityId::FunctionAdapter, detail)
    }
}

impl helper::Workbook for Port<'_, '_> {
    fn preflight(&mut self, request: helper::BudgetRequest) -> Result<(), helper::Failure> {
        let result = match request.work {
            helper::Work::SearchText => self.host.check_text(request.units),
            helper::Work::SearchSteps => self.host.charge_work(request.units as u64),
            _ => self.host.check_values(request.units),
        };
        self.operational(result)
    }

    fn argument(
        &mut self,
        index: usize,
        _: helper::Context,
    ) -> Result<helper::Value, helper::Failure> {
        if index >= self.host.len() {
            return Err(helper::Failure::Spreadsheet(helper::Error::Value));
        }
        if SelectionAccess::new(&mut self.host).is_missing(index) {
            return Ok(helper::Value::Scalar(helper::Scalar::Missing));
        }
        let result = self.host.raw_argument(index);
        let value = self.operational(result)?;
        let mut value = self.convert(value)?;
        if let helper::Value::Array(array) = &mut value {
            let missing = self
                .host
                .visit_argument_array_missing(index, |index, missing| {
                    if missing {
                        let value = array
                            .values_mut()
                            .get_mut(index)
                            .ok_or(EvaluationFailure::StalePlan)?;
                        *value = helper::Scalar::Missing;
                    }
                    Ok(())
                });
            self.operational(missing)?;
        }
        Ok(value)
    }

    fn coerce_numeric_text(&mut self, text: &str) -> Result<f64, helper::Failure> {
        let result = ArgumentCoercion::new(&mut self.host)
            .coerce_number_text(text, NumericPolicy::PinnedPoi);
        self.function(result)
    }

    fn coerce_number_to_text(&mut self, number: f64) -> Result<String, helper::Failure> {
        let result = ArgumentCoercion::new(&mut self.host)
            .coerce_number_to_text(number, NumericPolicy::PinnedPoi);
        self.function(result)
    }

    fn cell(
        &mut self,
        reference: &helper::Reference,
        sheet: u32,
        row: u32,
        col: u32,
    ) -> Result<helper::Scalar, helper::Failure> {
        if reference.workbook().is_some() {
            return Err(helper::Failure::Unsupported(
                helper::Unsupported::DynamicReference,
            ));
        }
        let result = DynamicReferences::new(&mut self.host)
            .sheet_cell(sheet as usize, CellAddress { row, col });
        let value = self.operational(result)?;
        self.scalar(value)
    }

    fn signed_cell(
        &mut self,
        reference: &helper::SignedReference,
        sheet: u32,
        row: i32,
        col: i32,
    ) -> Result<helper::Scalar, helper::Failure> {
        if reference.workbook.is_some() {
            return Err(helper::Failure::Unsupported(
                helper::Unsupported::DynamicReference,
            ));
        }
        if !(0..1_048_576).contains(&row) || !(0..16_384).contains(&col) {
            return Ok(helper::Scalar::Blank);
        }
        let result = DynamicReferences::new(&mut self.host).sheet_cell(
            sheet as usize,
            CellAddress {
                row: row as u32,
                col: col as u32,
            },
        );
        let value = self.operational(result)?;
        self.scalar(value)
    }

    fn dynamic_reference(
        &mut self,
        query: &helper::DynamicReference,
        context: helper::Context,
    ) -> Result<helper::Value, helper::Failure> {
        if query.workbook.is_some() {
            return Err(helper::Failure::Unsupported(
                helper::Unsupported::DynamicReference,
            ));
        }
        let sheet = match &query.sheet {
            Some(name) => self
                .sheet_index(name)?
                .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?,
            None => context.sheet,
        };
        let bytes = query
            .first
            .len()
            .checked_add(query.last.as_ref().map_or(0, String::len))
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(64))
            .ok_or(helper::Failure::Operational(
                helper::OperationalFailure::ResourceOverflow,
            ))?;
        let result = self.host.check_text(bytes);
        self.operational(result)?;
        let parse_policy = DynamicReferences::new(&mut self.host).parse_policy();
        let mut endpoint = |text: &str| -> Result<poirot_parsing::formula::Expr, helper::Failure> {
            let text = if query.a1 {
                text.to_owned()
            } else {
                r1c1(text, context)?
            };
            DynamicReferences::new(&mut self.host).record_dynamic_parse();
            let syntax = if text.contains(':') {
                match poirot_parsing::formula::parse_formula_with_policy(&text, parse_policy)
                    .map(poirot_parsing::formula::ParsedFormula::into_expression)
                    .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?
                {
                    poirot_parsing::formula::Expr::Binary {
                        op: BinaryOp::Range,
                        left,
                        ..
                    } => *left,
                    _ => return Err(helper::Failure::Spreadsheet(helper::Error::Ref)),
                }
            } else if query.a1 && query.last.is_some() {
                match poirot_parsing::formula::parse_formula_with_policy(
                    &format!("{text}:{text}"),
                    parse_policy,
                )
                .map(poirot_parsing::formula::ParsedFormula::into_expression)
                .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?
                {
                    poirot_parsing::formula::Expr::Binary {
                        op: BinaryOp::Range,
                        left,
                        ..
                    } => *left,
                    _ => return Err(helper::Failure::Spreadsheet(helper::Error::Ref)),
                }
            } else {
                poirot_parsing::formula::parse_formula_with_policy(&text, parse_policy)
                    .map(poirot_parsing::formula::ParsedFormula::into_expression)
                    .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?
            };
            if !matches!(
                syntax,
                poirot_parsing::formula::Expr::Cell(_)
                    | poirot_parsing::formula::Expr::Column { .. }
                    | poirot_parsing::formula::Expr::Row { .. }
                    | poirot_parsing::formula::Expr::Name(_)
                    | poirot_parsing::formula::Expr::QualifiedName { .. }
            ) {
                return Err(helper::Failure::Spreadsheet(helper::Error::Ref));
            }
            Ok(syntax)
        };
        let first = endpoint(&query.first)?;
        let syntax = if let Some(last) = &query.last {
            poirot_parsing::formula::Expr::Binary {
                op: BinaryOp::Range,
                left: Box::new(first),
                right: Box::new(endpoint(last)?),
            }
        } else {
            first
        };
        let result =
            DynamicReferences::new(&mut self.host).dynamic_reference(sheet as usize, &syntax);
        let value = self.function(result)?;
        match value {
            Value::Refs(_) | Value::SheetSpan(_) | Value::SignedRefs(_) => self.convert(value),
            _ => Err(helper::Failure::Spreadsheet(helper::Error::Ref)),
        }
    }

    fn structured_reference(
        &mut self,
        _: &str,
        _: helper::Context,
    ) -> Result<helper::Value, helper::Failure> {
        Err(helper::Failure::Unsupported(
            helper::Unsupported::DynamicReference,
        ))
    }
}
