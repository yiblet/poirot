//! Deferred reference-preserving execution of the pinned lookup family.
use crate::host::{CallHost, NumericPolicy};
use crate::*;
use poirot_functions_lookup as helper;

/// Lazy lookup bridge preserving reference results and deferred operational failures.
struct Port<'a, 'w> {
    session: &'a mut Session<'w>,
    sheet: usize,
    address: CellAddress,
    arguments: ArgumentsRef<'a>,
    failure: Option<EvaluationFailure>,
}
fn error(error: CellError) -> Result<helper::Error, helper::Failure> {
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
fn cell_error(error: helper::Error) -> CellError {
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
    fn operational<T>(&mut self, result: EvalResult<T>) -> Result<T, helper::Failure> {
        result.map_err(|failure| {
            self.failure = Some(failure);
            helper::Failure::Operational(helper::OperationalFailure::HostUnavailable)
        })
    }
    fn function<T>(&mut self, result: FunctionResult<T>) -> Result<T, helper::Failure> {
        match result {
            Ok(value) => Ok(value),
            Err(FunctionFailure::Spreadsheet(e)) => Err(helper::Failure::Spreadsheet(error(e)?)),
            Err(FunctionFailure::Operational(e)) => self.operational(Err(e)),
        }
    }
    fn scalar(&mut self, value: CellValue) -> Result<helper::Scalar, helper::Failure> {
        Ok(match value {
            Number(n) => helper::Scalar::Number(n),
            Text(s) => helper::Scalar::Text(s),
            CellValue::TextUtf16(text) => {
                let result = checked_text_view(
                    poirot_model::TextRef::Utf16(text.units()),
                    &mut self.session.budget,
                );
                helper::Scalar::Text(self.function(result)?.into_owned())
            }
            Boolean(b) => helper::Scalar::Bool(b),
            Blank => helper::Scalar::Blank,
            Error(e) => helper::Scalar::Error(error(e)?),
        })
    }
    fn convert(&mut self, value: Value) -> Result<helper::Value, helper::Failure> {
        Ok(match value {
            Value::SignedRefs(areas) => {
                self.session
                    .budget
                    .allocation(areas.len())
                    .map_err(|failure| {
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
            Value::SheetSpan(areas) => {
                let first = areas
                    .first()
                    .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
                // SAFETY: The single-reference branches check refs.len() == 1; the area branch requires first() to succeed before reading last(), and neither vector is shortened before access.
                let last = areas.last().unwrap();
                helper::Value::Reference(helper::Reference {
                    kind: match first.kind {
                        AreaKind::Cell => helper::ReferenceKind::Cell,
                        AreaKind::Area => helper::ReferenceKind::Area,
                    },
                    workbook: None,
                    first_sheet: first.sheet as u32,
                    last_sheet: last.sheet as u32,
                    first_row: first.r0,
                    last_row: first.r1,
                    first_col: first.c0,
                    last_col: first.c1,
                })
            }
            Value::Refs(areas) => {
                let result = self.session.budget.allocation(areas.len());
                self.operational(result)?;
                let refs: Vec<_> = areas
                    .into_iter()
                    .map(|a| helper::Reference {
                        kind: match a.kind {
                            AreaKind::Cell => helper::ReferenceKind::Cell,
                            AreaKind::Area => helper::ReferenceKind::Area,
                        },
                        workbook: None,
                        first_sheet: a.sheet as u32,
                        last_sheet: a.sheet as u32,
                        first_row: a.r0,
                        last_row: a.r1,
                        first_col: a.c0,
                        last_col: a.c1,
                    })
                    .collect();
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
                let result = self
                    .session
                    .budget
                    .allocation(rows.len().saturating_mul(columns));
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
    fn output(&mut self, value: helper::Value) -> EvalResult<Value> {
        fn scalar(value: helper::Scalar) -> CellValue {
            match value {
                helper::Scalar::Number(n) => Number(n),
                helper::Scalar::Text(s) => Text(s),
                helper::Scalar::Bool(b) => Boolean(b),
                helper::Scalar::Blank | helper::Scalar::Missing => Blank,
                helper::Scalar::Error(e) => Error(cell_error(e)),
            }
        }
        Ok(match value {
            helper::Value::Scalar(helper::Scalar::Missing) => Value::Missing,
            helper::Value::Scalar(v) => scalar(v).into(),
            helper::Value::Array(a) => {
                self.session.budget.allocation(a.values.len())?;
                let mut values = a.values.into_iter();
                let mut rows = Vec::with_capacity(a.rows);
                for _ in 0..a.rows {
                    rows.push(values.by_ref().take(a.cols).map(scalar).collect());
                }
                Value::Array(crate::ArrayView::unknown(rows))
            }
            helper::Value::Reference(r) => self.reference_output(vec![r])?,
            helper::Value::Union(r) => self.reference_output(r)?,
            helper::Value::SignedReference(r) => self.signed_output(vec![r])?,
            helper::Value::SignedUnion(r) => self.signed_output(r)?,
        })
    }
    fn signed_output(&mut self, refs: Vec<helper::SignedReference>) -> EvalResult<Value> {
        self.session.budget.allocation(refs.len())?;
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
    fn reference_output(&mut self, refs: Vec<helper::Reference>) -> EvalResult<Value> {
        self.session.budget.allocation(refs.len())?;
        if refs
            .iter()
            .any(|r| r.workbook.is_some() || r.first_sheet != r.last_sheet || !r.valid())
        {
            return Err(self
                .unsupported("lookup reference output requires unavailable external/3D identity"));
        }
        Ok(Value::Refs(
            refs.into_iter()
                .map(|r| Area {
                    kind: match r.kind {
                        helper::ReferenceKind::Cell => AreaKind::Cell,
                        helper::ReferenceKind::Area => AreaKind::Area,
                    },
                    sheet: r.first_sheet as usize,
                    r0: r.first_row,
                    r1: r.last_row,
                    c0: r.first_col,
                    c1: r.last_col,
                })
                .collect(),
        ))
    }
    fn unsupported(&self, detail: impl Into<String>) -> EvaluationFailure {
        EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::FunctionAdapter,
            source: Some(EvaluationTarget {
                sheet: self.sheet,
                address: self.address,
            }),
            detail: detail.into(),
        }
    }
}
impl helper::Workbook for Port<'_, '_> {
    fn preflight(&mut self, request: helper::BudgetRequest) -> Result<(), helper::Failure> {
        let result = match request.work {
            helper::Work::SearchText => self.session.budget.text(request.units),
            helper::Work::SearchSteps => self
                .session
                .budget
                .charge(WorkKind::HelperOperation, request.units as u64),
            _ => self.session.budget.allocation(request.units),
        };
        self.operational(result)
    }
    fn argument(
        &mut self,
        index: usize,
        _: helper::Context,
    ) -> Result<helper::Value, helper::Failure> {
        let expression = self
            .arguments
            .get(index)
            .ok_or(helper::Failure::Spreadsheet(helper::Error::Value))?;
        if expression.is_missing() {
            return Ok(helper::Value::Scalar(helper::Scalar::Missing));
        }
        let result = self.session.expr(self.sheet, self.address, expression);
        let value = self.operational(result)?;
        let mut value = self.convert(value)?;
        if let helper::Value::Array(array) = &mut value {
            let missing = expression.visit_array_missing(|index, missing| {
                if missing {
                    let value = array
                        .values
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
    fn numeric_text(&mut self, text: &str) -> Result<f64, helper::Failure> {
        let result = CallHost::new(self.session, self.sheet, self.address, ArgumentsRef::Empty)
            .coerce_number_text(text, NumericPolicy::PinnedPoi);
        self.function(result)
    }
    fn number_text(&mut self, number: f64) -> Result<String, helper::Failure> {
        let result = CallHost::new(self.session, self.sheet, self.address, ArgumentsRef::Empty)
            .number_text(number, NumericPolicy::PinnedPoi);
        self.function(result)
    }
    fn cell(
        &mut self,
        reference: &helper::Reference,
        sheet: u32,
        row: u32,
        col: u32,
    ) -> Result<helper::Scalar, helper::Failure> {
        if reference.workbook.is_some() {
            return Err(helper::Failure::Unsupported(
                helper::Unsupported::DynamicReference,
            ));
        }
        let result = self.session.cell(sheet as usize, CellAddress { row, col });
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
        let result = self.session.cell(
            sheet as usize,
            CellAddress {
                row: row as u32,
                col: col as u32,
            },
        );
        let value = self.operational(result)?;
        self.scalar(value)
    }
    fn sheet_index(&mut self, name: &str) -> Result<Option<u32>, helper::Failure> {
        let result = self.session.budget.text(name.len());
        self.operational(result)?;
        let result = self.session.sheet(self.sheet, &Some(name.to_owned()));
        self.operational(result).map(|v| v.map(|s| s as u32))
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
        let result = self.session.budget.text(bytes);
        self.operational(result)?;
        let parse_policy = self
            .session
            .plan
            .map_or_else(poirot_formula::FormulaParsePolicy::default, |plan| {
                plan.policy().parse
            });
        let mut endpoint = |text: &str| -> Result<poirot_formula::Expr, helper::Failure> {
            let text = if query.a1 {
                text.to_owned()
            } else {
                r1c1(text, context)?
            };
            self.session.run_usage.dynamic_parse_attempts += 1;
            let syntax = if text.contains(':') {
                match poirot_formula::parse_formula_with_policy(&text, parse_policy)
                    .map(poirot_formula::ParsedFormula::into_expression)
                    .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?
                {
                    poirot_formula::Expr::Binary {
                        op: BinaryOp::Range,
                        left,
                        ..
                    } => *left,
                    _ => return Err(helper::Failure::Spreadsheet(helper::Error::Ref)),
                }
            } else if query.a1 && query.last.is_some() {
                match poirot_formula::parse_formula_with_policy(
                    &format!("{text}:{text}"),
                    parse_policy,
                )
                .map(poirot_formula::ParsedFormula::into_expression)
                .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?
                {
                    poirot_formula::Expr::Binary {
                        op: BinaryOp::Range,
                        left,
                        ..
                    } => *left,
                    _ => return Err(helper::Failure::Spreadsheet(helper::Error::Ref)),
                }
            } else {
                poirot_formula::parse_formula_with_policy(&text, parse_policy)
                    .map(poirot_formula::ParsedFormula::into_expression)
                    .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?
            };
            if !matches!(
                syntax,
                poirot_formula::Expr::Cell(_)
                    | poirot_formula::Expr::Column { .. }
                    | poirot_formula::Expr::Row { .. }
                    | poirot_formula::Expr::Name(_)
                    | poirot_formula::Expr::QualifiedName { .. }
            ) {
                return Err(helper::Failure::Spreadsheet(helper::Error::Ref));
            }
            Ok(syntax)
        };
        let first = endpoint(&query.first)?;
        let syntax = if let Some(last) = &query.last {
            poirot_formula::Expr::Binary {
                op: BinaryOp::Range,
                left: Box::new(first),
                right: Box::new(endpoint(last)?),
            }
        } else {
            first
        };
        let program = crate::bytecode::compile_with_registry(&syntax, self.session.extensions)
            .map_err(|_| helper::Failure::Spreadsheet(helper::Error::Ref))?;
        let result = self
            .session
            .bytecode_expr(sheet as usize, self.address, &program);
        let value = self.operational(result)?;
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
fn r1c1(text: &str, context: helper::Context) -> Result<String, helper::Failure> {
    fn coordinate(text: &str, current: u32) -> Option<u32> {
        if text.is_empty() {
            Some(current)
        } else if let Some(relative) = text.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let n = relative.parse::<i64>().ok()?;
            u32::try_from(i64::from(current) + n).ok()
        } else {
            text.parse::<u32>().ok()?.checked_sub(1)
        }
    }
    let upper = text.to_ascii_uppercase();
    if !upper.contains('C')
        && let Some(body) = upper.strip_prefix('R')
    {
        if body.is_empty() {
            return Err(helper::Failure::Spreadsheet(helper::Error::Ref));
        }
        let row = coordinate(body, context.row)
            .filter(|n| *n < MAX_ROWS)
            .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?
            + 1;
        return Ok(format!("{row}:{row}"));
    }
    if let Some(body) = upper.strip_prefix('C') {
        if body.is_empty() {
            return Err(helper::Failure::Spreadsheet(helper::Error::Ref));
        }
        let col = coordinate(body, context.col)
            .filter(|n| *n < MAX_COLUMNS)
            .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
        let mut letters = String::new();
        let mut n = col + 1;
        while n > 0 {
            n -= 1;
            // SAFETY: Zero is always an in-bounds UTF-8 boundary, including for an empty string.
            letters.insert(0, (b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        return Ok(format!("{letters}:{letters}"));
    }
    let body = upper
        .strip_prefix('R')
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let (row, col) = body
        .split_once('C')
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let row = coordinate(row, context.row)
        .filter(|n| *n < MAX_ROWS)
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let col = coordinate(col, context.col)
        .filter(|n| *n < MAX_COLUMNS)
        .ok_or(helper::Failure::Spreadsheet(helper::Error::Ref))?;
    let mut letters = String::new();
    let mut n = col + 1;
    while n > 0 {
        n -= 1;
        // SAFETY: Zero is always an in-bounds UTF-8 boundary, including for an empty string.
        letters.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    Ok(format!("{letters}{}", row + 1))
}
/// Execute lookup helpers while retaining lazy arguments and reference-valued results.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: helper::Function,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let policy = if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
        helper::SemanticsPolicy::ExcelDocumentation
    } else {
        helper::SemanticsPolicy::PinnedPoi
    };
    let mut port = Port {
        session,
        sheet,
        address,
        arguments,
        failure: None,
    };
    let context = helper::Context {
        sheet: sheet as u32,
        row: address.row,
        col: address.col,
    };
    match helper::CallPlan::prepare(function, arguments.len())
        .and_then(|plan| plan.execute_with_policy(context, &mut port, policy))
    {
        Ok(value) => port.output(value),
        Err(helper::Failure::Spreadsheet(e)) => Ok(err(cell_error(e))),
        Err(failure) => Err(port
            .failure
            .take()
            .unwrap_or_else(|| port.unsupported(format!("pinned lookup boundary: {failure:?}")))),
    }
}
