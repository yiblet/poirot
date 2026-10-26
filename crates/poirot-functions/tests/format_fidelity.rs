//! UTF-16 ingress remains text; malformed units are a capability failure.
use poirot_functions::format::{
    FormatCompilePolicy, FormatErrorKind, FormatRenderContext, compile_format, render_value,
};
use poirot_functions::scalar::{CellValue, ExcelText};
#[test]
fn manually_retained_valid_utf16_uses_text_section_color_and_fill() {
    let plan = compile_format("0;0;0;[Red]\"name: \"@*🦀", FormatCompilePolicy::default()).unwrap();
    let context = FormatRenderContext {
        width: Some(11),
        ..Default::default()
    };
    let utf16 = CellValue::TextUtf16(ExcelText::from("é😀"));
    let ordinary = CellValue::Text("é😀".into());
    assert_eq!(
        plan.render_byte_bound(&utf16, context).unwrap(),
        plan.render_byte_bound(&ordinary, context).unwrap()
    );
    let rendered = render_value(&utf16, &plan, context).unwrap();
    assert_eq!(rendered.text, "name: é😀🦀🦀🦀");
    assert_eq!(rendered.color.as_deref(), Some("Red"));
    assert_eq!(rendered, render_value(&ordinary, &plan, context).unwrap());
}
#[test]
fn unpaired_units_are_typed_fidelity_failure_without_replacement() {
    for units in [vec![0xd800], vec![0xdc00], vec![0xd800, 65]] {
        let value = CellValue::TextUtf16(ExcelText::from_units(units));
        for pattern in ["General", "0;0;0;@", "0;0;0;"] {
            let plan = compile_format(pattern, FormatCompilePolicy::default()).unwrap();
            assert_eq!(
                plan.render_byte_bound(&value, Default::default())
                    .unwrap_err()
                    .kind,
                FormatErrorKind::UnsupportedFidelity
            );
            assert_eq!(
                render_value(&value, &plan, Default::default())
                    .unwrap_err()
                    .kind,
                FormatErrorKind::UnsupportedFidelity
            );
        }
    }
}
#[test]
fn tiny_character_budget_precedes_projection_and_repeated_substitution() {
    let policy = FormatCompilePolicy {
        max_output_characters: 2,
        ..Default::default()
    };
    let plan = compile_format("0;0;0;@@", policy).unwrap();
    let value = CellValue::TextUtf16(ExcelText::from("é😀"));
    assert_eq!(
        render_value(&value, &plan, Default::default())
            .unwrap_err()
            .kind,
        FormatErrorKind::ResourceLimit
    );
    let plan = compile_format("0;0;0;@", policy).unwrap();
    assert_eq!(
        render_value(&value, &plan, Default::default())
            .unwrap()
            .text,
        "é😀"
    );
}
