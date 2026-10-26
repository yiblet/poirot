//! Text-placeholder expectations follow Microsoft's custom-number-format guidelines:
//! https://support.microsoft.com/en-us/excel/review-guidelines-for-customizing-a-number-format
//! Numeric visibility under @ is inferred from the documented Text format:
//! https://support.microsoft.com/en-us/excel/get-started/available-number-formats-in-excel
use poirot_format::{
    FormatCompilePolicy, FormatErrorKind, FormatRenderContext, compile_format, render_value,
};
use poirot_model::{CellValue, ExcelText};

#[test]
fn single_text_section_preserves_numbers_and_substitutes_text() {
    for pattern in ["@", "\"Item: \"@", "[Red]\"Item: \"@"] {
        let plan = compile_format(pattern, Default::default()).unwrap();
        for number in [123., -123., 0., 1.25] {
            let value = CellValue::Number(number);
            let before = value.clone();
            let output = render_value(&value, &plan, Default::default()).unwrap();
            assert_eq!(output.text, number.to_string());
            assert_eq!(value, before);
            assert!(
                output.text.len() <= plan.render_byte_bound(&value, Default::default()).unwrap()
            );
        }
        for value in [
            CellValue::Text("é😀".into()),
            CellValue::TextUtf16(ExcelText::from("é😀")),
        ] {
            let before = value.clone();
            let output = render_value(&value, &plan, Default::default()).unwrap();
            assert_eq!(
                output.text,
                if pattern == "@" {
                    "é😀"
                } else {
                    "Item: é😀"
                }
            );
            assert_eq!(
                output.color.as_deref(),
                pattern.starts_with("[Red]").then_some("Red")
            );
            assert_eq!(value, before);
            assert!(
                output.text.len() <= plan.render_byte_bound(&value, Default::default()).unwrap()
            );
        }
    }
}

#[test]
fn numeric_formats_and_four_sections_keep_their_selection() {
    // Mixed numeric/text grammars remain unsupported rather than guessed.
    assert!(compile_format("0@", Default::default()).is_err());
    for pattern in ["0.00", "0;[Red](0)", "\"literal\"", "0;0;0;\"last: \"@"] {
        let plan = compile_format(pattern, Default::default()).unwrap();
        assert_eq!(
            render_value(&CellValue::Text("hello".into()), &plan, Default::default())
                .unwrap()
                .text,
            if pattern.contains("last:") {
                "last: hello"
            } else {
                "hello"
            }
        );
    }
    let plan = compile_format(
        "\"pos\"0;[Red]\"neg\"0;\"zero\";\"last: \"@",
        Default::default(),
    )
    .unwrap();
    for (value, expected, color) in [
        (2., "pos2", None),
        (-2., "neg2", Some("Red")),
        (0., "zero", None),
    ] {
        let output = render_value(&CellValue::Number(value), &plan, Default::default()).unwrap();
        assert_eq!(output.text, expected);
        assert_eq!(output.color.as_deref(), color);
    }
}

#[test]
fn single_text_sections_enforce_character_budgets_and_utf16_fidelity() {
    let value = CellValue::TextUtf16(ExcelText::from("é😀"));
    for (pattern, limit, expected) in [
        ("\"x\"@", 3, Some("xé😀")),
        ("@@", 4, Some("é😀é😀")),
        ("\"x\"@", 2, None),
        ("@@", 3, None),
    ] {
        let plan = compile_format(
            pattern,
            FormatCompilePolicy {
                max_output_characters: limit,
                ..Default::default()
            },
        )
        .unwrap();
        let output = render_value(&value, &plan, Default::default());
        match expected {
            Some(text) => assert_eq!(output.unwrap().text, text),
            None => assert_eq!(output.unwrap_err().kind, FormatErrorKind::ResourceLimit),
        }
    }
    for limit in [2, 3] {
        let plan = compile_format(
            "@",
            FormatCompilePolicy {
                max_output_characters: limit,
                ..Default::default()
            },
        )
        .unwrap();
        let output = render_value(&CellValue::Number(123.), &plan, Default::default());
        if limit == 3 {
            assert_eq!(output.unwrap().text, "123");
        } else {
            assert_eq!(output.unwrap_err().kind, FormatErrorKind::ResourceLimit);
        }
    }
    let plan = compile_format("[Red]\"x\"@*🦀", Default::default()).unwrap();
    let output = render_value(
        &value,
        &plan,
        FormatRenderContext {
            width: Some(5),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(output.text, "xé😀🦀🦀");
    assert_eq!(output.color.as_deref(), Some("Red"));
    for units in [vec![0xd800], vec![0xdc00]] {
        let value = CellValue::TextUtf16(ExcelText::from_units(units));
        assert_eq!(
            render_value(&value, &plan, Default::default())
                .unwrap_err()
                .kind,
            FormatErrorKind::UnsupportedFidelity
        );
        assert_eq!(
            plan.render_byte_bound(&value, Default::default())
                .unwrap_err()
                .kind,
            FormatErrorKind::UnsupportedFidelity
        );
    }
    let plan = compile_format(
        "@",
        FormatCompilePolicy {
            max_output_characters: usize::MAX,
            ..Default::default()
        },
    )
    .unwrap();
    for length in [32767, 32768] {
        let output = render_value(
            &CellValue::Text("é".repeat(length)),
            &plan,
            Default::default(),
        );
        if length == 32767 {
            assert_eq!(output.unwrap().text.chars().count(), length);
        } else {
            assert_eq!(output.unwrap_err().kind, FormatErrorKind::ResourceLimit);
        }
    }
}
