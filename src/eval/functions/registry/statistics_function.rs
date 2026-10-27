/// Aggregation, ranking, regression, and distribution handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StatisticsFunction {
    /// Documented CHISQ.TEST sample-statistics handler.
    ChisqTest,
    /// Documented F.TEST sample-statistics handler.
    FTest,
    /// Documented FISHER sample-statistics handler.
    Fisher,
    /// Documented FISHERINV sample-statistics handler.
    FisherInv,
    /// Documented HARMEAN sample-statistics handler.
    HarMean,
    /// Documented KURT sample-statistics handler.
    Kurt,
    /// Documented MODE.MULT sample-statistics handler.
    ModeMult,
    /// Documented MODE.SNGL sample-statistics handler.
    ModeSngl,
    /// Documented PERCENTILE.EXC sample-statistics handler.
    PercentileExc,
    /// Documented PERMUT sample-statistics handler.
    Permut,
    /// Documented PERMUTATIONA sample-statistics handler.
    PermutationA,
    /// Documented PROB sample-statistics handler.
    Prob,
    /// Documented QUARTILE.EXC sample-statistics handler.
    QuartileExc,
    /// Documented RANK.AVG sample-statistics handler.
    RankAvg,
    /// Documented RSQ sample-statistics handler.
    RSq,
    /// Documented SKEW sample-statistics handler.
    Skew,
    /// Documented SKEW.P sample-statistics handler.
    SkewP,
    /// Documented STEYX sample-statistics handler.
    Steyx,
    /// Documented T.TEST sample-statistics handler.
    TTest,
    /// Documented TRIMMEAN sample-statistics handler.
    TrimMean,
    /// Documented Z.TEST sample-statistics handler.
    ZTest,

    /// Legacy BETADIST worksheet contract.
    BetaDistLegacy,
    /// Legacy BETAINV worksheet contract.
    BetaInvLegacy,
    /// Legacy BINOMDIST worksheet contract.
    BinomDistLegacy,
    /// Legacy CHIDIST worksheet contract.
    ChisqDistRtLegacy,
    /// Legacy CHIINV worksheet contract.
    ChisqInvRtLegacy,
    /// Legacy CHITEST worksheet contract.
    ChisqTestLegacy,
    /// Legacy CONFIDENCE worksheet contract.
    ConfidenceNormLegacy,
    /// Legacy CRITBINOM worksheet contract.
    BinomInvLegacy,
    /// Legacy EXPONDIST worksheet contract.
    ExponDistLegacy,
    /// Legacy FDIST worksheet contract.
    FDistRtLegacy,
    /// Legacy FINV worksheet contract.
    FInvRtLegacy,
    /// Legacy FTEST worksheet contract.
    FTestLegacy,
    /// Legacy GAMMADIST worksheet contract.
    GammaDistLegacy,
    /// Legacy GAMMAINV worksheet contract.
    GammaInvLegacy,
    /// Legacy HYPGEOMDIST worksheet contract.
    HypgeomDistLegacy,
    /// Legacy LOGINV worksheet contract.
    LognormInvLegacy,
    /// Legacy LOGNORMDIST worksheet contract.
    LognormDistLegacy,
    /// Legacy NEGBINOMDIST worksheet contract.
    NegbinomDistLegacy,
    /// Legacy QUARTILE worksheet contract.
    QuartileIncLegacy,
    /// Legacy TINV worksheet contract.
    TInv2TLegacy,
    /// Legacy TTEST worksheet contract.
    TTestLegacy,
    /// Legacy WEIBULL worksheet contract.
    WeibullDistLegacy,
    /// Legacy ZTEST worksheet contract.
    ZTestLegacy,

    /// Documented BETA.INV inverse or confidence handler.
    BetaInv,
    /// Documented BINOM.INV inverse or confidence handler.
    BinomInv,
    /// Documented CHISQ.INV inverse or confidence handler.
    ChisqInv,
    /// Documented CHISQ.INV.RT inverse or confidence handler.
    ChisqInvRt,
    /// Documented CONFIDENCE.NORM inverse or confidence handler.
    ConfidenceNorm,
    /// Documented CONFIDENCE.T inverse or confidence handler.
    ConfidenceT,
    /// Documented F.INV inverse or confidence handler.
    FInv,
    /// Documented F.INV.RT inverse or confidence handler.
    FInvRt,
    /// Documented GAMMA.INV inverse or confidence handler.
    GammaInv,
    /// Documented LOGNORM.INV inverse or confidence handler.
    LognormInv,
    /// Documented T.INV inverse or confidence handler.
    TInv,
    /// Documented T.INV.2T inverse or confidence handler.
    TInv2T,

    /// Documented BETA.DIST distribution handler.
    BetaDist,
    /// Documented BINOM.DIST distribution handler.
    BinomDist,
    /// Documented BINOM.DIST.RANGE distribution handler.
    BinomDistRange,
    /// Documented CHISQ.DIST distribution handler.
    ChisqDist,
    /// Documented CHISQ.DIST.RT distribution handler.
    ChisqDistRt,
    /// Documented EXPON.DIST distribution handler.
    ExponDist,
    /// Documented F.DIST distribution handler.
    FDist,
    /// Documented F.DIST.RT distribution handler.
    FDistRt,
    /// Documented GAMMA distribution handler.
    Gamma,
    /// Documented GAMMA.DIST distribution handler.
    GammaDist,
    /// Documented GAMMALN distribution handler.
    Gammaln,
    /// Documented GAMMALN.PRECISE distribution handler.
    GammalnPrecise,
    /// Documented GAUSS distribution handler.
    Gauss,
    /// Documented HYPGEOM.DIST distribution handler.
    HypgeomDist,
    /// Documented LOGNORM.DIST distribution handler.
    LognormDist,
    /// Documented NEGBINOM.DIST distribution handler.
    NegbinomDist,
    /// Documented PHI distribution handler.
    Phi,
    /// Documented WEIBULL.DIST distribution handler.
    WeibullDist,

    /// AVEDEV handler.
    AveDev,
    /// AVERAGEA handler.
    AverageA,
    /// DEVSQ handler.
    DevSq,
    /// GEOMEAN handler.
    GeoMean,
    /// MAXA handler.
    MaxA,
    /// MINA handler.
    MinA,
    /// STDEVA handler.
    StdevA,
    /// STDEVPA handler.
    StdevPA,
    /// VARA handler.
    VarA,
    /// VARPA handler.
    VarPA,
    /// MODE handler.
    Mode,
    /// LARGE handler.
    Large,
    /// SMALL handler.
    Small,
    /// PERCENTILE handler.
    Percentile,
    /// QUARTILE.INC handler.
    QuartileInc,
    /// RANK.EQ handler.
    RankEq,
    /// Legacy RANK handler.
    Rank,
    /// Legacy PERCENTRANK handler.
    PercentRank,
    /// PERCENTRANK.INC handler.
    PercentRankInc,
    /// PERCENTRANK.EXC handler.
    PercentRankExc,
    /// CORREL handler.
    Correl,
    /// PEARSON handler.
    Pearson,
    /// Legacy COVAR handler.
    Covar,
    /// COVARIANCE.P handler.
    CovarianceP,
    /// COVARIANCE.S handler.
    CovarianceS,
    /// Legacy FORECAST handler.
    Forecast,
    /// FORECAST.ETS additive smoothing forecast.
    ForecastEts,
    /// FORECAST.ETS.CONFINT future-observation interval.
    ForecastEtsConfint,
    /// FORECAST.ETS.SEASONALITY automatic period.
    ForecastEtsSeasonality,
    /// FORECAST.ETS.STAT fitted gain or diagnostic.
    ForecastEtsStat,
    /// FORECAST.LINEAR handler.
    ForecastLinear,
    /// INTERCEPT handler.
    Intercept,
    /// SLOPE handler.
    Slope,
    /// SUMX2MY2 handler.
    SumX2MY2,
    /// SUMX2PY2 handler.
    SumX2PY2,
    /// SUMXMY2 handler.
    SumXMY2,
    /// NORM.DIST handler.
    NormDist,
    /// Legacy NORMDIST handler.
    NormDistLegacy,
    /// NORM.INV handler.
    NormInv,
    /// Legacy NORMINV handler.
    NormInvLegacy,
    /// NORM.S.INV handler.
    NormSInv,
    /// Legacy NORMSINV handler.
    NormSInvLegacy,
    /// NORM.S.DIST handler.
    NormSDist,
    /// Legacy NORMSDIST handler.
    NormSDistLegacy,
    /// STANDARDIZE handler.
    Standardize,
    /// Legacy POISSON handler.
    Poisson,
    /// POISSON.DIST handler.
    PoissonDist,
    /// Legacy TDIST handler.
    TDistLegacy,
    /// T.DIST handler.
    TDist,
    /// T.DIST.RT handler.
    TDistRt,
    /// T.DIST.2T handler.
    TDist2T,

    /// SUM handler.
    Sum,
    /// AVERAGE handler.
    Average,
    /// COUNT handler.
    Count,
    /// COUNTA handler.
    CountA,
    /// COUNTBLANK handler.
    CountBlank,
    /// MIN handler.
    Min,
    /// MAX handler.
    Max,
    /// PRODUCT handler.
    Product,
    /// SUMSQ handler.
    SumSq,
    /// Sample standard deviation handler.
    Stdev,
    /// Population standard deviation handler.
    StdevP,
    /// Sample variance handler.
    Var,
    /// Population variance handler.
    VarP,
    /// MEDIAN handler.
    Median,
}
