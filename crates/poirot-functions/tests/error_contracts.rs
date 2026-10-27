use std::error::Error;

use poirot_functions::{
    engineering::{self, BesselError},
    scalar::{CellError, ValueError},
    statistics::{SourceConvergenceError, StatisticalKernelError},
    text::{RegexError, regex},
};

/// Test operation failure whose identity must survive propagation.
#[derive(Debug, thiserror::Error)]
#[error("test work reservation was refused")]
struct AdmissionError;

/// Regex host admitting all work for dependency error checks.
struct RegexHost;

impl regex::Host for RegexHost {
    type Error = AdmissionError;

    fn work(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }

    fn text(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }

    fn values(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn scalar_validation_retains_rejected_domain_input() {
    let error = "#INVALID!".parse::<CellError>().unwrap_err();
    assert_eq!(
        error,
        ValueError::UnknownCellError {
            input: "#INVALID!".into()
        }
    );
    assert!(error.to_string().contains("#INVALID!"));
    assert!(error.source().is_none());
    assert_eq!("#DIV/0!".parse::<CellError>(), Ok(CellError::Div0));
}

#[test]
fn algorithm_errors_preserve_operation_and_convergence_causes() {
    let error = engineering::special::bessel(engineering::special::Bessel::First, 1.0, 0.0, |_| {
        Err(AdmissionError)
    })
    .unwrap_err();
    assert!(matches!(error, BesselError::Operational(_)));
    assert!(error.source().unwrap().is::<AdmissionError>());

    let convergence = StatisticalKernelError::<AdmissionError>::SourceConvergence(
        SourceConvergenceError::IterationLimit,
    );
    assert!(convergence.source().unwrap().is::<SourceConvergenceError>());
}

#[test]
fn regex_dependency_error_remains_a_typed_cause() {
    let error = match regex::Program::compile("[", regex::Case::Sensitive, &mut RegexHost) {
        Err(error) => error,
        Ok(_) => panic!("invalid regex was accepted"),
    };
    assert!(matches!(error, RegexError::Pattern(_)));
    assert!(error.source().unwrap().is::<pcre2::Error>());
}

#[test]
fn public_operation_errors_are_owned_and_thread_safe() {
    fn check<T: Error + Send + Sync + 'static>() {}
    check::<ValueError>();
    check::<BesselError<AdmissionError>>();
    check::<StatisticalKernelError<AdmissionError>>();
    check::<RegexError<AdmissionError>>();
}
