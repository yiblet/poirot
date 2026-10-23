//! Family-owned evaluation; generic argument coercion remains in the execution core.
use crate::*;
/// Statistical or logical operation sharing the sparse aggregation traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AggregateFunction {
    /// Statistical aggregate selected by its handler identity.
    Statistics(StatisticsFunction),
    /// Logical conjunction over selected members.
    And,
    /// Logical disjunction over selected members.
    Or,
    /// Odd parity over selected logical members.
    Xor,
}
/// Aggregate members retaining either eagerly read values or deferred sheet cells.
enum AggregateItems {
    Values(std::vec::IntoIter<(CellValue, bool)>),
    Projected(std::vec::IntoIter<AggregateMember>),
    Cells(std::vec::IntoIter<(usize, CellAddress)>),
}
/// Reference and array members retain only the information aggregate consumers inspect.
enum AggregateMember {
    Blank,
    Number(f64),
    Boolean(bool),
    Error(CellError),
    Text { empty: bool },
}
impl AggregateMember {
    /// Compact only after a full cell read has completed its observations and admission.
    fn project(value: CellValue, _indirect: bool) -> Self {
        match value {
            Blank => Self::Blank,
            Number(value) => Self::Number(value),
            Boolean(value) => Self::Boolean(value),
            Error(value) => Self::Error(value),
            // CellValue text equality compares UTF-16 units across both representations.
            Text(value) => Self::Text {
                empty: value.is_empty(),
            },
            CellValue::TextUtf16(value) => Self::Text {
                empty: value.units().is_empty(),
            },
        }
    }
}
impl Session<'_> {
    /// Dispatch statistical calls to helper policies or the shared aggregate path.
    pub(crate) fn statistics_function(
        &mut self,
        s: usize,
        at: CellAddress,
        function: StatisticsFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        if let Some(function) = function.helper() {
            return adapters::statistics::execute(self, s, at, function, args);
        }
        self.aggregate(s, at, AggregateFunction::Statistics(function), args)
    }
    /// Aggregate deferred members with source coercion and error ordering.
    pub(crate) fn aggregate(
        &mut self,
        s: usize,
        at: CellAddress,
        name: AggregateFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        use poirot_functions_statistics::{NumericReduction as R, NumericReductionState};
        let operation = match name {
            AggregateFunction::Statistics(StatisticsFunction::Sum) => Some(R::Sum),
            AggregateFunction::Statistics(StatisticsFunction::SumSq) => Some(R::SumSq),
            AggregateFunction::Statistics(StatisticsFunction::Average) => Some(R::Average),
            AggregateFunction::Statistics(StatisticsFunction::Min) => Some(R::Minimum),
            AggregateFunction::Statistics(StatisticsFunction::Max) => Some(R::Maximum),
            AggregateFunction::Statistics(StatisticsFunction::Product) => Some(R::Product),
            _ => None,
        };
        let mut reduction = operation.map(NumericReductionState::new);
        let mut nums = Vec::new();
        let mut count = 0u64;
        use poirot_functions_logical::{LogicalReduction, LogicalReductionState};
        let mut truth = LogicalReductionState::new(if name == AggregateFunction::And {
            LogicalReduction::And
        } else if name == AggregateFunction::Xor {
            LogicalReduction::Xor
        } else {
            LogicalReduction::Or
        });
        for e in args {
            let v = self.expr(s, at, e)?;
            // RefListEval is not an area; pinned SUM throws before traversing its cells.
            if name == AggregateFunction::Statistics(StatisticsFunction::Sum)
                && matches!(&v, Value::SignedRefs(areas) if areas.len() > 1)
            {
                return Err(EvaluationFailure::KnownSourceException {
                    source: Some(EvaluationTarget {
                        sheet: s,
                        address: at,
                    }),
                    exception: KnownSourceException::AggregateReferenceListConversion,
                });
            }
            if name == AggregateFunction::Statistics(StatisticsFunction::CountBlank)
                && let Value::Refs(areas) | Value::SheetSpan(areas) = &v
            {
                for area in areas {
                    let addresses = self.sparse_addresses(*area)?;
                    let mut used = 0;
                    for a in addresses {
                        let x = self.cell(area.sheet, a)?;
                        if !matches!(x, Blank) && x != Text(String::new()) {
                            used += 1
                        }
                    }
                    count += area.size() - used
                }
                continue;
            }
            if name == AggregateFunction::Statistics(StatisticsFunction::CountBlank)
                && let Value::SignedRefs(areas) = &v
            {
                // Pinned Countblank accepts RefEval/ThreeDEval, not a RefListEval union.
                if areas.len() != 1 {
                    return Ok(err(VALUE_ERROR));
                }
                for area in areas {
                    let mut used = 0;
                    if let Some(physical) = area.physical() {
                        for address in self.sparse_addresses(physical)? {
                            let value = self.cell(area.sheet, address)?;
                            if !matches!(value, Blank) && value != Text(String::new()) {
                                used += 1;
                            }
                        }
                    }
                    count += area.size() - used;
                }
                continue;
            }
            if name == AggregateFunction::Statistics(StatisticsFunction::CountBlank) {
                return Ok(match self.scalar(at, v)? {
                    Error(error) => err(error),
                    _ => err(VALUE_ERROR),
                });
            }
            // Keep true sheet spans deferred until each ordered value is consumed.
            // A spreadsheet error on an earlier sheet must win over a later cell's
            // operational failure, as MultiOperandNumericFunction.collectValues does.
            let (mut items, selected_origin) = match v {
                Value::SheetSpan(areas) => {
                    let mut items = Vec::new();
                    for area in areas {
                        let addresses = self.sparse_addresses(area)?;
                        self.budget
                            .check_allocation(items.len().saturating_add(addresses.len()))?;
                        self.budget
                            .charge(WorkKind::Materialization, addresses.len() as u64)?;
                        items.extend(addresses.into_iter().map(|address| (area.sheet, address)));
                    }
                    (AggregateItems::Cells(items.into_iter()), None)
                }
                Value::SelectedScalar(value, origin) => {
                    self.budget.allocation(1)?;
                    (
                        AggregateItems::Values(vec![(value, false)].into_iter()),
                        Some(match origin {
                            host::OperandOrigin::Direct => poirot_coercionv2::Origin::Direct,
                            host::OperandOrigin::ReferencedCell => {
                                poirot_coercionv2::Origin::Reference
                            }
                            host::OperandOrigin::ArrayElement => poirot_coercionv2::Origin::Array,
                        }),
                    )
                }
                Value::Missing => {
                    self.budget.allocation(1)?;
                    // SAFETY: One fixed-size element cannot overflow vector capacity; its logical allocation was admitted above.
                    (
                        AggregateItems::Values(vec![(Blank, false)].into_iter()),
                        None,
                    )
                }
                Value::Scalar(value) => {
                    self.budget.allocation(1)?;
                    // SAFETY: One fixed-size element cannot overflow vector capacity; its logical allocation was admitted above.
                    (
                        AggregateItems::Values(vec![(value, false)].into_iter()),
                        None,
                    )
                }
                other => (
                    AggregateItems::Projected(
                        self.items_map(other, AggregateMember::project)?.into_iter(),
                    ),
                    None,
                ),
            };
            'members: while let Some((v, indirect)) = match &mut items {
                AggregateItems::Values(values) => values.next(),
                AggregateItems::Projected(values) => match values.next() {
                    Some(AggregateMember::Text { empty }) => {
                        // All projected members are indirect. Text is ignored by numeric
                        // and logical aggregates, but retains its presence for counts.
                        if name == AggregateFunction::Statistics(StatisticsFunction::CountA)
                            || (empty
                                && name
                                    == AggregateFunction::Statistics(
                                        StatisticsFunction::CountBlank,
                                    ))
                        {
                            count = count.saturating_add(1);
                        }
                        continue 'members;
                    }
                    Some(value) => Some((
                        match value {
                            AggregateMember::Blank => Blank,
                            AggregateMember::Number(value) => Number(value),
                            AggregateMember::Boolean(value) => Boolean(value),
                            AggregateMember::Error(value) => Error(value),
                            AggregateMember::Text { .. } => continue 'members,
                        },
                        true,
                    )),
                    None => None,
                },
                AggregateItems::Cells(cells) => cells
                    .next()
                    .map(|(sheet, address)| self.cell(sheet, address).map(|value| (value, true)))
                    .transpose()?,
            } {
                let origin = selected_origin.unwrap_or(if indirect {
                    poirot_coercionv2::Origin::Reference
                } else {
                    poirot_coercionv2::Origin::Direct
                });
                let v = if matches!(name, AggregateFunction::Statistics(_)) && !e.is_missing() {
                    poirot_coercionv2::collections::coercev2_aggregate_argument(v, origin)
                } else {
                    v
                };
                if name == AggregateFunction::Statistics(StatisticsFunction::CountA) {
                    count += poirot_functions_statistics::count_member(
                        poirot_functions_statistics::BuiltinAggregator::CountA,
                        false,
                        v != Blank,
                        false,
                    );
                    continue;
                }
                if name == AggregateFunction::Statistics(StatisticsFunction::CountBlank) {
                    count += poirot_functions_statistics::count_member(
                        poirot_functions_statistics::BuiltinAggregator::CountBlank,
                        false,
                        v != Blank,
                        v == Text(String::new()),
                    );
                    continue;
                }
                if name == AggregateFunction::Statistics(StatisticsFunction::Count) {
                    if matches!(v, Number(_))
                        || (!indirect
                            && !matches!(v, Blank | Error(_))
                            && semantic(self.coercev2_scalar_number(&v))?.is_ok())
                    {
                        count += 1
                    }
                    continue;
                }
                if matches!(
                    name,
                    AggregateFunction::And | AggregateFunction::Or | AggregateFunction::Xor
                ) {
                    let collection = if name == AggregateFunction::Xor {
                        origin != poirot_coercionv2::Origin::Direct
                    } else {
                        indirect
                    };
                    if collection && matches!(v, Text(_) | CellValue::TextUtf16(_) | Blank) {
                        continue;
                    }
                    match core_projection::coercev2_operand_boolean(&v) {
                        Ok(b) => {
                            truth.push(b);
                        }
                        Err(e) => return Ok(err(e)),
                    }
                    continue;
                }
                if indirect && matches!(v, Text(_) | CellValue::TextUtf16(_) | Boolean(_) | Blank) {
                    continue;
                }
                if matches!(v, Blank) {
                    continue;
                }
                let raw_sum = name == AggregateFunction::Statistics(StatisticsFunction::Sum);
                if !raw_sum && matches!(v, Number(n) if !n.is_finite()) {
                    return Err(EvaluationFailure::UnsupportedCapability {
                        capability: CapabilityId::FunctionAdapter,
                        source: Some(EvaluationTarget { sheet:s, address:at }),
                        detail: "pinned special-number aggregate algorithm is not implemented for this handler".into(),
                    });
                }
                let number = if raw_sum {
                    self.coercev2_operator_number(&v)
                } else {
                    self.coercev2_scalar_number(&v)
                };
                match semantic(number)? {
                    Ok(n) => {
                        if let Some(reduction) = &mut reduction {
                            reduction.push(n);
                        } else {
                            self.budget.check_allocation(nums.len().saturating_add(1))?;
                            self.budget.charge(WorkKind::Materialization, 1)?;
                            nums.try_reserve(1)
                                .map_err(|_| EvaluationFailure::ResourceLimit {
                                    capability: CapabilityId::FunctionAdapter,
                                    source: self.consumer,
                                    detail: "numeric aggregate allocation failed".into(),
                                })?;
                            // SAFETY: The successful reservation provides room for this one push.
                            nums.push(n);
                        }
                    }
                    Err(e) => return Ok(err(e)),
                }
            }
        }
        if matches!(
            name,
            AggregateFunction::Statistics(StatisticsFunction::Count)
                | AggregateFunction::Statistics(StatisticsFunction::CountA)
                | AggregateFunction::Statistics(StatisticsFunction::CountBlank)
        ) {
            return Ok(Number(
                if name == AggregateFunction::Statistics(StatisticsFunction::CountBlank) {
                    count as i32 as f64
                } else {
                    count as f64
                },
            )
            .into());
        }
        if matches!(
            name,
            AggregateFunction::And | AggregateFunction::Or | AggregateFunction::Xor
        ) {
            return Ok(match truth.finish() {
                Some(value) => Boolean(value).into(),
                None => err(VALUE_ERROR),
            });
        }
        if let Some(reduction) = reduction {
            return Ok(match reduction.finish() {
                Some(value) => numeric_outcome::coerce_checked_numeric_result(value).into(),
                None => err(Div0),
            });
        }
        let result = match name {
            AggregateFunction::Xor => return Err(EvaluationFailure::StalePlan),
            AggregateFunction::Statistics(
                StatisticsFunction::AveDev
                | StatisticsFunction::AverageA
                | StatisticsFunction::DevSq
                | StatisticsFunction::GeoMean
                | StatisticsFunction::MaxA
                | StatisticsFunction::MinA
                | StatisticsFunction::StdevA
                | StatisticsFunction::StdevPA
                | StatisticsFunction::VarA
                | StatisticsFunction::VarPA
                | StatisticsFunction::Mode
                | StatisticsFunction::Large
                | StatisticsFunction::Small
                | StatisticsFunction::QuartileInc
                | StatisticsFunction::RankEq
                | StatisticsFunction::Percentile
                | StatisticsFunction::Rank
                | StatisticsFunction::PercentRank
                | StatisticsFunction::PercentRankInc
                | StatisticsFunction::PercentRankExc
                | StatisticsFunction::Correl
                | StatisticsFunction::Pearson
                | StatisticsFunction::Covar
                | StatisticsFunction::CovarianceP
                | StatisticsFunction::CovarianceS
                | StatisticsFunction::Forecast
                | StatisticsFunction::ForecastLinear
                | StatisticsFunction::Intercept
                | StatisticsFunction::Slope
                | StatisticsFunction::SumX2MY2
                | StatisticsFunction::SumX2PY2
                | StatisticsFunction::SumXMY2
                | StatisticsFunction::NormDist
                | StatisticsFunction::NormDistLegacy
                | StatisticsFunction::NormInv
                | StatisticsFunction::NormInvLegacy
                | StatisticsFunction::NormSInv
                | StatisticsFunction::NormSInvLegacy
                | StatisticsFunction::NormSDist
                | StatisticsFunction::NormSDistLegacy
                | StatisticsFunction::Standardize
                | StatisticsFunction::Poisson
                | StatisticsFunction::PoissonDist
                | StatisticsFunction::TDistLegacy
                | StatisticsFunction::TDist
                | StatisticsFunction::TDistRt
                | StatisticsFunction::TDist2T,
            ) => return Ok(err(VALUE_ERROR)),
            AggregateFunction::Statistics(
                StatisticsFunction::Sum
                | StatisticsFunction::SumSq
                | StatisticsFunction::Average
                | StatisticsFunction::Min
                | StatisticsFunction::Max
                | StatisticsFunction::Product,
            ) => return Ok(err(VALUE_ERROR)),
            AggregateFunction::Statistics(StatisticsFunction::Median) => {
                match poirot_functions_statistics::median(&mut nums) {
                    Ok(value) => value,
                    Err(error) => return Ok(err(error)),
                }
            }
            AggregateFunction::Statistics(
                StatisticsFunction::Stdev
                | StatisticsFunction::StdevP
                | StatisticsFunction::Var
                | StatisticsFunction::VarP,
            ) => {
                use poirot_functions_statistics::{
                    Aggregate as A, SamplePolicy, aggregate_with_policy,
                };
                let operation = match name {
                    AggregateFunction::Statistics(StatisticsFunction::Stdev) => A::StdevSample,
                    AggregateFunction::Statistics(StatisticsFunction::StdevP) => A::StdevPopulation,
                    AggregateFunction::Statistics(StatisticsFunction::Var) => A::VarianceSample,
                    _ => A::VariancePopulation,
                };
                // Operand admission and execution budgets remain owned by this adapter;
                // numeric reduction is shared with the independent statistics kernels.
                match aggregate_with_policy(operation, &nums, SamplePolicy::ExcelDocumentation) {
                    Ok(value) => value,
                    Err(error) => return Ok(err(error)),
                }
            }
            AggregateFunction::And
            | AggregateFunction::Or
            | AggregateFunction::Statistics(
                StatisticsFunction::Count
                | StatisticsFunction::CountA
                | StatisticsFunction::CountBlank,
                // SAFETY: The median branch rejects len == 0 before selecting middle elements; the count/logical branches return before the final numeric match.
            ) => unreachable!("handled before numeric aggregation"),
        };
        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
        Ok(numeric_outcome::coerce_checked_numeric_result(result).into())
    }
}
