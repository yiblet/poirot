use super::*;
fn render(plan: &FormatPlan, n: f64) -> String {
    render_value(&CellValue::Number(n), plan, FormatRenderContext::default())
        .unwrap()
        .text
}
#[test]
fn invalid_fraction_sections_fail_at_compilation() {
    let format = parse_format("0;?/?%").unwrap();
    assert!(compile_program(&format, FormatCompilePolicy::default()).is_err());
    let valid = compile_format("# ?/8", FormatCompilePolicy::default()).unwrap();
    assert_eq!(valid.sections()[0].kind(), SectionKind::Fraction);
    assert_eq!(render(&valid, 1.5), "1 4/8");
    let colored = compile_format(
        "[Red]\"USD \"# ?/?\" kg\";[Blue]\"USD \"# ?/?\" kg\"",
        FormatCompilePolicy::default(),
    )
    .unwrap();
    let negative = render_value(
        &CellValue::Number(-1.01),
        &colored,
        FormatRenderContext::default(),
    )
    .unwrap();
    assert_eq!(negative.text, "USD 1 kg");
    assert_eq!(negative.color.as_deref(), Some("Blue"));
}
#[test]
fn compiled_layout_is_reused_across_values_and_contexts() {
    let plan = compile_format("[Red]#,##0.00%;(0.0)", FormatCompilePolicy::default()).unwrap();
    let layout = plan.sections()[0].number_plan().unwrap().layout().unwrap();
    assert_eq!(layout.integer_placeholders, &['#', '#', '#', '0']);
    assert_eq!(layout.decimal_placeholders, &['0', '0']);
    assert!(layout.grouping);
    assert_eq!(layout.percent_signs, 1);
    assert_eq!(layout.suffix, "%");
    let address = layout.integer_placeholders.as_ptr();
    let before = plan.clone();
    let fingerprint = plan.fingerprint();
    for (n, text) in [
        (0., "0.00%"),
        (12.345, "1,234.50%"),
        (0.125, "12.50%"),
        (-2., "(200.00)"),
    ] {
        assert_eq!(render(&plan, n), text);
    }
    assert_eq!(plan, before);
    assert_eq!(plan.fingerprint(), fingerprint);
    assert_eq!(
        plan.sections()[0]
            .number_plan()
            .unwrap()
            .layout()
            .unwrap()
            .integer_placeholders
            .as_ptr(),
        address
    );
    let context = FormatRenderContext {
        date_system: DateSystem::Excel1904,
        width: Some(20),
        locale: FormatLocale::English,
    };
    assert_eq!(
        render_value(&CellValue::Number(0.125), &plan, context)
            .unwrap()
            .text,
        "12.50%"
    );
}
#[test]
fn scientific_scaling_general_and_optional_placeholders() {
    for (pattern, n, want) in [
        (".00", 0.5, ".50"),
        ("0.", 2., "2."),
        ("0.00,,", 1250000., "1.25"),
        ("0.##", 2., "2"),
        ("0.00E+00", 12345., "1.23E+04"),
        ("##0.0E+0", 12345., "12.3E+3"),
        ("0.00E+00", 999.995, "1.00E+03"),
        ("0.00", -0., "0.00"),
        ("\"value=\"General", 2.5, "value=2.5"),
    ] {
        let plan = compile_format(pattern, FormatCompilePolicy::default()).unwrap();
        assert_eq!(render(&plan, n), want, "{pattern}");
    }
    let plan = compile_format("0.00E+00", FormatCompilePolicy::default()).unwrap();
    assert_eq!(plan.sections()[0].kind(), SectionKind::Scientific);
    assert_eq!(
        plan.sections()[0]
            .number_plan()
            .unwrap()
            .layout()
            .unwrap()
            .scientific_exponent,
        Some((true, 2))
    );
    let overflow = compile_format("0.00", FormatCompilePolicy::default()).unwrap();
    assert!(
        render_value(
            &CellValue::Number(f64::MAX),
            &overflow,
            FormatRenderContext::default()
        )
        .is_err()
    );
}
#[test]
fn invalid_sections_fail_at_compilation_with_precise_diagnostics() {
    let parsed = parse_format("[>0]0;0.0.0;0;@").unwrap();
    let strict = compile_program(&parsed, FormatCompilePolicy::default()).unwrap_err();
    assert_eq!(strict.section, Some(1));
    let mut malformed = parsed.clone();
    malformed.sections[1].tokens = vec![Token::Digit('x')];
    let error = compile_program(&malformed, FormatCompilePolicy::default()).unwrap_err();
    assert_eq!(error.section, Some(1));
    assert_eq!(error.token, Some(0));
    assert!(format_value(&CellValue::Blank, &malformed, FormatOptions::default()).is_err());
}
#[test]
fn unsupported_numeric_token_placements_are_rejected() {
    for pattern in ["General0", "E+00", "0@", "0.0,0", "0E+00#"] {
        assert!(
            compile_format(pattern, FormatCompilePolicy::default()).is_err(),
            "{pattern}"
        );
    }
    for pattern in ["0.0.0", "0E+00E+00", "0\"x\"0"] {
        assert!(
            compile_format(pattern, FormatCompilePolicy::default()).is_err(),
            "{pattern}"
        );
    }
    let error = compile_format("0\"", FormatCompilePolicy::default()).unwrap_err();
    assert_eq!(error.section, None);
    assert_eq!(error.source.position, 1);
}
#[test]
fn compiled_sections_and_typed_values_are_inspectable() {
    let fraction = compile_format("# ?/?", FormatCompilePolicy::default()).unwrap();
    assert_eq!(fraction.sections()[0].kind(), SectionKind::Fraction);
    assert_eq!(render(&fraction, 1.5), "1 1/2");
    let date = compile_format("yyyy-mm-dd", FormatCompilePolicy::default()).unwrap();
    assert_eq!(date.sections()[0].kind(), SectionKind::DateTime);
    assert_eq!(render(&date, 60.), "1900-02-29");
    let text = compile_format("0;0;0;\"name: \"@", FormatCompilePolicy::default()).unwrap();
    assert!(text.sections()[3].uses_text_adapter());
    assert_eq!(
        render_value(
            &CellValue::Text("Ada".into()),
            &text,
            FormatRenderContext::default()
        )
        .unwrap()
        .text,
        "name: Ada"
    );
    let fill = compile_format("*x0", FormatCompilePolicy::default()).unwrap();
    assert!(
        render_value(
            &CellValue::Number(1.),
            &fill,
            FormatRenderContext::default()
        )
        .is_err()
    );
    let context = FormatRenderContext {
        width: Some(4),
        ..FormatRenderContext::default()
    };
    assert_eq!(
        render_value(&CellValue::Number(1.), &fill, context)
            .unwrap()
            .text,
        "xxx1"
    );
}
#[test]
fn policy_and_full_content_have_distinct_fingerprints() {
    let p = FormatCompilePolicy::default();
    let first = compile_format("[Red][>=0]0.00", p).unwrap();
    assert_eq!(
        first.fingerprint(),
        compile_format("[Red][>=0]0.00", p).unwrap().fingerprint()
    );
    for pattern in [
        "[Blue][>=0]0.00",
        "[Red][>0]0.00",
        "[Red][>=0]0.0",
        "[Red][>=0]0.00;",
    ] {
        assert_ne!(
            first.fingerprint(),
            compile_format(pattern, p).unwrap().fingerprint()
        );
    }
    let policy = FormatCompilePolicy {
        max_tokens_per_section: 1,
        ..p
    };
    assert!(compile_format("0.0", policy).is_err());
    let policy = FormatCompilePolicy {
        max_literal_characters: 1,
        ..p
    };
    assert!(compile_format("\"ab\"0", policy).is_err());
    let policy = FormatCompilePolicy {
        max_output_characters: 2,
        ..p
    };
    let plan = compile_format("0.0", policy).unwrap();
    assert!(
        render_value(
            &CellValue::Number(1.),
            &plan,
            FormatRenderContext::default()
        )
        .is_err()
    );
    assert_ne!(
        plan.fingerprint(),
        compile_format("0.0", p).unwrap().fingerprint()
    );
}
#[test]
fn compilation_is_snapshot_owned_and_bounded_for_public_programs() {
    let mut format = parse_format("0.00").unwrap();
    let plan = compile_program(&format, FormatCompilePolicy::default()).unwrap();
    format.sections[0].tokens.clear();
    assert_eq!(render(&plan, 1.5), "1.50");
    let alphabet = [
        Token::Digit('0'),
        Token::Digit('#'),
        Token::Decimal,
        Token::Comma,
        Token::Text,
        Token::General,
        Token::Exponent {
            positive_sign: true,
            digits: 2,
        },
    ];
    for i in 0..343 {
        let tokens = vec![
            alphabet[i % 7].clone(),
            alphabet[i / 7 % 7].clone(),
            alphabet[i / 49 % 7].clone(),
        ];
        let format = Format {
            sections: vec![Section {
                tokens,
                condition: None,
                color: None,
            }],
        };
        let result = std::panic::catch_unwind(|| {
            if let Ok(plan) = compile_program(&format, FormatCompilePolicy::default()) {
                let _ = render_value(
                    &CellValue::Number(1.25),
                    &plan,
                    FormatRenderContext::default(),
                );
            }
        });
        assert!(result.is_ok(), "{format:?}");
    }
}
