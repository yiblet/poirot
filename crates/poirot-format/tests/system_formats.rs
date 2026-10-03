use poirot_format::{
    FormatCompilePolicy, FormatErrorKind, FormatLocale, FormatRenderContext, SectionKind,
    compile_format, render_value,
};
use poirot_model::{CellValue, DateSystem};

fn display(serial: f64, pattern: &str, context: FormatRenderContext) -> String {
    let plan = compile_format(pattern, FormatCompilePolicy::default()).unwrap();
    assert_eq!(plan.sections()[0].kind(), SectionKind::LegacyDateTime);
    render_value(&CellValue::Number(serial), &plan, context)
        .unwrap()
        .text
}

#[test]
fn archived_serials_have_long_dates_under_pinned_us_policy() {
    for (serial, want) in [
        (46082., "Sunday, March 1, 2026"),
        (46447., "Monday, March 1, 2027"),
    ] {
        assert_eq!(
            display(serial, "[$-F800]", FormatRenderContext::default()),
            want
        );
        // Serialized creation-language fallback remains in the program, but the
        // configured system selector owns the date display.
        assert_eq!(
            display(serial, "[$-f800]yyyy-mm-dd", FormatRenderContext::default()),
            want
        );
    }
    assert_eq!(
        display(46082.99999999, "[$-F800]", FormatRenderContext::default()),
        "Sunday, March 1, 2026"
    );
}

#[test]
fn companion_time_and_date_system_are_explicit() {
    assert_eq!(
        display(
            46082.5,
            "[$-F400]h:mm:ss AM/PM",
            FormatRenderContext::default()
        ),
        "12:00:00 PM"
    );
    assert_eq!(
        display(
            0.,
            "[$-F800]",
            FormatRenderContext {
                date_system: DateSystem::Excel1904,
                ..Default::default()
            }
        ),
        "Friday, January 1, 1904"
    );
}

#[test]
fn locale_failure_is_render_time_and_plan_reusable() {
    let plan = compile_format("[$-F800]", FormatCompilePolicy::default()).unwrap();
    let identity = plan.clone();
    let context = FormatRenderContext {
        locale: FormatLocale::from_lcid(0x0407),
        ..Default::default()
    };
    let error = render_value(&CellValue::Number(46082.), &plan, context).unwrap_err();
    assert_eq!(error.kind, FormatErrorKind::UnsupportedFidelity);
    assert!(
        error
            .message
            .contains("unsupported system date/time locale")
    );
    assert_eq!(plan, identity);
    assert_eq!(
        display(
            46082.,
            "[$-F800]",
            FormatRenderContext {
                locale: FormatLocale::from_lcid(0x0409),
                ..Default::default()
            }
        ),
        "Sunday, March 1, 2026"
    );
    assert!(compile_format("[$-409]yyyy", FormatCompilePolicy::default()).is_err());
    for pattern in ["[$-F800][$-F400]", "[$-F800]0", "yyyy[$-F800]"] {
        assert!(compile_format(pattern, FormatCompilePolicy::default()).is_err());
    }
}

#[test]
fn fidelity_and_output_resource_guards_remain_active() {
    let plan = compile_format(
        "[$-F800]",
        FormatCompilePolicy {
            max_output_characters: 4,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(render_value(&CellValue::Number(46082.), &plan, Default::default()).is_err());
    // Invalid UTF-16 is rejected even when the selected format has no text section.
    let value = CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd800]));
    assert!(render_value(&value, &plan, Default::default()).is_err());
}
