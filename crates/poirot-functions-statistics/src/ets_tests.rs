//! Independent recursion, grid and fitted-pattern controls for the ETS kernel.
use super::*;
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < b.abs().max(1.0) * 1e-9, "{a} != {b}");
}
fn series(values: &[f64]) -> Series {
    prepare(
        values
            .iter()
            .enumerate()
            .map(|(i, y)| ((i + 1) as f64, *y))
            .collect(),
        Aggregation::Average,
        Completion::Interpolate,
        CalendarEpoch::Excel1900,
        1000,
        |_| Ok::<_, ()>(()),
    )
    .unwrap()
}
#[test]
fn rational_innovations_and_future_variance() {
    // Independently substituted rational equations from ETS_SOURCES.md.
    let gains = Gains {
        alpha: 0.5,
        beta: 0.25,
        gamma: 0.125,
    };
    let mut level = 10.0;
    let mut trend = 1.0;
    let mut phases = [-2.0, 2.0];
    let mut sse = 0.0;
    for (i, y) in [11.0, 14.0].into_iter().enumerate() {
        let (prediction, error) =
            innovation(&mut level, &mut trend, Some(&mut phases[i]), gains, y);
        near(prediction, if i == 0 { 9.0 } else { 15.5 });
        sse += error * error;
    }
    let fit = Fit {
        level,
        trend,
        seasons: phases.to_vec(),
        next_phase: 0,
        gains,
        sse,
        mae: 1.75,
        smape: 0.0,
        naive: 1.0,
        count: 2,
        scale: 1.0,
    };
    near(fit.forecast::<()>(1.0).unwrap(), 97.0 / 8.0);
    near(fit.forecast::<()>(2.0).unwrap(), 269.0 / 16.0);
    near(fit.statistic::<()>(7).unwrap(), (25.0f64 / 8.0).sqrt());
    let z = 1.959963984540054;
    for (h, v) in [
        (1.0, 25.0 / 8.0),
        (2.0, 625.0 / 128.0),
        (3.0, 4525.0 / 512.0),
    ] {
        near(fit.confidence::<()>(h, 0.95).unwrap(), z * f64::sqrt(v));
    }
}
#[test]
fn consecutive_gaps_are_weighted_and_zero_completion_is_distinct() {
    let points = vec![
        (11.0, 2.0),
        (12.0, 5.0),
        (15.0, 14.0),
        (16.0, 17.0),
        (17.0, 20.0),
    ];
    let s = prepare(
        points.clone(),
        Aggregation::Average,
        Completion::Interpolate,
        CalendarEpoch::Excel1900,
        100,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(s.values(), &[2.0, 5.0, 8.0, 11.0, 14.0, 17.0, 20.0]);
    let s = prepare(
        points,
        Aggregation::Average,
        Completion::Zero,
        CalendarEpoch::Excel1900,
        100,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(s.values(), &[2.0, 5.0, 0.0, 0.0, 14.0, 17.0, 20.0]);
}
#[test]
fn duplicate_enum_sorting_and_limits() {
    for (aggregation, expect) in [
        (Aggregation::Average, 3.0),
        (Aggregation::Sum, 6.0),
        (Aggregation::Count, 2.0),
        (Aggregation::CountA, 2.0),
        (Aggregation::Minimum, 2.0),
        (Aggregation::Maximum, 4.0),
        (Aggregation::Median, 3.0),
    ] {
        let s = prepare(
            vec![(2.0, 8.0), (1.0, 4.0), (1.0, 2.0)],
            aggregation,
            Completion::Interpolate,
            CalendarEpoch::Excel1900,
            100,
            |_| Ok::<_, ()>(()),
        )
        .unwrap();
        near(s.values()[0], expect);
    }
    assert_eq!(Aggregation::from_number(0.0), Err(CellError::Num));
    assert!(matches!(
        prepare(
            vec![(1.0, 1.0), (4.0, 4.0), (5.0, 5.0)],
            Aggregation::Average,
            Completion::Interpolate,
            CalendarEpoch::Excel1900,
            100,
            |_| Ok::<_, ()>(())
        ),
        Err(Failure::Spreadsheet(CellError::Num))
    ));
    assert!(matches!(
        prepare(
            vec![(1.0, 1.0), (2.0, 2.0)],
            Aggregation::Average,
            Completion::Interpolate,
            CalendarEpoch::Excel1900,
            1,
            |_| Ok::<_, ()>(())
        ),
        Err(Failure::ComputationalLimit)
    ));
}
#[test]
fn perfect_linear_and_repeated_seasonal_models() {
    let s = series(&[3.0, 5.0, 7.0, 9.0, 11.0, 13.0]);
    let f = fit(&s, Seasonality::Automatic, |_| Ok::<_, ()>(())).unwrap();
    assert_eq!(f.period(), 0);
    near(f.forecast::<()>(1.0).unwrap(), 15.0);
    near(f.forecast::<()>(2.5).unwrap(), 18.0);
    let s = series(&[8.0, 12.0, 8.0, 12.0, 8.0, 12.0, 8.0, 12.0]);
    let f = fit(&s, Seasonality::Automatic, |_| Ok::<_, ()>(())).unwrap();
    assert_eq!(f.period(), 2);
    near(f.forecast::<()>(1.0).unwrap(), 8.0);
    near(f.forecast::<()>(2.0).unwrap(), 12.0);
}
#[test]
fn optimizer_improves_noisy_objective_and_meter_aborts() {
    let s = series(&[10.0, 11.0, 13.0, 12.0, 15.0, 16.0, 15.0, 19.0, 20.0, 18.0]);
    let (v, _) = normalized::<()>(&s.values).unwrap();
    let initial = run(&v, 0, gains([0.0; 3], false), false, &mut |_| {
        Ok::<_, ()>(())
    })
    .unwrap();
    let fitted = fit(&s, Seasonality::None, |_| Ok::<_, ()>(())).unwrap();
    assert!(fitted.sse < initial.sse);
    assert!(matches!(
        fit(&s, Seasonality::None, |_| Err("stop")),
        Err(Failure::Operational("stop"))
    ));
}
#[test]
fn monthly_timeline_uses_calendar_horizon() {
    // 2024-01-01, 02-01, 03-01, 04-01 and future 05-01 in the 1900 epoch.
    let s = prepare(
        vec![
            (45292.0, 1.0),
            (45323.0, 2.0),
            (45352.0, 3.0),
            (45383.0, 4.0),
        ],
        Aggregation::Average,
        Completion::Interpolate,
        CalendarEpoch::Excel1900,
        100,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    near(s.step(), 1.0);
    near(s.horizon(45413.0).unwrap(), 1.0);
}

#[test]
fn ordinary_noisy_fits_preserve_translation_and_scaling() {
    let values: Vec<_> = (0..40)
        .map(|i| {
            let x = f64::from(i);
            100.0 + 0.7 * x + 2.0 * (x * 1.3).sin() + 0.2 * (x * 0.7).cos()
        })
        .collect();
    for seasonal in [
        Seasonality::None,
        Seasonality::Period(4),
        Seasonality::Automatic,
    ] {
        let base = fit(&series(&values), seasonal, |_| Ok::<_, ()>(())).unwrap();
        let expected = base.forecast::<()>(3.0).unwrap();
        let translated: Vec<_> = values.iter().map(|x| x + 1e6).collect();
        let shifted = fit(&series(&translated), seasonal, |_| Ok::<_, ()>(())).unwrap();
        let actual = shifted.forecast::<()>(3.0).unwrap() - 1e6;
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
        let scaled: Vec<_> = values.iter().map(|x| x * 1e100).collect();
        let large = fit(&series(&scaled), seasonal, |_| Ok::<_, ()>(())).unwrap();
        near(large.forecast::<()>(3.0).unwrap() / 1e100, expected);
    }
}
#[test]
fn inclusive_missing_boundary_and_small_coverage_are_defined() {
    let points = [1, 2, 3, 4, 5, 6, 10]
        .into_iter()
        .map(|i| (f64::from(i), f64::from(i)))
        .collect();
    let s = prepare(
        points,
        Aggregation::Average,
        Completion::Interpolate,
        CalendarEpoch::Excel1900,
        100,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(
        s.values(),
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
    );
    let s = series(&[1.0, 3.0, 2.0, 5.0, 4.0, 7.0, 9.0, 8.0]);
    let f = fit(&s, Seasonality::None, |_| Ok::<_, ()>(())).unwrap();
    let radius = f.confidence::<()>(1.0, 1e-100).unwrap();
    assert!(radius > 0.0 && radius.is_finite());
}
