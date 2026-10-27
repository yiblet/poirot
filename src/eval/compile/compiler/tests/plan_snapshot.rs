use super::super::plan_snapshot::{PlanSnapshot, VERSION};
use super::super::preparation_policy::PreparationPolicy;
use super::super::prepared_formula::PreparedFormula;
use crate::eval::compile::bytecode::{Instruction, InstructionIndex, Opcode};
use crate::eval::compile::compiler::formula_compiler::FormulaCompiler;
use crate::model::{CellAddress, Workbook};
use std::borrow::Cow;
use std::sync::Arc;

fn fixture() -> Workbook {
    let mut w = Workbook::new();
    let s = w.add_sheet("S").unwrap();
    for (col, formula) in ["A2+1", "A3+1", "A2+1"].into_iter().enumerate() {
        s.set_formula(
            CellAddress {
                row: 0,
                col: col as u32,
            },
            formula,
        )
        .unwrap();
    }
    w
}

#[test]
fn random_array_and_trim_current_plans_roundtrip_without_reparsing() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula("A1".parse().unwrap(), "RANDARRAY(2,3)")
        .unwrap();
    sheet
        .set_formula("G1".parse().unwrap(), "TRIMRANGE(A1:D5)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("K1".parse().unwrap(), "MUNIT(2)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            "P1".parse().unwrap(),
            "NETWORKDAYS.INTL(45292,45298,1,A1:A10)",
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("T1".parse().unwrap(), "IMSUM(IMCONJUGATE({\"i\";\"2i\"}))")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("W1".parse().unwrap(), "BIN2HEX({1;10})")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("Y1".parse().unwrap(), "CONVERT({1;2},\"byte\",\"bit\")")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AA1".parse().unwrap(), "BESSELI({0;1},0)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AD1".parse().unwrap(), "ERFC.PRECISE({0;1})")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            "AG1".parse().unwrap(),
            "XIRR({-100;110},{1000;1365},{0.05;0.2})",
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AJ1".parse().unwrap(), "FVSCHEDULE(100,{0.1;0.2})")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AM1".parse().unwrap(), "COUPDAYS({45000;45001},46000,2,1)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            "AN1".parse().unwrap(),
            "AMORLINC({1000;2000},45000,45090,100,0,0.2)",
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AO1".parse().unwrap(), "BETA.DIST({0.25;0.5},2,2,TRUE)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AP1".parse().unwrap(), "BETA.INV({0.25;0.5},2,1)")
        .unwrap();
    for (cell, formula) in [
        ("AQ1", "MODE.MULT({7;2;7;2})"),
        ("AR1", "RANK.AVG({2;7},{2;2;7},{0,1})"),
        ("AT1", "T.TEST({0;2},{2;3;4},{1;2},{2,3})"),
        ("AV1", "PROB({0;1},{0.25;0.75},{0;1})"),
        ("AW1", "Z.TEST({0;1;2},{0;1},{1,2})"),
        ("AZ1", "LINEST({1;2;2},{0;1;2},TRUE,TRUE)"),
        ("BC1", "LOGEST({3;6;12},{0;1;2})"),
        ("BF1", "GROWTH({3;6;12},{0;1;2},{3;4})"),
        ("BI1", "FORECAST.ETS({4;5},{1;2;3},{1;2;3},0)"),
        ("BL1", "FORECAST.ETS.CONFINT(4,{1;2;3},{1;2;3},,0)"),
        ("BO1", "FORECAST.ETS.SEASONALITY({1;2;3},{1;2;3})"),
        ("BR1", "FORECAST.ETS.STAT({1;2;3},{1;2;3},{7,8},0)"),
        ("C1", "BETADIST(3,1,1,2,6)"),
        ("C2", "BETAINV(0.25,1,1,2,6)"),
        ("C3", "BINOMDIST(1,4,0.5,FALSE)"),
        ("C4", "CHIDIST(2*LN(2),2)"),
        ("C5", "CHIINV(0.5,2)"),
        ("C6", "CHITEST({2,2;2,2},{2,2;2,2})"),
        ("C7", "CONFIDENCE(0.05,1,1)"),
        ("C8", "CRITBINOM(4,0.5,5/16)"),
        ("C9", "EXPONDIST(2,0.5,TRUE)"),
        ("C10", "FDIST(3,2,2)"),
        ("C11", "FINV(0.25,2,2)"),
        ("C12", "FTEST({0;1;2},{2;3;4})"),
        ("C13", "GAMMADIST(2,1,2,TRUE)"),
        ("C14", "GAMMAINV(0.5,1,2)"),
        ("C15", "HYPGEOMDIST(1,2,2,4)"),
        ("C16", "LOGINV(0.5,0,1)"),
        ("C17", "LOGNORMDIST(1,0,1)"),
        ("C18", "NEGBINOMDIST(1,2,0.5)"),
        ("C19", "QUARTILE({0;4},1)"),
        ("C20", "TINV(0.5,1)"),
        ("C21", "TTEST({0;1;2},{0;1;2},2,2)"),
        ("C22", "WEIBULL(2,1,2,TRUE)"),
        ("C23", "ZTEST({0;1;2},1)"),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(cell.parse().unwrap(), formula)
            .unwrap();
    }
    let plan = FormulaCompiler::new().compile_workbook(&book, &PreparationPolicy::default());
    let dto = PlanSnapshot::capture(&plan).unwrap().into_owned();
    let restored = dto.restore().unwrap();
    assert!(restored.matches(&book, &PreparationPolicy::default()));
    for (key, original) in &plan.formulas {
        let PreparedFormula::Ready(original) = original.as_ref() else {
            panic!()
        };
        let PreparedFormula::Ready(actual) = restored.formulas.get(key).unwrap().as_ref() else {
            panic!()
        };
        assert_eq!(actual.bytecode.typing, original.bytecode.typing);
    }
    let mut old = PlanSnapshot::capture(&plan).unwrap();
    old.version = VERSION - 1;
    assert_eq!(
        old.restore().unwrap_err(),
        "unsupported compiled snapshot version"
    );
}

#[test]
fn contextual_aggregation_metadata_roundtrips_and_rejects_malformed_plans() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(
            "A1".parse().unwrap(),
            "GROUPBY({1;1},{2;8},HSTACK(SUM,AVERAGE),0,0)",
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            "AG1".parse().unwrap(),
            "XIRR({-100;110},{1000;1365},{0.05;0.2})",
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("AJ1".parse().unwrap(), "FVSCHEDULE(100,{0.1;0.2})")
        .unwrap();
    let plan = FormulaCompiler::new().compile_workbook(&book, &PreparationPolicy::default());
    let dto = PlanSnapshot::capture(&plan).unwrap().into_owned();
    let restored = dto.restore().unwrap();
    assert!(restored.matches(&book, &PreparationPolicy::default()));
    for mutation in 0..5 {
        let mut dto = PlanSnapshot::capture(&plan).unwrap().into_owned();
        let entries = &mut dto.programs[0].template.to_mut().aggregations;
        match mutation {
            0 => entries[0].plan.columns = 3,
            1 => entries[0].start = usize::MAX,
            2 => entries[0].plan.rows = 2,
            3 => entries[0].plan.members[0].name.clear(),
            _ => entries.push(entries[0].clone()),
        }
        assert!(dto.restore().is_err());
    }
}

#[test]
fn snapshot_restores_source_and_program_sharing_without_lazy_trees() {
    let w = fixture();
    let plan = FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default());
    let dto = PlanSnapshot::capture(&plan).unwrap();
    assert_eq!(dto.counts(), (1, 2, 3));
    assert!(matches!(dto.identity, Cow::Borrowed(_)));
    assert!(matches!(dto.sources[0].source, Cow::Borrowed(_)));
    assert!(matches!(dto.sources[0].references, Cow::Borrowed(_)));
    assert!(matches!(
        dto.sources[0].potential_references,
        Cow::Borrowed(_)
    ));
    let original = plan
        .formulas
        .get(&(0, CellAddress { row: 0, col: 0 }))
        .unwrap();
    let PreparedFormula::Ready(original) = original.as_ref() else {
        panic!()
    };
    let Cow::Borrowed(template) = &dto.programs[0].template else {
        panic!("template copied")
    };
    assert!(std::ptr::eq(*template, &original.bytecode.template));
    let Cow::Borrowed(references) = &dto.sources[0].references else {
        panic!()
    };
    assert!(std::ptr::eq(*references, &original.bytecode.references));
    let restored = dto.restore().unwrap();
    assert!(restored.matches(&w, &PreparationPolicy::default()));
    let mut ready = Vec::new();
    for col in 0..3 {
        let p = restored
            .formulas
            .get(&(0, CellAddress { row: 0, col }))
            .unwrap();
        let PreparedFormula::Ready(c) = p.as_ref() else {
            panic!()
        };
        ready.push(c);
    }
    assert!(Arc::ptr_eq(ready[0], ready[2]));
    assert!(Arc::ptr_eq(
        &ready[0].bytecode.executable,
        &ready[1].bytecode.executable
    ));
}

#[test]
fn snapshot_rejects_identity_and_bytecode_corruption() {
    let w = fixture();
    let plan = FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default());
    let mut old = PlanSnapshot::capture(&plan).unwrap();
    old.version = VERSION - 1;
    assert_eq!(
        old.restore().unwrap_err(),
        "unsupported compiled snapshot version"
    );
    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.sources[0].program = usize::MAX;
    assert!(dto.restore().is_err());
    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.sources[0].references.to_mut().slots[0].length = u32::MAX;
    assert!(dto.restore().is_err());
    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.identity.to_mut().formulas[0].2 = "wrong".into();
    assert!(dto.restore().is_err());
    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.programs[0].template.to_mut().code.push(Instruction {
        opcode: Opcode::Jump,
        flags: 0,
        reserved: 0,
        a: 0,
        b: 0,
        c: u32::MAX,
    });
    assert!(dto.restore().is_err());
}

#[test]
fn snapshot_rejects_inconsistent_scalar_geometry() {
    let workbook = fixture();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &PreparationPolicy::default());
    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    let mut entries = dto.programs[0].typing.expressions().to_vec();
    entries.first_mut().unwrap().properties.shape = crate::model::ShapeBounds::new(2, 1);
    // FormulaTyping encodes one sequence field. Forge that field through the
    // serialization boundary instead of adding an unchecked mutation API.
    *dto.programs[0].typing.to_mut() =
        postcard::from_bytes(&postcard::to_allocvec(&entries).unwrap()).unwrap();
    assert_eq!(dto.restore().unwrap_err(), "snapshot typing boundaries");
}

#[test]
fn snapshot_rejects_invalid_labels_and_conditional_bindings() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("S")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "IF(TRUE,1,2)")
        .unwrap();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &PreparationPolicy::default());

    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.programs[0].template.to_mut().labels[0] = InstructionIndex(u32::MAX);
    assert_eq!(dto.restore().unwrap_err(), "snapshot label table");

    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    let template = dto.programs[0].template.to_mut();
    let invalid = template.labels.len() as u32;
    template
        .code
        .iter_mut()
        .find(|i| i.opcode == Opcode::Conditional)
        .unwrap()
        .c = invalid;
    assert_eq!(dto.restore().unwrap_err(), "snapshot jump label");

    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.programs[0].template.to_mut().labels[0] = InstructionIndex(0);
    assert_eq!(dto.restore().unwrap_err(), "snapshot backwards invocation");

    let mut dto = PlanSnapshot::capture(&plan).unwrap();
    dto.programs[0].calls[0].builtin = Some(Cow::Borrowed("SUM"));
    assert_eq!(dto.restore().unwrap_err(), "snapshot conditional binding");
}

#[test]
fn snapshot_rejects_unsupported_preparation_and_policy() {
    let mut w = Workbook::new();
    w.add_sheet("S")
        .unwrap()
        .set_formula(CellAddress { row: 0, col: 0 }, "1+")
        .unwrap();
    assert!(
        PlanSnapshot::capture(
            &FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default())
        )
        .is_err()
    );
    let w = fixture();
    let mut plan = FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default());
    plan.policy.registry.builtin_contract_version += 1;
    assert!(PlanSnapshot::capture(&plan).is_err());
}
