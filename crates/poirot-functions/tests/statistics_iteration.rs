use poirot_functions::statistics::{StatisticalKernelError, normal_dist_metered, poisson_metered};

/// Distinct operational error returned by a test meter at a chosen iteration boundary.
/// It lets tests verify that statistical kernels preserve host failures unchanged.
#[derive(Debug, PartialEq)]
struct Stop(&'static str);

#[test]
fn first_iteration_failure_is_returned_without_numeric_conversion() {
    let mut meter = |_| Err(Stop("sentinel"));
    // P series and Q continued-fraction branches, respectively.
    for (count, mean) in [(2.0, 1.0), (2.0, 4.0)] {
        assert_eq!(
            poisson_metered(count, mean, true, &mut meter),
            Err(StatisticalKernelError::Operational(Stop("sentinel")))
        );
    }
    assert_eq!(
        normal_dist_metered(1.0, 0.0, 1.0, true, &mut meter),
        Err(StatisticalKernelError::Operational(Stop("sentinel")))
    );
    assert_eq!(
        poisson_metered(10.0, 10.1, false, &mut meter),
        Err(StatisticalKernelError::Operational(Stop("sentinel")))
    );
}

#[test]
fn meter_stops_before_the_next_iteration_and_preserves_source_fast_paths() {
    let mut charged = 0;
    let mut meter = |units| {
        assert_eq!(units, 1);
        if charged == 2 {
            return Err(Stop("third iteration"));
        }
        charged += units;
        Ok(())
    };
    assert_eq!(
        poisson_metered(1000000.0, 1000000.0, true, &mut meter),
        Err(StatisticalKernelError::Operational(Stop("third iteration")))
    );
    assert_eq!(charged, 2);
    let mut reject = |_| Err(Stop("must not iterate"));
    assert_eq!(poisson_metered(0.0, 0.0, true, &mut reject), Ok(1.0));
    assert_eq!(
        poisson_metered(i32::MAX as f64, 3.0, true, &mut reject),
        Ok(1.0)
    );
    assert_eq!(
        normal_dist_metered(-121.0, 0.0, 3.0, true, &mut reject),
        Ok(0.0)
    );
    assert_eq!(
        normal_dist_metered(0.0, 0.0, 1.0, true, &mut reject),
        Ok(0.5)
    );
}
