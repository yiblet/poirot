//! Family-owned evaluation; generic argument coercion remains in the execution core.
use crate::*;
impl Session<'_> {
    /// Execute reference construction and selection while deferring cell dereferences.
    pub(crate) fn reference_function(
        &mut self,
        s: usize,
        at: CellAddress,
        name: ReferenceFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        if let Some(function) = name.helper() {
            if function.policy().volatile {
                self.read(DependencyRead::Volatile(FunctionId::Reference(name)));
            }
            return adapters::lookup::execute(self, s, at, function, args);
        }
        let n = args.len();
        Ok(match name {
            ReferenceFunction::Address
            | ReferenceFunction::Areas
            | ReferenceFunction::Hyperlink
            | ReferenceFunction::Indirect
            | ReferenceFunction::Offset
            | ReferenceFunction::Sheet
            | ReferenceFunction::Single => return Ok(err(VALUE_ERROR)),
            ReferenceFunction::Index => self.index(s, at, args)?,
            ReferenceFunction::Rows
            | ReferenceFunction::Columns
            | ReferenceFunction::Row
            | ReferenceFunction::Column => {
                if !FunctionId::Reference(name).descriptor().arity.accepts(n) {
                    return Ok(err(VALUE_ERROR));
                }
                if n == 0 {
                    return Ok(if name == ReferenceFunction::Row {
                        Number((at.row + 1).into()).into()
                    } else if name == ReferenceFunction::Column {
                        Number((at.col + 1).into()).into()
                    } else {
                        err(VALUE_ERROR)
                    });
                }
                let v = self.expr(s, at, args.get(0).ok_or(EvaluationFailure::StalePlan)?)?;
                match v {
                    Value::Refs(a) if a.len() == 1 => {
                        // SAFETY: The enclosing match selects only coordinate functions; each area access is guarded by areas.len() == 1.
                        let a = a[0];
                        Number(match name {
                            ReferenceFunction::Rows => (a.r1 - a.r0 + 1).into(),
                            ReferenceFunction::Columns => (a.c1 - a.c0 + 1).into(),
                            ReferenceFunction::Row => (a.r0 + 1).into(),
                            ReferenceFunction::Column => (a.c0 + 1).into(),
                            // SAFETY: The enclosing match selects only coordinate functions; each area access is guarded by areas.len() == 1.
                            ReferenceFunction::Index => unreachable!("INDEX has its own adapter"),
                            ReferenceFunction::Address
                            | ReferenceFunction::Areas
                            | ReferenceFunction::Hyperlink
                            | ReferenceFunction::Indirect
                            | ReferenceFunction::Offset
                            | ReferenceFunction::Sheet
                            | ReferenceFunction::Single => {
                                // SAFETY: The enclosing match selects only coordinate functions; each area access is guarded by areas.len() == 1.
                                unreachable!("typed helper handled before legacy selection")
                            }
                        })
                        .into()
                    }
                    Value::SignedRefs(areas) if areas.len() == 1 => {
                        // SAFETY: The enclosing match selects only coordinate functions; each area access is guarded by areas.len() == 1.
                        let area = areas[0];
                        Number(match name {
                            ReferenceFunction::Rows => f64::from(area.row_dimension().value()),
                            ReferenceFunction::Columns => {
                                f64::from(area.column_dimension().value())
                            }
                            ReferenceFunction::Row => f64::from(area.r0) + 1.,
                            ReferenceFunction::Column => f64::from(area.c0) + 1.,
                            // SAFETY: The enclosing match selects only coordinate functions; each area access is guarded by areas.len() == 1.
                            _ => unreachable!("only coordinate functions reach signed metadata"),
                        })
                        .into()
                    }
                    Value::Array(a)
                        if matches!(name, ReferenceFunction::Rows | ReferenceFunction::Columns) =>
                    {
                        Number(if name == ReferenceFunction::Rows {
                            a.len() as f64
                        } else {
                            a.first().map_or(0, Vec::len) as f64
                        })
                        .into()
                    }
                    Value::Scalar(Error(e)) => err(e),
                    _ => err(VALUE_ERROR),
                }
            }
        })
    }
    fn index_ordinal(
        &mut self,
        at: CellAddress,
        value: Value,
    ) -> EvalResult<Result<usize, CellError>> {
        let value = self.scalar(at, value)?;
        Ok(semantic(number(&value, &mut self.budget))?.and_then(index_ordinal))
    }
    fn index(&mut self, s: usize, at: CellAddress, args: ArgumentsRef<'_>) -> EvalResult<Value> {
        if !FunctionId::Reference(ReferenceFunction::Index)
            .descriptor()
            .arity
            .accepts(args.len())
        {
            return Ok(err(VALUE_ERROR));
        }
        // WorkbookEvaluator builds operands in source order. Index.java then
        // converts the first operand, followed by column and row coercion.
        // Keep expression evaluation separate so errors do not skip volatile
        // operands or let semantic errors hide later operational failures.
        let v = self
            .observed_value_argument(s, at, args.get(0).ok_or(EvaluationFailure::StalePlan)?, 0)?
            .1;
        let row_value = self
            .observed_value_argument(s, at, args.get(1).ok_or(EvaluationFailure::StalePlan)?, 1)?
            .1;
        let col_value = if args.len() > 2 {
            Some(
                self.observed_value_argument(
                    s,
                    at,
                    args.get(2).ok_or(EvaluationFailure::StalePlan)?,
                    2,
                )?
                .1,
            )
        } else {
            None
        };
        let area_value = if args.len() == 4 {
            Some(
                self.observed_value_argument(
                    s,
                    at,
                    args.get(3).ok_or(EvaluationFailure::StalePlan)?,
                    3,
                )?
                .1,
            )
        } else {
            None
        };
        if let Value::Scalar(Error(error)) = &v {
            return Ok(err(*error));
        }
        let col = match col_value {
            Some(value) => match self.index_ordinal(at, value)? {
                Ok(index) => index,
                Err(error) => return Ok(err(error)),
            },
            None => 1,
        };
        let row = match self.index_ordinal(at, row_value)? {
            Ok(index) => index,
            Err(error) => return Ok(err(error)),
        };
        if let Value::SignedRefs(areas) = &v {
            let area = if args.len() == 4 {
                match semantic({
                    let value = self.scalar(at, area_value.ok_or(EvaluationFailure::StalePlan)?)?;
                    number(&value, &mut self.budget)
                })? {
                    Ok(n) if n >= 1. && n.trunc() <= areas.len() as f64 => *areas
                        .get(
                            (n as usize)
                                .checked_sub(1)
                                .ok_or(EvaluationFailure::StalePlan)?,
                        )
                        .ok_or(EvaluationFailure::StalePlan)?,
                    Ok(_) => return Ok(err(Ref)),
                    Err(e) => return Ok(err(e)),
                }
            } else if areas.len() == 1 {
                // SAFETY: This branch checks areas.len() == 1 before selecting its first area.
                areas[0]
            } else {
                return Ok(err(VALUE_ERROR));
            };
            let (row, col) = match index_selection(
                IndexForm::from_arity(args.len()),
                area.height() as usize,
                area.width() as usize,
                row,
                col,
            ) {
                Ok(selection) => selection,
                Err(error) => return Ok(err(error)),
            };
            self.budget.allocation(1)?;
            return Ok(Value::SignedRefs(vec![area.select(row, col)]));
        }
        if let Value::Refs(areas) = v {
            let area = if args.len() == 4 {
                match semantic({
                    let value = self.scalar(at, area_value.ok_or(EvaluationFailure::StalePlan)?)?;
                    number(&value, &mut self.budget)
                })? {
                    Ok(n) if n >= 1. && n.trunc() <= areas.len() as f64 => *areas
                        .get(
                            (n as usize)
                                .checked_sub(1)
                                .ok_or(EvaluationFailure::StalePlan)?,
                        )
                        .ok_or(EvaluationFailure::StalePlan)?,
                    Ok(_) => return Ok(err(Ref)),
                    Err(e) => return Ok(err(e)),
                }
            } else if areas.len() == 1 {
                // SAFETY: This branch checks areas.len() == 1 before selecting its first area.
                areas[0]
            } else {
                return Ok(err(VALUE_ERROR));
            };
            let h = (area.r1 - area.r0 + 1) as usize;
            let w = (area.c1 - area.c0 + 1) as usize;
            let (row, col) =
                match index_selection(IndexForm::from_arity(args.len()), h, w, row, col) {
                    Ok(selection) => selection,
                    Err(error) => return Ok(err(error)),
                };
            self.budget.allocation(1)?;
            return Ok(Value::Refs(vec![Area {
                kind: AreaKind::Area,
                sheet: area.sheet,
                r0: if row == 0 {
                    area.r0
                } else {
                    area.r0 + row as u32 - 1
                },
                r1: if row == 0 {
                    area.r1
                } else {
                    area.r0 + row as u32 - 1
                },
                c0: if col == 0 {
                    area.c0
                } else {
                    area.c0 + col as u32 - 1
                },
                c1: if col == 0 {
                    area.c1
                } else {
                    area.c0 + col as u32 - 1
                },
            }]));
        }
        let anchor = match &v {
            Value::Array(view) => view.anchor,
            _ => None,
        };
        let a = match self.matrix(v)? {
            Ok(a) => a,
            Err(e) => return Ok(err(e)),
        };
        let h = a.len();
        let w = a.first().map_or(0, Vec::len);
        let (row, col) = match index_selection(IndexForm::from_arity(args.len()), h, w, row, col) {
            Ok(selection) => selection,
            Err(error) => return Ok(err(error)),
        };
        let anchor = anchor.and_then(|anchor| {
            Some(CellAddress {
                row: anchor.row.checked_add(row.saturating_sub(1) as u32)?,
                col: anchor.col.checked_add(col.saturating_sub(1) as u32)?,
            })
        });
        if row == 0 || col == 0 {
            self.budget
                .allocation((if row == 0 { h } else { 1 }).saturating_mul(if col == 0 {
                    w
                } else {
                    1
                }))?;
            let selected = a
                .into_iter()
                .enumerate()
                .filter(|(i, _)| row == 0 || *i == row - 1)
                .map(|(_, r)| {
                    r.into_iter()
                        .enumerate()
                        .filter(|(i, _)| col == 0 || *i == col - 1)
                        .map(|(_, v)| v)
                        .collect()
                })
                .collect();
            Ok(Value::Array(ArrayView::anchored(selected, anchor)))
        } else {
            if let Text(text) = a
                .get(row.checked_sub(1).ok_or(EvaluationFailure::StalePlan)?)
                .and_then(|values| col.checked_sub(1).and_then(|index| values.get(index)))
                .ok_or(EvaluationFailure::StalePlan)?
            {
                self.budget.text(text.len())?;
            }
            // Pinned Index.java returns a TwoDEval row/column view even for
            // a 1x1 selection. Scalar consumers still select its sole value.
            self.budget.allocation(1)?;
            Ok(Value::Array(ArrayView::anchored(
                vec![vec![
                    a.get(row.checked_sub(1).ok_or(EvaluationFailure::StalePlan)?)
                        .and_then(|values| col.checked_sub(1).and_then(|index| values.get(index)))
                        .ok_or(EvaluationFailure::StalePlan)?
                        .clone(),
                ]],
                anchor,
            )))
        }
    }
}

/// Pinned OperandResolver floors numeric indices, then rejects negatives.
fn index_ordinal(number: f64) -> Result<usize, CellError> {
    let number = number.floor();
    if !number.is_finite() || number < i32::MIN as f64 || number > i32::MAX as f64 {
        Err(Num)
    } else if number < 0. {
        Err(VALUE_ERROR)
    } else {
        Ok(number as usize)
    }
}

/// INDEX argument form controlling row/column defaults and vector selection.
#[derive(Clone, Copy)]
enum IndexForm {
    Vector,
    RowColumn,
}
impl IndexForm {
    fn from_arity(arity: usize) -> Self {
        if arity == 2 {
            Self::Vector
        } else {
            Self::RowColumn
        }
    }
}

/// Zero preserves the selected dimension; two-argument INDEX accepts vectors.
fn index_selection(
    form: IndexForm,
    height: usize,
    width: usize,
    row: usize,
    column: usize,
) -> Result<(usize, usize), CellError> {
    let (row, column) = if matches!(form, IndexForm::Vector) {
        if width == 1 {
            (row, 0)
        } else if height == 1 {
            (0, row)
        } else {
            return Err(Ref);
        }
    } else {
        (row, column)
    };
    if row > height || column > width {
        Err(Ref)
    } else {
        Ok((row, column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_evaluates_later_operands_before_returning_semantic_errors() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        for formula in [
            r#"INDEX(A1:B3,1/0,INDIRECT("[missing.xlsx]Sheet1!A1"))"#,
            r#"INDEX(1/0,1,INDIRECT("[missing.xlsx]Sheet1!A1"))"#,
        ] {
            let plan =
                bound::FormulaPlan::bind(&poirot_formula::parse_formula(formula).unwrap()).unwrap();
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            assert!(
                matches!(
                    session.expr(0, CellAddress { row: 0, col: 0 }, &plan.expression()),
                    Err(EvaluationFailure::UnsupportedCapability { .. })
                ),
                "later operational failure must survive: {formula}"
            );
        }
    }
    #[test]
    fn index_pure_selection_keeps_zero_dimensions_and_rejects_two_dimensional_vectors() {
        assert_eq!(index_selection(IndexForm::Vector, 3, 2, 2, 1), Err(Ref));
        assert_eq!(index_selection(IndexForm::Vector, 3, 1, 0, 1), Ok((0, 0)));
        assert_eq!(index_selection(IndexForm::Vector, 1, 3, 0, 1), Ok((0, 0)));
        assert_eq!(index_selection(IndexForm::Vector, 1, 3, 2, 1), Ok((0, 2)));
        assert_eq!(
            index_selection(IndexForm::RowColumn, 3, 2, 0, 2),
            Ok((0, 2))
        );
        assert_eq!(
            index_selection(IndexForm::RowColumn, 3, 2, 2, 0),
            Ok((2, 0))
        );
        assert_eq!(index_selection(IndexForm::RowColumn, 3, 2, 4, 0), Err(Ref));
        assert_eq!(index_ordinal(-0.1), Err(VALUE_ERROR));
        assert_eq!(index_ordinal(2.9), Ok(2));
        assert_eq!(index_ordinal(2147483648.), Err(Num));
    }
    #[test]
    fn index_keeps_reference_coordinates_and_whole_selected_areas() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        for (address, value) in [
            ("A1", 1.),
            ("B1", 10.),
            ("A2", 2.),
            ("B2", 20.),
            ("A3", 3.),
            ("B3", 30.),
        ] {
            workbook
                .sheet_mut(0)
                .unwrap()
                .set_value(address.parse().unwrap(), value)
                .unwrap();
        }
        let at = CellAddress { row: 0, col: 0 };
        for (formula, expected) in [
            ("INDEX(A1:B3,2,2)", Number(20.)),
            ("INDEX(A1:B3,2)", Error(Ref)),
            ("INDEX(A1:B3,0)", Error(Ref)),
            ("INDEX(A1:B3,-0.1)", Error(VALUE_ERROR)),
            ("INDEX(A1:B3,1/0,SQRT(-1))", Error(Num)),
            ("INDEX(A1:B3,-1,SQRT(-1))", Error(Num)),
            ("INDEX(1/0,-1,SQRT(-1))", Error(Div0)),
            ("INDEX(A1:B3,2147483648,1)", Error(Num)),
            ("INDEX(A1:A3,2)", Number(2.)),
            ("INDEX(A1:B1,2)", Number(10.)),
            ("SUM(INDEX(A1:A3,0))", Number(6.)),
            ("SUM(INDEX(A1:B1,0))", Number(11.)),
            ("SUM(INDEX(A1:B3,,2))", Number(60.)),
            ("SUM(INDEX(A1:B3,2,))", Number(22.)),
            ("ROW(INDEX(A1:B3,2,2))", Number(2.)),
            ("COLUMN(INDEX(A1:B3,2,2))", Number(2.)),
            ("SUM(INDEX(A1:B3,0,2))", Number(60.)),
            ("ROWS(INDEX(A1:B3,0,2))", Number(3.)),
            ("SUM(INDEX(A1:B3,2,0))", Number(22.)),
            ("INDEX({1,10;2,20},2,2)", Number(20.)),
            ("SUM(INDEX({1,10;2,20},0,2))", Number(30.)),
        ] {
            assert_eq!(
                match crate::run_plan(
                    &workbook,
                    0,
                    at,
                    &bound::FormulaPlan::bind(&poirot_formula::parse_formula(formula).unwrap())
                        .unwrap(),
                    Default::default(),
                    Default::default()
                ) {
                    crate::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                expected,
                "{formula}"
            );
        }
        let plan =
            bound::FormulaPlan::bind(&poirot_formula::parse_formula("INDEX(A1:B3,2,2)").unwrap())
                .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        assert!(
            matches!(session.expr(0,at,&plan.expression()).unwrap(),Value::Refs(areas) if areas.len()==1 && areas[0].r0==1 && areas[0].c0==1)
        );
    }
}
