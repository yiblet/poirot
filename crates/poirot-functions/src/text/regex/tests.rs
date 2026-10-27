//! Independent PCRE2/text examples derived from the Microsoft function contracts
//! linked by the parent module. No upstream tests or spreadsheet fixtures reused.
use super::*;
/// Test-controlled host refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("host refused admission")]
struct AdmissionError;
/// Test regex host that accepts resource requests and counts estimated work.
/// Tests use the count to check that repeated searches continue to request admission.
#[derive(Default)]
struct Admission {
    work: usize,
}
impl Host for Admission {
    /// A test-controlled refusal marker.
    type Error = AdmissionError;
    fn work(&mut self, units: usize) -> Result<(), Self::Error> {
        self.work += units;
        Ok(())
    }
    fn text(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn values(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
}
fn program(pattern: &str) -> Program {
    Program::compile(pattern, Case::Sensitive, &mut Admission::default()).unwrap()
}
#[test]
fn pcre_lookarounds_backreferences_and_unicode_are_real() {
    let mut host = Admission::default();
    assert!(
        program(r"(?<=id:)\d+(?=;)")
            .test("id:421;", &mut host)
            .unwrap()
    );
    assert!(
        program(r"\b(\w+)\s+\1\b")
            .test("pear pear", &mut host)
            .unwrap()
    );
    assert!(
        !program(r"\b(\w+)\s+\1\b")
            .test("pear peach", &mut host)
            .unwrap()
    );
    assert!(
        Program::compile("^ä$", Case::Insensitive, &mut host)
            .unwrap()
            .test("Ä", &mut host)
            .unwrap()
    );
    assert_eq!(
        program(r"\p{L}+")
            .extract("42 λη 7", Extraction::First, &mut host)
            .unwrap(),
        ["λη"]
    );
}
#[test]
fn extraction_modes_and_unmatched_optional_groups() {
    let mut host = Admission::default();
    let p = program(r"([a-z]+)-(\d+)");
    assert_eq!(
        p.extract("oak-12 elm-34", Extraction::First, &mut host)
            .unwrap(),
        ["oak-12"]
    );
    assert_eq!(
        p.extract("oak-12 elm-34", Extraction::All, &mut host)
            .unwrap(),
        ["oak-12", "elm-34"]
    );
    assert_eq!(
        p.extract("oak-12 elm-34", Extraction::Groups, &mut host)
            .unwrap(),
        ["oak", "12"]
    );
    assert_eq!(
        program("(a)?(b)")
            .extract("b", Extraction::Groups, &mut host)
            .unwrap(),
        ["", "b"]
    );
    assert!(matches!(
        p.extract("none", Extraction::First, &mut host),
        Err(RegexError::NoMatch)
    ));
}
#[test]
fn replacement_occurrences_captures_and_literal_dollars() {
    let p = program(r"([a-z])(\d)");
    let mut host = Admission::default();
    for (occurrence, expected) in [
        (0, "1:a 2:b 3:c"),
        (1, "1:a b2 c3"),
        (2, "a1 2:b c3"),
        (-1, "a1 b2 3:c"),
        (-2, "a1 2:b c3"),
        (8, "a1 b2 c3"),
        (-8, "a1 b2 c3"),
    ] {
        assert_eq!(
            p.replace("a1 b2 c3", "$2:$1", occurrence, &mut host)
                .unwrap(),
            expected
        );
    }
    assert_eq!(p.replace("a1", "$$$1/$0", 0, &mut host).unwrap(), "$a/a1");
    assert_eq!(
        program("(a)?b").replace("b", "$1!", 0, &mut host).unwrap(),
        "!"
    );
}
#[test]
fn empty_matches_resume_at_unicode_boundaries_and_retry_nonempty_alternatives() {
    let mut host = Admission::default();
    assert_eq!(
        program("")
            .extract("é😀", Extraction::All, &mut host)
            .unwrap(),
        ["", "", ""]
    );
    assert_eq!(
        program("").replace("é😀", "|", 0, &mut host).unwrap(),
        "|é|😀|"
    );
    assert_eq!(
        program("(?:|a)")
            .extract("a", Extraction::All, &mut host)
            .unwrap(),
        ["", "a", ""]
    );
    assert_eq!(
        program(r"(*UTF)(?i)(?:|a)")
            .replace("A", "x", 0, &mut host)
            .unwrap(),
        "xxx"
    );
}
#[test]
fn invalid_patterns_and_native_resource_exhaustion_are_distinct() {
    let mut host = Admission::default();
    assert!(matches!(
        Program::compile("[", Case::Sensitive, &mut host),
        Err(RegexError::Pattern(_))
    ));
    let deeply_nested = format!("{}a{}", "(".repeat(500), ")".repeat(500));
    assert!(matches!(
        Program::compile(&deeply_nested, Case::Sensitive, &mut host),
        Err(RegexError::Native(error)) if error.code() == 119
    ));
    let p = program(r"(*LIMIT_MATCH=10)(*NO_START_OPT)(a+)+$");
    assert!(matches!(
        p.test("aaaaaaaaaaaaaaaaaaaa!", &mut host),
        Err(RegexError::Native(error)) if error.code() == -47
    ));
    assert!(matches!(
        p.extract("aaaaaaaaaaaaaaaaaaaa!", Extraction::All, &mut host),
        Err(RegexError::Native(error)) if error.code() == -47
    ));
    assert!(matches!(
        p.replace("aaaaaaaaaaaaaaaaaaaa!", "x", 0, &mut host),
        Err(RegexError::Native(error)) if error.code() == -47
    ));
}
#[test]
fn byte_escape_is_refused_without_rejecting_escaped_or_quoted_literals() {
    let mut host = Admission::default();
    assert!(matches!(
        Program::compile(r"^\C$", Case::Sensitive, &mut host),
        Err(RegexError::ByteEscape)
    ));
    assert!(program(r"\\C").test(r"\C", &mut host).unwrap());
    assert!(program(r"\Q\C\E").test(r"\C", &mut host).unwrap());
}
#[test]
fn output_size_and_caller_refusal_are_not_silent_truncation() {
    let mut host = Admission::default();
    assert!(matches!(
        program("a").replace(&"a".repeat(18000), "aa", 0, &mut host),
        Err(RegexError::TextLimit)
    ));
    /// Admission that refuses native execution.
    struct Refuse;
    impl Host for Refuse {
        /// Exact operational marker propagated by the algorithm.
        type Error = AdmissionError;
        fn work(&mut self, _: usize) -> Result<(), Self::Error> {
            Err(AdmissionError)
        }
        fn text(&mut self, _: usize) -> Result<(), Self::Error> {
            Ok(())
        }
        fn values(&mut self, _: usize) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    assert!(matches!(
        program("a").test("a", &mut Refuse),
        Err(RegexError::Host(AdmissionError))
    ));
}
#[test]
fn native_empty_retry_preserves_lookbehind_recursion_and_extended_comments() {
    let mut host = Admission::default();
    assert_eq!(
        program(r"(?<=é)|x")
            .extract("éx", Extraction::All, &mut host)
            .unwrap(),
        ["", "x"]
    );
    assert_eq!(
        program(r"|a(?R)?a")
            .extract("aaaa", Extraction::All, &mut host)
            .unwrap(),
        ["", "aa", "", "aa", ""]
    );
    assert_eq!(
        program("(?x) # empty body")
            .extract("é😀", Extraction::All, &mut host)
            .unwrap(),
        ["", "", ""]
    );
    assert_eq!(
        program(r"|(?<=é)\w")
            .extract("éx", Extraction::All, &mut host)
            .unwrap(),
        ["", "", "x", ""]
    );
    assert!(matches!(
        Program::compile(r"\Q\\E\C", Case::Sensitive, &mut host),
        Err(RegexError::ByteEscape)
    ));
}
