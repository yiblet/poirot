//! Lazy matrix execution through engine-owned shape inspection and cell reads.
use poirot_functions_matrix as domain;
use poirot_model::CellError;

mod access;
mod collection;
mod plans;
mod sumproduct;
mod trend;

use access::{UNLIMITED, coercev2_matrix_number, describe, err, preflight, read, value, work};
use collection::{
    coercev2_frequency_collection, coercev2_matrix_collection, coercev2_trend_collection,
};
use plans::ResourcePlan;
/// Matrix algebra and array aggregation operations supported by this adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixFunction {
    /// Matrix determinant.
    Mdeterm,
    /// Matrix inverse.
    Minverse,
    /// Matrix multiplication.
    Mmult,
    /// Exchange rows and columns.
    Transpose,
    /// Bin frequency counts.
    Frequency,
    /// Sum of elementwise products.
    Sumproduct,
    /// Linear regression predictions.
    Trend,
}
/// Operand provenance controlling collection and coercion behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Direct scalar expression.
    Direct,
    /// Workbook reference.
    Reference,
    /// Materialized array expression.
    Array,
}
/// Argument dimensions, sheet count, omitted-argument flag, and scalar/reference/array origin.
/// Describing shape resolves the argument expression without traversing its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
    /// Scalar, reference, or array provenance.
    pub origin: Origin,
    /// Number of referenced sheets.
    pub sheets: usize,
    /// Whether the operand was syntactically omitted.
    pub missing: bool,
}
/// Uncoerced matrix member, preserving omitted arguments and spreadsheet errors.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Numeric member.
    Number(f64),
    /// Boolean member.
    Bool(bool),
    /// Text member.
    Text(String),
    /// Blank member.
    Blank,
    /// Omitted-argument member.
    Missing,
    /// Spreadsheet error member.
    Error(CellError),
}
/// Matrix source behaviors that the host cannot safely represent or coerce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedCapability {
    /// Exceptional behavior identified in the pinned source.
    SourceException(&'static str),
    /// TREND declared a shape inconsistent with its produced values.
    MalformedTrendArea {
        /// Declared row count.
        rows: usize,
        /// Declared column count.
        cols: usize,
        /// Actual number of produced values.
        values: usize,
    },
    /// Omitted-argument numeric coercion is unavailable.
    MissingNumericCoercion,
}
/// Matrix failures preserving semantic and operational error domains.
pub type Failure<O> = crate::adapters::failure::AdapterFailure<O, UnsupportedCapability>;
/// Scalar or rectangular numeric matrix result.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Scalar result, including a collapsed one-element matrix.
    Number(f64),
    /// Rectangular numeric array.
    Array(domain::Matrix<f64>),
}
/// describe evaluates argument expression/type but does not traverse area cells.
/// read_cell follows source row-major traversal, preserving operational failures.
/// The engine supplies first-sheet singleton selection for scalar matrix args;
/// TREND/SUMPRODUCT additionally reject multiple-sheet RefEval explicitly.
pub trait EvaluationPort {
    /// Deferred engine-owned operand handle.
    type Argument;
    /// Host operational failure type.
    type Operational;
    /// Resolve operand shape without reading its member cells.
    fn describe(&mut self, arg: &Self::Argument) -> Result<Shape, Failure<Self::Operational>>;
    /// Read one member at zero-based offsets, retaining typed failures.
    fn read_cell(
        &mut self,
        arg: &Self::Argument,
        row: usize,
        col: usize,
    ) -> Result<Cell, Failure<Self::Operational>>;
    /// Admit work before performing it.
    fn charge_work(&mut self, units: u64) -> Result<(), Self::Operational>;
    /// Check collection capacity before allocation.
    fn check_collection(&mut self, elements: usize) -> Result<(), Self::Operational>;
    /// Check text capacity before copying or allocating.
    fn check_text(&mut self, bytes: usize) -> Result<(), Self::Operational>;
    /// Coerce numeric text using the engine's shared conversion policy.
    fn coercev2_number_text(&mut self, text: &str) -> Result<f64, Failure<Self::Operational>>;
    /// A concrete engine operational budget failure; never manufacture #VALUE!.
    fn budget_exceeded(&mut self) -> Self::Operational;
    /// Apply explicit omitted-argument numeric coercion or reject the capability.
    fn missing_number(&mut self) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            UnsupportedCapability::MissingNumericCoercion,
        ))
    }
}

fn matrix_output(m: domain::Matrix<f64>) -> Output {
    if m.values().len() == 1 {
        // SAFETY: The enclosing condition checks m.values().len() == 1 before reading element zero.
        Output::Number(m.values()[0])
    } else {
        Output::Array(m)
    }
}
impl MatrixFunction {
    /// Check argument counts before reading matrix operands.
    pub const fn accepts(self, n: usize) -> bool {
        match self {
            Self::Mmult | Self::Frequency => n == 2,
            Self::Sumproduct => n > 0,
            Self::Trend => n >= 1 && n <= 4,
            _ => n == 1,
        }
    }
}
/// Execute a matrix operation with lazy reads and resource admission.
pub fn evaluate<P: EvaluationPort>(
    p: &mut P,
    f: MatrixFunction,
    args: &[P::Argument],
) -> Result<Output, Failure<P::Operational>> {
    use MatrixFunction::*;
    if !f.accepts(args.len()) {
        return value();
    }
    work(p, 1)?;
    if f == Sumproduct {
        return sumproduct::evaluate(p, args).map(Output::Number);
    }
    if f == Frequency {
        let data = coercev2_frequency_collection(p, args.first().ok_or(CellError::Value)?)?;
        let bins = coercev2_frequency_collection(p, args.get(1).ok_or(CellError::Value)?)?;
        ResourcePlan::frequency(data.len(), bins.len()).admit(p)?;
        return domain::frequency(&data, &bins, UNLIMITED)
            .map(Output::Array)
            .map_err(|e| err(p, e));
    }
    if f == Trend {
        return trend::evaluate(p, args);
    }
    let policy = match f {
        Mdeterm => domain::MatrixCollection::Determinant,
        Minverse => domain::MatrixCollection::Inverse,
        Mmult => domain::MatrixCollection::Multiply,
        Transpose => domain::MatrixCollection::Transpose,
        _ => return Err(CellError::Value.into()),
    };
    let a = coercev2_matrix_collection(p, args.first().ok_or(CellError::Value)?, policy)?;
    if f == Mmult {
        let b = coercev2_matrix_collection(p, args.get(1).ok_or(CellError::Value)?, policy)?;
        ResourcePlan::multiply(&a, &b).admit(p)?;
        return domain::multiply(&a, &b, UNLIMITED)
            .map(matrix_output)
            .map_err(|e| err(p, e));
    }
    ResourcePlan::unary(&a, f == Transpose).admit(p)?;
    match f {
        Mdeterm => domain::determinant(&a, UNLIMITED)
            .map(Output::Number)
            .map_err(|e| err(p, e)),
        Minverse => domain::inverse(&a, UNLIMITED)
            .map(matrix_output)
            .map_err(|e| err(p, e)),
        Transpose => domain::transpose(&a, UNLIMITED)
            .map(matrix_output)
            .map_err(|e| err(p, e)),
        _ => Err(CellError::Value.into()),
    }
}
impl<'a> From<&'a Cell> for poirot_coercionv2::compatibility::ScalarRef<'a> {
    fn from(value: &'a Cell) -> Self {
        match value {
            Cell::Number(n) => Self::Number(*n),
            Cell::Bool(b) => Self::Boolean(*b),
            Cell::Text(t) => Self::Text(poirot_coercionv2::TextRef::Utf8(t)),
            Cell::Blank => Self::Blank,
            Cell::Error(e) => Self::Error(*e),
            Cell::Missing => Self::Missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Test operand with independently controlled shape and cell contents.
    struct Arg {
        shape: Shape,
        cells: Vec<Cell>,
    }
    /// Test host recording member reads and enforcing a work allowance.
    struct Port {
        reads: Vec<(usize, usize)>,
        remaining: u64,
    }
    impl EvaluationPort for Port {
        /// Deferred argument identity supplied to the family's host port.
        type Argument = Arg;
        /// Host abort channel kept outside catchable spreadsheet values.
        type Operational = &'static str;
        fn describe(&mut self, a: &Arg) -> Result<Shape, Failure<Self::Operational>> {
            Ok(a.shape)
        }
        fn read_cell(
            &mut self,
            a: &Arg,
            r: usize,
            c: usize,
        ) -> Result<Cell, Failure<Self::Operational>> {
            self.reads.push((r, c));
            Ok(a.cells[r * a.shape.cols + c].clone())
        }
        fn charge_work(&mut self, n: u64) -> Result<(), Self::Operational> {
            if n > self.remaining {
                Err("budget")
            } else {
                self.remaining -= n;
                Ok(())
            }
        }
        fn check_collection(&mut self, _: usize) -> Result<(), Self::Operational> {
            Ok(())
        }
        fn check_text(&mut self, _: usize) -> Result<(), Self::Operational> {
            Ok(())
        }
        fn coercev2_number_text(&mut self, s: &str) -> Result<f64, Failure<Self::Operational>> {
            s.parse()
                .map_err(|_| Failure::Spreadsheet(CellError::Value))
        }
        fn budget_exceeded(&mut self) -> Self::Operational {
            "budget"
        }
    }
    fn area(r: usize, c: usize, v: &[f64]) -> Arg {
        Arg {
            shape: Shape {
                rows: r,
                cols: c,
                origin: Origin::Array,
                sheets: 1,
                missing: false,
            },
            cells: v.iter().copied().map(Cell::Number).collect(),
        }
    }
    fn port() -> Port {
        Port {
            reads: vec![],
            remaining: 100000,
        }
    }
    #[test]
    fn pinned_trend_single_prediction_is_scalar_despite_original_two_columns() {
        // Trend.evaluate checks vals.length==1 before constructing CacheAreaEval.
        let mut p = port();
        let result = evaluate(
            &mut p,
            MatrixFunction::Trend,
            &[
                area(4, 1, &[1., 3., 4., 6.]),
                area(4, 2, &[0., 0., 1., 0., 0., 1., 1., 1.]),
                area(1, 2, &[2., 3.]),
            ],
        )
        .unwrap();
        let Output::Number(n) = result else {
            panic!("source returns scalar")
        };
        assert!((n - 14.).abs() < 1e-10);
    }
    #[test]
    fn pinned_trend_original_shape_cannot_be_silently_repaired() {
        // Two predictions, original newX=2x2: source declares width 2 and
        // height 2 but has only two vals. row=1,col=0 indexes vals[2].
        let mut p = port();
        assert_eq!(
            evaluate(
                &mut p,
                MatrixFunction::Trend,
                &[
                    area(4, 1, &[1., 3., 4., 6.]),
                    area(4, 2, &[0., 0., 1., 0., 0., 1., 1., 1.]),
                    area(2, 2, &[2., 3., 4., 5.])
                ]
            ),
            Err(Failure::UnsupportedCapability(
                UnsupportedCapability::MalformedTrendArea {
                    rows: 2,
                    cols: 2,
                    values: 2
                }
            ))
        );
    }
    #[test]
    fn pinned_sumproduct_dimension_error_scans_later_areas_only() {
        let mut first = area(1, 2, &[1., 2.]);
        first.cells[0] = Cell::Error(CellError::NA);
        let mut second = area(1, 3, &[1., 2., 3.]);
        second.cells[2] = Cell::Error(CellError::Div0);
        let mut p = port();
        assert_eq!(
            evaluate(&mut p, MatrixFunction::Sumproduct, &[first, second]),
            Err(Failure::Spreadsheet(CellError::Div0))
        );
        assert_eq!(p.reads, vec![(0, 0), (0, 1), (0, 2)]);
    }
    #[test]
    fn operational_budget_aborts_before_cell_traversal() {
        let mut p = port();
        p.remaining = 0;
        assert_eq!(
            evaluate(
                &mut p,
                MatrixFunction::Minverse,
                &[area(2, 2, &[1., 0., 0., 1.])]
            ),
            Err(Failure::Operational("budget"))
        );
        assert!(p.reads.is_empty());
    }
}
