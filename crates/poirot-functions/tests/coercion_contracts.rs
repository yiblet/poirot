use poirot_functions::coercion::{
    ArrayMember, CellError, CellValue, Choice, CoercionContext, CoercionFailure, DecimalNumber,
    Direct, ExactBoolean, ExactError, ExactNumber, ExactText, FirstOf, FormulaText, Identity,
    Input, Limits, LogicalBoolean, Mismatch, Optional, Origin, Referenced, Resource, TextCount,
    coerce, coerce_args,
};
use poirot_functions::scalar::ExcelText;
use std::borrow::Cow;

fn is_mismatch<T>(result: Result<T, CoercionFailure>) -> bool {
    matches!(result, Err(CoercionFailure::Mismatch(_)))
}

#[test]
fn typed_pair_returns_numbers_without_changing_inputs() {
    let left = CellValue::Text("2.5".into());
    let right = CellValue::Boolean(true);
    let pair = coerce_args::<(DecimalNumber, DecimalNumber)>(
        &[Input::direct(&left), Input::referenced(&right)],
        &mut CoercionContext::default(),
    );
    assert_eq!(pair, Ok((2.5, 1.0)));
    assert_eq!(left, CellValue::Text("2.5".into()));
    assert_eq!(right, CellValue::Boolean(true));
}

#[test]
fn exact_number_does_not_interpret_blank_boolean_or_numeric_text() {
    let mut context = CoercionContext::default();
    for value in [CellValue::Blank, true.into(), "12".into()] {
        assert!(is_mismatch(coerce::<ExactNumber>(
            Input::direct(&value),
            &mut context
        )));
    }
    let value = CellValue::Number(-0.0);
    let number = coerce::<ExactNumber>(Input::direct(&value), &mut context).unwrap();
    assert_eq!(number.to_bits(), (-0.0f64).to_bits());
    let value = CellValue::Number(f64::from_bits(0x7ff8000000000012));
    assert_eq!(
        coerce::<ExactNumber>(Input::direct(&value), &mut context)
            .unwrap()
            .to_bits(),
        0x7ff8000000000012,
    );
}

#[test]
fn decimal_policy_has_an_explicit_finite_decimal_grammar() {
    let mut context = CoercionContext::default();
    for (text, expected) in [
        ("-3", -3.0),
        ("\t+.25e2\r\n", 25.0),
        ("1.", 1.0),
        ("1e-999", 0.0),
    ] {
        let value = CellValue::from(text);
        assert_eq!(
            coerce::<DecimalNumber>(Input::direct(&value), &mut context),
            Ok(expected)
        );
    }
    for text in [
        "",
        ".",
        "+",
        "1e",
        "1e+",
        "1e999",
        "NaN",
        "inf",
        "1%",
        "1 1/2",
        "90:00",
        "1,000",
        "(100)",
        "2026-10-17",
        "\u{a0}1",
    ] {
        let value = CellValue::from(text);
        assert!(
            is_mismatch(coerce::<DecimalNumber>(Input::direct(&value), &mut context)),
            "{text:?}"
        );
    }
    assert_eq!(
        coerce::<DecimalNumber>(Input::direct(&CellValue::Blank), &mut context),
        Ok(0.0)
    );
    assert_eq!(
        coerce::<DecimalNumber>(Input::direct(&CellValue::Boolean(false)), &mut context),
        Ok(0.0)
    );
    assert_eq!(
        coerce::<DecimalNumber>(
            Input::direct(&CellValue::Number(f64::INFINITY)),
            &mut context
        ),
        Err(CoercionFailure::Worksheet(CellError::Num))
    );
}

#[test]
fn missing_is_not_a_blank_or_an_empty_text_default() {
    let blank = CellValue::Blank;
    let empty = CellValue::from("");
    let mut context = CoercionContext::default();
    assert_eq!(
        coerce::<Optional<DecimalNumber>>(Input::missing(), &mut context),
        Ok(None)
    );
    assert_eq!(
        coerce::<Optional<DecimalNumber>>(Input::referenced(&blank), &mut context),
        Ok(Some(0.0))
    );
    assert!(is_mismatch(coerce::<Optional<DecimalNumber>>(
        Input::direct(&empty),
        &mut context
    )));
    assert!(is_mismatch(coerce::<DecimalNumber>(
        Input::missing(),
        &mut context
    )));
}

#[test]
fn alternatives_try_complete_signatures_from_original_inputs() {
    let left = CellValue::from("2");
    let right = CellValue::from("three");
    type Accepted = FirstOf<((DecimalNumber, DecimalNumber), (ExactText, ExactText))>;
    let result = coerce_args::<Accepted>(
        &[Input::direct(&left), Input::direct(&right)],
        &mut CoercionContext::default(),
    )
    .unwrap();
    let Choice::Second((a, b)) = result else {
        panic!("complete text signature must win")
    };
    assert_eq!(a, left.text_ref().unwrap());
    assert_eq!(b, right.text_ref().unwrap());
}

#[test]
fn first_matching_signature_has_priority_even_when_both_match() {
    let value = CellValue::from("12");
    let inputs = [Input::direct(&value)];
    let mut context = CoercionContext::default();
    type TextFirst = FirstOf<((ExactText,), (DecimalNumber,))>;
    type NumberFirst = FirstOf<((DecimalNumber,), (ExactText,))>;
    assert!(matches!(
        coerce_args::<TextFirst>(&inputs, &mut context),
        Ok(Choice::First(_))
    ));
    assert_eq!(
        coerce_args::<NumberFirst>(&inputs, &mut context),
        Ok(Choice::First((12.0,)))
    );
}

#[test]
fn a_successful_signature_does_not_spend_work_on_later_candidates() {
    let text = CellValue::from("12");
    let mut context = CoercionContext::new(Limits {
        work: 3,
        text_bytes: 0,
    });
    type Accepted = FirstOf<((ExactText,), (DecimalNumber,))>;
    assert!(matches!(
        coerce_args::<Accepted>(&[Input::direct(&text)], &mut context),
        Ok(Choice::First(_))
    ));
    assert_eq!(context.usage().work, 3);
}

#[test]
fn arity_mismatch_can_select_a_different_complete_signature() {
    let value = CellValue::Number(12.0);
    type Accepted = FirstOf<((DecimalNumber, DecimalNumber), (ExactNumber,))>;
    assert_eq!(
        coerce_args::<Accepted>(&[Input::direct(&value)], &mut CoercionContext::default()),
        Ok(Choice::Second((12.0,)))
    );
    assert_eq!(
        coerce_args::<()>(&[], &mut CoercionContext::default()),
        Ok(())
    );
    assert!(is_mismatch(coerce_args::<()>(
        &[Input::direct(&value)],
        &mut CoercionContext::default()
    )));
}

#[test]
fn rejected_tuple_identifies_the_argument_without_panicking_on_short_input() {
    let number = CellValue::Number(1.0);
    let text = CellValue::from("x");
    assert_eq!(
        coerce_args::<(ExactNumber, ExactNumber)>(
            &[Input::direct(&number), Input::direct(&text)],
            &mut CoercionContext::default()
        ),
        Err(CoercionFailure::Mismatch(Mismatch::Input {
            argument: Some(1),
            expected: "existing number"
        }))
    );
    assert_eq!(
        coerce_args::<(ExactNumber, ExactNumber)>(
            &[Input::direct(&number)],
            &mut CoercionContext::default()
        ),
        Err(CoercionFailure::Mismatch(Mismatch::Arity {
            expected: 2,
            actual: 1
        }))
    );
}

#[test]
fn worksheet_errors_do_not_fall_through_to_identity() {
    let error = CellValue::Error(CellError::Div0);
    type Accepted = FirstOf<((DecimalNumber,), (Identity,))>;
    assert!(matches!(
        coerce_args::<Accepted>(&[Input::direct(&error)], &mut CoercionContext::default()),
        Err(CoercionFailure::Worksheet(CellError::Div0))
    ));
    type ErrorAware = FirstOf<((ExactError,), (DecimalNumber,))>;
    assert_eq!(
        coerce_args::<ErrorAware>(&[Input::direct(&error)], &mut CoercionContext::default()),
        Ok(Choice::First((CellError::Div0,)))
    );
}

#[test]
fn argument_order_is_an_explicit_error_precedence_contract() {
    let text = CellValue::from("invalid");
    let error = CellValue::Error(CellError::Ref);
    let mut context = CoercionContext::default();
    assert!(is_mismatch(coerce_args::<(DecimalNumber, DecimalNumber)>(
        &[Input::direct(&text), Input::direct(&error)],
        &mut context
    )));
    assert_eq!(
        coerce_args::<(DecimalNumber, DecimalNumber)>(
            &[Input::direct(&error), Input::direct(&text)],
            &mut context
        ),
        Err(CoercionFailure::Worksheet(CellError::Ref))
    );
}

#[test]
fn identity_keeps_omission_errors_and_reference_provenance() {
    let error = CellValue::Error(CellError::NA);
    let mut context = CoercionContext::default();
    let selected = coerce::<Identity>(Input::referenced(&error), &mut context).unwrap();
    assert_eq!(selected.origin(), Origin::Reference);
    assert!(std::ptr::eq(selected.value().unwrap(), &error));
    assert!(
        coerce::<Identity>(Input::missing(), &mut context)
            .unwrap()
            .value()
            .is_none()
    );
}

#[test]
fn origin_gates_make_reference_and_direct_numeric_text_distinct() {
    let text = CellValue::from("12");
    type Accepted = FirstOf<((Direct<DecimalNumber>,), (Referenced<ExactNumber>,))>;
    let mut context = CoercionContext::default();
    assert_eq!(
        coerce_args::<Accepted>(&[Input::direct(&text)], &mut context),
        Ok(Choice::First((12.0,)))
    );
    assert!(is_mismatch(coerce_args::<Accepted>(
        &[Input::referenced(&text)],
        &mut context
    )));
    let value = CellValue::Number(12.0);
    assert_eq!(
        coerce_args::<Accepted>(&[Input::referenced(&value)], &mut context),
        Ok(Choice::Second((12.0,)))
    );
    assert_eq!(
        coerce::<ArrayMember<ExactNumber>>(Input::array_member(&value), &mut context),
        Ok(12.0)
    );
    assert!(is_mismatch(coerce::<ArrayMember<ExactNumber>>(
        Input::referenced(&value),
        &mut context
    )));
}

#[test]
fn exact_text_preserves_isolated_surrogates_but_scalar_projection_is_terminal() {
    let text = CellValue::from(ExcelText::from_units(vec![0xd800]));
    let mut context = CoercionContext::default();
    assert_eq!(
        coerce::<ExactText>(Input::direct(&text), &mut context)
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![0xd800]
    );
    type Accepted = FirstOf<((FormulaText,), (ExactText,))>;
    assert!(matches!(
        coerce_args::<Accepted>(&[Input::direct(&text)], &mut context),
        Err(CoercionFailure::UnsupportedText)
    ));
    assert_eq!(context.usage().text_bytes, 0);
}

#[test]
fn text_projection_borrows_utf8_and_admits_utf16_before_allocation() {
    let utf8 = CellValue::from("abc");
    let mut context = CoercionContext::new(Limits {
        work: 100,
        text_bytes: 0,
    });
    assert!(matches!(
        coerce::<FormulaText>(Input::direct(&utf8), &mut context),
        Ok(Cow::Borrowed("abc"))
    ));
    let utf16 = CellValue::TextUtf16(ExcelText::from_units(vec![65, 66]));
    assert_eq!(
        coerce::<FormulaText>(Input::direct(&utf16), &mut context),
        Err(CoercionFailure::Limit(Resource::TextBytes))
    );
}

#[test]
fn failed_candidates_retain_work_and_resource_failure_never_falls_back() {
    let text = CellValue::from("x");
    type Accepted = FirstOf<((DecimalNumber,), (ExactText,))>;
    let mut context = CoercionContext::default();
    assert!(matches!(
        coerce_args::<Accepted>(&[Input::direct(&text)], &mut context),
        Ok(Choice::Second(_))
    ));
    assert_eq!(context.usage().work, 6);
    let mut limited = CoercionContext::new(Limits {
        work: 3,
        text_bytes: 0,
    });
    assert!(matches!(
        coerce_args::<Accepted>(&[Input::direct(&text)], &mut limited),
        Err(CoercionFailure::Limit(Resource::Work))
    ));
    assert_eq!(limited.usage().work, 3);
}

#[test]
fn numeric_rendering_is_explicit_and_reserves_output_before_allocation() {
    let number = CellValue::Number(-14.000000000000002);
    let mut context = CoercionContext::default();
    assert_eq!(
        coerce::<FormulaText>(Input::direct(&number), &mut context).unwrap(),
        "-14"
    );
    let mut limited = CoercionContext::new(Limits {
        work: 100,
        text_bytes: 0,
    });
    type Accepted = FirstOf<((FormulaText,), (ExactNumber,))>;
    assert!(matches!(
        coerce_args::<Accepted>(&[Input::direct(&number)], &mut limited),
        Err(CoercionFailure::Limit(Resource::TextBytes))
    ));
}

#[test]
fn logical_policy_keeps_exact_boolean_and_text_interpretation_separate() {
    let mut context = CoercionContext::default();
    for (value, expected) in [
        (CellValue::Blank, false),
        (CellValue::Number(-2.0), true),
        (CellValue::Number(-0.0), false),
        (CellValue::from("tRuE"), true),
        (CellValue::from("FALSE"), false),
    ] {
        assert_eq!(
            coerce::<LogicalBoolean>(Input::direct(&value), &mut context),
            Ok(expected)
        );
    }
    for text in [" TRUE", "FALSE ", "1", "ＴＲＵＥ"] {
        assert!(is_mismatch(coerce::<LogicalBoolean>(
            Input::direct(&CellValue::from(text)),
            &mut context
        )));
    }
    assert!(is_mismatch(coerce::<ExactBoolean>(
        Input::direct(&CellValue::from("TRUE")),
        &mut context
    )));
    assert_eq!(
        coerce::<ExactBoolean>(Input::direct(&CellValue::Boolean(true)), &mut context),
        Ok(true)
    );
}

#[test]
fn text_count_checks_sign_and_truncated_integer_range() {
    let mut context = CoercionContext::default();
    assert_eq!(
        coerce::<TextCount>(Input::direct(&CellValue::Number(2.9)), &mut context),
        Ok(2)
    );
    assert_eq!(
        coerce::<TextCount>(
            Input::direct(&CellValue::Number(2147483647.5)),
            &mut context
        ),
        Ok(i32::MAX)
    );
    for value in [-0.1, 2147483648.0] {
        assert_eq!(
            coerce::<TextCount>(Input::direct(&CellValue::Number(value)), &mut context),
            Err(CoercionFailure::Worksheet(CellError::Value))
        );
    }
}
