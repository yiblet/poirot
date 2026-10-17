//! Source-derived OpenJDK/POI expectations; no new native observations.
use super::*;
fn parse(text: &str) -> Plan {
    Plan::inspect(SourceProfile::EnglishUtc, text).unwrap()
}
#[test]
fn optional_transactions_commit_and_restore_fields_and_input() {
    let mut plan = Plan::default();
    let pattern = b"[H:m][H:m]";
    let tail = section("12:x13:30", pattern, 0, pattern.len(), &mut plan).unwrap();
    assert_eq!(tail, "12:x13:30");
    assert_eq!(plan.fields, [None; 9]);
    let pattern = b"[H:m][ H:m]";
    let tail = section("12:30 13:30", pattern, 0, pattern.len(), &mut plan).unwrap();
    assert_eq!(tail, " 13:30");
    assert_eq!(plan.fields[3], Some(12));
    assert_eq!(plan.fields[5], Some(30));
    assert!(Plan::inspect(SourceProfile::EnglishUtc, "12:30 13:30").is_err());
    assert_eq!(parse("12:30 12:30").resolve(2000).unwrap().seconds, 45000);
}
#[test]
fn adjacent_month_tokens_require_matching_fields() {
    let plan = parse("1 JanuaryJan 2000");
    let date = plan.resolve(2000).unwrap().date.unwrap();
    assert_eq!((date.y, date.m, date.d), (2000, 1, 1));
    assert!(Plan::inspect(SourceProfile::EnglishUtc, "1 JanuaryFeb 2000").is_err());
    assert!(Plan::inspect(SourceProfile::EnglishUtc, "1 january 2000").is_err());
}
#[test]
fn smart_clock_nanos_and_excess_day_are_source_rules() {
    for (text, seconds) in [
        ("0:00 AM", 0),
        ("0:00 PM", 43200),
        ("12:00 AM", 0),
        ("12:00 PM", 43200),
        ("24:00:00.000", 0),
        ("12:03:45.386", 43425),
    ] {
        assert_eq!(
            parse(text).resolve(2000).unwrap().seconds,
            seconds,
            "{text}"
        );
    }
    for text in ["24:00:00.001", "24:01", "25:00"] {
        assert!(parse(text).resolve(2000).is_err(), "{text}");
    }
    let date = parse("12/31/1899 24:00")
        .resolve(2000)
        .unwrap()
        .date
        .unwrap();
    assert_eq!((date.y, date.m, date.d), (1900, 1, 1));
    let date = parse("2/31/2001").resolve(2000).unwrap().date.unwrap();
    assert_eq!((date.y, date.m, date.d), (2001, 2, 28));
}
#[test]
fn signed_width_and_overflow_leave_source_parser_tail() {
    assert_eq!(numeric("-1:", 1, false), Ok((":", -1)));
    for text in ["-0:", "+1:"] {
        assert!(numeric(text, 1, false).is_err());
    }
    assert!(numeric("+2000/", 4, true).is_err());
    assert!(numeric("10000/", 4, true).is_err());
    assert_eq!(numeric("+10000/", 4, true), Ok(("/", 10000)));
    assert!(numeric("1/", 2, false).is_err());
    assert_eq!(numeric("01/", 2, false), Ok(("/", 1)));
    assert_eq!(
        numeric("9223372036854775808:", 1, false),
        Ok(("8:", 922337203685477580))
    );
    assert_eq!(
        numeric("-9223372036854775809:", 1, false),
        Ok(("9:", -922337203685477580))
    );
}
