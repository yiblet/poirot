//! Pinned matrix adapter over deferred, memoized Session operands.
use crate::adapters::failure::{operational_only, project_host};
use crate::adapters::matrix::policy::{
    self as helper, Cell, EvaluationPort, Failure, Origin, Shape,
};
use crate::host::{CallHost, NumericPolicy, OperandKind};
use crate::*;

fn port_failure(failure: FunctionFailure) -> Failure<EvaluationFailure> {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}
/// Matrix host bridge exposing lazy shape inspection and individual member reads.
struct MatrixPort<'call, 'workbook> {
    host: CallHost<'call, 'workbook>,
    strict_numeric_matrices: bool,
}
impl EvaluationPort for MatrixPort<'_, '_> {
    /// Deferred argument identity supplied to the family's host port.
    type Argument = usize;
    /// Host abort channel kept outside catchable spreadsheet values.
    type Operational = EvaluationFailure;
    fn strict_numeric_matrices(&self) -> bool {
        self.strict_numeric_matrices
    }
    fn determined_trend_fit(&self) -> bool {
        self.strict_numeric_matrices
    }
    fn describe(&mut self, index: &usize) -> Result<Shape, Failure<EvaluationFailure>> {
        let description = self.host.describe(*index).map_err(port_failure)?;
        Ok(Shape {
            rows: description.rows,
            cols: description.columns,
            origin: match description.kind {
                OperandKind::Array => Origin::Array,
                // AreaKind is retained by the host even for A1:A1. Geometry
                // alone cannot distinguish POI AreaEval from scalar RefEval.
                OperandKind::Reference if description.is_area => Origin::Array,
                OperandKind::Reference => Origin::Reference,
                OperandKind::Scalar | OperandKind::Missing => Origin::Direct,
            },
            sheets: description.sheets,
            missing: description.missing,
        })
    }
    fn read_cell(
        &mut self,
        index: &usize,
        row: usize,
        col: usize,
    ) -> Result<Cell, Failure<EvaluationFailure>> {
        let cell = self
            .host
            .read_cell(*index, row, col)
            .map_err(port_failure)?;
        if cell.missing {
            return Ok(Cell::Missing);
        }
        Ok(match cell.value {
            Number(n) => Cell::Number(n),
            Boolean(b) => Cell::Bool(b),
            Text(s) => Cell::Text(s),
            CellValue::TextUtf16(text) => Cell::Text(
                self.host
                    .project_text(poirot_model::TextRef::Utf16(text.units()))
                    .map_err(port_failure)?,
            ),
            Blank => Cell::Blank,
            Error(e) => Cell::Error(e),
        })
    }
    fn charge_work(&mut self, units: u64) -> Result<(), EvaluationFailure> {
        self.host.charge_work(units).map_err(operational_only)
    }
    fn check_collection(&mut self, elements: usize) -> Result<(), EvaluationFailure> {
        self.host.check_values(elements).map_err(operational_only)
    }
    fn check_text(&mut self, bytes: usize) -> Result<(), EvaluationFailure> {
        self.host.check_text(bytes).map_err(operational_only)
    }
    fn coercev2_number_text(&mut self, text: &str) -> Result<f64, Failure<EvaluationFailure>> {
        self.host
            .coercev2_number_text(text, NumericPolicy::PinnedPoi)
            .map_err(port_failure)
    }
    fn budget_exceeded(&mut self) -> EvaluationFailure {
        self.host.budget_exceeded()
    }
}

/// Execute matrix helpers with admitted resources and source-preserving result shape.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: helper::MatrixFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    // Check arity before allocating argument handles or evaluating expressions.
    if !function.accepts(args.len()) {
        return Ok(err(VALUE_ERROR));
    }
    if function == helper::MatrixFunction::Transpose
        && session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
    {
        let mut host = CallHost::new_array(session, sheet, address, args, args.len());
        let shape = match host.describe(0) {
            Ok(shape) => shape,
            Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
            Err(FunctionFailure::Operational(error)) => return Err(error),
        };
        if shape.sheets != 1 || shape.missing {
            return Ok(err(VALUE_ERROR));
        }
        let count = shape
            .rows
            .checked_mul(shape.columns)
            .ok_or_else(|| host.budget_exceeded())?;
        host.check_values(count).map_err(operational_only)?;
        host.check_values(shape.columns).map_err(operational_only)?;
        host.charge_work(count as u64).map_err(operational_only)?;
        let mut rows = vec![Vec::with_capacity(shape.rows); shape.columns];
        for row in 0..shape.rows {
            for (col, output) in rows.iter_mut().enumerate() {
                let value = match host.read_cell(0, row, col) {
                    Ok(cell) => cell.value,
                    Err(FunctionFailure::Spreadsheet(error)) => return Ok(err(error)),
                    Err(FunctionFailure::Operational(error)) => return Err(error),
                };
                output.push(value);
            }
        }
        return Ok(Value::Array(ArrayView::anchored(rows, Some(address))));
    }
    let array_count = if function == helper::MatrixFunction::Trend {
        3
    } else {
        args.len()
    };
    let strict_numeric_matrices =
        session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation;
    let mut port = MatrixPort {
        strict_numeric_matrices,
        host: CallHost::new_array(session, sheet, address, args, array_count),
    };
    port.host
        .check_values(args.len())
        .map_err(operational_only)?;
    let indices: poirot_functions_core::ParamVec<usize> = (0..args.len()).collect();
    match helper::evaluate(&mut port, function, &indices) {
        Ok(helper::Output::Number(n)) => Ok(Number(n).into()),
        Ok(helper::Output::Array(matrix)) => {
            // MatrixFunction.OneArrayArg preserves its area's coordinates;
            // TwoArrayArg uses the first area operand. Frequency and Trend
            // explicitly construct CacheAreaEval at the formula coordinates.
            let anchor = match function {
                helper::MatrixFunction::Frequency | helper::MatrixFunction::Trend => Some(address),
                helper::MatrixFunction::Transpose | helper::MatrixFunction::Minverse => {
                    port.host.operand_anchor(0).map_err(operational_only)?
                }
                helper::MatrixFunction::Mmult => {
                    let index = if port.host.is_area(0).map_err(operational_only)? {
                        0
                    } else {
                        1
                    };
                    port.host.operand_anchor(index).map_err(operational_only)?
                }
                helper::MatrixFunction::Mdeterm | helper::MatrixFunction::Sumproduct => {
                    return Err(EvaluationFailure::StalePlan);
                }
            };
            // Reserve both rows and values before growing the engine result.
            port.host
                .check_values(matrix.values().len())
                .map_err(operational_only)?;
            port.host
                .check_values(matrix.rows())
                .map_err(operational_only)?;
            port.host
                .charge_work(matrix.values().len() as u64)
                .map_err(operational_only)?;
            if matrix.cols() == 0 {
                return Err(EvaluationFailure::StalePlan);
            }
            Ok(Value::Array(crate::ArrayView::anchored(
                // SAFETY: The local guard rejects a zero column count.
                matrix
                    .values()
                    .chunks(matrix.cols())
                    .map(|row| row.iter().copied().map(Number).collect())
                    .collect(),
                anchor,
            )))
        }
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(error)) => Err(error),
        Err(Failure::UnsupportedCapability(capability)) => Err(port.host.capability_failure(
            CapabilityId::FunctionAdapter,
            format!("pinned matrix source capability: {capability:?}"),
        )),
    }
}

#[cfg(test)]
mod lossless_port_tests {
    use super::*;

    fn evaluate(
        formula: &str,
        literal: CellValue,
        budgets: EvaluationBudgets,
    ) -> EvalResult<CellValue> {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), literal)
            .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.budget = BudgetMeter::new(budgets).unwrap();
        let at = "B1".parse().unwrap();
        let expression = crate::bound::bind(&poirot_formula::parse_formula(formula).unwrap());
        let value = session.expr(0, at, &expression)?;
        session.scalar(at, value)
    }

    #[test]
    fn consumed_helper_text_preserves_operational_projection_failure() {
        for formula in [
            "IFERROR(PV(A1,1,1),7)",
            "IFERROR(NORMDIST(A1,0,1,TRUE),7)",
            "IFERROR(MATCH(A1,A1:A1,0),7)",
            "IFERROR(MDETERM(A1:A1),7)",
        ] {
            let literal = CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d]));
            assert!(
                matches!(
                    evaluate(formula, literal, EvaluationBudgets::default()),
                    Err(EvaluationFailure::UnsupportedCapability { .. })
                ),
                "{formula}"
            );
        }
    }

    #[test]
    fn valid_helper_text_projection_matches_utf8_and_obeys_budget() {
        for formula in ["PV(A1,1,1)", "NORMDIST(A1,0,1,TRUE)", "MATCH(A1,A1:A1,0)"] {
            let plain = evaluate(formula, Text("12".into()), EvaluationBudgets::default());
            let projected = evaluate(
                formula,
                CellValue::TextUtf16(poirot_model::ExcelText::from("12")),
                EvaluationBudgets::default(),
            );
            assert_eq!(plain, projected, "{formula}");
        }
        let budgets = EvaluationBudgets {
            text_bytes: 1,
            ..EvaluationBudgets::default()
        };
        assert!(matches!(
            evaluate(
                "IFERROR(PV(A1,1,1),7)",
                CellValue::TextUtf16(poirot_model::ExcelText::from("12")),
                budgets,
            ),
            Err(EvaluationFailure::Budget(_))
        ));
        assert_eq!(
            evaluate(
                "CHOOSE(1,7,A1)",
                CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d])),
                EvaluationBudgets::default(),
            )
            .unwrap(),
            Number(7.)
        );
    }
}

/// Matrix operation contracts, collection rules, and resource admission plans.
pub mod policy;
