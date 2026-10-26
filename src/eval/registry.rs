//! Executable builtin registry. Aliases point to a typed family handler.
//! Arity is descriptive: family executors preserve function-specific error ordering.
/// Analysis ToolPak names supported by the pinned implementation.
pub use crate::eval::atp::ATP_SUPPORTED as PINNED_ATP_FUNCTIONS;
/// Date and time builtin handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DateFunction {
    /// DATE handler.
    Date,
    /// DATEDIF handler.
    DateDif,
    /// DATEVALUE handler.
    DateValue,
    /// DAY handler.
    Day,
    /// DAYS handler.
    Days,
    /// DAYS360 handler.
    Days360,
    /// EDATE handler.
    EDate,
    /// EOMONTH handler.
    EOMonth,
    /// HOUR handler.
    Hour,
    /// MINUTE handler.
    Minute,
    /// MONTH handler.
    Month,
    /// NETWORKDAYS handler.
    NetworkDays,
    /// NOW handler.
    Now,
    /// SECOND handler.
    Second,
    /// TIME handler.
    Time,
    /// TIMEVALUE handler.
    TimeValue,
    /// TODAY handler.
    Today,
    /// WEEKDAY handler.
    WeekDay,
    /// WEEKNUM handler.
    WeekNum,
    /// WORKDAY handler.
    WorkDay,
    /// WORKDAY.INTL handler.
    WorkDayIntl,
    /// YEAR handler.
    Year,
    /// YEARFRAC handler.
    YearFrac,
    /// ISO week number.
    IsoWeekNum,
    /// Custom weekend business-day count.
    NetworkDaysIntl,
}
/// Logical tests and conditional selection handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LogicalFunction {
    /// ERROR.TYPE handler.
    ErrorType,
    /// IFS handler.
    Ifs,
    /// ISEVEN handler.
    IsEven,
    /// ISODD handler.
    IsOdd,
    /// SWITCH handler.
    Switch,

    /// ISREF handler.
    IsRef,
    /// IF handler.
    If,
    /// IFERROR handler.
    IfError,
    /// IFNA handler.
    IfNa,
    /// CHOOSE handler.
    Choose,
    /// AND handler.
    And,
    /// OR handler.
    Or,
    /// NOT handler.
    Not,
    /// TRUE handler.
    True,
    /// FALSE handler.
    False,
    /// Odd logical parity.
    Xor,
}
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
/// Reference construction, selection, and shape handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ReferenceFunction {
    /// ADDRESS handler.
    Address,
    /// AREAS handler.
    Areas,
    /// HYPERLINK handler.
    Hyperlink,
    /// INDIRECT handler.
    Indirect,
    /// OFFSET handler.
    Offset,
    /// SHEET handler.
    Sheet,
    /// SINGLE implicit intersection handler.
    Single,

    /// ROWS handler.
    Rows,
    /// COLUMNS handler.
    Columns,
    /// ROW handler.
    Row,
    /// COLUMN handler.
    Column,
    /// INDEX handler.
    Index,
    /// Stored spelling of the postfix spill-range reference operator.
    AnchorArray,
}
/// Lookup and position matching handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SearchFunction {
    /// LOOKUP handler.
    Lookup,
    /// XLOOKUP handler.
    XLookup,
    /// XMATCH handler.
    XMatch,

    /// MATCH handler.
    Match,
    /// VLOOKUP handler.
    VLookup,
    /// HLOOKUP handler.
    HLookup,
}
/// Value inspection, conversion, and error construction handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum InformationFunction {
    /// ISERROR handler.
    IsError,
    /// ISERR handler.
    IsErr,
    /// ISNA handler.
    IsNa,
    /// ISNUMBER handler.
    IsNumber,
    /// ISTEXT handler.
    IsText,
    /// ISBLANK handler.
    IsBlank,
    /// ISLOGICAL handler.
    IsLogical,
    /// ISNONTEXT handler.
    IsNonText,
    /// TYPE handler.
    Type,
    /// N handler.
    N,
    /// T handler.
    T,
    /// NA handler.
    Na,
}
/// Arithmetic, rounding, trigonometry, and random number handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MathFunction {
    /// BASE positional text handler.
    Base,
    /// DECIMAL positional numeric handler.
    Decimal,
    /// RAND handler.
    Rand,
    /// RANDBETWEEN handler.
    Randbetween,
    /// ACOSH handler.
    Acosh,
    /// ASINH handler.
    Asinh,
    /// ATAN2 handler.
    Atan2,
    /// ATANH handler.
    Atanh,
    /// CEILING.MATH handler.
    CeilingMath,
    /// CEILING.PRECISE handler.
    CeilingPrecise,
    /// COMBIN handler.
    Combin,
    /// Combinations with replacement under the documented domain.
    Combina,
    /// COSH handler.
    Cosh,
    /// EVEN handler.
    Even,
    /// FACT handler.
    Fact,
    /// FACTDOUBLE handler.
    Factdouble,
    /// FLOOR.MATH handler.
    FloorMath,
    /// FLOOR.PRECISE handler.
    FloorPrecise,
    /// GCD handler.
    Gcd,
    /// LCM handler.
    Lcm,
    /// MROUND handler.
    Mround,
    /// ODD handler.
    Odd,
    /// QUOTIENT handler.
    Quotient,
    /// SINH handler.
    Sinh,
    /// SQRTPI handler.
    Sqrtpi,
    /// TANH handler.
    Tanh,

    /// PI handler.
    Pi,
    /// ABS handler.
    Abs,
    /// SIGN handler.
    Sign,
    /// SQRT handler.
    Sqrt,
    /// EXP handler.
    Exp,
    /// LN handler.
    Ln,
    /// LOG10 handler.
    Log10,
    /// LOG handler.
    Log,
    /// POWER handler.
    Power,
    /// MOD handler.
    Mod,
    /// INT handler.
    Int,
    /// TRUNC handler.
    Trunc,
    /// ROUND handler.
    Round,
    /// ROUNDUP handler.
    RoundUp,
    /// ROUNDDOWN handler.
    RoundDown,
    /// CEILING handler.
    Ceiling,
    /// FLOOR handler.
    Floor,
    /// SIN handler.
    Sin,
    /// COS handler.
    Cos,
    /// TAN handler.
    Tan,
    /// ASIN handler.
    Asin,
    /// ACOS handler.
    Acos,
    /// ATAN handler.
    Atan,
    /// DEGREES handler.
    Degrees,
    /// RADIANS handler.
    Radians,
    /// Direct power-series summation.
    SeriesSum,
    /// Acot mathematical utility.
    Acot,
    /// Acoth mathematical utility.
    Acoth,
    /// Cot mathematical utility.
    Cot,
    /// Coth mathematical utility.
    Coth,
    /// Csc mathematical utility.
    Csc,
    /// Csch mathematical utility.
    Csch,
    /// Sec mathematical utility.
    Sec,
    /// Sech mathematical utility.
    Sech,
    /// IsoCeiling mathematical utility.
    IsoCeiling,
    /// Multinomial mathematical utility.
    Multinomial,
}
/// Text construction, parsing, formatting, and matching handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TextFunction {
    /// NUMBERVALUE handler.
    NumberValue,
    /// PROPER handler.
    Proper,
    /// ARABIC handler.
    Arabic,
    /// ROMAN handler.
    Roman,
    /// TEXTJOIN handler.
    TextJoin,

    /// TEXT handler.
    Text,
    /// LEN handler.
    Len,
    /// LEFT handler.
    Left,
    /// RIGHT handler.
    Right,
    /// MID handler.
    Mid,
    /// LOWER handler.
    Lower,
    /// UPPER handler.
    Upper,
    /// TRIM handler.
    Trim,
    /// CLEAN handler.
    Clean,
    /// CONCATENATE handler.
    Concatenate,
    /// CONCAT handler.
    Concat,
    /// EXACT handler.
    Exact,
    /// FIND handler.
    Find,
    /// SEARCH handler.
    Search,
    /// REPLACE handler.
    Replace,
    /// SUBSTITUTE handler.
    Substitute,
    /// REPT handler.
    Rept,
    /// VALUE handler.
    Value,
    /// CHAR handler.
    Char,
    /// CODE handler.
    Code,
    /// UNICHAR handler.
    UniChar,
    /// UNICODE handler.
    Unicode,
    /// TEXTBEFORE delimiter extraction.
    TextBefore,
    /// TEXTAFTER delimiter extraction.
    TextAfter,
    /// TEXTSPLIT rectangular delimiter splitting.
    TextSplit,
    /// ARRAYTOTEXT aggregate serialization.
    ArrayToText,
    /// VALUETOTEXT elementwise serialization.
    ValueToText,
    /// REGEXTEST Unicode PCRE2 matching.
    RegexTest,
    /// REGEXEXTRACT text or capture extraction.
    RegexExtract,
    /// REGEXREPLACE occurrence-aware replacement.
    RegexReplace,
}
/// Cash flow, loan, and monetary formatting handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FinancialFunction {
    /// FV handler.
    Fv,
    /// PV handler.
    Pv,
    /// PMT handler.
    Pmt,
    /// NPER handler.
    Nper,
    /// IPMT handler.
    Ipmt,
    /// PPMT handler.
    Ppmt,
    /// RATE handler.
    Rate,
    /// NPV handler.
    Npv,
    /// IRR handler.
    Irr,
    /// MIRR handler.
    Mirr,
    /// DOLLAR handler.
    Dollar,
    /// DOLLARDE handler.
    DollarDe,
    /// DOLLARFR handler.
    DollarFr,
    /// FIXED handler.
    Fixed,
    /// Historical fixed-rate euro conversion.
    Euroconvert,
    /// Documented CUMIPMT calculation.
    Cumipmt,
    /// Documented CUMPRINC calculation.
    Cumprinc,
    /// Documented DB calculation.
    Db,
    /// Documented DDB calculation.
    Ddb,
    /// Documented EFFECT calculation.
    Effect,
    /// Documented FVSCHEDULE calculation.
    Fvschedule,
    /// Documented ISPMT calculation.
    Ispmt,
    /// Documented NOMINAL calculation.
    Nominal,
    /// Documented PDURATION calculation.
    Pduration,
    /// Documented RRI calculation.
    Rri,
    /// Documented SLN calculation.
    Sln,
    /// Documented SYD calculation.
    Syd,
    /// Documented VDB calculation.
    Vdb,
    /// Documented XIRR calculation.
    Xirr,
    /// Documented XNPV calculation.
    Xnpv,
    /// Documented ACCRINT security calculation.
    Accrint,
    /// Documented ACCRINTM security calculation.
    Accrintm,
    /// Documented COUPDAYBS security calculation.
    Coupdaybs,
    /// Documented COUPDAYS security calculation.
    Coupdays,
    /// Documented COUPDAYSNC security calculation.
    Coupdaysnc,
    /// Documented COUPNCD security calculation.
    Coupncd,
    /// Documented COUPNUM security calculation.
    Coupnum,
    /// Documented COUPPCD security calculation.
    Couppcd,
    /// Documented DISC security calculation.
    Disc,
    /// Documented DURATION security calculation.
    Duration,
    /// Documented INTRATE security calculation.
    Intrate,
    /// Documented MDURATION security calculation.
    Mduration,
    /// Documented PRICE security calculation.
    Price,
    /// Documented PRICEDISC security calculation.
    Pricedisc,
    /// Documented PRICEMAT security calculation.
    Pricemat,
    /// Documented RECEIVED security calculation.
    Received,
    /// Documented TBILLEQ security calculation.
    Tbilleq,
    /// Documented TBILLPRICE security calculation.
    Tbillprice,
    /// Documented TBILLYIELD security calculation.
    Tbillyield,
    /// Documented YIELD security calculation.
    Yield,
    /// Documented YIELDDISC security calculation.
    Yielddisc,
    /// Documented YIELDMAT security calculation.
    Yieldmat,
    /// Documented AMORDEGRC irregular financial calculation.
    Amordegrc,
    /// Documented AMORLINC irregular financial calculation.
    Amorlinc,
    /// Documented ODDFPRICE irregular financial calculation.
    Oddfprice,
    /// Documented ODDFYIELD irregular financial calculation.
    Oddfyield,
    /// Documented ODDLPRICE irregular financial calculation.
    Oddlprice,
    /// Documented ODDLYIELD irregular financial calculation.
    Oddlyield,
}
/// Radix, complex number, and special engineering function handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EngineeringFunction {
    /// Documented BESSELI numeric operation.
    Besseli,
    /// Documented BESSELK numeric operation.
    Besselk,
    /// Documented BESSELY numeric operation.
    Bessely,
    /// Documented ERF numeric operation.
    Erf,
    /// Documented ERF.PRECISE numeric operation.
    ErfPrecise,
    /// Documented ERFC numeric operation.
    Erfc,
    /// Documented ERFC.PRECISE numeric operation.
    ErfcPrecise,
    /// Documented GESTEP numeric operation.
    Gestep,
    /// BESSELJ handler.
    Besselj,
    /// BIN2DEC handler.
    Bin2dec,
    /// COMPLEX handler.
    Complex,
    /// DEC2BIN handler.
    Dec2bin,
    /// DEC2HEX handler.
    Dec2hex,
    /// DELTA handler.
    Delta,
    /// HEX2DEC handler.
    Hex2dec,
    /// IMAGINARY handler.
    Imaginary,
    /// IMREAL handler.
    Imreal,
    /// OCT2DEC handler.
    Oct2dec,
    /// Documented IMABS complex operation.
    Imabs,
    /// Documented IMARGUMENT complex operation.
    Imargument,
    /// Documented IMCONJUGATE complex operation.
    Imconjugate,
    /// Documented IMCOS complex operation.
    Imcos,
    /// Documented IMCOSH complex operation.
    Imcosh,
    /// Documented IMCOT complex operation.
    Imcot,
    /// Documented IMCSC complex operation.
    Imcsc,
    /// Documented IMCSCH complex operation.
    Imcsch,
    /// Documented IMDIV complex operation.
    Imdiv,
    /// Documented IMEXP complex operation.
    Imexp,
    /// Documented IMLN complex operation.
    Imln,
    /// Documented IMLOG10 complex operation.
    Imlog10,
    /// Documented IMLOG2 complex operation.
    Imlog2,
    /// Documented IMPOWER complex operation.
    Impower,
    /// Documented IMPRODUCT complex operation.
    Improduct,
    /// Documented IMSEC complex operation.
    Imsec,
    /// Documented IMSECH complex operation.
    Imsech,
    /// Documented IMSIN complex operation.
    Imsin,
    /// Documented IMSINH complex operation.
    Imsinh,
    /// Documented IMSQRT complex operation.
    Imsqrt,
    /// Documented IMSUB complex operation.
    Imsub,
    /// Documented IMSUM complex operation.
    Imsum,
    /// Documented IMTAN complex operation.
    Imtan,
    /// Documented BIN2HEX engineering operation.
    Bin2hex,
    /// Documented BIN2OCT engineering operation.
    Bin2oct,
    /// Documented BITAND engineering operation.
    Bitand,
    /// Documented BITLSHIFT engineering operation.
    Bitlshift,
    /// Documented BITOR engineering operation.
    Bitor,
    /// Documented BITRSHIFT engineering operation.
    Bitrshift,
    /// Documented BITXOR engineering operation.
    Bitxor,
    /// Documented CONVERT engineering operation.
    Convert,
    /// Documented DEC2OCT engineering operation.
    Dec2oct,
    /// Documented HEX2BIN engineering operation.
    Hex2bin,
    /// Documented HEX2OCT engineering operation.
    Hex2oct,
    /// Documented OCT2BIN engineering operation.
    Oct2bin,
    /// Documented OCT2HEX engineering operation.
    Oct2hex,
}
/// Matrix algebra and array aggregation handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MatrixFunction {
    /// Select rows or columns by Boolean conditions.
    Filter,
    /// Sort records by an internal key.
    Sort,
    /// Sort records by corresponding external keys.
    Sortby,
    /// Retain distinct or exactly-once records.
    Unique,
    /// Modern Excel sequential rectangular array generation.
    Sequence,
    /// Concatenate arrays horizontally.
    Hstack,
    /// Concatenate arrays vertically.
    Vstack,
    /// Retain signed row and column counts.
    Take,
    /// Remove signed row and column counts.
    Drop,
    /// Select ordered column indices.
    Choosecols,
    /// Select ordered row indices.
    Chooserows,
    /// Pad an array to larger dimensions.
    Expand,
    /// Flatten into a column.
    Tocol,
    /// Flatten into a row.
    Torow,
    /// Wrap a vector by columns.
    Wrapcols,
    /// Wrap a vector by rows.
    Wraprows,
    /// MDETERM handler.
    Mdeterm,
    /// MINVERSE handler.
    Minverse,
    /// MMULT handler.
    Mmult,
    /// TRANSPOSE handler.
    Transpose,
    /// FREQUENCY handler.
    Frequency,
    /// SUMPRODUCT handler.
    Sumproduct,
    /// TREND handler.
    Trend,
    /// Group rows with typed built-in reductions and optional hierarchy totals.
    Groupby,
    /// Ratio of two independently summed sets.
    Percentof,
    /// Seeded rectangular random numeric generation.
    Randarray,
    /// Remove blank outside borders while preserving the input channel.
    Trimrange,
    /// Numeric identity matrix.
    Munit,
    /// Exponential regression predictions.
    Growth,
    /// Linear least-squares coefficients and optional diagnostics.
    Linest,
    /// Exponential least-squares coefficients and optional diagnostics.
    Logest,
}
/// Criteria-based aggregation and subtotal handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CriteriaFunction {
    /// COUNTIF handler.
    CountIf,
    /// SUMIF handler.
    SumIf,
    /// AVERAGEIF handler.
    AverageIf,
    /// COUNTIFS handler.
    CountIfs,
    /// SUMIFS handler.
    SumIfs,
    /// AVERAGEIFS handler.
    AverageIfs,
    /// MAXIFS handler.
    MaxIfs,
    /// MINIFS handler.
    MinIfs,
    /// SUBTOTAL handler.
    Subtotal,
}
/// Database functions aggregating records selected by a criteria table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DatabaseFunction {
    /// DAVERAGE handler.
    Average,
    /// DCOUNT handler.
    Count,
    /// DCOUNTA handler.
    CountA,
    /// DGET handler.
    Get,
    /// DMAX handler.
    Max,
    /// DMIN handler.
    Min,
    /// DPRODUCT handler.
    Product,
    /// DSTDEV handler.
    Stdev,
    /// DSTDEVP handler.
    StdevP,
    /// DSUM handler.
    Sum,
    /// DVAR handler.
    Var,
    /// DVARP handler.
    VarP,
}
/// Typed builtin handler identity. Multiple advertised spellings can select one handler;
/// this identity is independent of the spelling retained for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FunctionId {
    /// Cash flow and monetary operations.
    Financial(FinancialFunction),
    /// Engineering and complex number operations.
    Engineering(EngineeringFunction),
    /// Matrix and array operations.
    Matrix(MatrixFunction),
    /// Conditional aggregation operations.
    Criteria(CriteriaFunction),
    /// Criteria-table database operations.
    Database(DatabaseFunction),
    /// Date and time operations.
    Date(DateFunction),
    /// Logical tests and conditional execution.
    Logical(LogicalFunction),
    /// Aggregation and statistical operations.
    Statistics(StatisticsFunction),
    /// Reference construction and selection.
    Reference(ReferenceFunction),
    /// Lookup and position matching.
    Search(SearchFunction),
    /// Value inspection and type conversion.
    Information(InformationFunction),
    /// Mathematical and random number operations.
    Math(MathFunction),
    /// Text manipulation and formatting.
    Text(TextFunction),
}
impl FunctionId {
    // Internal execution slots, not serialized identities. Separate family bands
    // avoid renumbering other families when one family gains a handler.
    const FAMILY_SLOTS: usize = 256;
    /// Capacity of the internal per-handler execution counter array.
    pub(in crate::eval) const SLOT_COUNT: usize = 13 * Self::FAMILY_SLOTS;
    /// Map a typed handler to its non-serialized execution counter slot.
    pub(in crate::eval) fn index(self) -> usize {
        let (family, member) = match self {
            Self::Financial(member) => (0, member as usize),
            Self::Engineering(member) => (1, member as usize),
            Self::Matrix(member) => (2, member as usize),
            Self::Criteria(member) => (3, member as usize),
            Self::Database(member) => (4, member as usize),
            Self::Date(member) => (5, member as usize),
            Self::Logical(member) => (6, member as usize),
            Self::Statistics(member) => (7, member as usize),
            Self::Reference(member) => (8, member as usize),
            Self::Search(member) => (9, member as usize),
            Self::Information(member) => (10, member as usize),
            Self::Math(member) => (11, member as usize),
            Self::Text(member) => (12, member as usize),
        };
        if member >= Self::FAMILY_SLOTS {
            return Self::SLOT_COUNT;
        }
        family * Self::FAMILY_SLOTS + member
    }
}
/// Advertised builtin name, typed handler, arity, evaluation strategy, and dependency flags.
/// This describes registration; it does not promise every input or execution mode is supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionDescriptor {
    /// Advertised formula spelling, including aliases.
    pub name: &'static str,
    /// Typed execution handler selected for this spelling.
    pub handler: FunctionId,
    /// Descriptive argument count contract.
    pub arity: Arity,
    /// Eager, lazy, or reference-aware argument evaluation strategy.
    pub strategy: EvaluationStrategy,
    /// Declared context, volatility, and reference dependencies.
    pub dependencies: DependencyPolicy,
    /// Possible result form and element-kind transformation.
    pub result: ResultRule,
}
/// Legacy lightweight metadata identity. Executable providers use
/// `extensions::ExtensionBinding` and the full explicit registry contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UdfIdentity {
    /// Caller-assigned function registration identity.
    pub registration: u64,
    /// Caller-assigned implementation version.
    pub version: u64,
}
mod adapters;
mod catalog;
mod policy;
mod result;
// Mandatory possible-result transfer rules for registered builtins.
pub use result::ResultRule;

/// Catalog used for Excel-compatible name resolution.
pub(in crate::eval) use catalog::{EXCEL_DESCRIPTORS, resolve_excel_function};
// Advertised builtin catalog and spelling resolver.
pub use catalog::{FUNCTION_DESCRIPTORS, SUPPORTED_FUNCTIONS, resolve_function};
// Registration contracts for argument count, evaluation strategy, and dependencies.
pub use policy::{Arity, DependencyPolicy, EvaluationStrategy};

#[cfg(test)]
mod tests;

/// Builtins available under the selected semantics policy, including aliases.
/// External/ATP registrations are not advertised by this builtin-only catalog.
pub fn descriptors_for_policy(
    policy: crate::eval::BuiltinSemanticsPolicy,
) -> impl Iterator<Item = &'static FunctionDescriptor> {
    FUNCTION_DESCRIPTORS.iter().chain(
        EXCEL_DESCRIPTORS
            .iter()
            .filter(move |_| policy == crate::eval::BuiltinSemanticsPolicy::ExcelDocumentation),
    )
}
