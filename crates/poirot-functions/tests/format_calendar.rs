use poirot_functions::format::{
    CalendarPolicy, FormatOptions, PhantomDayPolicy, format_value, parse_format,
};
use poirot_functions::scalar::{CellValue, DateSystem};

fn display(
    serial: f64,
    system: DateSystem,
    code: &str,
) -> Result<String, poirot_functions::format::FormatError> {
    format_value(
        &CellValue::Number(serial),
        &parse_format(code).unwrap(),
        FormatOptions {
            date_system: system,
            width: None,
        },
    )
    .map(|v| v.text)
}
#[test]
fn explicit_boundaries_and_final_day_times() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let policy = CalendarPolicy::display(system);
        let max = policy.maximum_day() as f64;
        assert_eq!(
            display(max + 0.5, system, "yyyy-mm-dd hh:mm:ss").unwrap(),
            "9999-12-31 12:00:00"
        );
        assert_eq!(
            display(max + 86399. / 86400., system, "yyyy-mm-dd hh:mm:ss").unwrap(),
            "9999-12-31 23:59:59"
        );
        for invalid in [-0.5, max + 1., f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert!(policy.validate(invalid).is_err());
        }
        assert!(display(max + 0.9999999, system, "yyyy-mm-dd hh:mm:ss").is_err());
    }
}
#[test]
fn phantom_display_and_gregorian_calculation_are_distinct() {
    for (serial, expected) in [
        (0., "1899-12-31"),
        (59., "1900-02-28"),
        (60., "1900-02-29"),
        (61., "1900-03-01"),
    ] {
        assert_eq!(
            display(serial, DateSystem::Excel1900, "yyyy-mm-dd").unwrap(),
            expected
        );
    }
    let display_policy = CalendarPolicy::display(DateSystem::Excel1900);
    assert!(display_policy.date(60.).unwrap().phantom);
    let calculation = CalendarPolicy {
        phantom_day: PhantomDayPolicy::GregorianMarch1,
        ..display_policy
    };
    let date = calculation.date(60.).unwrap();
    assert_eq!(
        (date.year, date.month, date.day, date.phantom),
        (1900, 3, 1, false)
    );
    assert_eq!(
        display(0., DateSystem::Excel1904, "yyyy-mm-dd").unwrap(),
        "1904-01-01"
    );
    assert_eq!(
        display(59., DateSystem::Excel1904, "yyyy-mm-dd").unwrap(),
        "1904-02-29"
    );
}
#[test]
fn time_rounding_elapsed_and_fractional_seconds() {
    assert_eq!(
        display(1.5, DateSystem::Excel1900, "[h]:mm:ss").unwrap(),
        "36:00:00"
    );
    assert_eq!(
        display(60.5, DateSystem::Excel1900, "yyyy-mm-dd hh:mm:ss.000").unwrap(),
        "1900-02-29 12:00:00.000"
    );
    assert_eq!(
        display(1. + 59.9996 / 86400., DateSystem::Excel1900, "hh:mm:ss.000").unwrap(),
        "00:01:00.000"
    );
    assert!(display(1., DateSystem::Excel1900, "ss.0000000").is_err());
}

#[test]
fn preflight_covers_utf8_fill_substitution_and_large_numeric_expansion() {
    use poirot_functions::format::{
        FormatCompilePolicy, FormatRenderContext, compile_format, render_value,
    };
    for (code, value, width) in [
        ("0.00", CellValue::Number(f64::MAX), None),
        ("0.00E+00", CellValue::Number(1e100), None),
        ("# ??/??", CellValue::Number(12345.25), None),
        ("yyyy-mm-dd hh:mm:ss.000000", CellValue::Number(60.5), None),
        ("*🦀0", CellValue::Number(1.), Some(32767)),
        ("0;0;0;@\"😀\"@", CellValue::Text("é🦀".repeat(100)), None),
    ] {
        let plan = compile_format(code, FormatCompilePolicy::default()).unwrap();
        let context = FormatRenderContext {
            width,
            ..Default::default()
        };
        let bound = plan.render_byte_bound(&value, context).unwrap();
        // Some numeric magnitudes deliberately fail finite rounding; their
        // preflight still succeeds without performing rendering allocations.
        if let Ok(output) = render_value(&value, &plan, context) {
            assert!(output.text.len() <= bound);
        }
    }
}
