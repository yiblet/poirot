//! Rust regex backend contracts; expected matching behavior follows regex 1.13.1
//! https://docs.rs/regex/1.13.1/regex/ (no upstream tests or source reused).
use super::{Case, ExecutionLimits, Extraction, Host, Program, RegexError};
use crate::text::{RegexBackend, RegexBackendError};

/// Test-controlled work refusal.
#[derive(Debug, thiserror::Error)]
#[error("work refused")]
struct RefusedError;

/// Admission host for backend success and resource-failure cases.
#[derive(Default)]
struct Admission {
    refuse: bool,
    limits: ExecutionLimits,
}
impl Host for Admission {
    type Error = RefusedError;
    fn execution_limits(&self) -> ExecutionLimits {
        self.limits
    }
    fn work(&mut self, _: usize) -> Result<(), RefusedError> {
        if self.refuse {
            Err(RefusedError)
        } else {
            Ok(())
        }
    }
    fn text(&mut self, _: usize) -> Result<(), RefusedError> {
        Ok(())
    }
    fn values(&mut self, _: usize) -> Result<(), RefusedError> {
        Ok(())
    }
}

#[test]
fn rust_syntax_capture_modes_and_replacement_occurrences() {
    assert_eq!(Program::BACKEND, RegexBackend::Rust);
    let mut host = Admission::default();
    let p = Program::compile(r"([a-z])([0-9])", Case::Sensitive, &mut host).unwrap();
    assert!(p.test("a1 b2 c3", &mut host).unwrap());
    assert_eq!(
        p.extract("a1 b2 c3", Extraction::First, &mut host).unwrap(),
        ["a1"]
    );
    assert_eq!(
        p.extract("a1 b2 c3", Extraction::All, &mut host).unwrap(),
        ["a1", "b2", "c3"]
    );
    assert_eq!(
        p.extract("a1 b2 c3", Extraction::Groups, &mut host)
            .unwrap(),
        ["a", "1"]
    );
    for (occurrence, expected) in [
        (0, "1:a 2:b 3:c"),
        (2, "a1 2:b c3"),
        (-1, "a1 b2 3:c"),
        (-9, "a1 b2 c3"),
    ] {
        assert_eq!(
            p.replace("a1 b2 c3", "$2:$1", occurrence, &mut host)
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        p.replace("a1", "$$$1/$0/$8", 0, &mut host).unwrap(),
        "$a/a1/$8"
    );
    let optional = Program::compile("(a)?b", Case::Sensitive, &mut host).unwrap();
    assert_eq!(
        optional
            .extract("b", Extraction::Groups, &mut host)
            .unwrap(),
        [""]
    );
}

#[test]
fn unsupported_pcre_syntax_is_a_typed_pattern_failure() {
    for pattern in ["[", "(?<=a)b", r"(a)\1", "(*UTF)a"] {
        assert!(matches!(
            Program::compile(pattern, Case::Sensitive, &mut Admission::default()),
            Err(RegexError::Pattern(RegexBackendError::Rust(
                ::regex::Error::Syntax(_)
            )))
        ));
    }
}

#[test]
fn unicode_case_and_empty_iteration_use_rust_semantics() {
    let mut host = Admission::default();
    assert!(
        Program::compile("^ä$", Case::Insensitive, &mut host)
            .unwrap()
            .test("Ä", &mut host)
            .unwrap()
    );
    let empty = Program::compile("", Case::Sensitive, &mut host).unwrap();
    assert_eq!(
        empty.extract("é😀", Extraction::All, &mut host).unwrap(),
        ["", "", ""]
    );
    assert_eq!(empty.replace("é😀", "|", 0, &mut host).unwrap(), "|é|😀|");
    let alternatives = Program::compile("(?:|a)", Case::Sensitive, &mut host).unwrap();
    assert_eq!(
        alternatives
            .extract("a", Extraction::All, &mut host)
            .unwrap(),
        ["", ""]
    );
}

#[test]
fn compilation_size_and_host_refusal_remain_operational() {
    let mut host = Admission {
        limits: ExecutionLimits::default().max_heap_kib(0),
        ..Default::default()
    };
    assert!(matches!(
        Program::compile("[a-z]+", Case::Sensitive, &mut host),
        Err(RegexError::Native(RegexBackendError::Rust(
            ::regex::Error::CompiledTooBig(0)
        )))
    ));
    let p = Program::compile("a", Case::Sensitive, &mut Admission::default()).unwrap();
    host.refuse = true;
    assert!(matches!(
        p.test("a", &mut host),
        Err(RegexError::Host(RefusedError))
    ));
    assert!(matches!(
        p.extract("a", Extraction::All, &mut host),
        Err(RegexError::Host(RefusedError))
    ));
    assert!(matches!(
        p.replace("a", "b", 0, &mut host),
        Err(RegexError::Host(RefusedError))
    ));
}
