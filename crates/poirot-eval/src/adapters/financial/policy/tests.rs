#[cfg(test)]
mod adapter_tests {
    use super::super::*;
    #[derive(Clone)]
    /// Controlled financial operand with scalar, collection, and provenance data.
    struct Arg {
        values: Vec<Scalar>,
        origin: Origin,
    }
    fn n(x: f64) -> Arg {
        Arg {
            values: vec![Scalar::Number(x)],
            origin: Origin::Direct,
        }
    }
    fn e(x: CellError) -> Arg {
        Arg {
            values: vec![Scalar::Error(x)],
            origin: Origin::Reference,
        }
    }
    fn area(values: Vec<Scalar>) -> Arg {
        Arg {
            values,
            origin: Origin::Reference,
        }
    }
    #[derive(Default)]
    /// Financial test host recording lazy access and admitting configured resource limits.
    struct Port {
        reads: usize,
        cells: usize,
        work: u64,
        work_limit: Option<u64>,
        collection_limit: Option<usize>,
        text_limit: Option<usize>,
        locale: bool,
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
        fn coercev2_text_number(&mut self, t: &str) -> Result<f64, Failure<Self::Operational>> {
            poirot_parsing::java_number::parse_java_number(t).ok_or(CellError::Value.into())
        }
        fn currency_format(&mut self) -> Result<CurrencyContext, Failure<Self::Operational>> {
            if self.locale {
                Ok(domain::CurrencyFormat::US.into())
            } else {
                Err(Failure::UnsupportedCapability("locale"))
            }
        }
    }

    #[test]
    fn arity_and_ignored_extras_are_lazy() {
        let mut p = Port::default();
        assert_eq!(
            evaluate(&mut p, FinancialFunction::Ipmt, &[e(CellError::NA)]),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert_eq!(p.reads, 0);
        let _ = evaluate(
            &mut p,
            FinancialFunction::Ppmt,
            &[n(0.1), n(1.0), n(2.0), n(100.0), e(CellError::NA)],
        );
        assert_eq!(p.reads, 4);
    }
    #[test]
    fn aggregate_origins_skip_reference_text_without_period() {
        let flows = area(vec![
            Scalar::Number(-100.0),
            Scalar::Text("200".into()),
            Scalar::Boolean(true),
            Scalar::Number(121.0),
        ]);
        let r = evaluate(
            &mut Port::default(),
            FinancialFunction::Npv,
            &[n(0.1), flows],
        )
        .unwrap();
        assert!(matches!(r,Output::Number(v) if (v-9.09090909090908).abs()<1e-10));
    }
    #[test]
    fn optional_missing_and_zero_rate_annuity() {
        let mut args = vec![n(0.0), n(10.0), n(-20.0), n(0.0), n(0.0)];
        args[3].values[0] = Scalar::Missing;
        args[4].values[0] = Scalar::Missing;
        assert_eq!(
            evaluate(&mut Port::default(), FinancialFunction::Fv, &args),
            Ok(Output::Number(200.0))
        );
    }
    #[test]
    fn malformed_mirr_and_first_error() {
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Mirr,
                &[area(vec![])]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Npv,
                &[n(0.0), e(CellError::Ref), e(CellError::NA)]
            ),
            Err(Failure::Spreadsheet(CellError::Ref))
        );
    }
    #[test]
    fn operational_limits_precede_collection_and_algorithm() {
        let mut p = Port {
            collection_limit: Some(1),
            ..Port::default()
        };
        assert_eq!(
            evaluate(
                &mut p,
                FinancialFunction::Npv,
                &[n(0.0), area(vec![Scalar::Number(1.0), Scalar::Number(2.0)])]
            ),
            Err(Failure::Operational("collection"))
        );
        assert_eq!(p.cells, 0);
        let mut p = Port {
            work_limit: Some(0),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut p, FinancialFunction::Pmt, &[n(0.0), n(2.0), n(4.0)]),
            Err(Failure::Operational("work"))
        );
        assert_eq!(p.reads, 0);
    }
    #[test]
    fn exact_fixed_currency_context_and_decimals() {
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Fixed,
                &[n(0.1 + 0.2), n(17.0)]
            ),
            Ok(Output::Text("0.30000000000000000".into()))
        );
        assert_eq!(
            evaluate(&mut Port::default(), FinancialFunction::Dollar, &[n(1.0)]),
            Err(Failure::UnsupportedCapability("locale"))
        );
        assert_eq!(
            evaluate(
                &mut Port {
                    locale: true,
                    ..Port::default()
                },
                FinancialFunction::Dollar,
                &[n(1234.5)]
            ),
            Ok(Output::Text("$1,234.50".into()))
        );
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Dollar,
                &[n(1.0), n(127.9)]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
    }
    #[test]
    fn string_numeric_and_rate_resolution_precedence() {
        let mut a = n(1.0);
        a.values[0] = Scalar::Boolean(true);
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::DollarDe,
                &[a, n(16.0)]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        let mut a = n(1.0);
        a.values[0] = Scalar::Text("bad".into());
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Rate,
                &[a, n(1.0), e(CellError::Ref)]
            ),
            Err(Failure::Spreadsheet(CellError::Ref))
        );
    }
    #[test]
    fn decimal_only_parser_rejects_java_extensions_and_overflow() {
        for text in [
            "NaN", "Infinity", "0x1p0", "16d", "1E400", "1e", "", "\u{a0}16",
        ] {
            assert_eq!(coercev2_financial_decimal_text(text), None, "{text:?}");
        }
        for (text, expected) in [
            ("\t +16.0 \n", 16.0),
            (".5", 0.5),
            ("1.", 1.0),
            ("-2e-3", -0.002),
        ] {
            assert_eq!(coercev2_financial_decimal_text(text), Some(expected));
        }
    }
    #[test]
    fn fraction_argument_uses_same_string_contract_as_number() {
        for function in [FinancialFunction::DollarDe, FinancialFunction::DollarFr] {
            for value in [
                Scalar::Boolean(true),
                Scalar::Blank,
                Scalar::Text("16d".into()),
                Scalar::Text("1E400".into()),
            ] {
                let mut fraction = n(0.0);
                fraction.values[0] = value;
                assert_eq!(
                    evaluate(&mut Port::default(), function, &[n(1.125), fraction]),
                    Err(Failure::Spreadsheet(CellError::Value))
                );
            }
            let mut fraction = n(0.0);
            fraction.values[0] = Scalar::Text(" 16 ".into());
            assert!(matches!(
                evaluate(&mut Port::default(), function, &[n(1.125), fraction]),
                Ok(Output::Number(_))
            ));
        }
    }
    #[test]
    fn payment_parts_resolve_before_coercion_and_integer_checks() {
        for function in [FinancialFunction::Ipmt, FinancialFunction::Ppmt] {
            let mut invalid = n(0.0);
            invalid.values[0] = Scalar::Text("bad".into());
            assert_eq!(
                evaluate(
                    &mut Port::default(),
                    function,
                    &[invalid, n(1.0), n(12.0), e(CellError::Ref)]
                ),
                Err(Failure::Spreadsheet(CellError::Ref))
            );
            assert_eq!(
                evaluate(
                    &mut Port::default(),
                    function,
                    &[n(0.1), n(1e30), n(12.0), e(CellError::NA)]
                ),
                Err(Failure::Spreadsheet(CellError::NA))
            );
        }
    }
    #[test]
    fn aggregate_direct_text_is_decimal_only_but_reference_text_is_skipped() {
        let mut invalid = n(0.0);
        invalid.values[0] = Scalar::Text("2001-01-01".into());
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Npv,
                &[n(0.0), invalid.clone()]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        invalid.origin = Origin::Reference;
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Npv,
                &[n(0.0), invalid, n(7.0)]
            ),
            Ok(Output::Number(7.0))
        );
    }
    #[test]
    fn irr_reserves_source_bounded_work_and_keeps_semantic_num_distinct() {
        let mut flows = vec![Scalar::Number(0.0); 100];
        flows[0] = Scalar::Number(-100.0);
        flows[1] = Scalar::Number(110.0);
        let mut port = Port {
            work_limit: Some(1_000_000),
            collection_limit: Some(100),
            ..Port::default()
        };
        assert!(
            matches!(evaluate(&mut port, FinancialFunction::Irr, &[area(flows)]), Ok(Output::Number(value)) if (value - 0.1).abs() < 1e-10)
        );
        let mut port = Port {
            work_limit: Some(100),
            ..Port::default()
        };
        assert_eq!(
            evaluate(
                &mut port,
                FinancialFunction::Irr,
                &[area(vec![Scalar::Number(-100.0), Scalar::Number(1.0)])]
            ),
            Err(Failure::Operational("work"))
        );
        let mut port = Port {
            collection_limit: Some(0),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut port, FinancialFunction::Irr, &[area(vec![])]),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        assert_eq!(
            evaluate(
                &mut Port::default(),
                FinancialFunction::Irr,
                &[area(vec![Scalar::Number(1.0), Scalar::Number(2.0)])]
            ),
            Err(Failure::Spreadsheet(CellError::Num))
        );
    }
    #[test]
    fn documented_interest_defaults_preserve_missing_errors_and_limits() {
        let mut args = vec![n(0.1), n(1.0), n(5.0), n(1000.0), n(0.0), n(0.0)];
        args[4].values[0] = Scalar::Missing;
        args[5].values[0] = Scalar::Missing;
        let policy = SemanticsPolicy::ExcelDocumentation;
        assert_eq!(
            evaluate_with_policy(&mut Port::default(), FinancialFunction::Ipmt, &args, policy),
            Ok(Output::Number(-100.0))
        );
        args[0].values[0] = Scalar::Missing;
        assert_eq!(
            evaluate_with_policy(&mut Port::default(), FinancialFunction::Ipmt, &args, policy),
            Ok(Output::Number(0.0))
        );
        args[5] = e(CellError::Ref);
        assert_eq!(
            evaluate_with_policy(&mut Port::default(), FinancialFunction::Ipmt, &args, policy),
            Err(Failure::Spreadsheet(CellError::Ref))
        );
        let mut port = Port {
            collection_limit: Some(5),
            ..Port::default()
        };
        assert_eq!(
            evaluate_with_policy(&mut port, FinancialFunction::Ipmt, &args, policy),
            Err(Failure::Operational("collection"))
        );
        assert_eq!(port.reads, 0);
        let mut port = Port {
            work_limit: Some(3),
            ..Port::default()
        };
        assert_eq!(
            evaluate_with_policy(&mut port, FinancialFunction::Ppmt, &args, policy),
            Err(Failure::Operational("work"))
        );
        assert_eq!(port.reads, 2);
        args[0].values[0] = Scalar::Text("1000".into());
        args[5] = n(0.0);
        let mut port = Port {
            text_limit: Some(3),
            ..Port::default()
        };
        assert_eq!(
            evaluate_with_policy(&mut port, FinancialFunction::Ppmt, &args, policy),
            Err(Failure::Operational("text"))
        );
        assert_eq!(port.reads, 6);
    }
    #[test]
    fn excel_arity_rejects_before_any_port_work_or_resolution() {
        for function in [FinancialFunction::Ipmt, FinancialFunction::Ppmt] {
            for count in [0, 3, 7] {
                let mut p = Port::default();
                assert!(!function.accepts_with_policy(count, SemanticsPolicy::ExcelDocumentation));
                assert_eq!(
                    evaluate_with_policy(
                        &mut p,
                        function,
                        &vec![n(0.0); count],
                        SemanticsPolicy::ExcelDocumentation
                    ),
                    Err(Failure::Spreadsheet(CellError::Value))
                );
                assert_eq!(p.reads, 0);
                assert_eq!(p.work, 0);
            }
            for count in 4..=6 {
                assert!(function.accepts_with_policy(count, SemanticsPolicy::ExcelDocumentation));
            }
            let mut p = Port::default();
            assert_eq!(
                evaluate_with_policy(
                    &mut p,
                    function,
                    &[n(0.0), n(1.0), n(2.0), n(100.0)],
                    SemanticsPolicy::ExcelDocumentation
                ),
                Ok(Output::Number(if function == FinancialFunction::Ipmt {
                    0.0
                } else {
                    -50.0
                }))
            );
            assert_eq!(p.reads, 4);
        }
    }
}
