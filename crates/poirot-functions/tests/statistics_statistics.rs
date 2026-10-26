use poirot_functions::scalar::CellError;
use poirot_functions::statistics::*;

fn near(actual: Result, expected: f64, tolerance: f64) {
    let actual = actual.unwrap();
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn aggregates_and_source_empty_singleton_contracts() {
    near(aggregate(Aggregate::AveDev, &[2., 4., 6.]), 4. / 3., 1e-15);
    near(aggregate(Aggregate::DevSq, &[2., 4., 6.]), 8., 0.);
    near(aggregate(Aggregate::GeoMean, &[2., 8.]), 4., 1e-14);
    near(aggregate(Aggregate::VarianceSample, &[2., 4., 6.]), 4., 0.);
    near(
        aggregate(Aggregate::VariancePopulation, &[2., 4., 6.]),
        8. / 3.,
        0.,
    );
    near(aggregate(Aggregate::StdevSample, &[2., 4., 6.]), 2., 0.);
    near(
        aggregate(Aggregate::StdevPopulation, &[2., 4., 6.]),
        (8f64 / 3.).sqrt(),
        0.,
    );
    assert_eq!(aggregate(Aggregate::Average, &[]), Err(CellError::Div0));
    assert_eq!(
        aggregate(Aggregate::VariancePopulation, &[2.]),
        Err(CellError::Num)
    );
    assert_eq!(
        aggregate(Aggregate::StdevPopulation, &[2.]),
        Err(CellError::Num)
    );
    assert_eq!(
        aggregate(Aggregate::GeoMean, &[1., 0.]),
        Err(CellError::Num)
    );
    assert_eq!(aggregate(Aggregate::Maximum, &[]), Ok(0.));
    assert_eq!(aggregate(Aggregate::Minimum, &[-3., 2.]), Ok(-3.));
    assert_eq!(aggregate(Aggregate::Mode, &[5., 2., 2., 5.]), Ok(5.));
    assert_eq!(aggregate(Aggregate::Mode, &[1., 2.]), Err(CellError::NA));
}

#[test]
fn reference_direct_and_array_collection_are_distinct() {
    let reference_text = Operand {
        value: Value::Text { numeric: Some(8.) },
        origin: Origin::Reference,
    };
    let direct_text = Operand {
        origin: Origin::Direct,
        ..reference_text
    };
    assert_eq!(
        coercev2_statistics_collection(
            Collection::NumericAggregate,
            &[reference_text, direct_text]
        ),
        Ok(vec![8.])
    );
    assert_eq!(
        coercev2_statistics_collection(Collection::AggregateA, &[reference_text, direct_text]),
        Ok(vec![0., 0.])
    );
    let reference_bool = Operand {
        value: Value::Boolean(true),
        origin: Origin::Reference,
    };
    assert_eq!(
        coercev2_statistics_collection(Collection::NumericAggregate, &[reference_bool]),
        Ok(vec![])
    );
    assert_eq!(
        coercev2_statistics_collection(Collection::AggregateA, &[reference_bool]),
        Ok(vec![1.])
    );
    assert_eq!(
        coercev2_statistics_collection(Collection::Mode, &[reference_text]),
        Err(CellError::Value)
    );
    assert_eq!(
        coercev2_statistics_collection(
            Collection::Mode,
            &[Operand {
                origin: Origin::Array,
                ..reference_text
            }]
        ),
        Ok(vec![])
    );
    assert_eq!(
        coercev2_statistics_collection(
            Collection::PercentRank,
            &[Operand {
                value: Value::Error(CellError::NA),
                origin: Origin::Reference
            }]
        ),
        Err(CellError::Num)
    );
    assert_eq!(
        coercev2_statistics_collection(
            Collection::GeoMean,
            &[Operand {
                value: Value::Missing,
                origin: Origin::Direct
            }]
        ),
        Ok(vec![0.])
    );
}

#[test]
fn order_percentile_rank_and_duplicate_percent_rank() {
    assert_eq!(order_statistic(&[1., 2., 3., 4.], 1.2, true), Ok(3.));
    assert_eq!(
        order_statistic(&[1., 2., 3., 4.], 0.2, false),
        Err(CellError::Num)
    );
    assert_eq!(percentile(&[4., 1., 2., 3.], 0.25), Ok(1.75));
    assert_eq!(percentile(&vec![1.; 8192], 0.5), Err(CellError::Num));
    assert_eq!(rank(&[1., 3., 3., 5.], 3., false), Ok(2.));
    assert_eq!(rank(&[1., 3., 3., 5.], 4., true), Ok(4.));
    assert_eq!(
        percent_rank(&[1., 2., 2., 3.], 2., 3, PercentRankKind::Inclusive),
        Ok(0.5)
    );
    assert_eq!(
        percent_rank(&[1., 2., 3.], 1., 3, PercentRankKind::Exclusive),
        Ok(0.25)
    );
    assert_eq!(
        percent_rank(&[1., 2., 3.], 1.5, 3, PercentRankKind::Inclusive),
        Ok(0.25)
    );
    assert_eq!(
        percent_rank(&[1., 2., 3.], 4., 3, PercentRankKind::Inclusive),
        Err(CellError::NA)
    );
}

#[test]
fn pair_alignment_coercion_and_error_precedence() {
    let x = [
        Value::Number(1.),
        Value::Text { numeric: None },
        Value::Number(3.),
    ];
    let y = [Value::Number(2.), Value::Number(4.), Value::Number(6.)];
    let pairs = coercev2_statistics_pairs(PairCollection::NumericCoercionV2, &x, &y).unwrap();
    assert_eq!(pairs.original_len, 3);
    assert_eq!(pairs.x, vec![1., 3.]);
    assert_eq!(pairs.y, vec![2., 6.]);
    assert_eq!(
        coercev2_statistics_pairs(
            PairCollection::NumericOnly,
            &[Value::Error(CellError::Ref)],
            &[Value::Error(CellError::Div0)]
        ),
        Err(CellError::Ref)
    );
    assert_eq!(
        coercev2_statistics_pairs(PairCollection::NumericOnly, &x, &y[..2]),
        Err(CellError::NA)
    );
    let bools = coercev2_statistics_pairs(
        PairCollection::NumericCoercionV2,
        &[Value::Boolean(true)],
        &[Value::Text { numeric: Some(2.) }],
    )
    .unwrap();
    assert_eq!(bools.x, vec![1.]);
    assert_eq!(bools.y, vec![2.]);
}

#[test]
fn covariance_correlation_regression_and_pair_sums() {
    let x = [1., 2., 3.];
    let y = [3., 5., 7.];
    near(paired(PairOperation::Correlation, &x, &y), 1., 1e-15);
    near(
        paired(PairOperation::CovariancePopulation, &x, &y),
        4. / 3.,
        1e-15,
    );
    near(paired(PairOperation::CovarianceSample, &x, &y), 2., 0.);
    assert_eq!(
        paired(PairOperation::Correlation, &[1., 1.], &[2., 3.]),
        Err(CellError::Div0)
    );
    assert_eq!(
        paired(PairOperation::CovariancePopulation, &[1.], &[2.]),
        Err(CellError::NA)
    );
    assert_eq!(
        regression(&x, &y, 3).unwrap(),
        Regression {
            slope: 2.,
            intercept: 1.
        }
    );
    assert_eq!(forecast(4., &x, &y), Ok(9.));
    // Pins POI's original-size denominator when a pair was skipped.
    let line = regression(&[1., 3.], &[2., 4.], 3).unwrap();
    near(Ok(line.slope), 15. / 13., 1e-15);
    near(Ok(line.intercept), 6. / 13., 1e-15);
    assert_eq!(
        paired(PairOperation::SumX2MinusY2, &[2., 3.], &[1., 2.]),
        Ok(8.)
    );
    assert_eq!(
        paired(PairOperation::SumX2PlusY2, &[2., 3.], &[1., 2.]),
        Ok(18.)
    );
    assert_eq!(
        paired(PairOperation::SumXMinusYSquared, &[2., 3.], &[1., 2.]),
        Ok(2.)
    );
}

#[test]
fn distributions_known_values_and_domain_errors() {
    near(normal_dist(0., 0., 1., true), 0.5, 1e-15);
    near(normal_dist(0., 0., 1., false), 0.3989422804014327, 1e-15);
    near(normal_inverse(0.975, 0., 1.), 1.959963984540054, 1e-9);
    assert_eq!(normal_inverse(0., 0., 1.), Err(CellError::Num));
    assert_eq!(normal_dist(1., 0., 0., true), Err(CellError::Num));
    near(poisson(2.9, 3., false), 0.22404180765538775, 1e-12);
    near(poisson(2., 3., true), 0.42319008112684353, 1e-12);
    assert_eq!(poisson(0., 0., false), Ok(1.));
    near(
        student_t(0., 2., StudentTKind::Density),
        0.3535533905932738,
        1e-12,
    );
    near(student_t(1., 1., StudentTKind::Cumulative), 0.75, 1e-12);
    near(student_t(-1., 1., StudentTKind::RightTail), 0.75, 1e-12);
    near(legacy_t_dist(1., 1.9, 2.9), 0.5, 1e-12);
    assert_eq!(legacy_t_dist(-1., 2., 1.), Err(CellError::Num));
    assert_eq!(legacy_t_dist(1., 2., 3.), Err(CellError::Num));
    assert_eq!(
        student_t(-1., 2., StudentTKind::TwoTails),
        Err(CellError::Num)
    );
    assert_eq!(standardize(10., 4., 2.), Ok(3.));
}

#[test]
fn extrema_a_ignores_reference_blanks_and_counts_reference_text_as_zero() {
    let input = [
        Operand {
            value: Value::Number(4.),
            origin: Origin::Reference,
        },
        Operand {
            value: Value::Text {
                numeric: Some(100.),
            },
            origin: Origin::Reference,
        },
        Operand {
            value: Value::Blank,
            origin: Origin::Reference,
        },
        Operand {
            value: Value::Boolean(true),
            origin: Origin::Array,
        },
    ];
    assert_eq!(
        coercev2_statistics_collection(Collection::ExtremaA, &input),
        Ok(vec![4., 0., 1.])
    );
    assert_eq!(
        coercev2_statistics_collection(Collection::AggregateA, &input),
        Ok(vec![4., 0., 1.])
    );
    assert_eq!(
        coercev2_statistics_collection(
            Collection::ExtremaA,
            &[Operand {
                value: Value::Text { numeric: None },
                origin: Origin::Direct,
            }]
        ),
        Err(CellError::Value)
    );
}

#[test]
fn extrema_a_blank_and_text_observations_are_distinct() {
    for policy in [
        Collection::ExtremaA,
        Collection::AggregateA,
        Collection::NumericAggregate,
    ] {
        let blank = Operand {
            value: Value::Blank,
            origin: Origin::Reference,
        };
        assert_eq!(coercev2_statistics_collection(policy, &[blank]), Ok(vec![]));
        assert_eq!(
            coercev2_statistics_collection(
                policy,
                &[Operand {
                    origin: Origin::Direct,
                    ..blank
                }]
            ),
            Ok(vec![0.])
        );
    }
    assert_eq!(
        coercev2_statistics_collection(
            Collection::ExtremaA,
            &[Operand {
                value: Value::Text { numeric: Some(12.) },
                origin: Origin::Reference,
            }]
        ),
        Ok(vec![0.])
    );
}

#[test]
fn explicit_sample_requirements_distinguish_population_and_sample() {
    use SamplePolicy::{ExcelDocumentation as Excel, PinnedPoi as Pinned};
    for operation in [Aggregate::VarianceSample, Aggregate::StdevSample] {
        for values in [&[][..], &[0.][..], &[2.][..]] {
            assert_eq!(
                aggregate_with_policy(operation, values, Excel),
                Err(CellError::Div0)
            );
        }
        assert_eq!(
            aggregate_with_policy(operation, &[0.], Pinned),
            Err(CellError::Num)
        );
        assert_eq!(aggregate_with_policy(operation, &[0., 0.], Excel), Ok(0.));
    }
    for operation in [Aggregate::VariancePopulation, Aggregate::StdevPopulation] {
        assert_eq!(
            aggregate_with_policy(operation, &[], Excel),
            Err(CellError::Div0)
        );
        assert_eq!(aggregate_with_policy(operation, &[0.], Excel), Ok(0.));
        assert_eq!(
            aggregate_with_policy(operation, &[2.], Pinned),
            Err(CellError::Num)
        );
    }
    for operation in [
        PairOperation::CovariancePopulation,
        PairOperation::CovarianceSample,
    ] {
        assert_eq!(
            paired_with_policy(operation, &[], &[], Excel),
            Err(CellError::Div0)
        );
        assert_eq!(
            paired_with_policy(operation, &[2.], &[5., 7.], Excel),
            Err(CellError::NA)
        );
        assert_eq!(
            paired_with_policy(operation, &[2.], &[5.], Pinned),
            Err(CellError::NA)
        );
    }
    assert_eq!(
        paired_with_policy(PairOperation::CovariancePopulation, &[2.], &[5.], Excel),
        Ok(0.)
    );
    assert_eq!(
        paired_with_policy(PairOperation::CovarianceSample, &[2.], &[5.], Excel),
        Err(CellError::Div0)
    );
    assert_eq!(
        paired_with_policy(PairOperation::Correlation, &[2.], &[5.], Excel),
        Err(CellError::Div0)
    );
}

#[test]
fn variance_is_invariant_under_exact_large_translations() {
    // Deviations of [0,1,2,3] from 1.5 have squared sum 5.
    // 2^40 and these integer offsets are exactly representable in binary64.
    for offset in [0.0, 1_099_511_627_776.0, -1_099_511_627_776.0] {
        let values = [offset, offset + 1., offset + 2., offset + 3.];
        for (operation, expected) in [
            (Aggregate::DevSq, 5.),
            (Aggregate::VariancePopulation, 1.25),
            (Aggregate::VarianceSample, 5. / 3.),
            (Aggregate::StdevPopulation, 1.25_f64.sqrt()),
            (Aggregate::StdevSample, (5_f64 / 3.).sqrt()),
        ] {
            near(
                aggregate_with_policy(operation, &values, SamplePolicy::ExcelDocumentation),
                expected,
                2e-15,
            );
        }
        // [0,0,1] has exact squared-deviation sum2/3; its fractional
        // mean exposes unshifted running-mean rounding at the same offset.
        let values = [offset, offset, offset + 1.];
        for (operation, expected) in [
            (Aggregate::DevSq, 2. / 3.),
            (Aggregate::VariancePopulation, 2. / 9.),
            (Aggregate::VarianceSample, 1. / 3.),
            (Aggregate::StdevPopulation, (2_f64 / 9.).sqrt()),
            (Aggregate::StdevSample, (1_f64 / 3.).sqrt()),
        ] {
            near(
                aggregate_with_policy(operation, &values, SamplePolicy::ExcelDocumentation),
                expected,
                2e-15,
            );
        }
    }
}

#[test]
fn variance_scaling_preserves_finite_standard_deviations_without_overflowing_means() {
    use Aggregate::*;
    use SamplePolicy::ExcelDocumentation as Excel;
    for operation in [
        VarianceSample,
        VariancePopulation,
        StdevSample,
        StdevPopulation,
        DevSq,
    ] {
        assert_eq!(
            aggregate_with_policy(operation, &[f64::MAX, f64::MAX], Excel),
            Ok(0.)
        );
        assert_eq!(
            aggregate_with_policy(operation, &[f64::NAN, 1.], Excel),
            Err(CellError::Num)
        );
    }
    assert_eq!(
        aggregate_with_policy(StdevPopulation, &[f64::MAX, -f64::MAX], Excel),
        Ok(f64::MAX)
    );
    assert_eq!(
        aggregate_with_policy(VariancePopulation, &[f64::MAX, -f64::MAX], Excel),
        Err(CellError::Num)
    );
    assert_eq!(
        aggregate_with_policy(StdevSample, &[f64::MAX, -f64::MAX], Excel),
        Err(CellError::Num)
    );
    assert_eq!(
        aggregate_with_policy(StdevPopulation, &[f64::MAX], Excel),
        Ok(0.)
    );
    assert_eq!(
        aggregate_with_policy(StdevSample, &[f64::MAX], Excel),
        Err(CellError::Div0)
    );
    // Two observations [0,2e154] have population variance1e308, which is finite
    // even though squaring the unnormalized difference would overflow.
    near(
        aggregate_with_policy(VariancePopulation, &[0., 2e154], Excel),
        1e308,
        2e292,
    );
    near(
        aggregate_with_policy(StdevPopulation, &[0., 2e-200], Excel),
        1e-200,
        0.,
    );
}
