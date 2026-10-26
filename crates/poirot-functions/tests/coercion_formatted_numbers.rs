use poirot_functions::coercion::coercev2_formatted_number;
use poirot_functions::coercion::compatibility::NumericTextDecision;
use poirot_functions::scalar::CellError;
use poirot_parsing::number::NumberLocale;
fn parse(text: &str, locale: &NumberLocale) -> Option<Result<f64, CellError>> {
    match coercev2_formatted_number(text, locale, |_, _| Ok::<_, ()>(())).unwrap() {
        NumericTextDecision::Number(value) => Some(value),
        NumericTextDecision::DateTime => None,
    }
}
#[test]
fn formatted_numeric_grammars_preserve_consumer_boundaries() {
    let locale = NumberLocale::EN_US;
    for (text, expected) in [
        ("1,234.5", 1234.5),
        ("-.5e+2", -50.),
        ("$1.44", 1.44),
        ("($ 1,000)", -1000.),
        ("1,000$", 1000.),
        ("1,000-", -1000.),
        ("12.5%", 0.125),
        (" (100) % ", -1.),
        (" -75 % ", -0.75),
        ("100.%", 1.),
        ("1 00100/025", 5.),
        ("- 1 1/2", -1.5),
        ("0 32767/1", 32767.),
        ("90:00", 90. / 24.),
        ("9999:", 9999. / 24.),
        ("  24   :  00  :   59  ", 1. + 59. / 86400.),
        ("24:0:", 1.),
        ("140:30.0", 8430. / 86400.),
        ("00:57.180", 57.18 / 86400.),
        ("0:00.0005", 0.0005 / 86400.),
        ("00:57.1809", 57.1809 / 86400.),
    ] {
        let actual = parse(text, &locale)
            .unwrap_or_else(|| panic!("no match: {text}"))
            .unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert!(
            (actual - expected).abs() <= f64::EPSILON * expected.abs().max(1.),
            "{text}: {actual} vs {expected}"
        );
    }
    for text in [
        "",
        ".",
        "1e",
        "1e+",
        "1,,2",
        "1,",
        ",1",
        "1.2.3",
        "1.2,3",
        "12%tail",
        "$",
        "$$1",
        "1+2",
        "0 /20",
        "000:01:01",
        "60:000.0",
        "24:60",
        "140:60.0",
        "00:60:60",
        "(-1)",
        "1 100/-2",
        "1 32768/2",
        "1 1/0",
        "1/2",
        "2020/01/02",
    ] {
        assert!(
            parse(text, &locale).is_none_or(|value| value.is_err()),
            "{text}"
        );
    }
    assert!(matches!(
        parse("1e999", &locale),
        Some(Err(CellError::Value))
    ));
    assert!(matches!(
        parse("hundred%", &locale),
        Some(Err(CellError::Value))
    ));
    assert!(matches!(
        parse("% 12.5", &locale),
        Some(Err(CellError::Value))
    ));
    // A failed numeric grammar is not proof that calendar interpretation fails.
    assert!(parse("1/2", &locale).is_none());
}
#[test]
fn locale_tokens_are_runtime_strings_and_recognition_never_guesses() {
    let comma = NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap();
    assert_eq!(parse("EUR1.234,5", &comma), Some(Ok(1234.5)));
    assert_eq!(parse("1,234", &comma), Some(Ok(1.234)));
    assert_eq!(parse("1,234", &NumberLocale::EN_US), Some(Ok(1234.)));
    assert_eq!(parse("1:02,5", &comma), Some(Ok(62.5 / 86400.)));
    let space = NumberLocale::new(",", Some(" "), Some("SEK"), "%", ":").unwrap();
    assert_eq!(parse("SEK 1 234,5", &space), Some(Ok(1234.5)));
    let unicode = NumberLocale::new(",", Some("\u{202f}"), Some("€"), "%", ":").unwrap();
    assert_eq!(parse("€1\u{202f}234,5", &unicode), Some(Ok(1234.5)));
    assert!(parse("€1 234,5", &unicode).is_none());
    let multi = NumberLocale::new("::", Some("__"), Some("USD"), "pct", "|").unwrap();
    assert_eq!(parse("USD1__234::5", &multi), Some(Ok(1234.5)));
    assert_eq!(parse("50::0pct", &multi), Some(Ok(0.5)));
    let disabled = NumberLocale::new(".", None, None, "%", ":").unwrap();
    for text in ["1\0-", "1\0$", "1\0 2"] {
        assert!(parse(text, &disabled).is_none(), "{text:?}");
    }
}
#[test]
fn empty_or_overlapping_tokens_cannot_create_nonadvancing_parsers() {
    for (decimal, group, currency, percent, time) in [
        ("", Some(","), Some("$"), "%", ":"),
        (".", Some(""), Some("$"), "%", ":"),
        (".", Some(".."), Some("$"), "%", ":"),
        (".", Some(","), Some(""), "%", ":"),
        (".", Some(","), Some("$"), "", ":"),
        (".", Some(","), Some("$"), "%", ""),
        ("e", Some(","), Some("$"), "%", ":"),
    ] {
        assert!(NumberLocale::new(decimal, group, currency, percent, time).is_none());
    }
}
#[test]
fn admission_failure_precedes_any_grammar_or_normalization() {
    let mut calls = 0;
    let result = coercev2_formatted_number("1,234.5%", &NumberLocale::EN_US, |bytes, work| {
        calls += 1;
        assert!(bytes >= 8 && work >= 8);
        Err("budget denied")
    });
    assert!(matches!(result, Err("budget denied")));
    assert_eq!(calls, 1);
}
