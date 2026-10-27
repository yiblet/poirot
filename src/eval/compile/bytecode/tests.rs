use super::compile_error::CompileError;
use super::compiled::Compiled;
use super::instruction::Instruction;
use super::label::Label;
use super::lowering::{binary, compile};
use super::opcode::Opcode;
use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};

fn compiled(source: &str) -> Compiled {
    let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
    compile(&syntax).unwrap()
}

#[test]
fn labels_are_dense_body_local_ids_independent_of_instruction_positions() {
    let program = compiled("IF(A1,SUM(1,2),IF(FALSE,1/0,3))");
    assert!(!program.template.labels.is_empty());
    assert_eq!(
        program.template.labels.len(),
        program.template.labels.capacity()
    );
    assert!(program.template.resolve_label(Label(u32::MAX)).is_none());
    for (ip, instruction) in program.template.code.iter().enumerate() {
        if instruction.opcode.is_call()
            || matches!(instruction.opcode, Opcode::Jump | Opcode::Argument)
        {
            let position = program
                .template
                .resolve_label(Label(instruction.c))
                .unwrap()
                .index();
            assert!(position > ip);
            assert!(position < program.template.code.len());
        }
    }
    assert!(
        program
            .template
            .labels
            .iter()
            .enumerate()
            .any(|(id, position)| id != position.index())
    );
}

#[test]
fn bytecode_lazy_calls_have_forward_blocks_and_continuations() {
    for name in ["IF", "IFERROR", "SUM", "UNKNOWN"] {
        let result = compiled(&format!("{name}(A1,IFERROR(B1,9),1/0)"));
        let code = &result.template.code;
        let invoke = code.iter().position(|i| i.opcode.is_call()).unwrap();
        assert_eq!(code[invoke].b, 3);
        assert_eq!(code[invoke + 1].opcode, Opcode::Jump);
        let done = result
            .template
            .resolve_label(Label(code[invoke].c))
            .unwrap()
            .index();
        assert_eq!(code[invoke + 1].c, code[invoke].c);
        assert_eq!(code[done].opcode, Opcode::Leave);
        let mut arg = invoke + 2;
        for index in 0..3 {
            assert_eq!(code[arg].opcode, Opcode::Argument);
            assert_eq!(code[arg].a, index);
            assert_eq!(code[arg + 1].opcode, Opcode::Enter);
            let next = result
                .template
                .resolve_label(Label(code[arg].c))
                .unwrap()
                .index();
            assert!(next > arg);
            assert_eq!(code[next - 1].opcode, Opcode::YieldArgument);
            arg = next;
        }
        assert_eq!(arg, done);
    }
}

#[test]
fn bytecode_templates_ignore_only_concrete_references() {
    let first = compiled("SUM(A1,$B$2)+C3:D4");
    let second = compiled("SUM(Z9,$C$8)+E5:F6");
    assert_eq!(first.template, second.template);
    assert_ne!(first.references, second.references);
    assert_eq!(first.template.reference_slots, 4);
    let slots: Vec<_> = first
        .template
        .code
        .iter()
        .filter(|i| i.opcode == Opcode::Reference)
        .map(|i| i.a)
        .collect();
    assert_eq!(slots, [0, 1, 2, 3]);
    assert_eq!(
        first
            .template
            .code
            .iter()
            .rfind(|i| i.opcode.binary_operation().is_some() || i.opcode == Opcode::Binary)
            .unwrap()
            .a,
        binary(BinaryOp::Add)
    );
}

#[test]
fn bytecode_owned_result_and_preorder_admission() {
    let result = compiled("-A1+{1,2;3,4}");
    let code = &result.template.code;
    let depths: Vec<_> = code
        .iter()
        .filter(|i| i.opcode == Opcode::Enter)
        .map(|i| i.a)
        .collect();
    assert_eq!(depths, [0, 1, 2, 1, 2, 2, 2, 2]);
    let array = code.iter().find(|i| i.opcode == Opcode::Array).unwrap();
    assert_eq!((array.a, array.b), (2, 2));
    assert_eq!(std::mem::size_of::<Instruction>(), 16);
    assert_eq!(code.len(), result.template.code.capacity());
    assert_eq!(
        result.references.data.len(),
        result.references.data.capacity()
    );
    assert_eq!(
        result.references.slots.len(),
        result.references.slots.capacity()
    );
}

#[test]
fn bytecode_all_syntax_atoms_and_operators() {
    let cell = poirot_parsing::formula::CellReference {
        row: 1,
        column: 2,
        row_absolute: true,
        column_absolute: false,
        sheet: Some("sheet".into()),
    };
    let atoms = [
        Expr::Number(3.0),
        Expr::String("hello".into()),
        Expr::Bool(true),
        Expr::Error("#N/A".into()),
        Expr::Missing,
        Expr::Name("named".into()),
        Expr::QualifiedName {
            sheet: "sheet".into(),
            name: "named".into(),
        },
        Expr::Cell(cell.clone()),
        Expr::Column {
            column: 3,
            absolute: true,
            sheet: None,
        },
        Expr::Row {
            row: 4,
            absolute: false,
            sheet: Some("sheet".into()),
        },
        Expr::SheetSpan(poirot_parsing::formula::SheetSpanReference {
            first_sheet: "one".into(),
            last_sheet: "three".into(),
            start: poirot_parsing::formula::CellReference {
                sheet: None,
                ..cell
            },
            end: None,
        }),
    ];
    for atom in atoms {
        assert_eq!(compile(&atom).unwrap().template.code.len(), 3);
    }
    for op in [
        BinaryOp::Add,
        BinaryOp::Subtract,
        BinaryOp::Multiply,
        BinaryOp::Divide,
        BinaryOp::Power,
        BinaryOp::Concat,
        BinaryOp::Equal,
        BinaryOp::NotEqual,
        BinaryOp::Less,
        BinaryOp::LessEqual,
        BinaryOp::Greater,
        BinaryOp::GreaterEqual,
        BinaryOp::Range,
        BinaryOp::Union,
        BinaryOp::Intersection,
    ] {
        let e = Expr::Binary {
            op,
            left: Box::new(Expr::Number(1.0)),
            right: Box::new(Expr::Number(2.0)),
        };
        assert_eq!(compile(&e).unwrap().template.code[7].a, binary(op));
    }
}

#[test]
fn bytecode_structural_errors_before_lowering() {
    assert_eq!(
        compile(&Expr::Array(vec![])),
        Err(CompileError::InvalidArray)
    );
    assert_eq!(
        compile(&Expr::Row {
            row: crate::model::MAX_ROWS,
            absolute: false,
            sheet: None
        }),
        Err(CompileError::InvalidReference)
    );
    let mut e = Expr::Missing;
    for _ in 0..128 {
        e = Expr::Unary {
            op: UnaryOp::Plus,
            operand: Box::new(e),
        };
    }
    assert_eq!(compile(&e), Err(CompileError::ResourceLimit));
}
