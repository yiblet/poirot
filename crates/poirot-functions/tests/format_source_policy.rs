use poirot_functions::format::{
    FormatCompilePolicy, compile_format, compile_program, parse_format, render_value,
};
use poirot_functions::scalar::CellValue;

#[test]
fn normalization_and_observed_zero_controls() {
    let policy = FormatCompilePolicy::default();
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
        let plan = compile_format(pattern, policy).unwrap();
        let old = compile_format(pattern, policy).unwrap();

        assert_eq!(plan, old);
        assert_eq!(plan.fingerprint(), old.fingerprint());
        assert_eq!(
            plan,
            compile_program(&parse_format(pattern).unwrap(), policy).unwrap()
        );
        assert_eq!(plan, compile_format(pattern, policy).unwrap());
        assert_eq!(
            plan.sections()[0]
                .number_plan()
                .unwrap()
                .render(number)
                .unwrap(),
            expected
        );
        assert_eq!(
            render_value(&CellValue::Number(number), &plan, Default::default())
                .unwrap()
                .text,
            expected
        );
    }
    let plan = compile_format("#.##", policy).unwrap();
    assert_eq!(
        plan.sections()[0]
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
        let plan = compile_format(pattern, Default::default()).unwrap();
        for number in [0., 0.4, 2.] {
            assert_eq!(
                render_value(&CellValue::Number(number), &old, Default::default()).unwrap(),
                render_value(&CellValue::Number(number), &plan, Default::default()).unwrap()
            );
        }
    }
}

#[test]
fn observed_fraction_and_section_routes_preserve_documented_layouts() {
    for (pattern, number, expected) in [
        ("# ?/?", 0., "0"),
        ("# ?/?", 1., "1"),
        ("# ?/?", 0.5, "1/2"),
        ("#.0;(#.00)", 1.25, "1.3"),
        ("#.0;(#.00)", -1.25, "(1.3)"),
        ("$###,###;-$###,###;\"ZERO\"", 0., "ZERO"),
    ] {
        let plan = compile_format(pattern, Default::default()).unwrap();

        assert_eq!(
            render_value(&CellValue::Number(number), &plan, Default::default())
                .unwrap()
                .text,
            expected,
            "{pattern} {number}"
        );
    }
}
