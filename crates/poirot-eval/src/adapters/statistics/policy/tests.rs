use super::*;
use poirot_functions_statistics as domain;
#[test]
fn inverse_parameter_errors_precede_later_reads_and_work() {
    for f in [
        StatisticsFunction::NormInv,
        StatisticsFunction::NormInvLegacy,
    ] {
        for probability in [-0.0, 1.0, -1.0, 2.0] {
            let mut p = Port {
                work_limit: Some(128),
                ..Port::default()
            };
            assert_eq!(
                evaluate(
                    &mut p,
                    f,
                    &[n(probability), e(CellError::Div0), e(CellError::Ref)]
                ),
                Err(Failure::Spreadsheet(CellError::Num))
            );
            assert_eq!(p.reads, 1);
            assert!(p.work < 128);
        }
        let mut p = Port {
            work_limit: Some(128),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut p, f, &[n(0.5), n(0.0), n(-0.0)]),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        assert!(p.work < 128);
        assert_eq!(
            evaluate(&mut p, f, &[n(0.5), n(0.0), n(1.0)]),
            Err(Failure::Operational("work"))
        );
    }
    for f in [
        StatisticsFunction::NormSInv,
        StatisticsFunction::NormSInvLegacy,
    ] {
        let mut p = Port {
            work_limit: Some(128),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut p, f, &[n(1.0)]),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        assert!(p.work < 128);
        assert_eq!(
            evaluate(&mut p, f, &[n(0.5)]),
            Err(Failure::Operational("work"))
        );
    }
}
#[derive(Clone)]
/// Controlled scalar or collection fixture with source provenance.
struct Arg {
    values: Vec<Scalar>,
    origin: Origin,
    area: bool,
}
fn n(x: f64) -> Arg {
    Arg {
        values: vec![Scalar::Number(x)],
        origin: Origin::Direct,
        area: false,
    }
}
fn e(x: CellError) -> Arg {
    Arg {
        values: vec![Scalar::Error(x)],
        origin: Origin::Reference,
        area: false,
    }
}
fn area(values: Vec<Scalar>) -> Arg {
    Arg {
        values,
        origin: Origin::Reference,
        area: true,
    }
}
#[derive(Default)]
/// Statistical test host recording lazy reads and enforcing configured limits.
struct Port {
    reads: usize,
    cells: usize,
    cell_reads: Vec<(usize, Scalar)>,
    work: u64,
    work_limit: Option<u64>,
    collection_limit: Option<usize>,
    text_limit: Option<usize>,
}
impl EvaluationPort for Port {
    /// Deferred argument identity supplied to the family's host port.
    type Argument = Arg;
    /// Host abort channel kept outside catchable spreadsheet values.
    type Operational = &'static str;
    fn resolve_scalar(&mut self, a: &Arg) -> Result<Scalar, Failure<Self::Operational>> {
        self.reads += 1;
        Ok(a.values[0].clone())
    }
    fn collection_len(&mut self, a: &Arg) -> Result<usize, Failure<Self::Operational>> {
        Ok(a.values.len())
    }
    fn collection_cell(
        &mut self,
        a: &Arg,
        i: usize,
    ) -> Result<(Scalar, Origin), Failure<Self::Operational>> {
        self.cells += 1;
        self.cell_reads.push((i, a.values[i].clone()));
        Ok((a.values[i].clone(), a.origin))
    }
    fn charge_work(&mut self, n: u64) -> Result<(), Self::Operational> {
        if self
            .work_limit
            .is_some_and(|m| self.work.saturating_add(n) > m)
        {
            Err("work")
        } else {
            self.work += n;
            Ok(())
        }
    }
    fn check_collection(&mut self, n: usize) -> Result<(), Self::Operational> {
        if self.collection_limit.is_some_and(|m| n > m) {
            Err("collection")
        } else {
            Ok(())
        }
    }
    fn check_text(&mut self, n: usize) -> Result<(), Self::Operational> {
        if self.text_limit.is_some_and(|m| n > m) {
            Err("text")
        } else {
            Ok(())
        }
    }
    fn coerce_text_number(&mut self, t: &str) -> Result<f64, Failure<Self::Operational>> {
        poirot_numeric::parse_java_number(t).ok_or(CellError::Value.into())
    }
    fn is_reference(&mut self, a: &Arg) -> Result<bool, Failure<Self::Operational>> {
        Ok(a.origin == Origin::Reference)
    }
    fn is_array(&mut self, a: &Arg) -> Result<bool, Failure<Self::Operational>> {
        Ok(a.origin == Origin::Array)
    }
    fn is_area(&mut self, a: &Arg) -> Result<bool, Failure<Self::Operational>> {
        Ok(a.area)
    }
}

#[test]
fn arity_and_poisson_default_are_lazy() {
    let mut p = Port::default();
    assert_eq!(
        evaluate(
            &mut p,
            StatisticsFunction::NormSDist,
            &[n(1.0), e(CellError::NA)]
        ),
        Err(Failure::Spreadsheet(CellError::Value))
    );
    assert_eq!(p.reads, 0);
    assert_eq!(
        evaluate(
            &mut p,
            StatisticsFunction::Poisson,
            &[n(0.0), n(0.0), e(CellError::Ref)]
        ),
        Ok(1.0)
    );
    assert_eq!(p.reads, 2);
}
#[test]
fn mode_area_and_single_reference_differ() {
    let a = area(vec![
        Scalar::Text("x".into()),
        Scalar::Boolean(true),
        Scalar::Blank,
        Scalar::Number(2.0),
        Scalar::Number(2.0),
    ]);
    assert_eq!(
        evaluate(&mut Port::default(), StatisticsFunction::Mode, &[a]),
        Ok(2.0)
    );
    let a = Arg {
        values: vec![Scalar::Boolean(true)],
        origin: Origin::Reference,
        area: false,
    };
    assert_eq!(
        evaluate(&mut Port::default(), StatisticsFunction::Mode, &[a]),
        Err(Failure::Spreadsheet(CellError::Value))
    );
}
#[test]
fn paired_alignment_original_length_and_error_order() {
    let x = area(vec![
        Scalar::Number(1.0),
        Scalar::Text("x".into()),
        Scalar::Number(3.0),
    ]);
    let y = area(vec![
        Scalar::Number(2.0),
        Scalar::Number(999.0),
        Scalar::Number(6.0),
    ]);
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::SumXMY2,
            &[x.clone(), y.clone()]
        ),
        Ok(10.0)
    );
    let expected = domain::regression(&[1.0, 3.0], &[2.0, 6.0], 3)
        .unwrap()
        .slope;
    assert_eq!(
        evaluate(&mut Port::default(), StatisticsFunction::Slope, &[y, x]),
        Ok(expected)
    );
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::Forecast,
            &[n(1.0), e(CellError::Ref), e(CellError::NA)]
        ),
        Err(Failure::Spreadsheet(CellError::Ref))
    );
}
#[test]
fn percentrank_and_rank_precedence_extras() {
    let mut p = Port::default();
    assert_eq!(
        evaluate(
            &mut p,
            StatisticsFunction::PercentRank,
            &[e(CellError::Ref), e(CellError::NA)]
        ),
        Err(Failure::Spreadsheet(CellError::Num))
    );
    assert_eq!(p.cells, 0);
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::PercentRankInc,
            &[
                area(vec![
                    Scalar::Number(1.0),
                    Scalar::Number(2.0),
                    Scalar::Number(3.0)
                ]),
                n(2.0),
                n(3.0),
                e(CellError::NA)
            ]
        ),
        Ok(0.5)
    );
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::Rank,
            &[n(1.0), e(CellError::Ref), n(-0.1)]
        ),
        Err(Failure::Spreadsheet(CellError::Num))
    );
}
#[test]
fn operational_failures_are_never_spreadsheet_errors() {
    let mut p = Port {
        collection_limit: Some(1),
        ..Port::default()
    };
    assert_eq!(
        evaluate(
            &mut p,
            StatisticsFunction::Large,
            &[area(vec![Scalar::Number(1.0), Scalar::Number(2.0)]), n(1.0)]
        ),
        Err(Failure::Operational("collection"))
    );
    assert_eq!(p.cells, 0);
    let mut p = Port {
        work_limit: Some(0),
        ..Port::default()
    };
    assert_eq!(
        evaluate(&mut p, StatisticsFunction::PercentRank, &[n(1.0), n(1.0)]),
        Err(Failure::Operational("work"))
    );
    assert_eq!(p.reads, 0);
}
#[test]
fn distribution_text_boolean_and_domain_precedence() {
    let mut a = n(1.0);
    a.values[0] = Scalar::Boolean(true);
    assert_eq!(
        evaluate(&mut Port::default(), StatisticsFunction::NormSInv, &[a]),
        Err(Failure::Spreadsheet(CellError::Value))
    );
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::NormDist,
            &[n(0.0), n(0.0), n(-1.0), e(CellError::NA)]
        ),
        Err(Failure::Spreadsheet(CellError::Num))
    );
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::TDistLegacy,
            &[n(-1.0), e(CellError::NA), n(1.0)]
        ),
        Err(Failure::Spreadsheet(CellError::Num))
    );
}

#[test]
fn pair_protocols_preserve_traversal_and_admission() {
    let x = area(vec![Scalar::Error(CellError::Ref), Scalar::Number(3.0)]);
    let y = area(vec![Scalar::Error(CellError::NA), Scalar::Number(6.0)]);
    let mut p = Port::default();
    assert_eq!(
        evaluate(&mut p, StatisticsFunction::SumXMY2, &[x.clone(), y.clone()]),
        Err(Failure::Spreadsheet(CellError::Ref))
    );
    assert_eq!(
        p.cell_reads,
        vec![
            (0, Scalar::Error(CellError::Ref)),
            (0, Scalar::Error(CellError::NA)),
            (1, Scalar::Number(3.0)),
            (1, Scalar::Number(6.0)),
        ]
    );

    let mut p = Port::default();
    assert_eq!(
        evaluate(&mut p, StatisticsFunction::Slope, &[x.clone(), y.clone()]),
        Err(Failure::Spreadsheet(CellError::NA))
    );
    assert_eq!(
        p.cell_reads,
        vec![
            (0, Scalar::Error(CellError::NA)),
            (0, Scalar::Error(CellError::Ref))
        ]
    );

    let mut p = Port::default();
    assert_eq!(
        evaluate(&mut p, StatisticsFunction::Correl, &[x.clone(), y.clone()]),
        Err(Failure::Spreadsheet(CellError::Ref))
    );
    assert_eq!(p.cell_reads, vec![(0, Scalar::Error(CellError::Ref))]);

    let mut p = Port::default();
    assert_eq!(
        evaluate(
            &mut p,
            StatisticsFunction::SumXMY2,
            &[x.clone(), e(CellError::NA)]
        ),
        Err(Failure::Spreadsheet(CellError::NA))
    );
    assert!(p.cell_reads.is_empty());

    let mut p = Port {
        work_limit: Some(12),
        ..Port::default()
    };
    assert_eq!(
        evaluate(&mut p, StatisticsFunction::SumXMY2, &[x, y]),
        Err(Failure::Operational("work"))
    );
    assert_eq!(p.work, 1);
    assert!(p.cell_reads.is_empty());
}

#[test]
fn standard_normal_distribution_uses_pinned_poi_negative_domain() {
    // FunctionEval[294] and AnalysisToolPak NORM.S.DIST both bind NormSDist.
    // NormSDist.evaluate rejects a negative x before calling NormDist.probability.
    for f in [
        StatisticsFunction::NormSDist,
        StatisticsFunction::NormSDistLegacy,
    ] {
        let mut p = Port::default();
        assert_eq!(
            evaluate(&mut p, f, &[n(-1.0)]),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        // Adapter entry1 + scalar acquisition1 + parsing the two bytes "-1".
        // No fixed512 setup or kernel work is reached for the negative domain.
        assert_eq!(p.work, 4);
        assert_eq!(p.reads, 1);
        assert!(p.cell_reads.is_empty());
        assert_eq!(evaluate(&mut p, f, &[n(-0.0)]), Ok(0.5));
    }
}

#[test]
fn excel_standard_normal_arity_is_shared_and_precedes_port_work() {
    let f = StatisticsFunction::NormSDist;
    for count in [0, 1, 3] {
        let mut p = Port::default();
        assert!(!f.accepts_with_policy(count, SemanticsPolicy::ExcelDocumentation));
        assert_eq!(
            evaluate_with_policy(
                &mut p,
                f,
                &vec![n(0.0); count],
                SemanticsPolicy::ExcelDocumentation
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert_eq!(p.reads, 0);
        assert_eq!(p.work, 0);
    }
    assert!(f.accepts_with_policy(2, SemanticsPolicy::ExcelDocumentation));
    assert_eq!(
        evaluate_with_policy(
            &mut Port::default(),
            f,
            &[n(0.0), n(1.0)],
            SemanticsPolicy::ExcelDocumentation
        ),
        Ok(0.5)
    );
}

#[test]
fn rank_array_admission_preserves_pinned_reference_guard_and_budgeted_collection() {
    let array = Arg {
        values: vec![
            Scalar::Number(1.),
            Scalar::Boolean(true),
            Scalar::Text("9".into()),
            Scalar::Error(CellError::Ref),
            Scalar::Number(4.),
            Scalar::Number(4.),
        ],
        origin: Origin::Array,
        area: true,
    };
    for f in [StatisticsFunction::Rank, StatisticsFunction::RankEq] {
        let args = [n(4.), array.clone(), n(-0.1)];
        let mut p = Port::default();
        assert_eq!(
            evaluate_with_policy(&mut p, f, &args, SemanticsPolicy::ExcelDocumentation),
            Ok(2.)
        );
        assert_eq!(p.cells, 6);
        let mut p = Port {
            collection_limit: Some(2),
            ..Port::default()
        };
        assert_eq!(
            evaluate_with_policy(&mut p, f, &args, SemanticsPolicy::ExcelDocumentation),
            Err(Failure::Operational("collection"))
        );
        assert_eq!(p.cells, 0);
        let mut p = Port {
            work_limit: Some(10),
            ..Port::default()
        };
        assert_eq!(
            evaluate_with_policy(&mut p, f, &args, SemanticsPolicy::ExcelDocumentation),
            Err(Failure::Operational("work"))
        );
        assert_eq!(p.cells, 0);
        assert_eq!(
            evaluate_with_policy(
                &mut Port::default(),
                f,
                &[n(4.), n(4.)],
                SemanticsPolicy::ExcelDocumentation
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
    }
    assert_eq!(
        evaluate(
            &mut Port::default(),
            StatisticsFunction::Rank,
            &[n(4.), array]
        ),
        Err(Failure::Spreadsheet(CellError::Value))
    );
}
