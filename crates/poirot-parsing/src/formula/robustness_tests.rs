use super::*;
use crate::formula::tokens::number_token;
#[test]
fn nom_token_regressions_and_byte_offsets() {
    for (source, expected) in [(".5", 0.5), ("1.", 1.), ("2e+3", 2000.), ("3E-2", 0.03)] {
        assert_eq!(parse(source), Expr::Number(expected));
    }
    assert_eq!(parse("\"\""), Expr::String(String::new()));
    assert_eq!(parse("\"a\"\"😀\""), Expr::String("a\"😀".into()));
    assert_eq!(
        parse("'O''Brien'!A1"),
        Expr::Cell(CellReference {
            row: 0,
            column: 0,
            row_absolute: false,
            column_absolute: false,
            sheet: Some("O'Brien".into())
        })
    );
    assert_eq!(parse_formula("\"😀\"+)").unwrap_err().position, 7);
    assert!(matches!(parse("IF(,1,,)"),Expr::Function{args,..} if args.len()==4));
    assert!(matches!(parse("SUM( (A1,B1), C1 )"),Expr::Function{args,..} if args.len()==2));
    assert!(parse_formula(&"1+".repeat(1100)).is_err());
}
#[test]
fn decimal_tokens_preserve_exact_bits_at_eof_and_boundaries() {
    for (source, rest, bits) in [
        ("0.5", "", 0x3fe0000000000000),
        ("2.9999999999999996", "", 0x4007ffffffffffff),
        ("0.5 ", " ", 0x3fe0000000000000),
        ("0.5+1", "+1", 0x3fe0000000000000),
        ("2.9999999999999996*1", "*1", 0x4007ffffffffffff),
        ("0.5e0", "", 0x3fe0000000000000),
        ("2.9999999999999996E+0", "", 0x4007ffffffffffff),
        (".5", "", 0x3fe0000000000000),
        (".5e0", "", 0x3fe0000000000000),
        ("2.", "", 0x4000000000000000),
        ("2.e0", "", 0x4000000000000000),
    ] {
        let (remaining, value) = number_token(source).unwrap();
        assert_eq!(remaining, rest, "source={source}");
        assert_eq!(value.to_bits(), bits, "source={source}");
    }
    for source in ["0.5", "2.9999999999999996", " 0.5 ", ".5", "0.5e0"] {
        let Expr::Number(value) = parse(source) else {
            panic!("{source}")
        };
        let expected = if source.contains("999") {
            0x4007ffffffffffff
        } else {
            0x3fe0000000000000
        };
        assert_eq!(value.to_bits(), expected, "source={source}");
    }
    for source in ["-0.5", "+2.9999999999999996"] {
        let Expr::Unary { operand, .. } = parse(source) else {
            panic!("{source}")
        };
        let Expr::Number(value) = *operand else {
            panic!("{source}")
        };
        assert_eq!(
            value.to_bits(),
            if source.starts_with('-') {
                0x3fe0000000000000
            } else {
                0x4007ffffffffffff
            }
        );
    }
    for source in [
        "0.5",
        "2.9999999999999996",
        "0.5+2.9999999999999996",
        "-0.5",
        "+2.9999999999999996",
        ".5",
        "2.",
        "0.5e-1",
        "SUM(0.5,2.9999999999999996)",
    ] {
        let expression = parse(source);
        let printed = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(
            parse(&printed),
            expression,
            "source={source}, printed={printed}"
        );
    }
}
#[test]
fn bounded_generated_input_never_panics() {
    let alphabet = [
        'A', '1', '$', '+', '-', '^', '%', '(', ')', ',', ';', ':', ' ', '\"', '\'', '{', '}', '!',
        '#', '😀', 'é', '\t',
    ];
    let mut seed = 0x42u64;
    for length in 0..80 {
        for _ in 0..40 {
            let mut input = String::new();
            for _ in 0..length {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                input.push(alphabet[(seed as usize) % alphabet.len()]);
            }
            let _ = parse_formula(&input);
        }
    }
    for depth in [64, 127, 128, 129, 256] {
        let input = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
        let result = parse_formula(&input);
        if depth >= 128 {
            assert!(result.is_err());
        }
    }
}
#[test]
fn malformed_input_is_rejected_without_panics() {
    for input in [
        "",
        "=",
        "1+",
        "(A1",
        "\"unfinished",
        "'Sheet!A1",
        "XFE1",
        "A1048577",
        "A0",
        "A1!B1!C1",
        "1e+",
        "1e999",
        "#BOGUS!",
        "SUM(1;2)",
        "{1,2;3}",
        "{A1}",
        "1 2",
        "A1)",
        "A1:",
    ] {
        assert!(parse_formula(input).is_err(), "accepted {input}");
    }
    assert_eq!(parse_formula("1+)").unwrap_err().position, 2);
    assert_eq!(parse_formula("=XFE1").unwrap_err().position, 1);
    assert!(parse_formula(&format!("{}1{}", "(".repeat(200), ")".repeat(200))).is_err());
}
