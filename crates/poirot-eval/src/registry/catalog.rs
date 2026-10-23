use super::{
    Arity, CriteriaFunction, DatabaseFunction, DateFunction, DependencyPolicy, EngineeringFunction,
    EvaluationStrategy, FinancialFunction, FunctionDescriptor, FunctionId, InformationFunction,
    LogicalFunction, MathFunction, MatrixFunction, ReferenceFunction, ResultRule, SearchFunction,
    StatisticsFunction, TextFunction,
};

// Both the public catalog and typed lookup come from these ordered declarations.
// Repeated handlers are aliases: the first declaration supplies canonical metadata.
macro_rules! registry {
    (builtin { $( $bn:literal => $bf:ident($bk:path), $bmin:expr, $bmax:expr, $bs:ident, $bd:ident, $br:ident; )* }
     excel { $( $en:literal => $ef:ident($ek:path), $emin:expr, $emax:expr, $es:ident, $ed:ident, $er:ident; )* }) => {
        registry! { $($bn => $bf($bk), $bmin, $bmax, $bs, $bd, $br;)* }
        registry! { pub(crate) EXCEL_DESCRIPTORS, excel_descriptor;
            $($en => $ef($ek), $emin, $emax, $es, $ed, $er;)* }
        impl FunctionId {
            /// Return canonical registration metadata with exhaustive typed dispatch.
            pub(crate) fn descriptor(self) -> &'static FunctionDescriptor {
                #[allow(unreachable_patterns)]
                match self {
                    $(FunctionId::$bf($bk) => &FunctionDescriptor {
                        name: $bn, handler: FunctionId::$bf($bk),
                        arity: Arity { minimum: $bmin, maximum: $bmax },
                        strategy: EvaluationStrategy::$bs, dependencies: DependencyPolicy::$bd, result: ResultRule::$br,
                    },)*
                    $(FunctionId::$ef($ek) => &FunctionDescriptor {
                        name: $en, handler: FunctionId::$ef($ek),
                        arity: Arity { minimum: $emin, maximum: $emax },
                        strategy: EvaluationStrategy::$es, dependencies: DependencyPolicy::$ed, result: ResultRule::$er,
                    },)*
                }
            }
        }
    };
    ($visibility:vis $descriptors:ident, $lookup:ident; $( $name:literal => $family:ident($kind:path), $min:expr, $max:expr, $strategy:ident, $dependency:ident, $result:ident; )*) => {
        /// Ordered registrations; the first alias supplies canonical handler metadata.
        $visibility const $descriptors: &[FunctionDescriptor] = &[$(FunctionDescriptor {
            name: $name, handler: FunctionId::$family($kind),
            arity: Arity { minimum: $min, maximum: $max },
            strategy: EvaluationStrategy::$strategy, dependencies: DependencyPolicy::$dependency, result: ResultRule::$result,
        }),*];


    };
    ($( $name:literal => $family:ident($kind:path), $min:expr, $max:expr, $strategy:ident, $dependency:ident, $result:ident; )*) => {
        /// Advertised builtin spellings in registration order.
        pub const SUPPORTED_FUNCTIONS: &[&str] = &[$($name),*];
        registry! { pub FUNCTION_DESCRIPTORS, builtin_descriptor;
            $($name => $family($kind), $min, $max, $strategy, $dependency, $result;)*
        }
    };
}
registry! {
    builtin {
    "RAND" => Math(MathFunction::Rand), 0, 0, FamilyControlled, RANDOM_READS, Number;
    "RANDBETWEEN" => Math(MathFunction::Randbetween), 2, 2, FamilyControlled, RANDOM_READS, Number;
    "ERROR.TYPE" => Logical(LogicalFunction::ErrorType), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "IFS" => Logical(LogicalFunction::Ifs), 0, usize::MAX, Lazy, REFERENCE_READS, Ifs;
    "ISEVEN" => Logical(LogicalFunction::IsEven), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "ISODD" => Logical(LogicalFunction::IsOdd), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "SWITCH" => Logical(LogicalFunction::Switch), 3, usize::MAX, Lazy, REFERENCE_READS, Switch;
    "LOOKUP" => Search(SearchFunction::Lookup), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "XLOOKUP" => Search(SearchFunction::XLookup), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Any;
    "XMATCH" => Search(SearchFunction::XMatch), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "ADDRESS" => Reference(ReferenceFunction::Address), 2, 5, FamilyControlled, REFERENCE_READS, Scalar;
    "AREAS" => Reference(ReferenceFunction::Areas), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "HYPERLINK" => Reference(ReferenceFunction::Hyperlink), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "INDIRECT" => Reference(ReferenceFunction::Indirect), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Reference;
    "OFFSET" => Reference(ReferenceFunction::Offset), 1, 5, FamilyControlled, REFERENCE_READS, Reference;
    "SHEET" => Reference(ReferenceFunction::Sheet), 0, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "SINGLE" => Reference(ReferenceFunction::Single), 0, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "ACOSH" => Math(MathFunction::Acosh), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "ASINH" => Math(MathFunction::Asinh), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "ATAN2" => Math(MathFunction::Atan2), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "ATANH" => Math(MathFunction::Atanh), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "BASE" => Math(MathFunction::Base), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DECIMAL" => Math(MathFunction::Decimal), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "CEILING.MATH" => Math(MathFunction::CeilingMath), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "CEILING.PRECISE" => Math(MathFunction::CeilingPrecise), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "SERIESSUM" => Math(MathFunction::SeriesSum), 4, 4, FamilyControlled, REFERENCE_READS, Scalar;
    "COMBINA" => Math(MathFunction::Combina), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "COMBIN" => Math(MathFunction::Combin), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "COSH" => Math(MathFunction::Cosh), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "EVEN" => Math(MathFunction::Even), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "FACT" => Math(MathFunction::Fact), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "FACTDOUBLE" => Math(MathFunction::Factdouble), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "FLOOR.MATH" => Math(MathFunction::FloorMath), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "FLOOR.PRECISE" => Math(MathFunction::FloorPrecise), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "GCD" => Math(MathFunction::Gcd), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "LCM" => Math(MathFunction::Lcm), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "MROUND" => Math(MathFunction::Mround), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "ODD" => Math(MathFunction::Odd), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "QUOTIENT" => Math(MathFunction::Quotient), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SINH" => Math(MathFunction::Sinh), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "SQRTPI" => Math(MathFunction::Sqrtpi), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "TANH" => Math(MathFunction::Tanh), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "AVEDEV" => Statistics(StatisticsFunction::AveDev), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "AVERAGEA" => Statistics(StatisticsFunction::AverageA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "DEVSQ" => Statistics(StatisticsFunction::DevSq), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "GEOMEAN" => Statistics(StatisticsFunction::GeoMean), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "MAXA" => Statistics(StatisticsFunction::MaxA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "MINA" => Statistics(StatisticsFunction::MinA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "STDEVA" => Statistics(StatisticsFunction::StdevA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "STDEVPA" => Statistics(StatisticsFunction::StdevPA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "VARA" => Statistics(StatisticsFunction::VarA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "VARPA" => Statistics(StatisticsFunction::VarPA), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "MODE" => Statistics(StatisticsFunction::Mode), 1, 30, FamilyControlled, REFERENCE_READS, Scalar;
    "LARGE" => Statistics(StatisticsFunction::Large), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SMALL" => Statistics(StatisticsFunction::Small), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "PERCENTILE" => Statistics(StatisticsFunction::Percentile), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "RANK" => Statistics(StatisticsFunction::Rank), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "PERCENTRANK" => Statistics(StatisticsFunction::PercentRank), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "PERCENTRANK.INC" => Statistics(StatisticsFunction::PercentRankInc), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "PERCENTRANK.EXC" => Statistics(StatisticsFunction::PercentRankExc), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "CORREL" => Statistics(StatisticsFunction::Correl), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "PEARSON" => Statistics(StatisticsFunction::Pearson), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "COVAR" => Statistics(StatisticsFunction::Covar), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "COVARIANCE.P" => Statistics(StatisticsFunction::CovarianceP), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "COVARIANCE.S" => Statistics(StatisticsFunction::CovarianceS), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "FORECAST" => Statistics(StatisticsFunction::Forecast), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "FORECAST.LINEAR" => Statistics(StatisticsFunction::ForecastLinear), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "INTERCEPT" => Statistics(StatisticsFunction::Intercept), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SLOPE" => Statistics(StatisticsFunction::Slope), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SUMX2MY2" => Statistics(StatisticsFunction::SumX2MY2), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SUMX2PY2" => Statistics(StatisticsFunction::SumX2PY2), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SUMXMY2" => Statistics(StatisticsFunction::SumXMY2), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "NORM.DIST" => Statistics(StatisticsFunction::NormDist), 4, 4, FamilyControlled, REFERENCE_READS, Scalar;
    "NORMDIST" => Statistics(StatisticsFunction::NormDistLegacy), 4, 4, FamilyControlled, REFERENCE_READS, Scalar;
    "NORM.INV" => Statistics(StatisticsFunction::NormInv), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "NORMINV" => Statistics(StatisticsFunction::NormInvLegacy), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "NORM.S.INV" => Statistics(StatisticsFunction::NormSInv), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "NORMSINV" => Statistics(StatisticsFunction::NormSInvLegacy), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "NORM.S.DIST" => Statistics(StatisticsFunction::NormSDist), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "NORMSDIST" => Statistics(StatisticsFunction::NormSDistLegacy), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "STANDARDIZE" => Statistics(StatisticsFunction::Standardize), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "POISSON" => Statistics(StatisticsFunction::Poisson), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "POISSON.DIST" => Statistics(StatisticsFunction::PoissonDist), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "TDIST" => Statistics(StatisticsFunction::TDistLegacy), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "T.DIST" => Statistics(StatisticsFunction::TDist), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "T.DIST.RT" => Statistics(StatisticsFunction::TDistRt), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "T.DIST.2T" => Statistics(StatisticsFunction::TDist2T), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "NUMBERVALUE" => Text(TextFunction::NumberValue), 0, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "PROPER" => Text(TextFunction::Proper), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "ROMAN" => Text(TextFunction::Roman), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "TEXTJOIN" => Text(TextFunction::TextJoin), 3, 254, FamilyControlled, REFERENCE_READS, Scalar;
    "FV" => Financial(FinancialFunction::Fv), 3, 5, FamilyControlled, REFERENCE_READS, Scalar;
    "PV" => Financial(FinancialFunction::Pv), 3, 5, FamilyControlled, REFERENCE_READS, Scalar;
    "PMT" => Financial(FinancialFunction::Pmt), 3, 5, FamilyControlled, REFERENCE_READS, Scalar;
    "NPER" => Financial(FinancialFunction::Nper), 3, 5, FamilyControlled, REFERENCE_READS, Scalar;
    "IPMT" => Financial(FinancialFunction::Ipmt), 4, 4, FamilyControlled, REFERENCE_READS, Scalar;
    "PPMT" => Financial(FinancialFunction::Ppmt), 4, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "RATE" => Financial(FinancialFunction::Rate), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "NPV" => Financial(FinancialFunction::Npv), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "IRR" => Financial(FinancialFunction::Irr), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "MIRR" => Financial(FinancialFunction::Mirr), 0, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DOLLAR" => Financial(FinancialFunction::Dollar), 1, 2, FamilyControlled, CURRENCY_FORMATTING, Scalar;
    "DOLLARDE" => Financial(FinancialFunction::DollarDe), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "DOLLARFR" => Financial(FinancialFunction::DollarFr), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "FIXED" => Financial(FinancialFunction::Fixed), 1, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "BESSELJ" => Engineering(EngineeringFunction::Besselj), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "BIN2DEC" => Engineering(EngineeringFunction::Bin2dec), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "COMPLEX" => Engineering(EngineeringFunction::Complex), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DEC2BIN" => Engineering(EngineeringFunction::Dec2bin), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "DEC2HEX" => Engineering(EngineeringFunction::Dec2hex), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "DELTA" => Engineering(EngineeringFunction::Delta), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "HEX2DEC" => Engineering(EngineeringFunction::Hex2dec), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "IMAGINARY" => Engineering(EngineeringFunction::Imaginary), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "IMREAL" => Engineering(EngineeringFunction::Imreal), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "OCT2DEC" => Engineering(EngineeringFunction::Oct2dec), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "MDETERM" => Matrix(MatrixFunction::Mdeterm), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "MINVERSE" => Matrix(MatrixFunction::Minverse), 1, 1, FamilyControlled, REFERENCE_READS, Array;
    "MMULT" => Matrix(MatrixFunction::Mmult), 2, 2, FamilyControlled, REFERENCE_READS, Array;
    "TRANSPOSE" => Matrix(MatrixFunction::Transpose), 1, 1, FamilyControlled, REFERENCE_READS, Transpose;
    "FREQUENCY" => Matrix(MatrixFunction::Frequency), 2, 2, FamilyControlled, REFERENCE_READS, Array;
    "SUMPRODUCT" => Matrix(MatrixFunction::Sumproduct), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "TREND" => Matrix(MatrixFunction::Trend), 1, 4, FamilyControlled, REFERENCE_READS, Array;
    "COUNTIF" => Criteria(CriteriaFunction::CountIf), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "SUMIF" => Criteria(CriteriaFunction::SumIf), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "AVERAGEIF" => Criteria(CriteriaFunction::AverageIf), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "COUNTIFS" => Criteria(CriteriaFunction::CountIfs), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "SUMIFS" => Criteria(CriteriaFunction::SumIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "AVERAGEIFS" => Criteria(CriteriaFunction::AverageIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "MAXIFS" => Criteria(CriteriaFunction::MaxIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "MINIFS" => Criteria(CriteriaFunction::MinIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "SUBTOTAL" => Criteria(CriteriaFunction::Subtotal), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "DAVERAGE" => Database(DatabaseFunction::Average), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DCOUNT" => Database(DatabaseFunction::Count), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DCOUNTA" => Database(DatabaseFunction::CountA), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DGET" => Database(DatabaseFunction::Get), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DMAX" => Database(DatabaseFunction::Max), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DMIN" => Database(DatabaseFunction::Min), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DPRODUCT" => Database(DatabaseFunction::Product), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DSTDEV" => Database(DatabaseFunction::Stdev), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DSTDEVP" => Database(DatabaseFunction::StdevP), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DSUM" => Database(DatabaseFunction::Sum), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DVAR" => Database(DatabaseFunction::Var), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DVARP" => Database(DatabaseFunction::VarP), 3, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "DATE" => Date(DateFunction::Date), 3, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "DATEDIF" => Date(DateFunction::DateDif), 3, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "DATEVALUE" => Date(DateFunction::DateValue), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "DAY" => Date(DateFunction::Day), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "DAYS" => Date(DateFunction::Days), 2, 2, FamilyControlled, DATE_FAMILY, Scalar;
    "DAYS360" => Date(DateFunction::Days360), 2, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "EDATE" => Date(DateFunction::EDate), 2, 2, FamilyControlled, DATE_FAMILY, Scalar;
    "EOMONTH" => Date(DateFunction::EOMonth), 2, 2, FamilyControlled, DATE_FAMILY, Scalar;
    "HOUR" => Date(DateFunction::Hour), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "MINUTE" => Date(DateFunction::Minute), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "MONTH" => Date(DateFunction::Month), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "NETWORKDAYS" => Date(DateFunction::NetworkDays), 2, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "NOW" => Date(DateFunction::Now), 0, 0, FamilyControlled, TIME_READS, Number;
    "SECOND" => Date(DateFunction::Second), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "TIME" => Date(DateFunction::Time), 3, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "TIMEVALUE" => Date(DateFunction::TimeValue), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "TODAY" => Date(DateFunction::Today), 0, 0, FamilyControlled, TIME_READS, Number;
    "WEEKDAY" => Date(DateFunction::WeekDay), 1, 2, FamilyControlled, DATE_FAMILY, Scalar;
    "WEEKNUM" => Date(DateFunction::WeekNum), 1, 2, FamilyControlled, DATE_FAMILY, Scalar;
    "WORKDAY" => Date(DateFunction::WorkDay), 2, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "WORKDAY.INTL" => Date(DateFunction::WorkDayIntl), 2, 4, FamilyControlled, DATE_FAMILY, Scalar;
    "YEAR" => Date(DateFunction::Year), 1, 1, FamilyControlled, DATE_FAMILY, Scalar;
    "YEARFRAC" => Date(DateFunction::YearFrac), 2, 3, FamilyControlled, DATE_FAMILY, Scalar;
    "TEXT" => Text(TextFunction::Text), 2, 2, FamilyControlled, TEXT_FORMATTING, Scalar;
    "IF" => Logical(LogicalFunction::If), 2, 3, Lazy, ARGUMENTS, If;
    "ISREF" => Logical(LogicalFunction::IsRef), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "IFERROR" => Logical(LogicalFunction::IfError), 2, 2, Lazy, ARGUMENTS, Fallback;
    "IFNA" => Logical(LogicalFunction::IfNa), 2, 2, Lazy, ARGUMENTS, Fallback;
    "CHOOSE" => Logical(LogicalFunction::Choose), 2, usize::MAX, Lazy, ARGUMENTS, Choose;
    "SUM" => Statistics(StatisticsFunction::Sum), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "AVERAGE" => Statistics(StatisticsFunction::Average), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "COUNT" => Statistics(StatisticsFunction::Count), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "COUNTA" => Statistics(StatisticsFunction::CountA), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "COUNTBLANK" => Statistics(StatisticsFunction::CountBlank), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "MIN" => Statistics(StatisticsFunction::Min), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "MAX" => Statistics(StatisticsFunction::Max), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "PRODUCT" => Statistics(StatisticsFunction::Product), 0, usize::MAX, FamilyControlled, ARGUMENTS, Number;
    "AND" => Logical(LogicalFunction::And), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "OR" => Logical(LogicalFunction::Or), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "NOT" => Logical(LogicalFunction::Not), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "SUMSQ" => Statistics(StatisticsFunction::SumSq), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "STDEV" => Statistics(StatisticsFunction::Stdev), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "STDEV.S" => Statistics(StatisticsFunction::Stdev), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "STDEV.P" => Statistics(StatisticsFunction::StdevP), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "STDEVP" => Statistics(StatisticsFunction::StdevP), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "VAR" => Statistics(StatisticsFunction::Var), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "VAR.S" => Statistics(StatisticsFunction::Var), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "VAR.P" => Statistics(StatisticsFunction::VarP), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "VARP" => Statistics(StatisticsFunction::VarP), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "MEDIAN" => Statistics(StatisticsFunction::Median), 0, usize::MAX, FamilyControlled, ARGUMENTS, Scalar;
    "ROWS" => Reference(ReferenceFunction::Rows), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "COLUMNS" => Reference(ReferenceFunction::Columns), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "ROW" => Reference(ReferenceFunction::Row), 0, 1, FamilyControlled, POSITION_READS, Scalar;
    "COLUMN" => Reference(ReferenceFunction::Column), 0, 1, FamilyControlled, POSITION_READS, Scalar;
    "INDEX" => Reference(ReferenceFunction::Index), 2, 4, FamilyControlled, REFERENCE_READS, Index;
    "MATCH" => Search(SearchFunction::Match), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "VLOOKUP" => Search(SearchFunction::VLookup), 3, 4, FamilyControlled, REFERENCE_READS, Scalar;
    "HLOOKUP" => Search(SearchFunction::HLookup), 3, 4, FamilyControlled, REFERENCE_READS, Scalar;
    "ISERROR" => Information(InformationFunction::IsError), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISERR" => Information(InformationFunction::IsErr), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISNA" => Information(InformationFunction::IsNa), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISNUMBER" => Information(InformationFunction::IsNumber), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISTEXT" => Information(InformationFunction::IsText), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISBLANK" => Information(InformationFunction::IsBlank), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISLOGICAL" => Information(InformationFunction::IsLogical), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "ISNONTEXT" => Information(InformationFunction::IsNonText), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
    "TYPE" => Information(InformationFunction::Type), 1, 1, FamilyControlled, ARGUMENTS, Scalar;
    "N" => Information(InformationFunction::N), 1, 1, FamilyControlled, ARGUMENTS, Scalar;
    "T" => Information(InformationFunction::T), 1, 1, FamilyControlled, ARGUMENTS, Scalar;
    "TRUE" => Logical(LogicalFunction::True), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "FALSE" => Logical(LogicalFunction::False), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "NA" => Information(InformationFunction::Na), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "PI" => Math(MathFunction::Pi), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "ABS" => Math(MathFunction::Abs), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "SIGN" => Math(MathFunction::Sign), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "SQRT" => Math(MathFunction::Sqrt), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "EXP" => Math(MathFunction::Exp), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "LN" => Math(MathFunction::Ln), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "LOG10" => Math(MathFunction::Log10), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "LOG" => Math(MathFunction::Log), 1, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "POWER" => Math(MathFunction::Power), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "MOD" => Math(MathFunction::Mod), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "INT" => Math(MathFunction::Int), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "TRUNC" => Math(MathFunction::Trunc), 1, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "ROUND" => Math(MathFunction::Round), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "ROUNDUP" => Math(MathFunction::RoundUp), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "ROUNDDOWN" => Math(MathFunction::RoundDown), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "CEILING" => Math(MathFunction::Ceiling), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "FLOOR" => Math(MathFunction::Floor), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "SIN" => Math(MathFunction::Sin), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "COS" => Math(MathFunction::Cos), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "TAN" => Math(MathFunction::Tan), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "ASIN" => Math(MathFunction::Asin), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "ACOS" => Math(MathFunction::Acos), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "ATAN" => Math(MathFunction::Atan), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "DEGREES" => Math(MathFunction::Degrees), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "RADIANS" => Math(MathFunction::Radians), 1, 1, FamilyControlled, NUMERIC_COERCION, Scalar;
    "LEN" => Text(TextFunction::Len), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "LEFT" => Text(TextFunction::Left), 1, 2, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "RIGHT" => Text(TextFunction::Right), 1, 2, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "MID" => Text(TextFunction::Mid), 3, 3, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "LOWER" => Text(TextFunction::Lower), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "UPPER" => Text(TextFunction::Upper), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "TRIM" => Text(TextFunction::Trim), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "CLEAN" => Text(TextFunction::Clean), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "CONCATENATE" => Text(TextFunction::Concatenate), 1, 255, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "CONCAT" => Text(TextFunction::Concat), 1, 255, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "EXACT" => Text(TextFunction::Exact), 2, 2, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "FIND" => Text(TextFunction::Find), 2, 3, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "SEARCH" => Text(TextFunction::Search), 2, 3, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "REPLACE" => Text(TextFunction::Replace), 4, 4, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "SUBSTITUTE" => Text(TextFunction::Substitute), 3, 4, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "REPT" => Text(TextFunction::Rept), 2, 2, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "VALUE" => Text(TextFunction::Value), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "CHAR" => Text(TextFunction::Char), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "CODE" => Text(TextFunction::Code), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "UNICHAR" => Text(TextFunction::UniChar), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "UNICODE" => Text(TextFunction::Unicode), 1, 1, EagerErrorsBeforeArity, ARGUMENTS, Scalar;
    "ARABIC" => Text(TextFunction::Arabic), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    }
    excel {
    "IMABS" => Engineering(EngineeringFunction::Imabs), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "IMARGUMENT" => Engineering(EngineeringFunction::Imargument), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "IMCONJUGATE" => Engineering(EngineeringFunction::Imconjugate), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMCOS" => Engineering(EngineeringFunction::Imcos), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMCOSH" => Engineering(EngineeringFunction::Imcosh), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMCOT" => Engineering(EngineeringFunction::Imcot), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMCSC" => Engineering(EngineeringFunction::Imcsc), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMCSCH" => Engineering(EngineeringFunction::Imcsch), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMDIV" => Engineering(EngineeringFunction::Imdiv), 2, 2, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMEXP" => Engineering(EngineeringFunction::Imexp), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMLN" => Engineering(EngineeringFunction::Imln), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMLOG10" => Engineering(EngineeringFunction::Imlog10), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMLOG2" => Engineering(EngineeringFunction::Imlog2), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMPOWER" => Engineering(EngineeringFunction::Impower), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "IMPRODUCT" => Engineering(EngineeringFunction::Improduct), 1, 255, FamilyControlled, REFERENCE_READS, Text;
    "IMSEC" => Engineering(EngineeringFunction::Imsec), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMSECH" => Engineering(EngineeringFunction::Imsech), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMSIN" => Engineering(EngineeringFunction::Imsin), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMSINH" => Engineering(EngineeringFunction::Imsinh), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMSQRT" => Engineering(EngineeringFunction::Imsqrt), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMSUB" => Engineering(EngineeringFunction::Imsub), 2, 2, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "IMSUM" => Engineering(EngineeringFunction::Imsum), 1, 255, FamilyControlled, REFERENCE_READS, Text;
    "IMTAN" => Engineering(EngineeringFunction::Imtan), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "BIN2HEX" => Engineering(EngineeringFunction::Bin2hex), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "BIN2OCT" => Engineering(EngineeringFunction::Bin2oct), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "BITAND" => Engineering(EngineeringFunction::Bitand), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BITLSHIFT" => Engineering(EngineeringFunction::Bitlshift), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BITOR" => Engineering(EngineeringFunction::Bitor), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BITRSHIFT" => Engineering(EngineeringFunction::Bitrshift), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BITXOR" => Engineering(EngineeringFunction::Bitxor), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CONVERT" => Engineering(EngineeringFunction::Convert), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "DEC2OCT" => Engineering(EngineeringFunction::Dec2oct), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "HEX2BIN" => Engineering(EngineeringFunction::Hex2bin), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "HEX2OCT" => Engineering(EngineeringFunction::Hex2oct), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "OCT2BIN" => Engineering(EngineeringFunction::Oct2bin), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "OCT2HEX" => Engineering(EngineeringFunction::Oct2hex), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "BESSELI" => Engineering(EngineeringFunction::Besseli), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BESSELK" => Engineering(EngineeringFunction::Besselk), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BESSELY" => Engineering(EngineeringFunction::Bessely), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ERF" => Engineering(EngineeringFunction::Erf), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ERF.PRECISE" => Engineering(EngineeringFunction::ErfPrecise), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ERFC" => Engineering(EngineeringFunction::Erfc), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ERFC.PRECISE" => Engineering(EngineeringFunction::ErfcPrecise), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GESTEP" => Engineering(EngineeringFunction::Gestep), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;


    "ACOT" => Math(MathFunction::Acot), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ACOTH" => Math(MathFunction::Acoth), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "COT" => Math(MathFunction::Cot), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "COTH" => Math(MathFunction::Coth), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CSC" => Math(MathFunction::Csc), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CSCH" => Math(MathFunction::Csch), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SEC" => Math(MathFunction::Sec), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SECH" => Math(MathFunction::Sech), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ISO.CEILING" => Math(MathFunction::IsoCeiling), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "MULTINOMIAL" => Math(MathFunction::Multinomial), 1, 255, FamilyControlled, NUMERIC_COERCION, Number;
    "ISOWEEKNUM" => Date(DateFunction::IsoWeekNum), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "NETWORKDAYS.INTL" => Date(DateFunction::NetworkDaysIntl), 2, 4, FamilyControlled, DATE_FAMILY, BusinessDays;
    "MUNIT" => Matrix(MatrixFunction::Munit), 1, 1, FamilyControlled, NUMERIC_COERCION, UnitMatrix;
    "EUROCONVERT" => Financial(FinancialFunction::Euroconvert), 3, 5, FamilyControlled, NUMERIC_COERCION, Number;
    "XOR" => Logical(LogicalFunction::Xor), 1, 254, FamilyControlled, REFERENCE_READS, Scalar;
    "GROUPBY" => Matrix(MatrixFunction::Groupby), 3, 8, FamilyControlled, NUMERIC_COERCION, Array;
    "PERCENTOF" => Matrix(MatrixFunction::Percentof), 2, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "REGEXTEST" => Text(TextFunction::RegexTest), 2, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseBoolean;
    "REGEXEXTRACT" => Text(TextFunction::RegexExtract), 2, 4, FamilyControlled, NUMERIC_COERCION, RegexExtract;
    "REGEXREPLACE" => Text(TextFunction::RegexReplace), 3, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "TEXTBEFORE" => Text(TextFunction::TextBefore), 2, 6, FamilyControlled, REFERENCE_READS, Array;
    "TEXTAFTER" => Text(TextFunction::TextAfter), 2, 6, FamilyControlled, REFERENCE_READS, Array;
    "TEXTSPLIT" => Text(TextFunction::TextSplit), 2, 6, FamilyControlled, REFERENCE_READS, Array;
    "ARRAYTOTEXT" => Text(TextFunction::ArrayToText), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "VALUETOTEXT" => Text(TextFunction::ValueToText), 1, 2, FamilyControlled, REFERENCE_READS, Array;
    "HSTACK" => Matrix(MatrixFunction::Hstack), 1, 255, FamilyControlled, REFERENCE_READS, Array;
    "VSTACK" => Matrix(MatrixFunction::Vstack), 1, 255, FamilyControlled, REFERENCE_READS, Array;
    "TAKE" => Matrix(MatrixFunction::Take), 2, 3, FamilyControlled, REFERENCE_READS, Array;
    "DROP" => Matrix(MatrixFunction::Drop), 2, 3, FamilyControlled, REFERENCE_READS, Array;
    "CHOOSECOLS" => Matrix(MatrixFunction::Choosecols), 2, 255, FamilyControlled, REFERENCE_READS, Array;
    "CHOOSEROWS" => Matrix(MatrixFunction::Chooserows), 2, 255, FamilyControlled, REFERENCE_READS, Array;
    "EXPAND" => Matrix(MatrixFunction::Expand), 2, 4, FamilyControlled, REFERENCE_READS, Array;
    "FILTER" => Matrix(MatrixFunction::Filter), 2, 3, FamilyControlled, REFERENCE_READS, Array;
    "SORT" => Matrix(MatrixFunction::Sort), 1, 4, FamilyControlled, REFERENCE_READS, Array;
    "SORTBY" => Matrix(MatrixFunction::Sortby), 2, 255, FamilyControlled, REFERENCE_READS, Array;
    "UNIQUE" => Matrix(MatrixFunction::Unique), 1, 3, FamilyControlled, REFERENCE_READS, Array;
    "TOCOL" => Matrix(MatrixFunction::Tocol), 1, 3, FamilyControlled, REFERENCE_READS, Array;
    "TOROW" => Matrix(MatrixFunction::Torow), 1, 3, FamilyControlled, REFERENCE_READS, Array;
    "WRAPCOLS" => Matrix(MatrixFunction::Wrapcols), 2, 3, FamilyControlled, REFERENCE_READS, Array;
    "WRAPROWS" => Matrix(MatrixFunction::Wraprows), 2, 3, FamilyControlled, REFERENCE_READS, Array;
    "RANDARRAY" => Matrix(MatrixFunction::Randarray), 0, 5, FamilyControlled, RANDOM_NUMERIC, RandomArray;
    "TRIMRANGE" => Matrix(MatrixFunction::Trimrange), 1, 3, FamilyControlled, NUMERIC_COERCION, TrimRange;
    "SEQUENCE" => Matrix(MatrixFunction::Sequence), 1, 4, FamilyControlled, NUMERIC_COERCION, Sequence;
    "ANCHORARRAY" => Reference(ReferenceFunction::AnchorArray), 1, 1, FamilyControlled, REFERENCE_READS, SpillRange;

    "PERCENTILE.INC" => Statistics(StatisticsFunction::Percentile), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "QUARTILE.INC" => Statistics(StatisticsFunction::QuartileInc), 2, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "RANK.EQ" => Statistics(StatisticsFunction::RankEq), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    }
}
/// POI ATP strips exactly lowercase `_xlfn.` before case-insensitive lookup.
/// Only pinned implemented ATP registrations have builtin prefix aliases.
pub fn resolve_function(
    name: &poirot_parsing::formula::FunctionName,
) -> Option<&'static FunctionDescriptor> {
    use crate::adapters::FunctionAdapter;
    crate::adapters::builtins().resolve(name)
}

// Profile-only handlers are observable typed builtins but are not advertised by
// the default POI registry. Binding remains independent of execution policy.

/// Runtime resolution for otherwise unresolved documented Excel spellings.
/// Arbitrary extensions have already been bound and never pass through here.
pub(crate) fn resolve_excel_function(
    name: &poirot_parsing::formula::FunctionName,
) -> Option<FunctionId> {
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
            let resolved =
                resolve_function(&poirot_parsing::formula::FunctionName::from(declared.name))
                    .map(|descriptor| descriptor.handler)
                    .or_else(|| {
                        resolve_excel_function(&poirot_parsing::formula::FunctionName::from(
                            declared.name,
                        ))
                    });
            assert_eq!(resolved, Some(declared.handler), "{}", declared.name);
        }
        // This documented Excel alias reuses an advertised handler without a row.
        let percentile = resolve_excel_function(&poirot_parsing::formula::FunctionName::from(
            "PERCENTILE.INC",
        ))
        .unwrap();
        assert_eq!(
            percentile.descriptor(),
            resolve_function(&poirot_parsing::formula::FunctionName::from("PERCENTILE")).unwrap()
        );
        assert_eq!(percentile.canonical_name(), "PERCENTILE");
    }
}
