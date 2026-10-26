use poirot_functions::format::{FormatOptions, format_value, parse_format};
use poirot_functions::scalar::CellValue;

#[test]
fn real_unicode_noncharacter_is_not_a_fill_marker() {
    let literal = '\u{10ffff}';
    let rows = [
        (
            format!("\"{literal}\"*x0"),
            CellValue::Number(2.),
            5,
            format!("{literal}xxx2"),
        ),
        (
            format!("*x\"{literal}\"0"),
            CellValue::Number(2.),
            5,
            format!("xxx{literal}2"),
        ),
        (
            format!("\"{literal}\"*xGeneral"),
            CellValue::Number(2.),
            5,
            format!("{literal}xxx2"),
        ),
        (
            format!("\"{literal}\"*x# ?/?"),
            CellValue::Number(1.5),
            10,
            format!("{literal}xxxx1 1/2"),
        ),
        (
            format!("\"{literal}\"*xyyyy"),
            CellValue::Number(61.),
            9,
            format!("{literal}xxxx1900"),
        ),
        (
            "0;0;0;@*x".into(),
            CellValue::Text(literal.to_string()),
            5,
            format!("{literal}xxxx"),
        ),
    ];
    let mut failures = Vec::new();
    for (code, value, width, expected) in rows {
        let result = format_value(
            &value,
            &parse_format(&code).unwrap(),
            FormatOptions {
                width: Some(width),
                ..FormatOptions::default()
            },
        )
        .unwrap()
        .text;
        if result != expected {
            failures.push(format!(
                "{code:?}: expected {expected:?}, actual {result:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn multibyte_literals_and_text_have_character_width_and_exact_positions() {
    let rows = [
        ("\"😀é\"*x0", CellValue::Number(2.), "😀éxx2"),
        ("*x\"😀é\"0", CellValue::Number(2.), "xx😀é2"),
        ("0;0;0;@*x", CellValue::Text("😀é".into()), "😀éxxx"),
    ];
    for (code, value, expected) in rows {
        let result = format_value(
            &value,
            &parse_format(code).unwrap(),
            FormatOptions {
                width: Some(5),
                ..FormatOptions::default()
            },
        )
        .unwrap();
        assert_eq!(result.text, expected);
        assert_eq!(result.text.chars().count(), 5);
    }
}
