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
}
/// Pending statistical aggregate operand retaining reference traversal state.
enum AggregateItem {
    Value(CellValue, bool, poirot_coercionv2::Origin),
    Cell(usize, CellAddress),
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
        let mut nums = Vec::new();
        let mut count = 0u64;
        let mut truth = Vec::new();
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
            let items = match v {
                Value::SheetSpan(areas) => {
                    let mut items = Vec::new();
                    for area in areas {
                        let addresses = self.sparse_addresses(area)?;
                        self.budget
                            .check_allocation(items.len().saturating_add(addresses.len()))?;
                        self.budget
                            .charge(WorkKind::Materialization, addresses.len() as u64)?;
                        items.extend(
                            addresses
                                .into_iter()
                                .map(|address| AggregateItem::Cell(area.sheet, address)),
                        );
                    }
                    items
                }
                Value::SelectedScalar(value, origin) => {
                    self.budget.allocation(1)?;
                    vec![AggregateItem::Value(
                        value,
                        false,
                        match origin {
                            host::OperandOrigin::Direct => poirot_coercionv2::Origin::Direct,
                            host::OperandOrigin::ReferencedCell => {
                                poirot_coercionv2::Origin::Reference
                            }
                            host::OperandOrigin::ArrayElement => poirot_coercionv2::Origin::Array,
                        },
                    )]
                }
                other => self
                    .items(other)?
                    .into_iter()
                    .map(|(value, referenced)| {
                        AggregateItem::Value(
                            value,
                            referenced,
                            if referenced {
                                poirot_coercionv2::Origin::Reference
                            } else {
                                poirot_coercionv2::Origin::Direct
                            },
                        )
                    })
                    .collect(),
            };
            for item in items {
                let (v, indirect, origin) = match item {
                    AggregateItem::Value(value, referenced, origin) => (value, referenced, origin),
                    AggregateItem::Cell(sheet, address) => (
                        self.cell(sheet, address)?,
                        true,
                        poirot_coercionv2::Origin::Reference,
                    ),
                };
                let v = if matches!(name, AggregateFunction::Statistics(_)) && !e.is_missing() {
                    poirot_coercionv2::collections::coercev2_aggregate_argument(v, origin)
                } else {
                    v
                };
                if name == AggregateFunction::Statistics(StatisticsFunction::CountA) {
                    count += u64::from(v != Blank);
                    continue;
                }
                if name == AggregateFunction::Statistics(StatisticsFunction::CountBlank) {
                    count += u64::from(v == Blank || v == Text(String::new()));
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
                if matches!(name, AggregateFunction::And | AggregateFunction::Or) {
                    if indirect && matches!(v, Text(_) | CellValue::TextUtf16(_) | Blank) {
                        continue;
                    }
                    match core_projection::coercev2_operand_boolean(&v, self.numeric_policy) {
                        Ok(b) => {
                            self.budget.check_allocation(truth.len() + 1)?;
                            self.budget.charge(WorkKind::Materialization, 1)?;
                            truth.push(b)
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
                let raw_sum = self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209
                    && name == AggregateFunction::Statistics(StatisticsFunction::Sum);
                if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209
                    && !raw_sum
                    && matches!(v, Number(n) if !n.is_finite())
                {
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
                        self.budget.check_allocation(nums.len() + 1)?;
                        self.budget.charge(WorkKind::Materialization, 1)?;
                        nums.push(n)
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
        if matches!(name, AggregateFunction::And | AggregateFunction::Or) {
            return Ok(if truth.is_empty() {
                err(VALUE_ERROR)
            } else {
                Boolean(if name == AggregateFunction::And {
                    truth.iter().all(|b| *b)
                } else {
                    truth.iter().any(|b| *b)
                })
                .into()
            });
        }
        let len = nums.len();
        let sum: f64 = nums.iter().sum();
        let result = match name {
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
            AggregateFunction::Statistics(StatisticsFunction::Sum) => {
                if self.numeric_policy == NumericOutcomePolicy::PinnedPoi7321209 {
                    nums.iter().fold(0.0, |acc, n| acc + *n)
                } else {
                    sum
                }
            }
            AggregateFunction::Statistics(StatisticsFunction::SumSq) => {
                nums.iter().map(|x| x * x).sum()
            }
            AggregateFunction::Statistics(StatisticsFunction::Average) => {
                if len == 0 {
                    return Ok(err(Div0));
                }
                sum / len as f64
            }
            AggregateFunction::Statistics(StatisticsFunction::Min) => {
                nums.iter().copied().reduce(f64::min).unwrap_or(0.)
            }
            AggregateFunction::Statistics(StatisticsFunction::Max) => {
                nums.iter().copied().reduce(f64::max).unwrap_or(0.)
            }
            AggregateFunction::Statistics(StatisticsFunction::Product) => {
                if len == 0 {
                    0.
                } else {
                    nums.iter().product()
                }
            }
            AggregateFunction::Statistics(StatisticsFunction::Median) => {
                if len == 0 {
                    return Ok(err(Num));
                }
                nums.sort_by(f64::total_cmp);
                if len % 2 == 0 {
                    // SAFETY: The median branch rejects len == 0 before selecting middle elements; the count/logical branches return before the final numeric match.
                    nums[len / 2 - 1] / 2. + nums[len / 2] / 2.
                } else {
                    // SAFETY: The median branch rejects len == 0 before selecting middle elements; the count/logical branches return before the final numeric match.
                    nums[len / 2]
                }
            }
            AggregateFunction::Statistics(
                StatisticsFunction::Stdev
                | StatisticsFunction::StdevP
                | StatisticsFunction::Var
                | StatisticsFunction::VarP,
            ) => {
                let population = matches!(
                    name,
                    AggregateFunction::Statistics(StatisticsFunction::VarP)
                        | AggregateFunction::Statistics(StatisticsFunction::StdevP)
                );
                if len < if population { 1 } else { 2 } {
                    return Ok(err(Div0));
                } // Welford avoids cancellation for large offsets.
                let (mut mean, mut m2) = (0., 0.);
                for (i, x) in nums.iter().enumerate() {
                    let d = x - mean;
                    mean += d / (i + 1) as f64;
                    m2 += d * (x - mean)
                }
                let v = m2 / (len - usize::from(!population)) as f64;
                if matches!(
                    name,
                    AggregateFunction::Statistics(
                        StatisticsFunction::Stdev | StatisticsFunction::StdevP
                    )
                ) {
                    v.sqrt()
                } else {
                    v
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
        Ok(self
            .numeric_policy
            .coerce_checked_numeric_result(result)
            .into())
    }
}
