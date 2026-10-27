//! Workbook-independent function symbols. Numeric slots are append-only.
//! Unknown spellings remain owned data for extension/namespace resolution.
use std::{fmt, ops::Deref};

macro_rules! common_functions {
    ($( $variant:ident = $index:literal => $name:literal; )*) => {
        /// Interned common spellings for compact compiler and registry lookup.
        /// The evaluator registry determines whether a call is supported.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u16)]
        pub enum CommonFunction {
            $(
                #[doc = concat!("Interned `", $name, "` spelling for compiler and registry lookup; membership does not imply support.")]
                $variant = $index,
            )*
        }
        impl CommonFunction {
            /// Number of dense interned builtin slots.
            pub const COUNT: usize = common_functions!(@count $( $variant )*);
            /// Interned identities in stable slot order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];
            /// Return the stable dense slot used by compiler and registry tables.
            pub const fn index(self) -> usize { self as usize }
            /// Borrow this builtin's canonical spelling without allocating.
            pub const fn as_str(self) -> &'static str { match self { $(Self::$variant => $name),* } }
            fn from_canonical(name: &str) -> Option<Self> {
                match name { $( $name => Some(Self::$variant), )* _ => None }
            }
        }
    };
    (@count $($variant:ident)*) => { <[()]>::len(&[$(common_functions!(@one $variant)),*]) };
    (@one $variant:ident) => { () };
}

common_functions! {
    Rand = 0 => "RAND";
    Randbetween = 1 => "RANDBETWEEN";
    ErrorType = 2 => "ERROR.TYPE";
    Ifs = 3 => "IFS";
    IsEven = 4 => "ISEVEN";
    IsOdd = 5 => "ISODD";
    Switch = 6 => "SWITCH";
    Lookup = 7 => "LOOKUP";
    XLookup = 8 => "XLOOKUP";
    XMatch = 9 => "XMATCH";
    Address = 10 => "ADDRESS";
    Areas = 11 => "AREAS";
    Hyperlink = 12 => "HYPERLINK";
    Indirect = 13 => "INDIRECT";
    Offset = 14 => "OFFSET";
    Sheet = 15 => "SHEET";
    Single = 16 => "SINGLE";
    Acosh = 17 => "ACOSH";
    Asinh = 18 => "ASINH";
    Atan2 = 19 => "ATAN2";
    Atanh = 20 => "ATANH";
    CeilingMath = 21 => "CEILING.MATH";
    CeilingPrecise = 22 => "CEILING.PRECISE";
    Combin = 23 => "COMBIN";
    Cosh = 24 => "COSH";
    Even = 25 => "EVEN";
    Fact = 26 => "FACT";
    Factdouble = 27 => "FACTDOUBLE";
    FloorMath = 28 => "FLOOR.MATH";
    FloorPrecise = 29 => "FLOOR.PRECISE";
    Gcd = 30 => "GCD";
    Lcm = 31 => "LCM";
    Mround = 32 => "MROUND";
    Odd = 33 => "ODD";
    Quotient = 34 => "QUOTIENT";
    Sinh = 35 => "SINH";
    Sqrtpi = 36 => "SQRTPI";
    Tanh = 37 => "TANH";
    AveDev = 38 => "AVEDEV";
    AverageA = 39 => "AVERAGEA";
    DevSq = 40 => "DEVSQ";
    GeoMean = 41 => "GEOMEAN";
    MaxA = 42 => "MAXA";
    MinA = 43 => "MINA";
    StdevA = 44 => "STDEVA";
    StdevPA = 45 => "STDEVPA";
    VarA = 46 => "VARA";
    VarPA = 47 => "VARPA";
    Mode = 48 => "MODE";
    Large = 49 => "LARGE";
    Small = 50 => "SMALL";
    Percentile = 51 => "PERCENTILE";
    Rank = 52 => "RANK";
    PercentRank = 53 => "PERCENTRANK";
    PercentRankInc = 54 => "PERCENTRANK.INC";
    PercentRankExc = 55 => "PERCENTRANK.EXC";
    Correl = 56 => "CORREL";
    Pearson = 57 => "PEARSON";
    Covar = 58 => "COVAR";
    CovarianceP = 59 => "COVARIANCE.P";
    CovarianceS = 60 => "COVARIANCE.S";
    Forecast = 61 => "FORECAST";
    ForecastLinear = 62 => "FORECAST.LINEAR";
    Intercept = 63 => "INTERCEPT";
    Slope = 64 => "SLOPE";
    SumX2MY2 = 65 => "SUMX2MY2";
    SumX2PY2 = 66 => "SUMX2PY2";
    SumXMY2 = 67 => "SUMXMY2";
    NormDist = 68 => "NORM.DIST";
    Normdist = 69 => "NORMDIST";
    NormInv = 70 => "NORM.INV";
    Norminv = 71 => "NORMINV";
    NormSInv = 72 => "NORM.S.INV";
    Normsinv = 73 => "NORMSINV";
    NormSDist = 74 => "NORM.S.DIST";
    Normsdist = 75 => "NORMSDIST";
    Standardize = 76 => "STANDARDIZE";
    Poisson = 77 => "POISSON";
    PoissonDist = 78 => "POISSON.DIST";
    Tdist = 79 => "TDIST";
    TDist = 80 => "T.DIST";
    TDistRt = 81 => "T.DIST.RT";
    TDist2T = 82 => "T.DIST.2T";
    NumberValue = 83 => "NUMBERVALUE";
    Proper = 84 => "PROPER";
    Roman = 85 => "ROMAN";
    TextJoin = 86 => "TEXTJOIN";
    Fv = 87 => "FV";
    Pv = 88 => "PV";
    Pmt = 89 => "PMT";
    Nper = 90 => "NPER";
    Ipmt = 91 => "IPMT";
    Ppmt = 92 => "PPMT";
    Rate = 93 => "RATE";
    Npv = 94 => "NPV";
    Irr = 95 => "IRR";
    Mirr = 96 => "MIRR";
    Dollar = 97 => "DOLLAR";
    DollarDe = 98 => "DOLLARDE";
    DollarFr = 99 => "DOLLARFR";
    Fixed = 100 => "FIXED";
    Besselj = 101 => "BESSELJ";
    Bin2dec = 102 => "BIN2DEC";
    Complex = 103 => "COMPLEX";
    Dec2bin = 104 => "DEC2BIN";
    Dec2hex = 105 => "DEC2HEX";
    Delta = 106 => "DELTA";
    Hex2dec = 107 => "HEX2DEC";
    Imaginary = 108 => "IMAGINARY";
    Imreal = 109 => "IMREAL";
    Oct2dec = 110 => "OCT2DEC";
    Mdeterm = 111 => "MDETERM";
    Minverse = 112 => "MINVERSE";
    Mmult = 113 => "MMULT";
    Transpose = 114 => "TRANSPOSE";
    Frequency = 115 => "FREQUENCY";
    Sumproduct = 116 => "SUMPRODUCT";
    Trend = 117 => "TREND";
    CountIf = 118 => "COUNTIF";
    SumIf = 119 => "SUMIF";
    AverageIf = 120 => "AVERAGEIF";
    CountIfs = 121 => "COUNTIFS";
    SumIfs = 122 => "SUMIFS";
    AverageIfs = 123 => "AVERAGEIFS";
    MaxIfs = 124 => "MAXIFS";
    MinIfs = 125 => "MINIFS";
    Subtotal = 126 => "SUBTOTAL";
    Daverage = 127 => "DAVERAGE";
    Dcount = 128 => "DCOUNT";
    Dcounta = 129 => "DCOUNTA";
    Dget = 130 => "DGET";
    Dmax = 131 => "DMAX";
    Dmin = 132 => "DMIN";
    Dproduct = 133 => "DPRODUCT";
    Dstdev = 134 => "DSTDEV";
    Dstdevp = 135 => "DSTDEVP";
    Dsum = 136 => "DSUM";
    Dvar = 137 => "DVAR";
    Dvarp = 138 => "DVARP";
    Date = 139 => "DATE";
    DateValue = 140 => "DATEVALUE";
    Day = 141 => "DAY";
    Days = 142 => "DAYS";
    Days360 = 143 => "DAYS360";
    EDate = 144 => "EDATE";
    EOMonth = 145 => "EOMONTH";
    Hour = 146 => "HOUR";
    Minute = 147 => "MINUTE";
    Month = 148 => "MONTH";
    NetworkDays = 149 => "NETWORKDAYS";
    Now = 150 => "NOW";
    Second = 151 => "SECOND";
    Time = 152 => "TIME";
    TimeValue = 153 => "TIMEVALUE";
    Today = 154 => "TODAY";
    WeekDay = 155 => "WEEKDAY";
    WeekNum = 156 => "WEEKNUM";
    WorkDay = 157 => "WORKDAY";
    WorkDayIntl = 158 => "WORKDAY.INTL";
    Year = 159 => "YEAR";
    YearFrac = 160 => "YEARFRAC";
    Text = 161 => "TEXT";
    If = 162 => "IF";
    IsRef = 163 => "ISREF";
    IfError = 164 => "IFERROR";
    IfNa = 165 => "IFNA";
    Choose = 166 => "CHOOSE";
    Sum = 167 => "SUM";
    Average = 168 => "AVERAGE";
    Count = 169 => "COUNT";
    CountA = 170 => "COUNTA";
    CountBlank = 171 => "COUNTBLANK";
    Min = 172 => "MIN";
    Max = 173 => "MAX";
    Product = 174 => "PRODUCT";
    And = 175 => "AND";
    Or = 176 => "OR";
    Not = 177 => "NOT";
    SumSq = 178 => "SUMSQ";
    Stdev = 179 => "STDEV";
    StdevS = 180 => "STDEV.S";
    StdevP = 181 => "STDEV.P";
    Stdevp = 182 => "STDEVP";
    Var = 183 => "VAR";
    VarS = 184 => "VAR.S";
    VarP = 185 => "VAR.P";
    Varp = 186 => "VARP";
    Median = 187 => "MEDIAN";
    Rows = 188 => "ROWS";
    Columns = 189 => "COLUMNS";
    Row = 190 => "ROW";
    Column = 191 => "COLUMN";
    Index = 192 => "INDEX";
    Match = 193 => "MATCH";
    VLookup = 194 => "VLOOKUP";
    HLookup = 195 => "HLOOKUP";
    IsError = 196 => "ISERROR";
    IsErr = 197 => "ISERR";
    IsNa = 198 => "ISNA";
    IsNumber = 199 => "ISNUMBER";
    IsText = 200 => "ISTEXT";
    IsBlank = 201 => "ISBLANK";
    IsLogical = 202 => "ISLOGICAL";
    IsNonText = 203 => "ISNONTEXT";
    Type = 204 => "TYPE";
    N = 205 => "N";
    T = 206 => "T";
    True = 207 => "TRUE";
    False = 208 => "FALSE";
    Na = 209 => "NA";
    Pi = 210 => "PI";
    Abs = 211 => "ABS";
    Sign = 212 => "SIGN";
    Sqrt = 213 => "SQRT";
    Exp = 214 => "EXP";
    Ln = 215 => "LN";
    Log10 = 216 => "LOG10";
    Log = 217 => "LOG";
    Power = 218 => "POWER";
    Mod = 219 => "MOD";
    Int = 220 => "INT";
    Trunc = 221 => "TRUNC";
    Round = 222 => "ROUND";
    RoundUp = 223 => "ROUNDUP";
    RoundDown = 224 => "ROUNDDOWN";
    Ceiling = 225 => "CEILING";
    Floor = 226 => "FLOOR";
    Sin = 227 => "SIN";
    Cos = 228 => "COS";
    Tan = 229 => "TAN";
    Asin = 230 => "ASIN";
    Acos = 231 => "ACOS";
    Atan = 232 => "ATAN";
    Degrees = 233 => "DEGREES";
    Radians = 234 => "RADIANS";
    Len = 235 => "LEN";
    Left = 236 => "LEFT";
    Right = 237 => "RIGHT";
    Mid = 238 => "MID";
    Lower = 239 => "LOWER";
    Upper = 240 => "UPPER";
    Trim = 241 => "TRIM";
    Clean = 242 => "CLEAN";
    Concatenate = 243 => "CONCATENATE";
    Concat = 244 => "CONCAT";
    Exact = 245 => "EXACT";
    Find = 246 => "FIND";
    Search = 247 => "SEARCH";
    Replace = 248 => "REPLACE";
    Substitute = 249 => "SUBSTITUTE";
    Rept = 250 => "REPT";
    Value = 251 => "VALUE";
    Char = 252 => "CHAR";
    Code = 253 => "CODE";
    UniChar = 254 => "UNICHAR";
    Unicode = 255 => "UNICODE";
    QuartileInc = 256 => "QUARTILE.INC";
    RankEq = 257 => "RANK.EQ";
    PercentileInc = 258 => "PERCENTILE.INC";
    DateDif = 259 => "DATEDIF";
    Base = 260 => "BASE";
    Decimal = 261 => "DECIMAL";
    Arabic = 262 => "ARABIC";
    Combina = 263 => "COMBINA";
    SeriesSum = 264 => "SERIESSUM";
    AnchorArray = 265 => "ANCHORARRAY";
    Sequence = 266 => "SEQUENCE";
    Hstack = 267 => "HSTACK";
    Vstack = 268 => "VSTACK";
    Take = 269 => "TAKE";
    Drop = 270 => "DROP";
    Choosecols = 271 => "CHOOSECOLS";
    Chooserows = 272 => "CHOOSEROWS";
    Expand = 273 => "EXPAND";
    Tocol = 274 => "TOCOL";
    Torow = 275 => "TOROW";
    Wrapcols = 276 => "WRAPCOLS";
    Wraprows = 277 => "WRAPROWS";
    Filter = 278 => "FILTER";
    Sort = 279 => "SORT";
    Sortby = 280 => "SORTBY";
    Unique = 281 => "UNIQUE";
    TextBefore = 282 => "TEXTBEFORE";
    TextAfter = 283 => "TEXTAFTER";
    TextSplit = 284 => "TEXTSPLIT";
    ArrayToText = 285 => "ARRAYTOTEXT";
    ValueToText = 286 => "VALUETOTEXT";
    RegexTest = 287 => "REGEXTEST";
    RegexExtract = 288 => "REGEXEXTRACT";
    RegexReplace = 289 => "REGEXREPLACE";
    Groupby = 290 => "GROUPBY";
    Percentof = 291 => "PERCENTOF";
    Randarray = 292 => "RANDARRAY";
    Trimrange = 293 => "TRIMRANGE";
    Acot = 294 => "ACOT";
    Acoth = 295 => "ACOTH";
    Cot = 296 => "COT";
    Coth = 297 => "COTH";
    Csc = 298 => "CSC";
    Csch = 299 => "CSCH";
    Euroconvert = 300 => "EUROCONVERT";
    IsoCeiling = 301 => "ISO.CEILING";
    IsoWeekNum = 302 => "ISOWEEKNUM";
    Multinomial = 303 => "MULTINOMIAL";
    Munit = 304 => "MUNIT";
    NetworkDaysIntl = 305 => "NETWORKDAYS.INTL";
    Sec = 306 => "SEC";
    Sech = 307 => "SECH";
    Xor = 308 => "XOR";
    Imabs = 309 => "IMABS";
    Imargument = 310 => "IMARGUMENT";
    Imconjugate = 311 => "IMCONJUGATE";
    Imcos = 312 => "IMCOS";
    Imcosh = 313 => "IMCOSH";
    Imcot = 314 => "IMCOT";
    Imcsc = 315 => "IMCSC";
    Imcsch = 316 => "IMCSCH";
    Imdiv = 317 => "IMDIV";
    Imexp = 318 => "IMEXP";
    Imln = 319 => "IMLN";
    Imlog10 = 320 => "IMLOG10";
    Imlog2 = 321 => "IMLOG2";
    Impower = 322 => "IMPOWER";
    Improduct = 323 => "IMPRODUCT";
    Imsec = 324 => "IMSEC";
    Imsech = 325 => "IMSECH";
    Imsin = 326 => "IMSIN";
    Imsinh = 327 => "IMSINH";
    Imsqrt = 328 => "IMSQRT";
    Imsub = 329 => "IMSUB";
    Imsum = 330 => "IMSUM";
    Imtan = 331 => "IMTAN";
    Bin2hex = 332 => "BIN2HEX";
    Bin2oct = 333 => "BIN2OCT";
    Bitand = 334 => "BITAND";
    Bitlshift = 335 => "BITLSHIFT";
    Bitor = 336 => "BITOR";
    Bitrshift = 337 => "BITRSHIFT";
    Bitxor = 338 => "BITXOR";
    Convert = 339 => "CONVERT";
    Dec2oct = 340 => "DEC2OCT";
    Hex2bin = 341 => "HEX2BIN";
    Hex2oct = 342 => "HEX2OCT";
    Oct2bin = 343 => "OCT2BIN";
    Oct2hex = 344 => "OCT2HEX";
    Besseli = 345 => "BESSELI";
    Besselk = 346 => "BESSELK";
    Bessely = 347 => "BESSELY";
    Erf = 348 => "ERF";
    ErfPrecise = 349 => "ERF.PRECISE";
    Erfc = 350 => "ERFC";
    ErfcPrecise = 351 => "ERFC.PRECISE";
    Gestep = 352 => "GESTEP";
    Cumipmt = 353 => "CUMIPMT";
    Cumprinc = 354 => "CUMPRINC";
    Db = 355 => "DB";
    Ddb = 356 => "DDB";
    Effect = 357 => "EFFECT";
    Fvschedule = 358 => "FVSCHEDULE";
    Ispmt = 359 => "ISPMT";
    Nominal = 360 => "NOMINAL";
    Pduration = 361 => "PDURATION";
    Rri = 362 => "RRI";
    Sln = 363 => "SLN";
    Syd = 364 => "SYD";
    Vdb = 365 => "VDB";
    Xirr = 366 => "XIRR";
    Xnpv = 367 => "XNPV";
    Accrint = 368 => "ACCRINT";
    Accrintm = 369 => "ACCRINTM";
    Coupdaybs = 370 => "COUPDAYBS";
    Coupdays = 371 => "COUPDAYS";
    Coupdaysnc = 372 => "COUPDAYSNC";
    Coupncd = 373 => "COUPNCD";
    Coupnum = 374 => "COUPNUM";
    Couppcd = 375 => "COUPPCD";
    Disc = 376 => "DISC";
    Duration = 377 => "DURATION";
    Intrate = 378 => "INTRATE";
    Mduration = 379 => "MDURATION";
    Price = 380 => "PRICE";
    Pricedisc = 381 => "PRICEDISC";
    Pricemat = 382 => "PRICEMAT";
    Received = 383 => "RECEIVED";
    Tbilleq = 384 => "TBILLEQ";
    Tbillprice = 385 => "TBILLPRICE";
    Tbillyield = 386 => "TBILLYIELD";
    Yield = 387 => "YIELD";
    Yielddisc = 388 => "YIELDDISC";
    Yieldmat = 389 => "YIELDMAT";
    Amordegrc = 390 => "AMORDEGRC";
    Amorlinc = 391 => "AMORLINC";
    Oddfprice = 392 => "ODDFPRICE";
    Oddfyield = 393 => "ODDFYIELD";
    Oddlprice = 394 => "ODDLPRICE";
    Oddlyield = 395 => "ODDLYIELD";
    BetaDist = 396 => "BETA.DIST";
    BinomDist = 397 => "BINOM.DIST";
    BinomDistRange = 398 => "BINOM.DIST.RANGE";
    ChisqDist = 399 => "CHISQ.DIST";
    ChisqDistRt = 400 => "CHISQ.DIST.RT";
    ExponDist = 401 => "EXPON.DIST";
    FDist = 402 => "F.DIST";
    FDistRt = 403 => "F.DIST.RT";
    Gamma = 404 => "GAMMA";
    GammaDist = 405 => "GAMMA.DIST";
    Gammaln = 406 => "GAMMALN";
    GammalnPrecise = 407 => "GAMMALN.PRECISE";
    Gauss = 408 => "GAUSS";
    HypgeomDist = 409 => "HYPGEOM.DIST";
    LognormDist = 410 => "LOGNORM.DIST";
    NegbinomDist = 411 => "NEGBINOM.DIST";
    Phi = 412 => "PHI";
    WeibullDist = 413 => "WEIBULL.DIST";
    BetaInv = 414 => "BETA.INV";
    BinomInv = 415 => "BINOM.INV";
    ChisqInv = 416 => "CHISQ.INV";
    ChisqInvRt = 417 => "CHISQ.INV.RT";
    ConfidenceNorm = 418 => "CONFIDENCE.NORM";
    ConfidenceT = 419 => "CONFIDENCE.T";
    FInv = 420 => "F.INV";
    FInvRt = 421 => "F.INV.RT";
    GammaInv = 422 => "GAMMA.INV";
    LognormInv = 423 => "LOGNORM.INV";
    TInv = 424 => "T.INV";
    TInv2T = 425 => "T.INV.2T";
    ChisqTest = 426 => "CHISQ.TEST";
    FTest = 427 => "F.TEST";
    Fisher = 428 => "FISHER";
    FisherInv = 429 => "FISHERINV";
    HarMean = 430 => "HARMEAN";
    Kurt = 431 => "KURT";
    ModeMult = 432 => "MODE.MULT";
    ModeSngl = 433 => "MODE.SNGL";
    PercentileExc = 434 => "PERCENTILE.EXC";
    Permut = 435 => "PERMUT";
    PermutationA = 436 => "PERMUTATIONA";
    Prob = 437 => "PROB";
    QuartileExc = 438 => "QUARTILE.EXC";
    RankAvg = 439 => "RANK.AVG";
    RSq = 440 => "RSQ";
    Skew = 441 => "SKEW";
    SkewP = 442 => "SKEW.P";
    Steyx = 443 => "STEYX";
    TTest = 444 => "T.TEST";
    TrimMean = 445 => "TRIMMEAN";
    ZTest = 446 => "Z.TEST";
    Growth = 447 => "GROWTH";
    Linest = 448 => "LINEST";
    Logest = 449 => "LOGEST";
    ForecastEts = 450 => "FORECAST.ETS";
    ForecastEtsConfint = 451 => "FORECAST.ETS.CONFINT";
    ForecastEtsSeasonality = 452 => "FORECAST.ETS.SEASONALITY";
    ForecastEtsStat = 453 => "FORECAST.ETS.STAT";
    BetaDistLegacy = 454 => "BETADIST";
    BetaInvLegacy = 455 => "BETAINV";
    BinomDistLegacy = 456 => "BINOMDIST";
    ChisqDistRtLegacy = 457 => "CHIDIST";
    ChisqInvRtLegacy = 458 => "CHIINV";
    ChisqTestLegacy = 459 => "CHITEST";
    ConfidenceNormLegacy = 460 => "CONFIDENCE";
    BinomInvLegacy = 461 => "CRITBINOM";
    ExponDistLegacy = 462 => "EXPONDIST";
    FDistRtLegacy = 463 => "FDIST";
    FInvRtLegacy = 464 => "FINV";
    FTestLegacy = 465 => "FTEST";
    GammaDistLegacy = 466 => "GAMMADIST";
    GammaInvLegacy = 467 => "GAMMAINV";
    HypgeomDistLegacy = 468 => "HYPGEOMDIST";
    LognormInvLegacy = 469 => "LOGINV";
    LognormDistLegacy = 470 => "LOGNORMDIST";
    NegbinomDistLegacy = 471 => "NEGBINOMDIST";
    QuartileIncLegacy = 472 => "QUARTILE";
    TInv2TLegacy = 473 => "TINV";
    TTestLegacy = 474 => "TTEST";
    WeibullDistLegacy = 475 => "WEIBULL";
    ZTestLegacy = 476 => "ZTEST";
    LenB = 477 => "LENB";
    LeftB = 478 => "LEFTB";
    RightB = 479 => "RIGHTB";
    MidB = 480 => "MIDB";
    FindB = 481 => "FINDB";
    SearchB = 482 => "SEARCHB";
    ReplaceB = 483 => "REPLACEB";
    FilterXml = 484 => "FILTERXML";
    Pivotby = 485 => "PIVOTBY";
    Asc = 486 => "ASC";
    Dbcs = 487 => "DBCS";
    Jis = 488 => "JIS";
    Cell = 489 => "CELL";
    Aggregate = 490 => "AGGREGATE";
    BahtText = 491 => "BAHTTEXT";
    Sheets = 492 => "SHEETS";
    FormulaText = 493 => "FORMULATEXT";
    IsFormula = 494 => "ISFORMULA";
    EncodeUrl = 495 => "ENCODEURL";












}

/// Parsed function spelling, interned when it belongs to the common symbol table.
/// `Known` does not mean an evaluator supports the call; `Unknown` can still be
/// resolved as an extension or prefixed builtin. Namespace resolution happens later.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FunctionName {
    /// Interned common builtin spelling, avoiding a string allocation.
    Known(CommonFunction),
    /// Owned spelling for extensions or unrecognized functions.
    Unknown(String),
}
impl FunctionName {
    /// Borrow the stored function spelling.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(name) => name.as_str(),
            Self::Unknown(name) => name,
        }
    }
    /// Return the common builtin identity when interned.
    pub fn common(&self) -> Option<CommonFunction> {
        match self {
            Self::Known(name) => Some(*name),
            Self::Unknown(_) => None,
        }
    }
    /// Return the interned lookup index, if this name has one.
    pub fn index(&self) -> Option<usize> {
        self.common().map(CommonFunction::index)
    }
}
impl CommonFunction {
    /// ASCII case folding stays on the stack. Prefixed and extension names are
    /// deliberately not collapsed: namespace/prefix spelling can be significant.
    pub fn parse(name: &str) -> Option<Self> {
        let mut canonical = [0u8; 64];
        let bytes = canonical.get_mut(..name.len())?;
        for (output, input) in bytes.iter_mut().zip(name.bytes()) {
            *output = input.to_ascii_uppercase();
        }
        Self::from_canonical(std::str::from_utf8(bytes).ok()?)
    }
}
impl From<&str> for FunctionName {
    fn from(name: &str) -> Self {
        CommonFunction::parse(name)
            .map(Self::Known)
            .unwrap_or_else(|| Self::Unknown(name.to_owned()))
    }
}
impl From<String> for FunctionName {
    fn from(name: String) -> Self {
        CommonFunction::parse(&name)
            .map(Self::Known)
            .unwrap_or(Self::Unknown(name))
    }
}
impl From<CommonFunction> for FunctionName {
    fn from(name: CommonFunction) -> Self {
        Self::Known(name)
    }
}
impl From<FunctionName> for String {
    fn from(name: FunctionName) -> String {
        match name {
            FunctionName::Known(name) => name.as_str().to_owned(),
            FunctionName::Unknown(name) => name,
        }
    }
}
impl Deref for FunctionName {
    /// Borrowed canonical or preserved function spelling.
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl AsRef<str> for FunctionName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for FunctionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl PartialEq<str> for FunctionName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<&str> for FunctionName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_slots_are_dense_and_round_trip() {
        for (index, &name) in CommonFunction::ALL.iter().enumerate() {
            assert_eq!(name.index(), index);
            assert_eq!(
                FunctionName::from(name.as_str().to_ascii_lowercase()),
                FunctionName::Known(name)
            );
        }
        assert_eq!(CommonFunction::ALL.len(), CommonFunction::COUNT);
    }
    #[test]
    fn parser_emits_numeric_symbols_and_keeps_unknown_calls() {
        assert!(matches!(
            crate::formula::parse_formula("sum(1)").unwrap(),
            crate::formula::Expr::Function {
                name: FunctionName::Known(CommonFunction::Sum),
                ..
            }
        ));
        assert!(
            matches!(crate::formula::parse_formula("custom.fn(1)").unwrap(), crate::formula::Expr::Function {
            name: FunctionName::Unknown(name), ..
        } if name == "CUSTOM.FN")
        );
    }
    #[test]
    fn parser_keeps_extension_and_prefix_spellings() {
        for name in ["_xlfn.IFNA", "_XLFN.IFNA", "custom.fn", "漢字"] {
            assert_eq!(
                FunctionName::from(name),
                FunctionName::Unknown(name.to_owned())
            );
        }
        assert_eq!(
            FunctionName::from("sum"),
            FunctionName::Known(CommonFunction::Sum)
        );
    }
}
