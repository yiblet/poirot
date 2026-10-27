use poirot_functions::coercion::compatibility::*;
use poirot_functions::coercion::{CellError, CellValue, Origin, TextRef};
use poirot_functions::scalar::Scalar;

#[derive(Debug, PartialEq)]
enum Failure {
    Worksheet(CellError),
    Operational,
}
impl From<CellError> for Failure {
    fn from(value: CellError) -> Self {
        Self::Worksheet(value)
    }
}

#[test]
fn finite_consumers_reject_nonfinite_numbers_without_entering_text_fallback() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let result =
            coerce_number::<Failure>(ScalarRef::Number(number), NumberPolicy::Finite, |_| {
                panic!("numeric input must not parse text")
            });
        assert_eq!(result, Err(Failure::Worksheet(CellError::Num)));
    }
}

#[test]
fn arithmetic_operand_policy_preserves_original_nan_payload_and_signed_zero() {
    for bits in [0xfff8_0000_0000_0042, 0x8000_0000_0000_0000] {
        let result =
            coerce_arithmetic_operand::<CellError>(ScalarRef::Number(f64::from_bits(bits)), |_| {
                panic!("existing numbers retain their bits")
            })
            .unwrap();
        assert_eq!(result.to_bits(), bits);
    }
}

#[test]
fn text_overflow_and_existing_numeric_infinity_keep_different_errors() {
    let result = coerce_number::<CellError>(
        ScalarRef::Text(TextRef::Utf8("1e999")),
        NumberPolicy::Finite,
        |request| match request {
            DeferredNumber::Text(TextRef::Utf8(text)) => coerce_arithmetic_text(text),
            _ => panic!("only UTF8 text was supplied"),
        },
    );
    assert_eq!(result, Err(CellError::Value));
    assert_eq!(
        coerce_number::<CellError>(
            ScalarRef::Number(f64::INFINITY),
            NumberPolicy::Finite,
            |_| panic!()
        ),
        Err(CellError::Num)
    );
}

#[test]
fn host_failures_are_terminal_and_worksheet_errors_do_not_enter_fallback() {
    let text = ScalarRef::Text(TextRef::Utf8("1"));
    assert_eq!(
        coerce_number::<Failure>(text, NumberPolicy::Finite, |_| Err(Failure::Operational)),
        Err(Failure::Operational)
    );
    assert_eq!(
        coerce_number::<Failure>(
            ScalarRef::Error(CellError::NA),
            NumberPolicy::Finite,
            |_| panic!("errors propagate immediately")
        ),
        Err(Failure::Worksheet(CellError::NA))
    );
}

#[test]
fn boolean_profiles_distinguish_nan_blank_and_missing() {
    let pinned = BooleanPolicy {
        reject_nan: true,
        blank: true,
    };
    let permissive = BooleanPolicy {
        reject_nan: false,
        blank: true,
    };
    assert_eq!(
        coerce_boolean(ScalarRef::Number(f64::NAN), pinned),
        Err(CellError::Value)
    );
    assert_eq!(
        coerce_boolean(ScalarRef::Number(f64::NAN), permissive),
        Ok(Some(true))
    );
    assert_eq!(
        coerce_boolean(ScalarRef::Number(f64::INFINITY), pinned),
        Ok(Some(true))
    );
    assert_eq!(coerce_boolean(ScalarRef::Blank, pinned), Ok(Some(false)));
    assert_eq!(coerce_boolean(ScalarRef::Missing, pinned), Ok(None));
    assert_eq!(
        coerce_boolean(
            ScalarRef::Blank,
            BooleanPolicy {
                blank: false,
                ..pinned
            }
        ),
        Err(CellError::Value)
    );
}

#[test]
fn boolean_text_uses_exact_units_and_never_replaces_isolated_surrogates() {
    let policy = BooleanPolicy {
        reject_nan: true,
        blank: true,
    };
    assert_eq!(
        coerce_boolean(ScalarRef::Text(TextRef::Utf16(&[84, 114, 85, 101])), policy),
        Ok(Some(true))
    );
    for units in [&[0xd800][..], &[32, 84, 82, 85, 69][..]] {
        assert_eq!(
            coerce_boolean(ScalarRef::Text(TextRef::Utf16(units)), policy),
            Err(CellError::Value)
        );
    }
}

#[test]
fn source_numeric_grammars_keep_control_whitespace_hex_and_date_fallback_distinct() {
    assert_eq!(coerce_financial_decimal_text("\u{0}1.5\u{1f}"), Some(1.5));
    assert_eq!(
        coerce_arithmetic_text("\u{0}1.5\u{1f}"),
        Err(CellError::Value)
    );
    assert_eq!(coerce_arithmetic_text("\u{a0}1.5\u{a0}"), Ok(1.5));
    assert_eq!(coerce_statistics_decimal_text("\u{a0}1.5\u{a0}"), None);
    assert_eq!(coerce_financial_decimal_text("0x1p2"), None);
    assert_eq!(coerce_java_number_text("0x1p2"), Some(4.));
    assert!(matches!(
        coerce_numeric_text("2020/01/02"),
        NumericTextDecision::DateTime
    ));
    assert!(matches!(
        coerce_numeric_text("1e999"),
        NumericTextDecision::Number(Err(CellError::Value))
    ));
}

#[test]
fn optional_defaults_only_apply_to_omission_and_owned_text_is_transferred() {
    assert_eq!(
        coerce_optional_number::<CellError>(Scalar::Missing, 7., |_| panic!()),
        Ok(7.)
    );
    assert_eq!(
        coerce_optional_number::<CellError>(Scalar::Blank, 7., |value| coerce_number(
            (&value).into(),
            NumberPolicy::Finite,
            |_| panic!()
        )),
        Ok(0.)
    );
    let text = String::from("already allocated");
    let pointer = text.as_ptr();
    let rendered = coerce_owned_text::<CellError>(Scalar::Text(text), |request| match request {
        OwnedDeferredText::Text(text) => Ok(text),
        _ => panic!("text must remain text"),
    })
    .unwrap();
    assert_eq!(rendered.as_ptr(), pointer);
}

#[test]
fn collection_origin_controls_whether_text_is_parsed_at_all() {
    use poirot_functions::coercion::collections::coerce_cashflow_member;
    let text = ScalarRef::Text(TextRef::Utf8("bad"));
    for origin in [Origin::Reference, Origin::Array] {
        assert_eq!(
            coerce_cashflow_member::<CellError>(text, origin, |_| panic!(
                "ignored text must not be parsed"
            )),
            Ok(None)
        );
    }
    assert_eq!(
        coerce_cashflow_member::<CellError>(text, Origin::Direct, |_| Err(CellError::Value)),
        Err(CellError::Value)
    );
}

#[test]
fn matrix_member_rules_keep_error_remapping_and_area_defaults_separate() {
    use poirot_functions::coercion::collections::*;
    assert_eq!(
        coerce_matrix_member(ScalarRef::Blank, MatrixCollection::Determinant),
        Err(CellError::Value)
    );
    assert_eq!(
        coerce_matrix_member(ScalarRef::Blank, MatrixCollection::Transpose),
        Ok(Some(0.))
    );
    assert_eq!(
        coerce_matrix_member(ScalarRef::Boolean(true), MatrixCollection::Multiply),
        Ok(None)
    );
    assert_eq!(
        coerce_trend_member(ScalarRef::Error(CellError::NA)),
        Err(CellError::Value)
    );
    assert_eq!(
        coerce_sumproduct_member(ScalarRef::Error(CellError::NA), false),
        Err(CellError::NA)
    );
    assert_eq!(
        coerce_sumproduct_member(ScalarRef::Text(TextRef::Utf8("2")), false),
        Ok(0.)
    );
}

#[test]
fn text_formatting_preserves_nonnumeric_types_until_requested() {
    let result = coerce_text_format_value::<CellError>(
        CellValue::Blank,
        TextFormatPolicy::PinnedPoi,
        |_| panic!(),
    )
    .unwrap();
    assert_eq!(result, (CellValue::Number(0.), true));
    let result = coerce_text_format_value::<CellError>(
        CellValue::Text("NaN".into()),
        TextFormatPolicy::PinnedPoi,
        |_| panic!(),
    )
    .unwrap();
    assert_eq!(result, (CellValue::Text("NaN".into()), false));
    let result = coerce_text_format_value::<CellError>(
        CellValue::Boolean(true),
        TextFormatPolicy::PinnedPoi,
        |_| panic!(),
    )
    .unwrap();
    assert_eq!(result, (CellValue::Text("TRUE".into()), false));
}

#[test]
fn text_integer_target_checks_truncated_range_and_preserves_sign_checks() {
    let maximum = f64::from(i32::MAX);
    assert_eq!(
        coerce_text_integer_number(maximum, TextIntegerPolicy::Checked),
        Ok(maximum)
    );
    assert_eq!(
        coerce_text_integer_number(maximum + 0.5, TextIntegerPolicy::Checked),
        Ok(maximum + 0.5)
    );
    assert_eq!(
        coerce_text_integer_number(maximum + 0.5, TextIntegerPolicy::PinnedPoi),
        Ok(maximum + 0.5)
    );
    assert_eq!(
        coerce_text_integer_number(maximum + 1., TextIntegerPolicy::PinnedPoi),
        Err(CellError::Num)
    );
    assert_eq!(
        coerce_text_integer_number(-0.25, TextIntegerPolicy::Checked),
        Ok(-0.25)
    );
    for n in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from(i32::MIN) - 1.,
    ] {
        assert_eq!(
            coerce_text_integer_number(n, TextIntegerPolicy::Checked),
            Err(CellError::Value)
        );
    }
    assert_eq!(
        coerce_text_integer_number(-0., TextIntegerPolicy::Checked)
            .unwrap()
            .to_bits(),
        (-0_f64).to_bits()
    );
}

#[test]
fn aggregate_blank_admission_depends_on_origin_without_rewriting_other_types() {
    use poirot_functions::coercion::{Origin, collections::coerce_aggregate_argument};
    for origin in [Origin::Direct, Origin::Reference, Origin::Array] {
        assert_eq!(
            coerce_aggregate_argument(CellValue::Blank, origin),
            if origin == Origin::Direct {
                CellValue::Number(0.)
            } else {
                CellValue::Blank
            }
        );
        for value in [
            CellValue::Number(0.),
            CellValue::Boolean(true),
            CellValue::Text("12".into()),
            CellValue::Text(String::new()),
            CellValue::Error(CellError::NA),
        ] {
            assert_eq!(coerce_aggregate_argument(value.clone(), origin), value);
        }
    }
}

#[test]
fn kth_rank_admission_handles_fractional_boundaries() {
    for policy in [KthRankPolicy::PinnedPoi, KthRankPolicy::DocumentedLarge] {
        for number in [
            0.,
            -0.,
            -0.1,
            -1.,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            i32::MAX as f64 + 0.1,
        ] {
            assert_eq!(
                coerce_statistics_kth_rank(number, policy),
                Err(CellError::Num)
            );
        }
        for (number, expected) in [
            (1., 1.),
            (1.000001, 2.),
            (2.999999, 3.),
            (3., 3.),
            (i32::MAX as f64, i32::MAX as f64),
        ] {
            assert_eq!(coerce_statistics_kth_rank(number, policy), Ok(expected));
        }
        for number in [f64::MIN_POSITIVE, f64::from_bits(1), 0.1, 0.999999] {
            assert_eq!(
                coerce_statistics_kth_rank(number, policy),
                if policy == KthRankPolicy::DocumentedLarge {
                    Ok(1.)
                } else {
                    Err(CellError::Num)
                }
            );
        }
    }
}

#[test]
fn documented_search_start_truncates_before_checking_text_domain() {
    for (number, length, expected) in [(1., 3, 0), (3.9, 3, 2), (2.9, 2, 1)] {
        assert_eq!(coerce_text_search_start(number, length), Ok(expected));
    }
    for (number, length) in [
        (1., 0),
        (0.9, 3),
        (4., 3),
        (f64::NAN, 3),
        (f64::INFINITY, 3),
        (2147483648., usize::MAX),
    ] {
        assert_eq!(
            coerce_text_search_start(number, length),
            Err(CellError::Value)
        );
    }
}

#[test]
fn contextual_text_format_admission_delegates_both_encodings_and_preserves_failures() {
    for value in [
        CellValue::Text("15:43:09".into()),
        CellValue::TextUtf16(poirot_functions::scalar::ExcelText::from_units(
            "15:43:09".encode_utf16().collect::<Vec<_>>(),
        )),
    ] {
        assert_eq!(
            coerce_text_format_value::<Failure>(
                value.clone(),
                TextFormatPolicy::Contextual,
                |_| Ok(Some(0.5))
            ),
            Ok((CellValue::Number(0.5), true))
        );
        assert_eq!(
            coerce_text_format_value::<Failure>(
                value.clone(),
                TextFormatPolicy::Contextual,
                |_| Ok(None)
            ),
            Ok((value.clone(), false))
        );
        assert_eq!(
            coerce_text_format_value::<Failure>(value, TextFormatPolicy::Contextual, |_| Err(
                Failure::Operational
            )),
            Err(Failure::Operational)
        );
    }
}
