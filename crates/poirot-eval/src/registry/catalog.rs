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
    "RANDBETWEEN" => Math(MathFunction::Randbetween), 2, 2, FamilyControlled, RANDOM_NUMERIC, ElementwiseNumber;
    "ERROR.TYPE" => Logical(LogicalFunction::ErrorType), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "IFS" => Logical(LogicalFunction::Ifs), 0, usize::MAX, Lazy, REFERENCE_READS, Ifs;
    "ISEVEN" => Logical(LogicalFunction::IsEven), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseBoolean;
    "ISODD" => Logical(LogicalFunction::IsOdd), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseBoolean;
    "SWITCH" => Logical(LogicalFunction::Switch), 3, usize::MAX, Lazy, REFERENCE_READS, Switch;
    "LOOKUP" => Search(SearchFunction::Lookup), 2, 3, FamilyControlled, REFERENCE_READS, Scalar;
    "XLOOKUP" => Search(SearchFunction::XLookup), 3, usize::MAX, FamilyControlled, REFERENCE_READS, Any;
    "XMATCH" => Search(SearchFunction::XMatch), 2, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "ADDRESS" => Reference(ReferenceFunction::Address), 2, 5, FamilyControlled, REFERENCE_READS, Scalar;
    "AREAS" => Reference(ReferenceFunction::Areas), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "HYPERLINK" => Reference(ReferenceFunction::Hyperlink), 1, 2, FamilyControlled, REFERENCE_READS, Scalar;
    "INDIRECT" => Reference(ReferenceFunction::Indirect), 1, usize::MAX, FamilyControlled, REFERENCE_READS, Reference;
    "OFFSET" => Reference(ReferenceFunction::Offset), 3, 5, FamilyControlled, REFERENCE_READS, Reference;
    "SHEET" => Reference(ReferenceFunction::Sheet), 0, usize::MAX, FamilyControlled, REFERENCE_READS, Scalar;
    "SINGLE" => Reference(ReferenceFunction::Single), 1, 1, FamilyControlled, REFERENCE_READS, Scalar;
    "ACOSH" => Math(MathFunction::Acosh), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ASINH" => Math(MathFunction::Asinh), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ATAN2" => Math(MathFunction::Atan2), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ATANH" => Math(MathFunction::Atanh), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BASE" => Math(MathFunction::Base), 2, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "DECIMAL" => Math(MathFunction::Decimal), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CEILING.MATH" => Math(MathFunction::CeilingMath), 1, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CEILING.PRECISE" => Math(MathFunction::CeilingPrecise), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SERIESSUM" => Math(MathFunction::SeriesSum), 4, 4, FamilyControlled, NUMERIC_COERCION, FirstThreeNumericArguments;
    "COMBINA" => Math(MathFunction::Combina), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "COMBIN" => Math(MathFunction::Combin), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "COSH" => Math(MathFunction::Cosh), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "EVEN" => Math(MathFunction::Even), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FACT" => Math(MathFunction::Fact), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FACTDOUBLE" => Math(MathFunction::Factdouble), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FLOOR.MATH" => Math(MathFunction::FloorMath), 1, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FLOOR.PRECISE" => Math(MathFunction::FloorPrecise), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GCD" => Math(MathFunction::Gcd), 1, 255, FamilyControlled, NUMERIC_COERCION, Number;
    "LCM" => Math(MathFunction::Lcm), 1, 255, FamilyControlled, NUMERIC_COERCION, Number;
    "MROUND" => Math(MathFunction::Mround), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ODD" => Math(MathFunction::Odd), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "QUOTIENT" => Math(MathFunction::Quotient), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SINH" => Math(MathFunction::Sinh), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SQRTPI" => Math(MathFunction::Sqrtpi), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "TANH" => Math(MathFunction::Tanh), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "AVEDEV" => Statistics(StatisticsFunction::AveDev), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "AVERAGEA" => Statistics(StatisticsFunction::AverageA), 1, 255, FamilyControlled, REFERENCE_READS, Number;
    "DEVSQ" => Statistics(StatisticsFunction::DevSq), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "GEOMEAN" => Statistics(StatisticsFunction::GeoMean), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "MAXA" => Statistics(StatisticsFunction::MaxA), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "MINA" => Statistics(StatisticsFunction::MinA), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "STDEVA" => Statistics(StatisticsFunction::StdevA), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "STDEVPA" => Statistics(StatisticsFunction::StdevPA), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "VARA" => Statistics(StatisticsFunction::VarA), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "VARPA" => Statistics(StatisticsFunction::VarPA), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "MODE" => Statistics(StatisticsFunction::Mode), 1, 30, FamilyControlled, REFERENCE_READS, Number;
    "LARGE" => Statistics(StatisticsFunction::Large), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "SMALL" => Statistics(StatisticsFunction::Small), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "PERCENTILE" => Statistics(StatisticsFunction::Percentile), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "RANK" => Statistics(StatisticsFunction::Rank), 2, 3, FamilyControlled, NUMERIC_COERCION, RankArguments;
    "PERCENTRANK" => Statistics(StatisticsFunction::PercentRank), 2, usize::MAX, FamilyControlled, NUMERIC_COERCION, AfterFirstNumericArguments;
    "PERCENTRANK.INC" => Statistics(StatisticsFunction::PercentRankInc), 2, usize::MAX, FamilyControlled, NUMERIC_COERCION, AfterFirstNumericArguments;
    "PERCENTRANK.EXC" => Statistics(StatisticsFunction::PercentRankExc), 2, usize::MAX, FamilyControlled, NUMERIC_COERCION, AfterFirstNumericArguments;
    "CORREL" => Statistics(StatisticsFunction::Correl), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "PEARSON" => Statistics(StatisticsFunction::Pearson), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "COVAR" => Statistics(StatisticsFunction::Covar), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "COVARIANCE.P" => Statistics(StatisticsFunction::CovarianceP), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "COVARIANCE.S" => Statistics(StatisticsFunction::CovarianceS), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "FORECAST" => Statistics(StatisticsFunction::Forecast), 3, 3, FamilyControlled, NUMERIC_COERCION, FirstNumericArgument;
    "FORECAST.LINEAR" => Statistics(StatisticsFunction::ForecastLinear), 3, 3, FamilyControlled, NUMERIC_COERCION, FirstNumericArgument;
    "INTERCEPT" => Statistics(StatisticsFunction::Intercept), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "SLOPE" => Statistics(StatisticsFunction::Slope), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "SUMX2MY2" => Statistics(StatisticsFunction::SumX2MY2), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "SUMX2PY2" => Statistics(StatisticsFunction::SumX2PY2), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "SUMXMY2" => Statistics(StatisticsFunction::SumXMY2), 2, 2, FamilyControlled, REFERENCE_READS, Number;
    "NORM.DIST" => Statistics(StatisticsFunction::NormDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORMDIST" => Statistics(StatisticsFunction::NormDistLegacy), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORM.INV" => Statistics(StatisticsFunction::NormInv), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORMINV" => Statistics(StatisticsFunction::NormInvLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORM.S.INV" => Statistics(StatisticsFunction::NormSInv), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORMSINV" => Statistics(StatisticsFunction::NormSInvLegacy), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORM.S.DIST" => Statistics(StatisticsFunction::NormSDist), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NORMSDIST" => Statistics(StatisticsFunction::NormSDistLegacy), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "STANDARDIZE" => Statistics(StatisticsFunction::Standardize), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "POISSON" => Statistics(StatisticsFunction::Poisson), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "POISSON.DIST" => Statistics(StatisticsFunction::PoissonDist), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "TDIST" => Statistics(StatisticsFunction::TDistLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "T.DIST" => Statistics(StatisticsFunction::TDist), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "T.DIST.RT" => Statistics(StatisticsFunction::TDistRt), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "T.DIST.2T" => Statistics(StatisticsFunction::TDist2T), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NUMBERVALUE" => Text(TextFunction::NumberValue), 1, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "PROPER" => Text(TextFunction::Proper), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "ROMAN" => Text(TextFunction::Roman), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "TEXTJOIN" => Text(TextFunction::TextJoin), 3, 254, FamilyControlled, REFERENCE_READS, Scalar;
    "FV" => Financial(FinancialFunction::Fv), 3, 5, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "PV" => Financial(FinancialFunction::Pv), 3, 5, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "PMT" => Financial(FinancialFunction::Pmt), 3, 5, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "NPER" => Financial(FinancialFunction::Nper), 3, 5, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "IPMT" => Financial(FinancialFunction::Ipmt), 4, 6, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "PPMT" => Financial(FinancialFunction::Ppmt), 4, 6, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "RATE" => Financial(FinancialFunction::Rate), 3, 6, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "NPV" => Financial(FinancialFunction::Npv), 2, 255, FamilyControlled, REFERENCE_READS, FirstNumericArgument;
    "IRR" => Financial(FinancialFunction::Irr), 1, 2, FamilyControlled, REFERENCE_READS, SecondNumericArgument;
    "MIRR" => Financial(FinancialFunction::Mirr), 3, 3, FamilyControlled, REFERENCE_READS, AfterFirstNumericArguments;
    "DOLLAR" => Financial(FinancialFunction::Dollar), 1, 2, FamilyControlled, CURRENCY_FORMATTING, ElementwiseText;
    "DOLLARDE" => Financial(FinancialFunction::DollarDe), 2, 2, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "DOLLARFR" => Financial(FinancialFunction::DollarFr), 2, 2, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "FIXED" => Financial(FinancialFunction::Fixed), 1, 3, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "BESSELJ" => Engineering(EngineeringFunction::Besselj), 2, 2, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "BIN2DEC" => Engineering(EngineeringFunction::Bin2dec), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "COMPLEX" => Engineering(EngineeringFunction::Complex), 2, 3, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "DEC2BIN" => Engineering(EngineeringFunction::Dec2bin), 1, 2, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "DEC2HEX" => Engineering(EngineeringFunction::Dec2hex), 1, 2, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "DELTA" => Engineering(EngineeringFunction::Delta), 1, 2, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "HEX2DEC" => Engineering(EngineeringFunction::Hex2dec), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "IMAGINARY" => Engineering(EngineeringFunction::Imaginary), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "IMREAL" => Engineering(EngineeringFunction::Imreal), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "OCT2DEC" => Engineering(EngineeringFunction::Oct2dec), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "MDETERM" => Matrix(MatrixFunction::Mdeterm), 1, 1, FamilyControlled, REFERENCE_READS, Number;
    "MINVERSE" => Matrix(MatrixFunction::Minverse), 1, 1, FamilyControlled, REFERENCE_READS, NumericInputBounds;
    "MMULT" => Matrix(MatrixFunction::Mmult), 2, 2, FamilyControlled, REFERENCE_READS, MatrixProduct;
    "TRANSPOSE" => Matrix(MatrixFunction::Transpose), 1, 1, FamilyControlled, REFERENCE_READS, Transpose;
    "FREQUENCY" => Matrix(MatrixFunction::Frequency), 2, 2, FamilyControlled, NUMERIC_COERCION, Array;
    "SUMPRODUCT" => Matrix(MatrixFunction::Sumproduct), 1, 255, FamilyControlled, REFERENCE_READS, Number;
    "TREND" => Matrix(MatrixFunction::Trend), 1, 4, FamilyControlled, REFERENCE_READS, Array;
    "COUNTIF" => Criteria(CriteriaFunction::CountIf), 2, 2, FamilyControlled, REFERENCE_READS, SecondNumericArgument;
    "SUMIF" => Criteria(CriteriaFunction::SumIf), 2, 3, FamilyControlled, REFERENCE_READS, SecondNumericArgument;
    "AVERAGEIF" => Criteria(CriteriaFunction::AverageIf), 2, 3, FamilyControlled, REFERENCE_READS, SecondNumericArgument;
    "COUNTIFS" => Criteria(CriteriaFunction::CountIfs), 2, usize::MAX, FamilyControlled, REFERENCE_READS, PairedCriteria;
    "SUMIFS" => Criteria(CriteriaFunction::SumIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, PayloadCriteria;
    "AVERAGEIFS" => Criteria(CriteriaFunction::AverageIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, PayloadCriteria;
    "MAXIFS" => Criteria(CriteriaFunction::MaxIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, PayloadCriteria;
    "MINIFS" => Criteria(CriteriaFunction::MinIfs), 3, usize::MAX, FamilyControlled, REFERENCE_READS, PayloadCriteria;
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
    "DATE" => Date(DateFunction::Date), 3, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DATEDIF" => Date(DateFunction::DateDif), 3, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DATEVALUE" => Date(DateFunction::DateValue), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DAY" => Date(DateFunction::Day), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DAYS" => Date(DateFunction::Days), 2, 2, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DAYS360" => Date(DateFunction::Days360), 2, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "EDATE" => Date(DateFunction::EDate), 2, 2, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "EOMONTH" => Date(DateFunction::EOMonth), 2, 2, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "HOUR" => Date(DateFunction::Hour), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "MINUTE" => Date(DateFunction::Minute), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "MONTH" => Date(DateFunction::Month), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "NETWORKDAYS" => Date(DateFunction::NetworkDays), 2, 3, FamilyControlled, DATE_FAMILY, FirstTwoNumericArguments;
    "NOW" => Date(DateFunction::Now), 0, 0, FamilyControlled, TIME_READS, Number;
    "SECOND" => Date(DateFunction::Second), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TIME" => Date(DateFunction::Time), 3, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TIMEVALUE" => Date(DateFunction::TimeValue), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TODAY" => Date(DateFunction::Today), 0, 0, FamilyControlled, TIME_READS, Number;
    "WEEKDAY" => Date(DateFunction::WeekDay), 1, 2, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "WEEKNUM" => Date(DateFunction::WeekNum), 1, 2, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "WORKDAY" => Date(DateFunction::WorkDay), 2, 3, FamilyControlled, DATE_FAMILY, FirstTwoNumericArguments;
    "WORKDAY.INTL" => Date(DateFunction::WorkDayIntl), 2, 4, FamilyControlled, DATE_FAMILY, FirstThreeNumericArguments;
    "YEAR" => Date(DateFunction::Year), 1, 1, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "YEARFRAC" => Date(DateFunction::YearFrac), 2, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TEXT" => Text(TextFunction::Text), 2, 2, FamilyControlled, TEXT_FORMATTING, ElementwiseText;
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
    "NOT" => Logical(LogicalFunction::Not), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseBoolean;
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
    "TYPE" => Information(InformationFunction::Type), 1, 1, FamilyControlled, ARGUMENTS, Number;
    "N" => Information(InformationFunction::N), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseNumber;
    "T" => Information(InformationFunction::T), 1, 1, FamilyControlled, ARGUMENTS, ElementwiseText;
    "TRUE" => Logical(LogicalFunction::True), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "FALSE" => Logical(LogicalFunction::False), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "NA" => Information(InformationFunction::Na), 0, 0, FamilyControlled, ARGUMENTS, Scalar;
    "PI" => Math(MathFunction::Pi), 0, 0, FamilyControlled, ARGUMENTS, Number;
    "ABS" => Math(MathFunction::Abs), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SIGN" => Math(MathFunction::Sign), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SQRT" => Math(MathFunction::Sqrt), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "EXP" => Math(MathFunction::Exp), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LN" => Math(MathFunction::Ln), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LOG10" => Math(MathFunction::Log10), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LOG" => Math(MathFunction::Log), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "POWER" => Math(MathFunction::Power), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "MOD" => Math(MathFunction::Mod), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "INT" => Math(MathFunction::Int), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "TRUNC" => Math(MathFunction::Trunc), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ROUND" => Math(MathFunction::Round), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ROUNDUP" => Math(MathFunction::RoundUp), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ROUNDDOWN" => Math(MathFunction::RoundDown), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CEILING" => Math(MathFunction::Ceiling), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FLOOR" => Math(MathFunction::Floor), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SIN" => Math(MathFunction::Sin), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "COS" => Math(MathFunction::Cos), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "TAN" => Math(MathFunction::Tan), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ASIN" => Math(MathFunction::Asin), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ACOS" => Math(MathFunction::Acos), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ATAN" => Math(MathFunction::Atan), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "DEGREES" => Math(MathFunction::Degrees), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "RADIANS" => Math(MathFunction::Radians), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LEN" => Text(TextFunction::Len), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "LEFT" => Text(TextFunction::Left), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "RIGHT" => Text(TextFunction::Right), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "MID" => Text(TextFunction::Mid), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "LOWER" => Text(TextFunction::Lower), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "UPPER" => Text(TextFunction::Upper), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "TRIM" => Text(TextFunction::Trim), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "CLEAN" => Text(TextFunction::Clean), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "CONCATENATE" => Text(TextFunction::Concatenate), 1, 255, FamilyControlled, REFERENCE_READS, ElementwiseText;
    "CONCAT" => Text(TextFunction::Concat), 1, 255, FamilyControlled, REFERENCE_READS, Scalar;
    "EXACT" => Text(TextFunction::Exact), 2, 2, FamilyControlled, REFERENCE_READS, ElementwiseBoolean;
    "FIND" => Text(TextFunction::Find), 2, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SEARCH" => Text(TextFunction::Search), 2, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "REPLACE" => Text(TextFunction::Replace), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "SUBSTITUTE" => Text(TextFunction::Substitute), 3, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "REPT" => Text(TextFunction::Rept), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "VALUE" => Text(TextFunction::Value), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHAR" => Text(TextFunction::Char), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "CODE" => Text(TextFunction::Code), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "UNICHAR" => Text(TextFunction::UniChar), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "UNICODE" => Text(TextFunction::Unicode), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    "ARABIC" => Text(TextFunction::Arabic), 1, 1, FamilyControlled, REFERENCE_READS, ElementwiseNumber;
    }
    excel {
    "GROWTH" => Matrix(MatrixFunction::Growth), 1, 4, FamilyControlled, REFERENCE_READS, Array;

    "FORECAST.ETS" => Statistics(StatisticsFunction::ForecastEts), 3, 6, FamilyControlled, FORECAST_READS, ForecastArguments;
    "FORECAST.ETS.CONFINT" => Statistics(StatisticsFunction::ForecastEtsConfint), 3, 7, FamilyControlled, FORECAST_READS, ForecastArguments;
    "FORECAST.ETS.SEASONALITY" => Statistics(StatisticsFunction::ForecastEtsSeasonality), 2, 4, FamilyControlled, FORECAST_READS, AfterSecondNumericArguments;
    "FORECAST.ETS.STAT" => Statistics(StatisticsFunction::ForecastEtsStat), 3, 6, FamilyControlled, FORECAST_READS, AfterSecondNumericArguments;

    "LINEST" => Matrix(MatrixFunction::Linest), 1, 4, FamilyControlled, REFERENCE_READS, Array;
    "LOGEST" => Matrix(MatrixFunction::Logest), 1, 4, FamilyControlled, REFERENCE_READS, Array;
    "CHISQ.TEST" => Statistics(StatisticsFunction::ChisqTest), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "F.TEST" => Statistics(StatisticsFunction::FTest), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "FISHER" => Statistics(StatisticsFunction::Fisher), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FISHERINV" => Statistics(StatisticsFunction::FisherInv), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "HARMEAN" => Statistics(StatisticsFunction::HarMean), 1, 255, FamilyControlled, NUMERIC_COERCION, Number;
    "KURT" => Statistics(StatisticsFunction::Kurt), 1, 255, FamilyControlled, NUMERIC_COERCION, Number;
    "MODE.MULT" => Statistics(StatisticsFunction::ModeMult), 1, 254, FamilyControlled, NUMERIC_COERCION, Array;
    "MODE.SNGL" => Statistics(StatisticsFunction::ModeSngl), 1, 254, FamilyControlled, NUMERIC_COERCION, Number;
    "PERCENTILE.EXC" => Statistics(StatisticsFunction::PercentileExc), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "PERMUT" => Statistics(StatisticsFunction::Permut), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "PERMUTATIONA" => Statistics(StatisticsFunction::PermutationA), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "PROB" => Statistics(StatisticsFunction::Prob), 2, 4, FamilyControlled, NUMERIC_COERCION, AfterSecondNumericArguments;
    "QUARTILE.EXC" => Statistics(StatisticsFunction::QuartileExc), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "RANK.AVG" => Statistics(StatisticsFunction::RankAvg), 2, 3, FamilyControlled, NUMERIC_COERCION, RankArguments;
    "RSQ" => Statistics(StatisticsFunction::RSq), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "SKEW" => Statistics(StatisticsFunction::Skew), 1, 255, FamilyControlled, NUMERIC_COERCION, Number;
    "SKEW.P" => Statistics(StatisticsFunction::SkewP), 1, 254, FamilyControlled, NUMERIC_COERCION, Number;
    "STEYX" => Statistics(StatisticsFunction::Steyx), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "T.TEST" => Statistics(StatisticsFunction::TTest), 4, 4, FamilyControlled, NUMERIC_COERCION, AfterSecondNumericArguments;
    "TRIMMEAN" => Statistics(StatisticsFunction::TrimMean), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "Z.TEST" => Statistics(StatisticsFunction::ZTest), 2, 3, FamilyControlled, NUMERIC_COERCION, AfterFirstNumericArguments;
    "BETA.INV" => Statistics(StatisticsFunction::BetaInv), 3, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BINOM.INV" => Statistics(StatisticsFunction::BinomInv), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHISQ.INV" => Statistics(StatisticsFunction::ChisqInv), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHISQ.INV.RT" => Statistics(StatisticsFunction::ChisqInvRt), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CONFIDENCE.NORM" => Statistics(StatisticsFunction::ConfidenceNorm), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CONFIDENCE.T" => Statistics(StatisticsFunction::ConfidenceT), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "F.INV" => Statistics(StatisticsFunction::FInv), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "F.INV.RT" => Statistics(StatisticsFunction::FInvRt), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAMMA.INV" => Statistics(StatisticsFunction::GammaInv), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LOGNORM.INV" => Statistics(StatisticsFunction::LognormInv), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "T.INV" => Statistics(StatisticsFunction::TInv), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "T.INV.2T" => Statistics(StatisticsFunction::TInv2T), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BETA.DIST" => Statistics(StatisticsFunction::BetaDist), 4, 6, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BINOM.DIST" => Statistics(StatisticsFunction::BinomDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BINOM.DIST.RANGE" => Statistics(StatisticsFunction::BinomDistRange), 3, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHISQ.DIST" => Statistics(StatisticsFunction::ChisqDist), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHISQ.DIST.RT" => Statistics(StatisticsFunction::ChisqDistRt), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "EXPON.DIST" => Statistics(StatisticsFunction::ExponDist), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "F.DIST" => Statistics(StatisticsFunction::FDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "F.DIST.RT" => Statistics(StatisticsFunction::FDistRt), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAMMA" => Statistics(StatisticsFunction::Gamma), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAMMA.DIST" => Statistics(StatisticsFunction::GammaDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAMMALN" => Statistics(StatisticsFunction::Gammaln), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAMMALN.PRECISE" => Statistics(StatisticsFunction::GammalnPrecise), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAUSS" => Statistics(StatisticsFunction::Gauss), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "HYPGEOM.DIST" => Statistics(StatisticsFunction::HypgeomDist), 5, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LOGNORM.DIST" => Statistics(StatisticsFunction::LognormDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NEGBINOM.DIST" => Statistics(StatisticsFunction::NegbinomDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "PHI" => Statistics(StatisticsFunction::Phi), 1, 1, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "WEIBULL.DIST" => Statistics(StatisticsFunction::WeibullDist), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;

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
    "NETWORKDAYS.INTL" => Date(DateFunction::NetworkDaysIntl), 2, 4, FamilyControlled, DATE_FAMILY, FirstThreeNumericArguments;
    "MUNIT" => Matrix(MatrixFunction::Munit), 1, 1, FamilyControlled, NUMERIC_COERCION, UnitMatrix;
    "CUMIPMT" => Financial(FinancialFunction::Cumipmt), 6, 6, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CUMPRINC" => Financial(FinancialFunction::Cumprinc), 6, 6, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "DB" => Financial(FinancialFunction::Db), 4, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "DDB" => Financial(FinancialFunction::Ddb), 4, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "EFFECT" => Financial(FinancialFunction::Effect), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FVSCHEDULE" => Financial(FinancialFunction::Fvschedule), 2, 2, FamilyControlled, NUMERIC_COERCION, FirstNumericArgument;
    "ISPMT" => Financial(FinancialFunction::Ispmt), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NOMINAL" => Financial(FinancialFunction::Nominal), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "PDURATION" => Financial(FinancialFunction::Pduration), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "RRI" => Financial(FinancialFunction::Rri), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SLN" => Financial(FinancialFunction::Sln), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "SYD" => Financial(FinancialFunction::Syd), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "VDB" => Financial(FinancialFunction::Vdb), 5, 7, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "XIRR" => Financial(FinancialFunction::Xirr), 2, 3, FamilyControlled, DATE_FAMILY, ThirdNumericArgument;
    "XNPV" => Financial(FinancialFunction::Xnpv), 3, 3, FamilyControlled, DATE_FAMILY, FirstNumericArgument;
    "ACCRINT" => Financial(FinancialFunction::Accrint), 6, 8, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "ACCRINTM" => Financial(FinancialFunction::Accrintm), 3, 5, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "COUPDAYBS" => Financial(FinancialFunction::Coupdaybs), 3, 4, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "COUPDAYS" => Financial(FinancialFunction::Coupdays), 3, 4, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "COUPDAYSNC" => Financial(FinancialFunction::Coupdaysnc), 3, 4, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "COUPNCD" => Financial(FinancialFunction::Coupncd), 3, 4, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "COUPNUM" => Financial(FinancialFunction::Coupnum), 3, 4, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "COUPPCD" => Financial(FinancialFunction::Couppcd), 3, 4, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DISC" => Financial(FinancialFunction::Disc), 4, 5, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "DURATION" => Financial(FinancialFunction::Duration), 5, 6, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "INTRATE" => Financial(FinancialFunction::Intrate), 4, 5, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "MDURATION" => Financial(FinancialFunction::Mduration), 5, 6, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "PRICE" => Financial(FinancialFunction::Price), 6, 7, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "PRICEDISC" => Financial(FinancialFunction::Pricedisc), 4, 5, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "PRICEMAT" => Financial(FinancialFunction::Pricemat), 5, 6, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "RECEIVED" => Financial(FinancialFunction::Received), 4, 5, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TBILLEQ" => Financial(FinancialFunction::Tbilleq), 3, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TBILLPRICE" => Financial(FinancialFunction::Tbillprice), 3, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "TBILLYIELD" => Financial(FinancialFunction::Tbillyield), 3, 3, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "YIELD" => Financial(FinancialFunction::Yield), 6, 7, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "YIELDDISC" => Financial(FinancialFunction::Yielddisc), 4, 5, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "YIELDMAT" => Financial(FinancialFunction::Yieldmat), 5, 6, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "AMORDEGRC" => Financial(FinancialFunction::Amordegrc), 6, 7, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "AMORLINC" => Financial(FinancialFunction::Amorlinc), 6, 7, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "ODDFPRICE" => Financial(FinancialFunction::Oddfprice), 8, 9, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "ODDFYIELD" => Financial(FinancialFunction::Oddfyield), 8, 9, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "ODDLPRICE" => Financial(FinancialFunction::Oddlprice), 7, 8, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "ODDLYIELD" => Financial(FinancialFunction::Oddlyield), 7, 8, FamilyControlled, DATE_FAMILY, ElementwiseNumber;
    "EUROCONVERT" => Financial(FinancialFunction::Euroconvert), 3, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "XOR" => Logical(LogicalFunction::Xor), 1, 254, FamilyControlled, REFERENCE_READS, Scalar;
    "GROUPBY" => Matrix(MatrixFunction::Groupby), 3, 8, FamilyControlled, NUMERIC_COERCION, Array;
    "PERCENTOF" => Matrix(MatrixFunction::Percentof), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "REGEXTEST" => Text(TextFunction::RegexTest), 2, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseBoolean;
    "REGEXEXTRACT" => Text(TextFunction::RegexExtract), 2, 4, FamilyControlled, NUMERIC_COERCION, RegexExtract;
    "REGEXREPLACE" => Text(TextFunction::RegexReplace), 3, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "TEXTBEFORE" => Text(TextFunction::TextBefore), 2, 6, FamilyControlled, NUMERIC_COERCION, TextWithFallback;
    "TEXTAFTER" => Text(TextFunction::TextAfter), 2, 6, FamilyControlled, NUMERIC_COERCION, TextWithFallback;
    "TEXTSPLIT" => Text(TextFunction::TextSplit), 2, 6, FamilyControlled, NUMERIC_COERCION, Array;
    "ARRAYTOTEXT" => Text(TextFunction::ArrayToText), 1, 2, FamilyControlled, NUMERIC_COERCION, Scalar;
    "VALUETOTEXT" => Text(TextFunction::ValueToText), 1, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseText;
    "HSTACK" => Matrix(MatrixFunction::Hstack), 1, 255, FamilyControlled, REFERENCE_READS, StackColumns;
    "VSTACK" => Matrix(MatrixFunction::Vstack), 1, 255, FamilyControlled, REFERENCE_READS, StackRows;
    "TAKE" => Matrix(MatrixFunction::Take), 2, 3, FamilyControlled, NUMERIC_COERCION, InputBounds;
    "DROP" => Matrix(MatrixFunction::Drop), 2, 3, FamilyControlled, NUMERIC_COERCION, InputBounds;
    "CHOOSECOLS" => Matrix(MatrixFunction::Choosecols), 2, 255, FamilyControlled, NUMERIC_COERCION, SelectColumns;
    "CHOOSEROWS" => Matrix(MatrixFunction::Chooserows), 2, 255, FamilyControlled, NUMERIC_COERCION, SelectRows;
    "EXPAND" => Matrix(MatrixFunction::Expand), 2, 4, FamilyControlled, NUMERIC_COERCION, Array;
    "FILTER" => Matrix(MatrixFunction::Filter), 2, 3, FamilyControlled, REFERENCE_READS, Filter;
    "SORT" => Matrix(MatrixFunction::Sort), 1, 4, FamilyControlled, NUMERIC_COERCION, InputBounds;
    "SORTBY" => Matrix(MatrixFunction::Sortby), 2, 255, FamilyControlled, NUMERIC_COERCION, InputBounds;
    "UNIQUE" => Matrix(MatrixFunction::Unique), 1, 3, FamilyControlled, REFERENCE_READS, InputBounds;
    "TOCOL" => Matrix(MatrixFunction::Tocol), 1, 3, FamilyControlled, NUMERIC_COERCION, FlattenColumns;
    "TOROW" => Matrix(MatrixFunction::Torow), 1, 3, FamilyControlled, NUMERIC_COERCION, FlattenRows;
    "WRAPCOLS" => Matrix(MatrixFunction::Wrapcols), 2, 3, FamilyControlled, NUMERIC_COERCION, Array;
    "WRAPROWS" => Matrix(MatrixFunction::Wraprows), 2, 3, FamilyControlled, NUMERIC_COERCION, Array;
    "RANDARRAY" => Matrix(MatrixFunction::Randarray), 0, 5, FamilyControlled, RANDOM_NUMERIC, RandomArray;
    "TRIMRANGE" => Matrix(MatrixFunction::Trimrange), 1, 3, FamilyControlled, NUMERIC_COERCION, TrimRange;
    "SEQUENCE" => Matrix(MatrixFunction::Sequence), 1, 4, FamilyControlled, NUMERIC_COERCION, Sequence;
    "ANCHORARRAY" => Reference(ReferenceFunction::AnchorArray), 1, 1, FamilyControlled, REFERENCE_READS, SpillRange;

    "PERCENTILE.INC" => Statistics(StatisticsFunction::Percentile), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "QUARTILE.INC" => Statistics(StatisticsFunction::QuartileInc), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "RANK.EQ" => Statistics(StatisticsFunction::RankEq), 2, 3, FamilyControlled, NUMERIC_COERCION, RankArguments;
    "BETADIST" => Statistics(StatisticsFunction::BetaDistLegacy), 3, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BETAINV" => Statistics(StatisticsFunction::BetaInvLegacy), 3, 5, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "BINOMDIST" => Statistics(StatisticsFunction::BinomDistLegacy), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHIDIST" => Statistics(StatisticsFunction::ChisqDistRtLegacy), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHIINV" => Statistics(StatisticsFunction::ChisqInvRtLegacy), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CHITEST" => Statistics(StatisticsFunction::ChisqTestLegacy), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "CONFIDENCE" => Statistics(StatisticsFunction::ConfidenceNormLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "CRITBINOM" => Statistics(StatisticsFunction::BinomInvLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "EXPONDIST" => Statistics(StatisticsFunction::ExponDistLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FDIST" => Statistics(StatisticsFunction::FDistRtLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FINV" => Statistics(StatisticsFunction::FInvRtLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "FTEST" => Statistics(StatisticsFunction::FTestLegacy), 2, 2, FamilyControlled, NUMERIC_COERCION, Number;
    "GAMMADIST" => Statistics(StatisticsFunction::GammaDistLegacy), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "GAMMAINV" => Statistics(StatisticsFunction::GammaInvLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "HYPGEOMDIST" => Statistics(StatisticsFunction::HypgeomDistLegacy), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LOGINV" => Statistics(StatisticsFunction::LognormInvLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "LOGNORMDIST" => Statistics(StatisticsFunction::LognormDistLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "NEGBINOMDIST" => Statistics(StatisticsFunction::NegbinomDistLegacy), 3, 3, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "QUARTILE" => Statistics(StatisticsFunction::QuartileIncLegacy), 2, 2, FamilyControlled, NUMERIC_COERCION, SecondNumericArgument;
    "TINV" => Statistics(StatisticsFunction::TInv2TLegacy), 2, 2, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "TTEST" => Statistics(StatisticsFunction::TTestLegacy), 4, 4, FamilyControlled, NUMERIC_COERCION, AfterSecondNumericArguments;
    "WEIBULL" => Statistics(StatisticsFunction::WeibullDistLegacy), 4, 4, FamilyControlled, NUMERIC_COERCION, ElementwiseNumber;
    "ZTEST" => Statistics(StatisticsFunction::ZTestLegacy), 2, 3, FamilyControlled, NUMERIC_COERCION, AfterFirstNumericArguments;
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
