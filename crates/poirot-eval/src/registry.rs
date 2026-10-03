//! Executable builtin registry. Aliases point to a typed family handler.
//! Arity is descriptive: family executors preserve function-specific error ordering.
/// Analysis ToolPak names supported by the pinned implementation.
pub use crate::atp::ATP_SUPPORTED as PINNED_ATP_FUNCTIONS;
/// Date and time builtin handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DateFunction {
    /// DATE handler.
    Date,
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
}
/// Aggregation, ranking, regression, and distribution handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StatisticsFunction {
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
}
/// Text construction, parsing, formatting, and matching handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TextFunction {
    /// NUMBERVALUE handler.
    NumberValue,
    /// PROPER handler.
    Proper,
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
}
/// Radix, complex number, and special engineering function handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EngineeringFunction {
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
}
/// Matrix algebra and array aggregation handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MatrixFunction {
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
    pub(crate) const SLOT_COUNT: usize = 13 * Self::FAMILY_SLOTS;
    /// Map a typed handler to its non-serialized execution counter slot.
    pub(crate) fn index(self) -> usize {
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

/// Catalog used for Excel-compatible name resolution.
pub(crate) use catalog::{EXCEL_DESCRIPTORS, resolve_excel_function};
/// Advertised builtin catalog and spelling resolver.
pub use catalog::{FUNCTION_DESCRIPTORS, SUPPORTED_FUNCTIONS, resolve_function};
/// Registration contracts for argument count, evaluation strategy, and dependencies.
pub use policy::{Arity, DependencyPolicy, EvaluationStrategy};

#[cfg(test)]
mod tests;
