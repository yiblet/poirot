//! Reference selection policies over shared intact-source execution.
use crate::eval::adapters::elementwise::controls::{
    OffsetControls, OffsetValues, OrdinalControls, Ordinals,
};
use crate::eval::adapters::elementwise::{Prepared, SourceIdentity, source};
use crate::eval::host::CallHost;
use crate::eval::*;

/// Route reference selectors through shared typed preparation and publication.
pub(in crate::eval) fn execute(
    host: &mut CallHost<'_, '_>,
    name: ReferenceFunction,
) -> EvalResult<Value> {
    if !FunctionId::Reference(name)
        .descriptor()
        .arity
        .accepts(host.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    host.selection_builtin_policy();
    let result = match name {
        ReferenceFunction::Index => source::execute(
            host,
            OrdinalControls::new(host.len()),
            IndexProjection(IndexForm::from_arity(host.len())),
        ),
        ReferenceFunction::Offset => {
            host.record_volatile(FunctionId::Reference(name));
            source::execute(host, OffsetControls::new(host.len()), OffsetProjection)
        }
        ReferenceFunction::Single => source::execute(host, (), Intersection),
        ReferenceFunction::AnchorArray => source::execute(host, (), Spill),
        _ => return Ok(err(VALUE_ERROR)),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
/// Apply modern implicit intersection to an already evaluated unary-operator source.
pub(in crate::eval) fn intersect(
    session: &mut Session<'_>,
    sheet: usize,
    at: CellAddress,
    value: Value,
) -> EvalResult<Value> {
    let arguments: &[bytecode::Compiled] = &[];
    let mut host = CallHost::new(session, sheet, at, arguments);
    match source::project(
        &mut host,
        Prepared::<SourceIdentity>::new(value),
        (),
        Intersection,
    ) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
/// Translates INDEX controls into a rectangle within the selected source area.
/// The shared source executor evaluates controls and publishes the resulting value or reference.
struct IndexProjection(IndexForm);
impl source::Selector<Ordinals> for IndexProjection {
    fn select(
        &self,
        source: &Prepared<SourceIdentity>,
        controls: Ordinals,
        _: CellAddress,
    ) -> FunctionResult<source::Projection> {
        let (height, width) = source.dimensions(controls.area)?;
        let (row, column) = index_selection(self.0, height, width, controls.row, controls.column)?;
        Ok(source::Projection::Rectangle {
            area: controls.area,
            row,
            column,
        })
    }
}
/// Translates OFFSET displacement and dimensions into a bounded reference rectangle.
/// The shared source executor retains reference identity and publishes the selected area.
struct OffsetProjection;
impl source::Selector<OffsetValues> for OffsetProjection {
    fn select(
        &self,
        source: &Prepared<SourceIdentity>,
        controls: OffsetValues,
        _: CellAddress,
    ) -> FunctionResult<source::Projection> {
        let geometry = SourceIdentity::geometry(source.value())?;
        Ok(source::Projection::BoundedRectangle {
            rows: poirot_functions::lookup::offset_extent(
                geometry.row,
                controls.row,
                controls.height,
                MAX_ROWS,
            )
            .map_err(|_| Ref)?,
            columns: poirot_functions::lookup::offset_extent(
                geometry.column,
                controls.column,
                controls.width,
                MAX_COLUMNS,
            )
            .map_err(|_| Ref)?,
        })
    }
}
/// Implicit-intersection policy that selects one item using caller coordinates.
/// It leaves source evaluation and result publication to the shared source executor.
struct Intersection;
impl source::Selector<()> for Intersection {
    fn work(&self, source: &Prepared<SourceIdentity>) -> usize {
        source.reference_work()
    }
    fn select(
        &self,
        source: &Prepared<SourceIdentity>,
        _: (),
        at: CellAddress,
    ) -> FunctionResult<source::Projection> {
        source.intersection(at)
    }
}
/// Spill-reference policy that requests the source anchor's finalized spill footprint.
/// The shared source executor obtains and validates that footprint through the host service.
struct Spill;
impl source::Selector<()> for Spill {
    fn select(
        &self,
        _: &Prepared<SourceIdentity>,
        _: (),
        _: CellAddress,
    ) -> FunctionResult<source::Projection> {
        Ok(source::Projection::Spill)
    }
}
/// INDEX argument form controlling row/column defaults and vector selection.
#[derive(Clone, Copy)]
enum IndexForm {
    Vector,
    RowColumn,
}
impl IndexForm {
    /// Bind vector versus row/column selection from the admitted call shape.
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
    poirot_functions::lookup::index_selection(
        matches!(form, IndexForm::Vector),
        height,
        width,
        row,
        column,
    )
    .ok_or(Ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poirot_functions::coercion::compatibility::coercev2_index_ordinal;
    #[test]
    fn index_evaluates_later_operands_before_returning_semantic_errors() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        for formula in [
            r#"INDEX(A1:B3,1/0,INDIRECT("[missing.xlsx]Sheet1!A1"))"#,
            r#"INDEX(1/0,1,INDIRECT("[missing.xlsx]Sheet1!A1"))"#,
        ] {
            let plan =
                bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
                    .unwrap();
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
        assert_eq!(coercev2_index_ordinal(-0.1), Err(VALUE_ERROR));
        assert_eq!(coercev2_index_ordinal(2.9), Ok(2));
        assert_eq!(coercev2_index_ordinal(2147483648.), Err(Num));
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
                match crate::eval::run_plan(
                    &workbook,
                    0,
                    at,
                    &bound::FormulaPlan::bind(
                        &poirot_parsing::formula::parse_formula(formula).unwrap()
                    )
                    .unwrap(),
                    Default::default(),
                    Default::default()
                ) {
                    crate::eval::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                expected,
                "{formula}"
            );
        }
        let plan = bound::FormulaPlan::bind(
            &poirot_parsing::formula::parse_formula("INDEX(A1:B3,2,2)").unwrap(),
        )
        .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        assert!(
            matches!(session.expr(0,at,&plan.expression()).unwrap(),Value::Refs(areas) if areas.len()==1 && areas[0].r0==1 && areas[0].c0==1)
        );
    }
}
