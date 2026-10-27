//! Pinned POI DataFormatter numeric affixes use Java DecimalFormat semantics.
use poirot_functions::format::{
    FormatCompilePolicy, FormatErrorKind, FormatRenderContext, compile_format, render_value,
};
use poirot_functions::scalar::{CellValue, ExcelText};

#[test]
fn omitted_negative_subpattern_sign_precedes_entire_numeric_prefix() {
    for (number, pattern, expected) in [
        (-6653., "$###,###", "-$6,653"),
        (6653., "$###,###", "$6,653"),
        (0., "$0", "$0"),
        (-0., "$0", "$0"),
        (-6653., "#,##0$", "-6,653$"),
        (-2., "\\-0", "--2"),
        (-2., "\"-\"0", "--2"),
        (-2., "(0)", "-(2)"),
        (-2., "0-", "-2-"),
        (-0.125, "$0.0%", "-$12.5%"),
        (-12345., "$0.00E+00", "-$1.23E+04"),
        (-0.125, "0.0%$", "-12.5%$"),
        (-12345., "0.00E+00$", "-1.23E+04$"),
        (-1250000., "$0.00,,", "-$1.25"),
        (-6653., "$#,##0;($#,##0)", "($6,653)"),
        (-6653., "$#,##0;$-#,##0", "$-6,653"),
        (-6653., "$#,##0;#,##0$-", "6,653$-"),
    ] {
        let plan = compile_format(pattern, FormatCompilePolicy::default()).unwrap();
        assert_eq!(
            render_value(&CellValue::Number(number), &plan, Default::default())
                .unwrap()
                .text,
            expected,
            "{number} {pattern}"
        );
    }
}

#[test]
fn moved_sign_preserves_unicode_fill_color_and_character_budget() {
    let plan = compile_format("[Red]\"é😀$\"*x0", FormatCompilePolicy::default()).unwrap();
    let context = FormatRenderContext {
        width: Some(8),
        ..Default::default()
    };
    let rendered = render_value(&CellValue::Number(-2.), &plan, context).unwrap();
    assert_eq!(rendered.text, "-é😀$xxx2");
    assert_eq!(rendered.color.as_deref(), Some("Red"));
    assert!(
        rendered.text.len()
            <= plan
                .render_byte_bound(&CellValue::Number(-2.), context)
                .unwrap()
    );
    let policy = FormatCompilePolicy {
        max_output_characters: 2,
        ..Default::default()
    };
    let plan = compile_format("$0", policy).unwrap();
    assert_eq!(
        render_value(&CellValue::Number(2.), &plan, Default::default())
            .unwrap()
            .text,
        "$2"
    );
    assert_eq!(
        render_value(&CellValue::Number(-2.), &plan, Default::default())
            .unwrap_err()
            .kind,
        FormatErrorKind::ResourceLimit
    );
}

#[test]
fn numeric_affix_change_retains_text_fidelity_errors() {
    let plan = compile_format("$0;($0);$0;@", FormatCompilePolicy::default()).unwrap();
    let malformed = CellValue::TextUtf16(ExcelText::from_units(vec![0xd800]));
    assert_eq!(
        plan.render_byte_bound(&malformed, Default::default())
            .unwrap_err()
            .kind,
        FormatErrorKind::UnsupportedFidelity
    );
    assert_eq!(
        render_value(&malformed, &plan, Default::default())
            .unwrap_err()
            .kind,
        FormatErrorKind::UnsupportedFidelity
    );
    let valid = CellValue::TextUtf16(ExcelText::from("é😀"));
    assert_eq!(
        render_value(&valid, &plan, Default::default())
            .unwrap()
            .text,
        "é😀"
    );
}
