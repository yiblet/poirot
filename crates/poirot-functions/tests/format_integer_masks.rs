//! Public integer-mask contracts derived from documented Excel placeholders.
use poirot_functions::format::{
    FormatCompilePolicy, FormatRenderContext, Token, compile_format, compile_program, parse_format,
    render_value,
};
use poirot_functions::scalar::CellValue;

/// Render a numeric value through the reusable public format plan.
fn display(pattern: &str, value: f64) -> String {
    let plan = compile_format(pattern, FormatCompilePolicy::default()).unwrap();
    render_value(
        &CellValue::Number(value),
        &plan,
        FormatRenderContext::default(),
    )
    .unwrap()
    .text
}

#[test]
fn integer_masks_place_digits_and_literal_separators() {
    // Primary semantics, not library code or fixtures:
    // https://support.microsoft.com/en-us/excel/keeping-leading-zeros-and-large-numbers
    // https://support.microsoft.com/en-us/excel/review-guidelines-for-customizing-a-number-format
    // Digits below are independently chosen and partitioned from the right.
    for (pattern, value, expected) in [
        ("000-00-0000", 24681357., "024-68-1357"),
        ("00000-0000", 12345., "00001-2345"),
        ("(000) 000-0000", 2468135790., "(246) 813-5790"),
        ("000-00-0000", 12345678901., "12345-67-8901"),
        ("000-00-0000", -24681357., "-024-68-1357"),
        ("000-00-0000", 0., "000-00-0000"),
        ("00-00", -0., "00-00"),
        ("00-00", 123.6, "01-24"),
        ("##-##", 12., "-12"),
        ("##-##", 0., "-"),
        ("??-??", 12., "  -12"),
        ("??-??", 0., "  -  "),
        ("##-00", 1., "-01"),
        ("00\"🦀é\"00", 123., "01🦀é23"),
        ("00\\🦀00", 123., "01🦀23"),
        ("0\"x\"0", 7., "0x7"),
    ] {
        assert_eq!(display(pattern, value), expected, "{pattern} / {value}");
    }
}

#[test]
fn integer_masks_keep_section_specific_layouts_and_conditions() {
    // Microsoft's documented TEXT phone example uses this conditional layout:
    // https://support.microsoft.com/en-us/excel/functions/text-function
    let phone = "[<=9999999]###-####;(###) ###-####";
    assert_eq!(display(phone, 1234567898.), "(123) 456-7898");
    assert_eq!(display(phone, 2345678.), "234-5678");
    assert_eq!(display("00-00;[Red](000-00)", -12.), "(000-12)");
    assert_eq!(display("0.0;(00-00)", -12.), "(00-12)");
    assert_eq!(display("00-00;(0.0)", -12.), "(12.0)");
    assert_eq!(display("00-00;(00-00);\"zero\"", 0.), "zero");
    assert_eq!(display("[<0]00-00;[>=0]000-00", -12.), "-00-12");
    let plan = compile_format("00-00;[Red](000-00)", Default::default()).unwrap();
    let output = render_value(&CellValue::Number(-12.), &plan, Default::default()).unwrap();
    assert_eq!(output.color.as_deref(), Some("Red"));
}

#[test]
fn integer_mask_layout_is_inspectable_and_reusable() {
    let plan = compile_format("(000) 000-0000", Default::default()).unwrap();
    let number = plan.sections()[0].number_plan().unwrap();
    let layout = number.layout().unwrap();
    assert_eq!(layout.integer_placeholders, &['0'; 10]);
    assert_eq!(
        layout.integer_literals,
        &["", "", "", ") ", "", "", "-", "", "", ""]
    );
    assert_eq!(layout.prefix, "(");
    assert_eq!(layout.suffix, "");
    let before = plan.clone();
    assert_eq!(number.render(2468135790.).unwrap(), "(246) 813-5790");
    assert_eq!(number.render(0.).unwrap(), "(000) 000-0000");
    assert_eq!(plan, before);
    let ordinary = compile_format("0.00", Default::default()).unwrap();
    assert!(
        ordinary.sections()[0]
            .number_plan()
            .unwrap()
            .layout()
            .unwrap()
            .integer_literals
            .is_empty()
    );
}

#[test]
fn integer_masks_reject_ambiguous_directives_and_respect_budgets() {
    for pattern in [
        "0-x0.0",
        "0.0-x0",
        "0-x0E+00",
        "0-x0%",
        "0-x0,",
        "0,x0",
        "0_x0",
        "0*x0",
        "0\"x\"0\"z\"0/0",
    ] {
        assert!(
            compile_format(pattern, Default::default()).is_err(),
            "{pattern}"
        );
    }
    let pattern = "00\"😀é\"00";
    let policy = FormatCompilePolicy {
        max_output_characters: 6,
        ..Default::default()
    };
    let plan = compile_format(pattern, policy).unwrap();
    let value = CellValue::Number(12.);
    let output = render_value(&value, &plan, Default::default()).unwrap();
    assert_eq!(output.text, "00😀é12");
    assert!(plan.render_byte_bound(&value, Default::default()).unwrap() >= output.text.len());
    let plan = compile_format(
        pattern,
        FormatCompilePolicy {
            max_output_characters: 5,
            ..policy
        },
    )
    .unwrap();
    assert!(render_value(&value, &plan, Default::default()).is_err());
    assert!(
        compile_format(
            pattern,
            FormatCompilePolicy {
                max_literal_characters: 1,
                ..policy
            }
        )
        .is_err()
    );
    assert!(
        compile_format(
            "00-00",
            FormatCompilePolicy {
                max_tokens_per_section: 4,
                ..policy
            }
        )
        .is_err()
    );
    let plan = compile_format("00-00", Default::default()).unwrap();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(render_value(&CellValue::Number(value), &plan, Default::default()).is_err());
    }
    let mut parsed = parse_format("0\"x\"0").unwrap();
    parsed.sections[0].tokens = vec![
        Token::Digit('0'),
        Token::Literal("🦀".repeat(32767)),
        Token::Digit('0'),
    ];
    let plan = compile_program(&parsed, Default::default()).unwrap();
    assert!(render_value(&CellValue::Number(1.), &plan, Default::default()).is_err());
}
