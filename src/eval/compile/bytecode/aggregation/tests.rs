use super::lowering::{lower, lower_tree};
use crate::eval::compile::bytecode::lowering::{compile, compile_aggregation_argument};

#[test]
fn contextual_lowering_has_no_ordinary_name_overhead_and_admits_large_nested_vectors() {
    for formula in ["SUM", "HSTACK(RangeOne,RangeTwo)", "IF(FALSE,SUM,AVERAGE)"] {
        assert!(
            compile(&poirot_parsing::formula::parse_formula(formula).unwrap())
                .unwrap()
                .template
                .aggregations
                .is_empty()
        );
    }
    let formula = "GROUPBY({1;1},{2;4},HSTACK(SUM,AVERAGE),0,0)";
    let compiled = compile(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
    assert_eq!(compiled.template.aggregations.len(), 1);
    assert_eq!(compiled.template.aggregations[0].plan.columns, 2);
    assert_eq!(
        compiled.template.aggregations[0].plan.members[0].name,
        "SUM"
    );
    let over_arity = format!("HSTACK({})", vec!["SUM"; 256].join(","));
    assert!(
        lower(&poirot_parsing::formula::parse_formula(&over_arity).unwrap())
            .unwrap()
            .is_none()
    );
    let row = format!("HSTACK({})", vec!["SUM"; 200].join(","));
    let syntax = poirot_parsing::formula::parse_formula(&format!("HSTACK({row},{row})")).unwrap();
    assert_eq!(
        compile_aggregation_argument(&syntax)
            .unwrap()
            .template
            .aggregations[0]
            .plan
            .columns,
        400
    );
    assert!(
        lower(&poirot_parsing::formula::parse_formula("\"SUM\"").unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        lower(&poirot_parsing::formula::parse_formula("HSTACK(VSTACK(SUM,MAX),AVERAGE)").unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn projection_agrees_between_inline_program_and_tree_arguments() {
    use crate::eval::vm::ArgumentsRef;
    let syntax = poirot_parsing::formula::parse_formula("VSTACK(SUM,MEDIAN)").unwrap();
    let compiled = compile_aggregation_argument(&syntax).unwrap();
    let programs = [compiled];
    let program = ArgumentsRef::Programs(&programs)
        .get(0)
        .unwrap()
        .aggregation()
        .unwrap()
        .unwrap();
    let tree = crate::eval::compile::bound::bind(&syntax);
    let tree = lower_tree(&tree).unwrap().unwrap();
    assert_eq!(*program, tree);
    assert_eq!((tree.rows, tree.columns), (2, 1));
}
