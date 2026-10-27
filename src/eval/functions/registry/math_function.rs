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
