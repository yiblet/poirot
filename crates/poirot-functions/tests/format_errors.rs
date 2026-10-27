//! Formatting errors expose conditions, positions, and retained compilation causes.
use poirot_functions::format::{
    FormatCompilePolicy, FormatError, FormatErrorKind, FormatReasonError, compile_format,
    parse_format,
};
use std::error::Error;

#[test]
fn format_errors_retain_rejected_input_and_parser_position() {
    let error = parse_format("[unsupported]0").unwrap_err();
    assert_eq!(error.kind, FormatErrorKind::InvalidFormat);
    assert_eq!(
        error.reason,
        FormatReasonError::UnsupportedBracketDirective {
            directive: "unsupported".into(),
        }
    );
    let error = parse_format("0\"").unwrap_err();
    assert_eq!(error.position, 1);
    assert_eq!(
        error.reason,
        FormatReasonError::UnterminatedQuoteBracketOrEscape
    );
}

#[test]
fn compilation_preserves_a_typed_cause() {
    let error = compile_format("[unsupported]0", FormatCompilePolicy::default()).unwrap_err();
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<FormatError>()
        .unwrap();
    assert_eq!(cause, &error.source);
    let reason = cause
        .source()
        .unwrap()
        .downcast_ref::<FormatReasonError>()
        .unwrap();
    assert_eq!(reason, &cause.reason);
    assert_eq!(
        reason,
        &FormatReasonError::UnsupportedBracketDirective {
            directive: "unsupported".into()
        }
    );
    assert!(reason.source().is_none());
    assert_eq!(cause.to_string(), "number format failed at byte 0");
    assert!(!cause.to_string().contains(&reason.to_string()));
    assert_eq!(error.section, None);
    assert_eq!(error.token, None);
}

#[test]
fn formatting_errors_can_cross_thread_boundaries() {
    fn require_thread_safe<T: Send + Sync + 'static>() {}
    require_thread_safe::<FormatError>();
    require_thread_safe::<poirot_functions::format::FormatCompileError>();
    require_thread_safe::<poirot_functions::matrix::MatrixError>();
}
