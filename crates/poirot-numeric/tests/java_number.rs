use poirot_numeric::parse_java_number;

#[test]
fn java_grammar_and_binary64_rounding_are_separate_policies() {
    for (text, expected) in [
        ("\u{0}\t +3.0f \r\n", 3.0),
        (".5D", 0.5),
        ("1.e+2", 100.0),
        ("0x1.00000000000008p0D", 1.0),
        ("0x1.00000000000009p0", f64::from_bits(1f64.to_bits() + 1)),
        ("-0x1p-1075", -0.0),
        ("0x0.0000000000001p-1022f", f64::from_bits(1)),
        ("-Infinity", f64::NEG_INFINITY),
        ("1e999", f64::INFINITY),
    ] {
        assert_eq!(
            parse_java_number(text).map(f64::to_bits),
            Some(expected.to_bits()),
            "{text:?}"
        );
    }
    for text in ["NaN", "+NaN", "-NaN"] {
        assert!(parse_java_number(text).is_some_and(f64::is_nan), "{text}");
    }
    for text in [
        "",
        "inf",
        "nan",
        "InfinityD",
        "NaNf",
        "3\u{a0}",
        "３",
        "++1",
        "+-0x1p0",
        "0xp0",
        "0x1p0p1",
        "1_0",
        "1,0",
    ] {
        assert_eq!(parse_java_number(text), None, "{text:?}");
    }
}
