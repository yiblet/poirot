//! Source-derived DateParser regex/strict-calendar expectations.
use super::*;
#[test]
fn six_ordered_patterns_and_strict_first_match_resolution() {
    for text in [
        "2000-Jan-01",
        "01-Jan-2000",
        "Jan-01",
        "Jan/01/2000",
        "2000/Jan/01",
        "Jan/01",
    ] {
        let date = DatePlan::inspect(text).unwrap().resolve(2000).unwrap();
        assert_eq!((date.y, date.m, date.d), (2000, 1, 1), "{text}");
    }
    for text in ["2/29/1900", "2/31/2000", "13/01/2000"] {
        assert!(
            DatePlan::inspect(text).unwrap().resolve(2000).is_err(),
            "{text}"
        );
    }
    assert_eq!(
        DatePlan::inspect("0000-Jan-01")
            .unwrap()
            .resolve(2000)
            .unwrap()
            .y,
        0
    );
    assert!(DatePlan::inspect("1/2/2000").unwrap().resolve(2000).is_ok());
}
#[test]
fn anchors_java_line_endings_and_original_literal_space() {
    for ending in ["", "\n", "\r", "\r\n", "\u{85}", "\u{2028}", "\u{2029}"] {
        for prefix in ["1/01/2000", "1/01/2000 suffix"] {
            assert!(DatePlan::inspect(&format!("{prefix}{ending}")).is_ok());
        }
    }
    for text in [
        " 1/01/2000",
        "1/01/2000\ttime",
        "1/01/2000 suffix\nmore",
        "1/01/2000\n\n",
        "1/01/2000suffix",
    ] {
        assert!(DatePlan::inspect(text).is_err(), "{text:?}");
    }
    assert!(DatePlan::inspect("1/01/2000 \tsuffix").is_ok());
    assert!(
        DatePlan::inspect("jan/01/2000")
            .unwrap()
            .resolve(2000)
            .is_ok()
    );
}
