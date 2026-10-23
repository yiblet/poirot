//! Explicit per-family helper membership. The macro expands to exhaustive typed matches;
//! `local` variants stay in evaluator-owned execution paths.
use super::{
    CriteriaFunction, DatabaseFunction, EngineeringFunction, FinancialFunction, LogicalFunction,
    MathFunction, MatrixFunction, ReferenceFunction, SearchFunction, StatisticsFunction,
    TextFunction,
};

macro_rules! helper_map {
    (total $family:ident => $target:ty; helpers: [$($helper:ident),+ $(,)?];) => {
        impl $family {
            /// Map this builtin identity to its family helper.
            pub(crate) fn helper(self) -> $target {
                match self {
                    $(Self::$helper => <$target>::$helper,)+
                }
            }
        }
    };
    (partial $family:ident => $target:ty;
        helpers: [$($helper:ident),+ $(,)?];
        local: [$($local:ident),+ $(,)?];) => {
        impl $family {
            /// Return the family helper, or `None` for evaluator-owned handlers.
            pub(crate) fn helper(self) -> Option<$target> {
                match self {
                    $(Self::$helper => Some(<$target>::$helper),)+
                    $(Self::$local)|+ => None,
                }
            }
        }
    };
    (wrapped $family:ident => $outer:ty, $inner:ty, $wrap:ident;
        helpers: [$($helper:ident),+ $(,)?];
        local: [$($local:ident),+ $(,)?];) => {
        impl $family {
            /// Wrap a family helper, or return `None` for evaluator-owned handlers.
            pub(crate) fn helper(self) -> Option<$outer> {
                match self {
                    $(Self::$helper => Some(<$outer>::$wrap(<$inner>::$helper)),)+
                    $(Self::$local)|+ => None,
                }
            }
        }
    };
}

helper_map! {
    partial FinancialFunction => crate::adapters::financial::policy::FinancialFunction;
    helpers: [
        Fv, Pv, Pmt, Nper, Ipmt,
        Ppmt, Rate, Npv, Irr, Mirr,
        Dollar, DollarDe, DollarFr, Fixed,
    ];
    local: [Euroconvert, Cumipmt, Cumprinc, Db, Ddb, Effect, Fvschedule, Ispmt, Nominal, Pduration, Rri, Sln, Syd, Vdb, Xirr, Xnpv, Accrint, Accrintm, Coupdaybs, Coupdays, Coupdaysnc, Coupncd, Coupnum, Couppcd, Disc, Duration, Intrate, Mduration, Price, Pricedisc, Pricemat, Received, Tbilleq, Tbillprice, Tbillyield, Yield, Yielddisc, Yieldmat, Amordegrc, Amorlinc, Oddfprice, Oddfyield, Oddlprice, Oddlyield];
}

helper_map! {
    partial EngineeringFunction => crate::adapters::engineering::policy::EngineeringFunction;
    helpers: [
        Besselj, Bin2dec, Complex, Dec2bin, Dec2hex,
        Delta, Hex2dec, Imaginary, Imreal, Oct2dec,
    ];
    local: [Besseli,Besselk,Bessely,Erf,ErfPrecise,Erfc,ErfcPrecise,Gestep,Imabs, Imargument, Imconjugate, Imcos, Imcosh, Imcot, Imcsc, Imcsch, Imdiv, Imexp, Imln, Imlog10, Imlog2, Impower, Improduct, Imsec, Imsech, Imsin, Imsinh, Imsqrt, Imsub, Imsum, Imtan, Bin2hex, Bin2oct, Bitand, Bitlshift, Bitor, Bitrshift, Bitxor, Convert, Dec2oct, Hex2bin, Hex2oct, Oct2bin, Oct2hex];
}

helper_map! {
    partial MatrixFunction => crate::adapters::matrix::policy::MatrixFunction;
    helpers: [
        Mdeterm, Minverse, Mmult, Transpose, Frequency,
        Sumproduct, Trend,
    ];
    local: [Munit, Randarray, Trimrange, Groupby, Percentof, Filter, Sort, Sortby, Unique, Sequence, Hstack, Vstack, Take, Drop, Choosecols, Chooserows, Expand, Tocol, Torow, Wrapcols, Wraprows];
}

helper_map! {
    total CriteriaFunction => crate::criteria_adapter::Function;
    helpers: [
        CountIf, SumIf, AverageIf, CountIfs, SumIfs,
        AverageIfs, MaxIfs, MinIfs, Subtotal,
    ];
}

helper_map! {
    total DatabaseFunction => poirot_functions_database::DatabaseFunction;
    helpers: [
        Average, Count, CountA, Get, Max,
        Min, Product, Stdev, StdevP, Sum,
        Var, VarP,
    ];
}

helper_map! {
    partial MathFunction => crate::adapters::math::policy::MathFunction;
    helpers: [
        Rand, Randbetween, Acosh, Asinh, Atan2,
        Atanh, CeilingMath, CeilingPrecise, Combin, Combina, Cosh,
        Even, Fact, Factdouble, FloorMath, FloorPrecise,
        Gcd, Lcm, Mround, Odd, Quotient,
        Sinh, Sqrtpi, Tanh,
    ];
    local: [
        Acot, Acoth, Cot, Coth, Csc, Csch, Sec, Sech, IsoCeiling, Multinomial,
        Base, Decimal, SeriesSum, Pi, Abs, Sign, Sqrt, Exp,
        Ln, Log10, Log, Power, Mod,
        Int, Trunc, Round, RoundUp, RoundDown,
        Ceiling, Floor, Sin, Cos, Tan,
        Asin, Acos, Atan, Degrees, Radians,
    ];
}

helper_map! {
    partial StatisticsFunction => crate::adapters::statistics::policy::StatisticsFunction;
    helpers: [
        AveDev, AverageA, DevSq, GeoMean, MaxA,
        MinA, StdevA, StdevPA, VarA, VarPA,
        Mode, Large, Small, Percentile, QuartileInc,
        RankEq, Rank, PercentRank, PercentRankInc, PercentRankExc,
        Correl, Pearson, Covar, CovarianceP, CovarianceS,
        Forecast, ForecastLinear, Intercept, Slope, SumX2MY2,
        SumX2PY2, SumXMY2, NormDist, NormDistLegacy, NormInv,
        NormInvLegacy, NormSInv, NormSInvLegacy, NormSDist, NormSDistLegacy,
        Standardize, Poisson, PoissonDist, TDistLegacy, TDist,
        TDistRt, TDist2T,
    ];
    local: [
        Sum, Average, Count, CountA, CountBlank,
        Min, Max, Product, SumSq, Stdev,
        StdevP, Var, VarP, Median,
    ];
}

helper_map! {
    partial TextFunction => crate::adapters::text::policy::Function;
    helpers: [
        NumberValue, Proper, Roman, Arabic, TextJoin,
    ];
    local: [
        Text, Len, Left, Right, Mid,
        Lower, Upper, Trim, Clean, Concat,
        Concatenate, Exact, Find, Search, Replace,
        Substitute, Rept, Value, Char, Code,
        UniChar, Unicode, TextBefore, TextAfter, TextSplit, ArrayToText, ValueToText, RegexTest, RegexExtract, RegexReplace,
    ];
}

helper_map! {
    partial LogicalFunction => crate::adapters::logical::policy::LogicalFunction;
    helpers: [
        IsRef, ErrorType, Ifs, IsEven, IsOdd,
        Switch,
    ];
    local: [
        If, IfError, IfNa, Choose, And,
        Or, Xor, Not, True, False,
    ];
}

helper_map! {
    wrapped SearchFunction => poirot_functions_lookup::Function, poirot_functions_lookup::Search, Search;
    helpers: [
        Lookup, XLookup, XMatch,
    ];
    local: [
        Match, VLookup, HLookup,
    ];
}

helper_map! {
    wrapped ReferenceFunction => poirot_functions_lookup::Function, poirot_functions_lookup::ReferenceFunction, Reference;
    helpers: [
        Address, Areas, Hyperlink, Indirect, Offset,
        Sheet, Single,
    ];
    local: [
        Rows, Columns, Row, Column, Index, AnchorArray,
    ];
}
