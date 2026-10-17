use super::{
    Arity, CriteriaFunction, DatabaseFunction, DateFunction, DependencyPolicy, EngineeringFunction,
    EvaluationStrategy, FinancialFunction, FunctionDescriptor, FunctionId, InformationFunction,
    LogicalFunction, MathFunction, MatrixFunction, ReferenceFunction, SearchFunction,
    StatisticsFunction, TextFunction,
};

// Both the public catalog and typed lookup come from these ordered declarations.
// Repeated handlers are aliases: the first declaration supplies canonical metadata.
macro_rules! registry {
    (builtin { $( $bn:literal => $bf:ident($bk:path), $bmin:expr, $bmax:expr, $bs:ident, $bd:ident; )* }
     excel { $( $en:literal => $ef:ident($ek:path), $emin:expr, $emax:expr, $es:ident, $ed:ident; )* }) => {
        registry! { $($bn => $bf($bk), $bmin, $bmax, $bs, $bd;)* }
        registry! { pub(crate) EXCEL_DESCRIPTORS, excel_descriptor;
            $($en => $ef($ek), $emin, $emax, $es, $ed;)* }
        impl FunctionId {
            /// Return canonical registration metadata with exhaustive typed dispatch.
            pub(crate) fn descriptor(self) -> &'static FunctionDescriptor {
                #[allow(unreachable_patterns)]
                match self {
                    $(FunctionId::$bf($bk) => &FunctionDescriptor {
                        name: $bn, handler: FunctionId::$bf($bk),
                        arity: Arity { minimum: $bmin, maximum: $bmax },
                        strategy: EvaluationStrategy::$bs, dependencies: DependencyPolicy::$bd,
                    },)*
                    $(FunctionId::$ef($ek) => &FunctionDescriptor {
                        name: $en, handler: FunctionId::$ef($ek),
                        arity: Arity { minimum: $emin, maximum: $emax },
                        strategy: EvaluationStrategy::$es, dependencies: DependencyPolicy::$ed,
                    },)*
                }
            }
        }
    };
    ($visibility:vis $descriptors:ident, $lookup:ident; $( $name:literal => $family:ident($kind:path), $min:expr, $max:expr, $strategy:ident, $dependency:ident; )*) => {
        /// Ordered registrations; the first alias supplies canonical handler metadata.
        $visibility const $descriptors: &[FunctionDescriptor] = &[$(FunctionDescriptor {
            name: $name, handler: FunctionId::$family($kind),
            arity: Arity { minimum: $min, maximum: $max },
            strategy: EvaluationStrategy::$strategy, dependencies: DependencyPolicy::$dependency,
        }),*];


    };
    ($( $name:literal => $family:ident($kind:path), $min:expr, $max:expr, $strategy:ident, $dependency:ident; )*) => {
        /// Advertised builtin spellings in registration order.
        pub const SUPPORTED_FUNCTIONS: &[&str] = &[$($name),*];
        registry! { pub FUNCTION_DESCRIPTORS, builtin_descriptor;
            $($name => $family($kind), $min, $max, $strategy, $dependency;)*
        }
    };
}
registry! {
    builtin {
    "RAND" => Math(MathFunction::Rand), 0, 0, FamilyControlled, RANDOM_READS;
    "RANDBETWEEN" => Math(MathFunction::Randbetween), 2, 2, FamilyControlled, RANDOM_READS;
    "ERROR.TYPE" => Logical(LogicalFunction::ErrorType), 1, 1, FamilyControlled, REFERENCE_READS;
    "IFS" => Logical(LogicalFunction::Ifs), 0, usize::MAX, Lazy, REFERENCE_READS;
    "ISEVEN" => Logical(LogicalFunction::IsEven), 1, 1, FamilyControlled, REFERENCE_READS;
    "ISODD" => Logical(LogicalFunction::IsOdd), 1, 1, FamilyControlled, REFERENCE_READS;
    "SWITCH" => Logical(LogicalFunction::Switch), 3, usize::MAX, Lazy, REFERENCE_READS;
    "LOOKUP" => Search(SearchFunction::Lookup), 2, 3, FamilyControlled, REFERENCE_READS;
    "XLOOKUP" => Search(SearchFunction::XLookup), 3, usize::MAX, FamilyControlled, REFERENCE_READS;
    "XMATCH" => Search(SearchFunction::XMatch), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "ADDRESS" => Reference(ReferenceFunction::Address), 2, 5, FamilyControlled, REFERENCE_READS;
    "AREAS" => Reference(ReferenceFunction::Areas), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "HYPERLINK" => Reference(ReferenceFunction::Hyperlink), 1, 2, FamilyControlled, REFERENCE_READS;
    "INDIRECT" => Reference(ReferenceFunction::Indirect), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "OFFSET" => Reference(ReferenceFunction::Offset), 1, 5, FamilyControlled, REFERENCE_READS;
    "SHEET" => Reference(ReferenceFunction::Sheet), 0, usize::MAX, FamilyControlled, REFERENCE_READS;
    "SINGLE" => Reference(ReferenceFunction::Single), 0, usize::MAX, FamilyControlled, REFERENCE_READS;
    "ACOSH" => Math(MathFunction::Acosh), 1, 1, FamilyControlled, REFERENCE_READS;
    "ASINH" => Math(MathFunction::Asinh), 1, 1, FamilyControlled, REFERENCE_READS;
    "ATAN2" => Math(MathFunction::Atan2), 2, 2, FamilyControlled, REFERENCE_READS;
    "ATANH" => Math(MathFunction::Atanh), 1, 1, FamilyControlled, REFERENCE_READS;
    "BASE" => Math(MathFunction::Base), 2, 3, FamilyControlled, REFERENCE_READS;
    "DECIMAL" => Math(MathFunction::Decimal), 2, 2, FamilyControlled, REFERENCE_READS;
    "CEILING.MATH" => Math(MathFunction::CeilingMath), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "CEILING.PRECISE" => Math(MathFunction::CeilingPrecise), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "SERIESSUM" => Math(MathFunction::SeriesSum), 4, 4, FamilyControlled, REFERENCE_READS;
    "COMBINA" => Math(MathFunction::Combina), 2, 2, FamilyControlled, REFERENCE_READS;
    "COMBIN" => Math(MathFunction::Combin), 2, 2, FamilyControlled, REFERENCE_READS;
    "COSH" => Math(MathFunction::Cosh), 1, 1, FamilyControlled, REFERENCE_READS;
    "EVEN" => Math(MathFunction::Even), 1, 1, FamilyControlled, REFERENCE_READS;
    "FACT" => Math(MathFunction::Fact), 1, 1, FamilyControlled, REFERENCE_READS;
    "FACTDOUBLE" => Math(MathFunction::Factdouble), 1, 1, FamilyControlled, REFERENCE_READS;
    "FLOOR.MATH" => Math(MathFunction::FloorMath), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "FLOOR.PRECISE" => Math(MathFunction::FloorPrecise), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "GCD" => Math(MathFunction::Gcd), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "LCM" => Math(MathFunction::Lcm), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "MROUND" => Math(MathFunction::Mround), 2, 2, FamilyControlled, REFERENCE_READS;
    "ODD" => Math(MathFunction::Odd), 1, 1, FamilyControlled, REFERENCE_READS;
    "QUOTIENT" => Math(MathFunction::Quotient), 2, 2, FamilyControlled, REFERENCE_READS;
    "SINH" => Math(MathFunction::Sinh), 1, 1, FamilyControlled, REFERENCE_READS;
    "SQRTPI" => Math(MathFunction::Sqrtpi), 1, 1, FamilyControlled, REFERENCE_READS;
    "TANH" => Math(MathFunction::Tanh), 1, 1, FamilyControlled, REFERENCE_READS;
    "AVEDEV" => Statistics(StatisticsFunction::AveDev), 1, 30, FamilyControlled, REFERENCE_READS;
    "AVERAGEA" => Statistics(StatisticsFunction::AverageA), 1, 30, FamilyControlled, REFERENCE_READS;
    "DEVSQ" => Statistics(StatisticsFunction::DevSq), 1, 30, FamilyControlled, REFERENCE_READS;
    "GEOMEAN" => Statistics(StatisticsFunction::GeoMean), 1, 30, FamilyControlled, REFERENCE_READS;
    "MAXA" => Statistics(StatisticsFunction::MaxA), 1, 30, FamilyControlled, REFERENCE_READS;
    "MINA" => Statistics(StatisticsFunction::MinA), 1, 30, FamilyControlled, REFERENCE_READS;
    "STDEVA" => Statistics(StatisticsFunction::StdevA), 1, 30, FamilyControlled, REFERENCE_READS;
    "STDEVPA" => Statistics(StatisticsFunction::StdevPA), 1, 30, FamilyControlled, REFERENCE_READS;
    "VARA" => Statistics(StatisticsFunction::VarA), 1, 30, FamilyControlled, REFERENCE_READS;
    "VARPA" => Statistics(StatisticsFunction::VarPA), 1, 30, FamilyControlled, REFERENCE_READS;
    "MODE" => Statistics(StatisticsFunction::Mode), 1, 30, FamilyControlled, REFERENCE_READS;
    "LARGE" => Statistics(StatisticsFunction::Large), 2, 2, FamilyControlled, REFERENCE_READS;
    "SMALL" => Statistics(StatisticsFunction::Small), 2, 2, FamilyControlled, REFERENCE_READS;
    "PERCENTILE" => Statistics(StatisticsFunction::Percentile), 2, 2, FamilyControlled, REFERENCE_READS;
    "RANK" => Statistics(StatisticsFunction::Rank), 2, 3, FamilyControlled, REFERENCE_READS;
    "PERCENTRANK" => Statistics(StatisticsFunction::PercentRank), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "PERCENTRANK.INC" => Statistics(StatisticsFunction::PercentRankInc), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "PERCENTRANK.EXC" => Statistics(StatisticsFunction::PercentRankExc), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "CORREL" => Statistics(StatisticsFunction::Correl), 2, 2, FamilyControlled, REFERENCE_READS;
    "PEARSON" => Statistics(StatisticsFunction::Pearson), 2, 2, FamilyControlled, REFERENCE_READS;
    "COVAR" => Statistics(StatisticsFunction::Covar), 2, 2, FamilyControlled, REFERENCE_READS;
    "COVARIANCE.P" => Statistics(StatisticsFunction::CovarianceP), 2, 2, FamilyControlled, REFERENCE_READS;
    "COVARIANCE.S" => Statistics(StatisticsFunction::CovarianceS), 2, 2, FamilyControlled, REFERENCE_READS;
    "FORECAST" => Statistics(StatisticsFunction::Forecast), 3, 3, FamilyControlled, REFERENCE_READS;
    "FORECAST.LINEAR" => Statistics(StatisticsFunction::ForecastLinear), 3, 3, FamilyControlled, REFERENCE_READS;
    "INTERCEPT" => Statistics(StatisticsFunction::Intercept), 2, 2, FamilyControlled, REFERENCE_READS;
    "SLOPE" => Statistics(StatisticsFunction::Slope), 2, 2, FamilyControlled, REFERENCE_READS;
    "SUMX2MY2" => Statistics(StatisticsFunction::SumX2MY2), 2, 2, FamilyControlled, REFERENCE_READS;
    "SUMX2PY2" => Statistics(StatisticsFunction::SumX2PY2), 2, 2, FamilyControlled, REFERENCE_READS;
    "SUMXMY2" => Statistics(StatisticsFunction::SumXMY2), 2, 2, FamilyControlled, REFERENCE_READS;
    "NORM.DIST" => Statistics(StatisticsFunction::NormDist), 4, 4, FamilyControlled, REFERENCE_READS;
    "NORMDIST" => Statistics(StatisticsFunction::NormDistLegacy), 4, 4, FamilyControlled, REFERENCE_READS;
    "NORM.INV" => Statistics(StatisticsFunction::NormInv), 3, 3, FamilyControlled, REFERENCE_READS;
    "NORMINV" => Statistics(StatisticsFunction::NormInvLegacy), 3, 3, FamilyControlled, REFERENCE_READS;
    "NORM.S.INV" => Statistics(StatisticsFunction::NormSInv), 1, 1, FamilyControlled, REFERENCE_READS;
    "NORMSINV" => Statistics(StatisticsFunction::NormSInvLegacy), 1, 1, FamilyControlled, REFERENCE_READS;
    "NORM.S.DIST" => Statistics(StatisticsFunction::NormSDist), 1, 1, FamilyControlled, REFERENCE_READS;
    "NORMSDIST" => Statistics(StatisticsFunction::NormSDistLegacy), 1, 1, FamilyControlled, REFERENCE_READS;
    "STANDARDIZE" => Statistics(StatisticsFunction::Standardize), 3, 3, FamilyControlled, REFERENCE_READS;
    "POISSON" => Statistics(StatisticsFunction::Poisson), 3, 3, FamilyControlled, REFERENCE_READS;
    "POISSON.DIST" => Statistics(StatisticsFunction::PoissonDist), 3, 3, FamilyControlled, REFERENCE_READS;
    "TDIST" => Statistics(StatisticsFunction::TDistLegacy), 3, 3, FamilyControlled, REFERENCE_READS;
    "T.DIST" => Statistics(StatisticsFunction::TDist), 3, 3, FamilyControlled, REFERENCE_READS;
    "T.DIST.RT" => Statistics(StatisticsFunction::TDistRt), 2, 2, FamilyControlled, REFERENCE_READS;
    "T.DIST.2T" => Statistics(StatisticsFunction::TDist2T), 2, 2, FamilyControlled, REFERENCE_READS;
    "NUMBERVALUE" => Text(TextFunction::NumberValue), 0, usize::MAX, FamilyControlled, REFERENCE_READS;
    "PROPER" => Text(TextFunction::Proper), 1, 1, FamilyControlled, REFERENCE_READS;
    "ROMAN" => Text(TextFunction::Roman), 1, 2, FamilyControlled, REFERENCE_READS;
    "TEXTJOIN" => Text(TextFunction::TextJoin), 3, 254, FamilyControlled, REFERENCE_READS;
    "FV" => Financial(FinancialFunction::Fv), 3, 5, FamilyControlled, REFERENCE_READS;
    "PV" => Financial(FinancialFunction::Pv), 3, 5, FamilyControlled, REFERENCE_READS;
    "PMT" => Financial(FinancialFunction::Pmt), 3, 5, FamilyControlled, REFERENCE_READS;
    "NPER" => Financial(FinancialFunction::Nper), 3, 5, FamilyControlled, REFERENCE_READS;
    "IPMT" => Financial(FinancialFunction::Ipmt), 4, 4, FamilyControlled, REFERENCE_READS;
    "PPMT" => Financial(FinancialFunction::Ppmt), 4, usize::MAX, FamilyControlled, REFERENCE_READS;
    "RATE" => Financial(FinancialFunction::Rate), 3, usize::MAX, FamilyControlled, REFERENCE_READS;
    "NPV" => Financial(FinancialFunction::Npv), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "IRR" => Financial(FinancialFunction::Irr), 1, 2, FamilyControlled, REFERENCE_READS;
    "MIRR" => Financial(FinancialFunction::Mirr), 0, 3, FamilyControlled, REFERENCE_READS;
    "DOLLAR" => Financial(FinancialFunction::Dollar), 1, 2, FamilyControlled, CURRENCY_FORMATTING;
    "DOLLARDE" => Financial(FinancialFunction::DollarDe), 2, 2, FamilyControlled, REFERENCE_READS;
    "DOLLARFR" => Financial(FinancialFunction::DollarFr), 2, 2, FamilyControlled, REFERENCE_READS;
    "FIXED" => Financial(FinancialFunction::Fixed), 1, 3, FamilyControlled, REFERENCE_READS;
    "BESSELJ" => Engineering(EngineeringFunction::Besselj), 2, 2, FamilyControlled, REFERENCE_READS;
    "BIN2DEC" => Engineering(EngineeringFunction::Bin2dec), 1, 1, FamilyControlled, REFERENCE_READS;
    "COMPLEX" => Engineering(EngineeringFunction::Complex), 2, 3, FamilyControlled, REFERENCE_READS;
    "DEC2BIN" => Engineering(EngineeringFunction::Dec2bin), 1, 2, FamilyControlled, REFERENCE_READS;
    "DEC2HEX" => Engineering(EngineeringFunction::Dec2hex), 1, 2, FamilyControlled, REFERENCE_READS;
    "DELTA" => Engineering(EngineeringFunction::Delta), 2, 2, FamilyControlled, REFERENCE_READS;
    "HEX2DEC" => Engineering(EngineeringFunction::Hex2dec), 1, 1, FamilyControlled, REFERENCE_READS;
    "IMAGINARY" => Engineering(EngineeringFunction::Imaginary), 1, 1, FamilyControlled, REFERENCE_READS;
    "IMREAL" => Engineering(EngineeringFunction::Imreal), 1, 1, FamilyControlled, REFERENCE_READS;
    "OCT2DEC" => Engineering(EngineeringFunction::Oct2dec), 1, 1, FamilyControlled, REFERENCE_READS;
    "MDETERM" => Matrix(MatrixFunction::Mdeterm), 1, 1, FamilyControlled, REFERENCE_READS;
    "MINVERSE" => Matrix(MatrixFunction::Minverse), 1, 1, FamilyControlled, REFERENCE_READS;
    "MMULT" => Matrix(MatrixFunction::Mmult), 2, 2, FamilyControlled, REFERENCE_READS;
    "TRANSPOSE" => Matrix(MatrixFunction::Transpose), 1, 1, FamilyControlled, REFERENCE_READS;
    "FREQUENCY" => Matrix(MatrixFunction::Frequency), 2, 2, FamilyControlled, REFERENCE_READS;
    "SUMPRODUCT" => Matrix(MatrixFunction::Sumproduct), 1, usize::MAX, FamilyControlled, REFERENCE_READS;
    "TREND" => Matrix(MatrixFunction::Trend), 1, 4, FamilyControlled, REFERENCE_READS;
    "COUNTIF" => Criteria(CriteriaFunction::CountIf), 2, 2, FamilyControlled, REFERENCE_READS;
    "SUMIF" => Criteria(CriteriaFunction::SumIf), 2, 3, FamilyControlled, REFERENCE_READS;
    "AVERAGEIF" => Criteria(CriteriaFunction::AverageIf), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "COUNTIFS" => Criteria(CriteriaFunction::CountIfs), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "SUMIFS" => Criteria(CriteriaFunction::SumIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS;
    "AVERAGEIFS" => Criteria(CriteriaFunction::AverageIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS;
    "MAXIFS" => Criteria(CriteriaFunction::MaxIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS;
    "MINIFS" => Criteria(CriteriaFunction::MinIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS;
    "SUBTOTAL" => Criteria(CriteriaFunction::Subtotal), 2, usize::MAX, FamilyControlled, REFERENCE_READS;
    "DAVERAGE" => Database(DatabaseFunction::Average), 3, 3, FamilyControlled, REFERENCE_READS;
    "DCOUNT" => Database(DatabaseFunction::Count), 3, 3, FamilyControlled, REFERENCE_READS;
    "DCOUNTA" => Database(DatabaseFunction::CountA), 3, 3, FamilyControlled, REFERENCE_READS;
    "DGET" => Database(DatabaseFunction::Get), 3, 3, FamilyControlled, REFERENCE_READS;
    "DMAX" => Database(DatabaseFunction::Max), 3, 3, FamilyControlled, REFERENCE_READS;
    "DMIN" => Database(DatabaseFunction::Min), 3, 3, FamilyControlled, REFERENCE_READS;
    "DPRODUCT" => Database(DatabaseFunction::Product), 3, 3, FamilyControlled, REFERENCE_READS;
    "DSTDEV" => Database(DatabaseFunction::Stdev), 3, 3, FamilyControlled, REFERENCE_READS;
    "DSTDEVP" => Database(DatabaseFunction::StdevP), 3, 3, FamilyControlled, REFERENCE_READS;
    "DSUM" => Database(DatabaseFunction::Sum), 3, 3, FamilyControlled, REFERENCE_READS;
    "DVAR" => Database(DatabaseFunction::Var), 3, 3, FamilyControlled, REFERENCE_READS;
    "DVARP" => Database(DatabaseFunction::VarP), 3, 3, FamilyControlled, REFERENCE_READS;
    "DATE" => Date(DateFunction::Date), 3, 3, FamilyControlled, DATE_FAMILY;
    "DATEDIF" => Date(DateFunction::DateDif), 3, 3, FamilyControlled, DATE_FAMILY;
    "DATEVALUE" => Date(DateFunction::DateValue), 1, 1, FamilyControlled, DATE_FAMILY;
    "DAY" => Date(DateFunction::Day), 1, 1, FamilyControlled, DATE_FAMILY;
    "DAYS" => Date(DateFunction::Days), 2, 2, FamilyControlled, DATE_FAMILY;
    "DAYS360" => Date(DateFunction::Days360), 2, 3, FamilyControlled, DATE_FAMILY;
    "EDATE" => Date(DateFunction::EDate), 2, 2, FamilyControlled, DATE_FAMILY;
    "EOMONTH" => Date(DateFunction::EOMonth), 2, 2, FamilyControlled, DATE_FAMILY;
    "HOUR" => Date(DateFunction::Hour), 1, 1, FamilyControlled, DATE_FAMILY;
    "MINUTE" => Date(DateFunction::Minute), 1, 1, FamilyControlled, DATE_FAMILY;
    "MONTH" => Date(DateFunction::Month), 1, 1, FamilyControlled, DATE_FAMILY;
    "NETWORKDAYS" => Date(DateFunction::NetworkDays), 2, 3, FamilyControlled, DATE_FAMILY;
    "NOW" => Date(DateFunction::Now), 0, 0, FamilyControlled, TIME_READS;
    "SECOND" => Date(DateFunction::Second), 1, 1, FamilyControlled, DATE_FAMILY;
    "TIME" => Date(DateFunction::Time), 3, 3, FamilyControlled, DATE_FAMILY;
    "TIMEVALUE" => Date(DateFunction::TimeValue), 1, 1, FamilyControlled, DATE_FAMILY;
    "TODAY" => Date(DateFunction::Today), 0, 0, FamilyControlled, TIME_READS;
    "WEEKDAY" => Date(DateFunction::WeekDay), 1, 2, FamilyControlled, DATE_FAMILY;
    "WEEKNUM" => Date(DateFunction::WeekNum), 1, 2, FamilyControlled, DATE_FAMILY;
    "WORKDAY" => Date(DateFunction::WorkDay), 2, 3, FamilyControlled, DATE_FAMILY;
    "WORKDAY.INTL" => Date(DateFunction::WorkDayIntl), 2, 4, FamilyControlled, DATE_FAMILY;
    "YEAR" => Date(DateFunction::Year), 1, 1, FamilyControlled, DATE_FAMILY;
    "YEARFRAC" => Date(DateFunction::YearFrac), 2, 3, FamilyControlled, DATE_FAMILY;
    "TEXT" => Text(TextFunction::Text), 2, 2, FamilyControlled, TEXT_FORMATTING;
    "IF" => Logical(LogicalFunction::If), 2, 3, Lazy, ARGUMENTS;
    "ISREF" => Logical(LogicalFunction::IsRef), 1, 1, FamilyControlled, REFERENCE_READS;
    "IFERROR" => Logical(LogicalFunction::IfError), 2, 2, Lazy, ARGUMENTS;
    "IFNA" => Logical(LogicalFunction::IfNa), 2, 2, Lazy, ARGUMENTS;
    "CHOOSE" => Logical(LogicalFunction::Choose), 2, usize::MAX, Lazy, ARGUMENTS;
    "SUM" => Statistics(StatisticsFunction::Sum), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "AVERAGE" => Statistics(StatisticsFunction::Average), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "COUNT" => Statistics(StatisticsFunction::Count), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "COUNTA" => Statistics(StatisticsFunction::CountA), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "COUNTBLANK" => Statistics(StatisticsFunction::CountBlank), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "MIN" => Statistics(StatisticsFunction::Min), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "MAX" => Statistics(StatisticsFunction::Max), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "PRODUCT" => Statistics(StatisticsFunction::Product), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "AND" => Logical(LogicalFunction::And), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "OR" => Logical(LogicalFunction::Or), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "NOT" => Logical(LogicalFunction::Not), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "SUMSQ" => Statistics(StatisticsFunction::SumSq), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "STDEV" => Statistics(StatisticsFunction::Stdev), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "STDEV.S" => Statistics(StatisticsFunction::Stdev), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "STDEV.P" => Statistics(StatisticsFunction::StdevP), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "STDEVP" => Statistics(StatisticsFunction::StdevP), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "VAR" => Statistics(StatisticsFunction::Var), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "VAR.S" => Statistics(StatisticsFunction::Var), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "VAR.P" => Statistics(StatisticsFunction::VarP), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "VARP" => Statistics(StatisticsFunction::VarP), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "MEDIAN" => Statistics(StatisticsFunction::Median), 0, usize::MAX, FamilyControlled, ARGUMENTS;
    "ROWS" => Reference(ReferenceFunction::Rows), 1, 1, FamilyControlled, REFERENCE_READS;
    "COLUMNS" => Reference(ReferenceFunction::Columns), 1, 1, FamilyControlled, REFERENCE_READS;
    "ROW" => Reference(ReferenceFunction::Row), 0, 1, FamilyControlled, POSITION_READS;
    "COLUMN" => Reference(ReferenceFunction::Column), 0, 1, FamilyControlled, POSITION_READS;
    "INDEX" => Reference(ReferenceFunction::Index), 2, 4, FamilyControlled, REFERENCE_READS;
    "MATCH" => Search(SearchFunction::Match), 2, 3, FamilyControlled, REFERENCE_READS;
    "VLOOKUP" => Search(SearchFunction::VLookup), 3, 4, FamilyControlled, REFERENCE_READS;
    "HLOOKUP" => Search(SearchFunction::HLookup), 3, 4, FamilyControlled, REFERENCE_READS;
    "ISERROR" => Information(InformationFunction::IsError), 1, 1, FamilyControlled, ARGUMENTS;
    "ISERR" => Information(InformationFunction::IsErr), 1, 1, FamilyControlled, ARGUMENTS;
    "ISNA" => Information(InformationFunction::IsNa), 1, 1, FamilyControlled, ARGUMENTS;
    "ISNUMBER" => Information(InformationFunction::IsNumber), 1, 1, FamilyControlled, ARGUMENTS;
    "ISTEXT" => Information(InformationFunction::IsText), 1, 1, FamilyControlled, ARGUMENTS;
    "ISBLANK" => Information(InformationFunction::IsBlank), 1, 1, FamilyControlled, ARGUMENTS;
    "ISLOGICAL" => Information(InformationFunction::IsLogical), 1, 1, FamilyControlled, ARGUMENTS;
    "ISNONTEXT" => Information(InformationFunction::IsNonText), 1, 1, FamilyControlled, ARGUMENTS;
    "TYPE" => Information(InformationFunction::Type), 1, 1, FamilyControlled, ARGUMENTS;
    "N" => Information(InformationFunction::N), 1, 1, FamilyControlled, ARGUMENTS;
    "T" => Information(InformationFunction::T), 1, 1, FamilyControlled, ARGUMENTS;
    "TRUE" => Logical(LogicalFunction::True), 0, 0, FamilyControlled, ARGUMENTS;
    "FALSE" => Logical(LogicalFunction::False), 0, 0, FamilyControlled, ARGUMENTS;
    "NA" => Information(InformationFunction::Na), 0, 0, FamilyControlled, ARGUMENTS;
    "PI" => Math(MathFunction::Pi), 0, 0, FamilyControlled, ARGUMENTS;
    "ABS" => Math(MathFunction::Abs), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "SIGN" => Math(MathFunction::Sign), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "SQRT" => Math(MathFunction::Sqrt), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "EXP" => Math(MathFunction::Exp), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "LN" => Math(MathFunction::Ln), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "LOG10" => Math(MathFunction::Log10), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "LOG" => Math(MathFunction::Log), 1, 2, FamilyControlled, NUMERIC_COERCION;
    "POWER" => Math(MathFunction::Power), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "MOD" => Math(MathFunction::Mod), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "INT" => Math(MathFunction::Int), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "TRUNC" => Math(MathFunction::Trunc), 1, 2, FamilyControlled, NUMERIC_COERCION;
    "ROUND" => Math(MathFunction::Round), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "ROUNDUP" => Math(MathFunction::RoundUp), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "ROUNDDOWN" => Math(MathFunction::RoundDown), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "CEILING" => Math(MathFunction::Ceiling), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "FLOOR" => Math(MathFunction::Floor), 2, 2, FamilyControlled, NUMERIC_COERCION;
    "SIN" => Math(MathFunction::Sin), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "COS" => Math(MathFunction::Cos), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "TAN" => Math(MathFunction::Tan), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "ASIN" => Math(MathFunction::Asin), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "ACOS" => Math(MathFunction::Acos), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "ATAN" => Math(MathFunction::Atan), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "DEGREES" => Math(MathFunction::Degrees), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "RADIANS" => Math(MathFunction::Radians), 1, 1, FamilyControlled, NUMERIC_COERCION;
    "LEN" => Text(TextFunction::Len), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "LEFT" => Text(TextFunction::Left), 1, 2, EagerErrorsBeforeArity, ARGUMENTS;
    "RIGHT" => Text(TextFunction::Right), 1, 2, EagerErrorsBeforeArity, ARGUMENTS;
    "MID" => Text(TextFunction::Mid), 3, 3, EagerErrorsBeforeArity, ARGUMENTS;
    "LOWER" => Text(TextFunction::Lower), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "UPPER" => Text(TextFunction::Upper), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "TRIM" => Text(TextFunction::Trim), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "CLEAN" => Text(TextFunction::Clean), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "CONCATENATE" => Text(TextFunction::Concatenate), 1, 255, EagerErrorsBeforeArity, ARGUMENTS;
    "CONCAT" => Text(TextFunction::Concat), 1, 255, EagerErrorsBeforeArity, ARGUMENTS;
    "EXACT" => Text(TextFunction::Exact), 2, 2, EagerErrorsBeforeArity, ARGUMENTS;
    "FIND" => Text(TextFunction::Find), 2, 3, EagerErrorsBeforeArity, ARGUMENTS;
    "SEARCH" => Text(TextFunction::Search), 2, 3, EagerErrorsBeforeArity, ARGUMENTS;
    "REPLACE" => Text(TextFunction::Replace), 4, 4, EagerErrorsBeforeArity, ARGUMENTS;
    "SUBSTITUTE" => Text(TextFunction::Substitute), 3, 4, EagerErrorsBeforeArity, ARGUMENTS;
    "REPT" => Text(TextFunction::Rept), 2, 2, EagerErrorsBeforeArity, ARGUMENTS;
    "VALUE" => Text(TextFunction::Value), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "CHAR" => Text(TextFunction::Char), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "CODE" => Text(TextFunction::Code), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "UNICHAR" => Text(TextFunction::UniChar), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "UNICODE" => Text(TextFunction::Unicode), 1, 1, EagerErrorsBeforeArity, ARGUMENTS;
    "ARABIC" => Text(TextFunction::Arabic), 1, 1, FamilyControlled, REFERENCE_READS;
    }
    excel {

    "PERCENTILE.INC" => Statistics(StatisticsFunction::Percentile), 2, 2, FamilyControlled, REFERENCE_READS;
    "QUARTILE.INC" => Statistics(StatisticsFunction::QuartileInc), 2, 2, FamilyControlled, REFERENCE_READS;
    "RANK.EQ" => Statistics(StatisticsFunction::RankEq), 2, 3, FamilyControlled, REFERENCE_READS;
    }
}
/// POI ATP strips exactly lowercase `_xlfn.` before case-insensitive lookup.
/// Only pinned implemented ATP registrations have builtin prefix aliases.
pub fn resolve_function(
    name: &poirot_formula::FunctionName,
) -> Option<&'static FunctionDescriptor> {
    use crate::adapters::FunctionAdapter;
    crate::adapters::builtins().resolve(name)
}

// Profile-only handlers are observable typed builtins but are not advertised by
// the default POI registry. Binding remains independent of execution policy.

/// Runtime resolution for otherwise unresolved documented Excel spellings.
/// Arbitrary extensions have already been bound and never pass through here.
pub(crate) fn resolve_excel_function(name: &poirot_formula::FunctionName) -> Option<FunctionId> {
    crate::adapters::profile_function(name)
}

impl FunctionId {
    /// Canonical registry name of this typed builtin handler.
    pub fn canonical_name(self) -> &'static str {
        self.descriptor().name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_descriptors_match_ordered_catalog_scan_for_every_alias() {
        for declared in FUNCTION_DESCRIPTORS.iter().chain(EXCEL_DESCRIPTORS) {
            let expected = FUNCTION_DESCRIPTORS
                .iter()
                .chain(EXCEL_DESCRIPTORS)
                .find(|descriptor| descriptor.handler == declared.handler)
                .unwrap();
            assert_eq!(declared.handler.descriptor(), expected, "{}", declared.name);
            assert_eq!(
                declared.handler.canonical_name(),
                expected.name,
                "{}",
                declared.name
            );
            let resolved = resolve_function(&poirot_formula::FunctionName::from(declared.name))
                .map(|descriptor| descriptor.handler)
                .or_else(|| {
                    resolve_excel_function(&poirot_formula::FunctionName::from(declared.name))
                });
            assert_eq!(resolved, Some(declared.handler), "{}", declared.name);
        }
        // This documented Excel alias reuses an advertised handler without a row.
        let percentile =
            resolve_excel_function(&poirot_formula::FunctionName::from("PERCENTILE.INC")).unwrap();
        assert_eq!(
            percentile.descriptor(),
            resolve_function(&poirot_formula::FunctionName::from("PERCENTILE")).unwrap()
        );
        assert_eq!(percentile.canonical_name(), "PERCENTILE");
    }
}
