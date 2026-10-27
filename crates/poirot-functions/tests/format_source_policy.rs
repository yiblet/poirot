use poirot_functions::format::{
    FormatCompilePolicy, FormatSourcePolicy, compile_format, compile_format_with_source,
    compile_program_with_source, parse_format, render_value,
};
use poirot_functions::scalar::CellValue;

#[test]
fn source_identity_normalization_and_observed_zero_controls() {
    let policy = FormatCompilePolicy::default();
    let source = FormatSourcePolicy::Poi7321209DataFormatter;
    for (pattern, number, expected) in [
        ("$###,###", 0., "$0"),
        ("###", 0., "0"),
        ("$###,###", 0.4, "$0"),
        ("$###,###", -0.4, "-$0"),
        ("$###,###", 0.5, "$1"),
        ("$###,###", -0.5, "-$1"),
        ("$#,##0", 0., "$0"),
        ("#.##", 0., "0"),
        (".###", 0., ".0"),
        (".###", 0.4, ".4"),
        ("###.", 0., "0."),
        ("#.##", 0.4, "0.4"),
        ("###%", 0.004, "0%"),
        ("###,", 400., "0"),
    ] {
        let pinned = compile_format_with_source(pattern, policy, source).unwrap();
        let old = compile_format(pattern, policy).unwrap();
        assert_eq!(pinned.source_policy(), source);
        assert_eq!(pinned, old);
        assert_eq!(pinned.fingerprint(), old.fingerprint());
        assert_eq!(
            pinned,
            compile_program_with_source(&parse_format(pattern).unwrap(), policy, source).unwrap()
        );
        assert_eq!(
            pinned,
            compile_format_with_source(pattern, policy, source).unwrap()
        );
        assert_eq!(
            pinned.sections()[0]
                .number_plan()
                .unwrap()
                .render(number)
                .unwrap(),
            expected
        );
        assert_eq!(
            render_value(&CellValue::Number(number), &pinned, Default::default())
                .unwrap()
                .text,
            expected
        );
    }
    let pinned = compile_format_with_source("#.##", policy, source).unwrap();
    assert_eq!(
        pinned.sections()[0]
            .number_plan()
            .unwrap()
            .layout()
            .unwrap()
            .decimal_placeholders,
        &['#', '#']
    );
}

#[test]
fn unobserved_routes_preserve_existing_layouts_and_intentional_hiding() {
    for pattern in ["???", "#E+00", "\"hidden\"", "", "###;###;", "[>1]###;###"] {
        let old = compile_format(pattern, Default::default()).unwrap();
        let pinned = compile_format_with_source(
            pattern,
            Default::default(),
            FormatSourcePolicy::Poi7321209DataFormatter,
        )
        .unwrap();
        for number in [0., 0.4, 2.] {
            assert_eq!(
                render_value(&CellValue::Number(number), &old, Default::default()).unwrap(),
                render_value(&CellValue::Number(number), &pinned, Default::default()).unwrap()
            );
        }
    }
}

#[test]
fn observed_fraction_and_section_routes_preserve_source_specific_layouts() {
    for (pattern, number, pinned) in [
        ("# ?/?", 0., "0"),
        ("# ?/?", 1., "1"),
        ("# ?/?", 0.5, "1/2"),
        ("#.0;(#.00)", 1.25, "1.3"),
        ("#.0;(#.00)", -1.25, "(1.3)"),
        ("$###,###;-$###,###;\"ZERO\"", 0., "ZERO"),
    ] {
        let source = FormatSourcePolicy::Poi7321209DataFormatter;
        let plan = compile_format_with_source(pattern, Default::default(), source).unwrap();
        assert_eq!(plan.source_policy(), source);
        assert_eq!(
            render_value(&CellValue::Number(number), &plan, Default::default())
                .unwrap()
                .text,
            pinned,
            "{pattern} {number}"
        );
    }
}
