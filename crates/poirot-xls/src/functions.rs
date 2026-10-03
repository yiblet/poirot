//! Checked BIFF encoding metadata, distinct from evaluator function support.
//! Derived from pinned POI functionMetadata.txt; ellipsis is normalized here.
use super::{XlsError, fail};

/// BIFF token class controlling reference, scalar, or array consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperandClass {
    /// Reference-class token.
    Reference,
    /// Scalar-value-class token.
    Value,
    /// Array-class token.
    Array,
}
impl OperandClass {
    /// Return the BIFF class bits used when emitting token opcodes.
    pub(super) fn bits(self) -> u8 {
        match self {
            Self::Reference => 0,
            Self::Value => 0x20,
            Self::Array => 0x40,
        }
    }
}
/// How argument-class metadata extends beyond its explicit prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepeatedTail {
    /// No repeated trailing argument class.
    None,
    /// Repeat the last declared argument class.
    Last,
}
/// Builtin BIFF function ID, arity, and operand-class contract.
#[derive(Debug, Clone, Copy)]
pub(super) struct BiffFunctionMetadata {
    /// Builtin function identifier encoded in BIFF tokens.
    pub(super) id: u16,
    /// Minimum admitted argument count.
    pub(super) min_args: usize,
    /// Maximum admitted argument count.
    pub(super) max_args: usize,
    /// Operand class of the function result.
    pub(super) result: OperandClass,
    parameters: &'static [OperandClass],
    repeated_tail: RepeatedTail,
}
impl BiffFunctionMetadata {
    fn checked(
        id: u16,
        min_args: usize,
        max_args: usize,
        result: OperandClass,
        parameters: &'static [OperandClass],
        repeated_tail: RepeatedTail,
    ) -> Result<Self, XlsError> {
        if id > 0x7fff
            || min_args > max_args
            || max_args > 255
            || parameters.len() > max_args
            || (max_args > 0 && parameters.is_empty())
            || (repeated_tail == RepeatedTail::Last
                && (parameters.is_empty() || parameters.len() >= max_args))
            || (repeated_tail == RepeatedTail::None && parameters.len() != max_args)
        {
            return Err(fail("invalid BIFF function operand metadata"));
        }
        Ok(Self {
            id,
            min_args,
            max_args,
            result,
            parameters,
            repeated_tail,
        })
    }
    /// Resolve the expected operand class for one argument position.
    pub(super) fn parameter(self, index: usize) -> Result<OperandClass, XlsError> {
        if index >= self.max_args {
            return Err(fail("BIFF parameter exceeds metadata arity"));
        }
        self.parameters
            .get(index)
            .copied()
            .or_else(|| {
                if self.repeated_tail == RepeatedTail::Last {
                    self.parameters.last().copied()
                } else {
                    None
                }
            })
            .ok_or_else(|| fail("missing BIFF function operand metadata"))
    }
}

/// Look up the BIFF encoding contract for a supported function spelling.
pub(super) fn metadata(name: &str) -> Result<Option<BiffFunctionMetadata>, XlsError> {
    use OperandClass::{Array as A, Reference as R, Value as V};
    use RepeatedTail::{Last, None as NoTail};
    let (id, min, max, result, parameters, tail): (_, _, _, _, &'static [_], _) = match name {
        "ABS" => (24, 1, 1, V, &[V], NoTail),
        "ABSREF" => (79, 2, 2, R, &[V, R], NoTail),
        "ACOS" => (99, 1, 1, V, &[V], NoTail),
        "ACOSH" => (233, 1, 1, V, &[V], NoTail),
        "ADDRESS" => (219, 2, 5, V, &[V, V, V, V, V], NoTail),
        "AND" => (36, 1, 30, V, &[R], Last),
        "APP.TITLE" => (262, 0, 1, V, &[V], NoTail),
        "AREAS" => (75, 1, 1, V, &[R], NoTail),
        "ARGUMENT" => (81, 0, 3, V, &[V, V, R], NoTail),
        "ASC" => (214, 1, 1, V, &[V], NoTail),
        "ASIN" => (98, 1, 1, V, &[V], NoTail),
        "ASINH" => (232, 1, 1, V, &[V], NoTail),
        "ATAN" => (18, 1, 1, V, &[V], NoTail),
        "ATAN2" => (97, 2, 2, V, &[V, V], NoTail),
        "ATANH" => (234, 1, 1, V, &[V], NoTail),
        "AVEDEV" => (269, 1, 30, V, &[R], Last),
        "AVERAGE" => (5, 1, 30, V, &[R], Last),
        "AVERAGEA" => (361, 1, 30, V, &[R], Last),
        "BETADIST" => (270, 3, 5, V, &[V, V, V, V, V], NoTail),
        "BETAINV" => (272, 3, 5, V, &[V, V, V, V, V], NoTail),
        "BINOMDIST" => (273, 4, 4, V, &[V, V, V, V], NoTail),
        "CALL" => (150, 1, 3, V, &[V, R, R], NoTail),
        "CEILING" => (288, 2, 2, V, &[V, V], NoTail),
        "CELL" => (125, 1, 2, V, &[V, R], NoTail),
        "CHAR" => (111, 1, 1, V, &[V], NoTail),
        "CHIDIST" => (274, 2, 2, V, &[V, V], NoTail),
        "CHIINV" => (275, 2, 2, V, &[V, V], NoTail),
        "CHITEST" => (306, 2, 2, V, &[A, A], NoTail),
        "CHOOSE" => (100, 2, 30, R, &[V, R], Last),
        "CLEAN" => (162, 1, 1, V, &[V], NoTail),
        "CODE" => (121, 1, 1, V, &[V], NoTail),
        "COLUMN" => (9, 0, 1, V, &[R], NoTail),
        "COLUMNS" => (77, 1, 1, V, &[A], NoTail),
        "COMBIN" => (276, 2, 2, V, &[V, V], NoTail),
        "CONCATENATE" => (336, 0, 30, V, &[V], Last),
        "CONFIDENCE" => (277, 3, 3, V, &[V, V, V], NoTail),
        "CORREL" => (307, 2, 2, V, &[A, A], NoTail),
        "COS" => (16, 1, 1, V, &[V], NoTail),
        "COSH" => (230, 1, 1, V, &[V], NoTail),
        "COUNT" => (0, 0, 30, V, &[R], Last),
        "COUNTA" => (169, 0, 30, V, &[R], Last),
        "COUNTBLANK" => (347, 1, 1, V, &[R], NoTail),
        "COUNTIF" => (346, 2, 2, V, &[R, V], NoTail),
        "COVAR" => (308, 2, 2, V, &[A, A], NoTail),
        "CRITBINOM" => (278, 3, 3, V, &[V, V, V], NoTail),
        "DATE" => (65, 3, 3, V, &[V, V, V], NoTail),
        "DATEDIF" => (351, 3, 3, V, &[V, V, V], NoTail),
        "DATESTRING" => (352, 1, 1, V, &[V], NoTail),
        "DATEVALUE" => (140, 1, 1, V, &[V], NoTail),
        "DAVERAGE" => (42, 3, 3, V, &[R, R, R], NoTail),
        "DAY" => (67, 1, 1, V, &[V], NoTail),
        "DAYS360" => (220, 2, 3, V, &[V, V, V], NoTail),
        "DB" => (247, 4, 5, V, &[V, V, V, V, V], NoTail),
        "DBCS" => (215, 1, 1, V, &[V], NoTail),
        "DCOUNT" => (40, 3, 3, V, &[R, R, R], NoTail),
        "DCOUNTA" => (199, 3, 3, V, &[R, R, R], NoTail),
        "DDB" => (144, 4, 5, V, &[V, V, V, V, V], NoTail),
        "DEGREES" => (343, 1, 1, V, &[V], NoTail),
        "DEVSQ" => (318, 1, 30, V, &[R], Last),
        "DGET" => (235, 3, 3, V, &[R, R, R], NoTail),
        "DMAX" => (44, 3, 3, V, &[R, R, R], NoTail),
        "DMIN" => (43, 3, 3, V, &[R, R, R], NoTail),
        "DOLLAR" => (13, 1, 2, V, &[V, V], NoTail),
        "DPRODUCT" => (189, 3, 3, V, &[R, R, R], NoTail),
        "DSTDEV" => (45, 3, 3, V, &[R, R, R], NoTail),
        "DSTDEVP" => (195, 3, 3, V, &[R, R, R], NoTail),
        "DSUM" => (41, 3, 3, V, &[R, R, R], NoTail),
        "DVAR" => (47, 3, 3, V, &[R, R, R], NoTail),
        "DVARP" => (196, 3, 3, V, &[R, R, R], NoTail),
        "ENABLE.TOOL" => (265, 3, 3, V, &[V, V, V], NoTail),
        "ERROR" => (84, 0, 2, V, &[V, R], NoTail),
        "ERROR.TYPE" => (261, 1, 1, V, &[V], NoTail),
        "EVALUATE" => (257, 1, 1, V, &[V], NoTail),
        "EVEN" => (279, 1, 1, V, &[V], NoTail),
        "EXACT" => (117, 2, 2, V, &[V, V], NoTail),
        "EXEC" => (110, 1, 4, V, &[V, V, V, V], NoTail),
        "EXP" => (21, 1, 1, V, &[V], NoTail),
        "EXPONDIST" => (280, 3, 3, V, &[V, V, V], NoTail),
        "FACT" => (184, 1, 1, V, &[V], NoTail),
        "FALSE" => (35, 0, 0, V, &[], NoTail),
        "FDIST" => (281, 3, 3, V, &[V, V, V], NoTail),
        "FIND" => (124, 2, 3, V, &[V, V, V], NoTail),
        "FINDB" => (205, 2, 3, V, &[V, V, V], NoTail),
        "FINV" => (282, 3, 3, V, &[V, V, V], NoTail),
        "FISHER" => (283, 1, 1, V, &[V], NoTail),
        "FISHERINV" => (284, 1, 1, V, &[V], NoTail),
        "FIXED" => (14, 1, 3, V, &[V, V, V], NoTail),
        "FLOOR" => (285, 2, 2, V, &[V, V], NoTail),
        "FORECAST" => (309, 3, 3, V, &[V, A, A], NoTail),
        "FREQUENCY" => (252, 2, 2, A, &[R, R], NoTail),
        "FTEST" => (310, 2, 2, V, &[A, A], NoTail),
        "FV" => (57, 3, 5, V, &[V, V, V, V, V], NoTail),
        "GAMMADIST" => (286, 4, 4, V, &[V, V, V, V], NoTail),
        "GAMMAINV" => (287, 3, 3, V, &[V, V, V], NoTail),
        "GAMMALN" => (271, 1, 1, V, &[V], NoTail),
        "GEOMEAN" => (319, 1, 30, V, &[R], Last),
        "GET.CELL" => (185, 1, 2, V, &[V, R], NoTail),
        "GET.DOCUMENT" => (188, 1, 2, V, &[V, V], NoTail),
        "GET.WINDOW" => (187, 1, 2, V, &[V, V], NoTail),
        "GET.WORKBOOK" => (268, 1, 2, V, &[V, V], NoTail),
        "GET.WORKSPACE" => (186, 1, 1, V, &[V], NoTail),
        "GETPIVOTDATA" => (358, 2, 30, V, &[V, R], Last),
        "GOTO" => (53, 1, 1, R, &[R], NoTail),
        "GROWTH" => (52, 1, 4, A, &[R, R, R, V], NoTail),
        "HARMEAN" => (320, 1, 30, V, &[R], Last),
        "HLOOKUP" => (101, 3, 4, V, &[V, R, R, V], NoTail),
        "HOUR" => (71, 1, 1, V, &[V], NoTail),
        "HYPERLINK" => (359, 1, 2, V, &[V, V], NoTail),
        "HYPGEOMDIST" => (289, 4, 4, V, &[V, V, V, V], NoTail),
        "IF" => (1, 2, 3, R, &[V, R, R], NoTail),
        "INDEX" => (29, 2, 4, R, &[R, V, V, V], NoTail),
        "INDIRECT" => (148, 1, 2, R, &[V, V], NoTail),
        "INFO" => (244, 1, 1, V, &[V], NoTail),
        "INT" => (25, 1, 1, V, &[V], NoTail),
        "INTERCEPT" => (311, 2, 2, V, &[A, A], NoTail),
        "IPMT" => (167, 4, 6, V, &[V, V, V, V, V, V], NoTail),
        "IRR" => (62, 1, 2, V, &[A, V], NoTail),
        "ISBLANK" => (129, 1, 1, V, &[V], NoTail),
        "ISERR" => (126, 1, 1, V, &[V], NoTail),
        "ISERROR" => (3, 1, 1, V, &[V], NoTail),
        "ISLOGICAL" => (198, 1, 1, V, &[V], NoTail),
        "ISNA" => (2, 1, 1, V, &[V], NoTail),
        "ISNONTEXT" => (190, 1, 1, V, &[V], NoTail),
        "ISNUMBER" => (128, 1, 1, V, &[V], NoTail),
        "ISPMT" => (350, 4, 4, V, &[V, V, V, V], NoTail),
        "ISREF" => (105, 1, 1, V, &[R], NoTail),
        "ISTEXT" => (127, 1, 1, V, &[V], NoTail),
        "JIS" => (215, 1, 1, V, &[V], NoTail),
        "KURT" => (322, 1, 30, V, &[R], Last),
        "LARGE" => (325, 2, 2, V, &[R, V], NoTail),
        "LEFT" => (115, 1, 2, V, &[V, V], NoTail),
        "LEFTB" => (208, 1, 2, V, &[V, V], NoTail),
        "LEN" => (32, 1, 1, V, &[V], NoTail),
        "LENB" => (211, 1, 1, V, &[V], NoTail),
        "LINEST" => (49, 1, 4, A, &[R, R, V, V], NoTail),
        "LN" => (22, 1, 1, V, &[V], NoTail),
        "LOG" => (109, 1, 2, V, &[V, V], NoTail),
        "LOG10" => (23, 1, 1, V, &[V], NoTail),
        "LOGEST" => (51, 1, 4, A, &[R, R, V, V], NoTail),
        "LOGINV" => (291, 3, 3, V, &[V, V, V], NoTail),
        "LOGNORMDIST" => (290, 3, 3, V, &[V, V, V], NoTail),
        "LOOKUP" => (28, 2, 3, V, &[V, R, R], NoTail),
        "LOWER" => (112, 1, 1, V, &[V], NoTail),
        "MATCH" => (64, 2, 3, V, &[V, R, R], NoTail),
        "MAX" => (7, 1, 30, V, &[R], Last),
        "MAXA" => (362, 1, 30, V, &[R], Last),
        "MDETERM" => (163, 1, 1, V, &[A], NoTail),
        "MEDIAN" => (227, 1, 30, V, &[R], Last),
        "MID" => (31, 3, 3, V, &[V, V, V], NoTail),
        "MIDB" => (210, 3, 3, V, &[V, V, V], NoTail),
        "MIN" => (6, 1, 30, V, &[R], Last),
        "MINA" => (363, 1, 30, V, &[R], Last),
        "MINUTE" => (72, 1, 1, V, &[V], NoTail),
        "MINVERSE" => (164, 1, 1, A, &[A], NoTail),
        "MIRR" => (61, 3, 3, V, &[A, V, V], NoTail),
        "MMULT" => (165, 2, 2, A, &[A, A], NoTail),
        "MOD" => (39, 2, 2, V, &[V, V], NoTail),
        "MODE" => (330, 1, 30, V, &[A], Last),
        "MONTH" => (68, 1, 1, V, &[V], NoTail),
        "N" => (131, 1, 1, V, &[R], NoTail),
        "NA" => (10, 0, 0, V, &[], NoTail),
        "NEGBINOMDIST" => (292, 3, 3, V, &[V, V, V], NoTail),
        "NORMDIST" => (293, 4, 4, V, &[V, V, V, V], NoTail),
        "NORMINV" => (295, 3, 3, V, &[V, V, V], NoTail),
        "NORMSDIST" => (294, 1, 1, V, &[V], NoTail),
        "NORMSINV" => (296, 1, 1, V, &[V], NoTail),
        "NOT" => (38, 1, 1, V, &[V], NoTail),
        "NOW" => (74, 0, 0, V, &[], NoTail),
        "NPER" => (58, 3, 5, V, &[V, V, V, V, V], NoTail),
        "NPV" => (11, 2, 30, V, &[V, R], Last),
        "NUMBERSTRING" => (353, 2, 2, V, &[V, V], NoTail),
        "ODD" => (298, 1, 1, V, &[V], NoTail),
        "OFFSET" => (78, 3, 5, R, &[R, V, V, V, V], NoTail),
        "OR" => (37, 1, 30, V, &[R], Last),
        "PEARSON" => (312, 2, 2, V, &[A, A], NoTail),
        "PERCENTILE" => (328, 2, 2, V, &[R, V], NoTail),
        "PERCENTRANK" => (329, 2, 3, V, &[R, V, V], NoTail),
        "PERMUT" => (299, 2, 2, V, &[V, V], NoTail),
        "PHONETIC" => (360, 1, 1, V, &[R], NoTail),
        "PI" => (19, 0, 0, V, &[], NoTail),
        "PMT" => (59, 3, 5, V, &[V, V, V, V, V], NoTail),
        "POISSON" => (300, 3, 3, V, &[V, V, V], NoTail),
        "POWER" => (337, 2, 2, V, &[V, V], NoTail),
        "PPMT" => (168, 4, 6, V, &[V, V, V, V, V, V], NoTail),
        "PRESS.TOOL" => (266, 3, 3, V, &[V, V, V], NoTail),
        "PROB" => (317, 3, 4, V, &[A, A, V, V], NoTail),
        "PRODUCT" => (183, 0, 30, V, &[R], Last),
        "PROPER" => (114, 1, 1, V, &[V], NoTail),
        "PV" => (56, 3, 5, V, &[V, V, V, V, V], NoTail),
        "QUARTILE" => (327, 2, 2, V, &[R, V], NoTail),
        "RADIANS" => (342, 1, 1, V, &[V], NoTail),
        "RAND" => (63, 0, 0, V, &[], NoTail),
        "RANK" => (216, 2, 3, V, &[V, R, V], NoTail),
        "RATE" => (60, 3, 6, V, &[V, V, V, V, V, V], NoTail),
        "REGISTER.ID" => (267, 2, 3, V, &[V, V, V], NoTail),
        "RELREF" => (80, 2, 2, R, &[V, V], NoTail),
        "REPLACE" => (119, 4, 4, V, &[V, V, V, V], NoTail),
        "REPLACEB" => (207, 4, 4, V, &[V, V, V, V], NoTail),
        "REPT" => (30, 2, 2, V, &[V, V], NoTail),
        "RETURN" => (55, 1, 1, V, &[V], NoTail),
        "RIGHT" => (116, 1, 2, V, &[V, V], NoTail),
        "RIGHTB" => (209, 1, 2, V, &[V, V], NoTail),
        "ROMAN" => (354, 1, 2, V, &[V, V], NoTail),
        "ROUND" => (27, 2, 2, V, &[V, V], NoTail),
        "ROUNDDOWN" => (213, 2, 2, V, &[V, V], NoTail),
        "ROUNDUP" => (212, 2, 2, V, &[V, V], NoTail),
        "ROW" => (8, 0, 1, V, &[R], NoTail),
        "ROWS" => (76, 1, 1, V, &[A], NoTail),
        "RSQ" => (313, 2, 2, V, &[A, A], NoTail),
        "SAVE.TOOLBAR" => (264, 0, 2, V, &[V, V], NoTail),
        "SEARCH" => (82, 2, 3, V, &[V, V, V], NoTail),
        "SEARCHB" => (206, 2, 3, V, &[V, V, V], NoTail),
        "SECOND" => (73, 1, 1, V, &[V], NoTail),
        "SIGN" => (26, 1, 1, V, &[V], NoTail),
        "SIN" => (15, 1, 1, V, &[V], NoTail),
        "SINH" => (229, 1, 1, V, &[V], NoTail),
        "SKEW" => (323, 1, 30, V, &[R], Last),
        "SLN" => (142, 3, 3, V, &[V, V, V], NoTail),
        "SLOPE" => (315, 2, 2, V, &[A, A], NoTail),
        "SMALL" => (326, 2, 2, V, &[R, V], NoTail),
        "SQRT" => (20, 1, 1, V, &[V], NoTail),
        "STANDARDIZE" => (297, 3, 3, V, &[V, V, V], NoTail),
        "STDEV" => (12, 1, 30, V, &[R], Last),
        "STDEVA" => (366, 1, 30, V, &[R], Last),
        "STDEVP" => (193, 1, 30, V, &[R], Last),
        "STDEVPA" => (364, 1, 30, V, &[R], Last),
        "STEYX" => (314, 2, 2, V, &[A, A], NoTail),
        "SUBSTITUTE" => (120, 3, 4, V, &[V, V, V, V], NoTail),
        "SUBTOTAL" => (344, 2, 30, V, &[V, R], Last),
        "SUM" => (4, 0, 30, V, &[R], Last),
        "SUMIF" => (345, 2, 3, V, &[R, V, R], NoTail),
        "SUMPRODUCT" => (228, 1, 30, V, &[A], Last),
        "SUMSQ" => (321, 0, 30, V, &[R], Last),
        "SUMX2MY2" => (304, 2, 2, V, &[A, A], NoTail),
        "SUMX2PY2" => (305, 2, 2, V, &[A, A], NoTail),
        "SUMXMY2" => (303, 2, 2, V, &[A, A], NoTail),
        "SYD" => (143, 4, 4, V, &[V, V, V, V], NoTail),
        "T" => (130, 1, 1, V, &[R], NoTail),
        "TAN" => (17, 1, 1, V, &[V], NoTail),
        "TANH" => (231, 1, 1, V, &[V], NoTail),
        "TDIST" => (301, 3, 3, V, &[V, V, V], NoTail),
        "TEXT" => (48, 2, 2, V, &[V, V], NoTail),
        "TIME" => (66, 3, 3, V, &[V, V, V], NoTail),
        "TIMEVALUE" => (141, 1, 1, V, &[V], NoTail),
        "TINV" => (332, 2, 2, V, &[V, V], NoTail),
        "TODAY" => (221, 0, 0, V, &[], NoTail),
        "TRANSPOSE" => (83, 1, 1, A, &[A], NoTail),
        "TREND" => (50, 1, 4, A, &[R, R, R, V], NoTail),
        "TRIM" => (118, 1, 1, V, &[V], NoTail),
        "TRIMMEAN" => (331, 2, 2, V, &[R, V], NoTail),
        "TRUE" => (34, 0, 0, V, &[], NoTail),
        "TRUNC" => (197, 1, 2, V, &[V, V], NoTail),
        "TTEST" => (316, 4, 4, V, &[A, A, V, V], NoTail),
        "TYPE" => (86, 1, 1, V, &[V], NoTail),
        "UPPER" => (113, 1, 1, V, &[V], NoTail),
        "USDOLLAR" => (204, 1, 2, V, &[V, V], NoTail),
        "VALUE" => (33, 1, 1, V, &[V], NoTail),
        "VAR" => (46, 1, 30, V, &[R], Last),
        "VARA" => (367, 1, 30, V, &[R], Last),
        "VARP" => (194, 1, 30, V, &[R], Last),
        "VARPA" => (365, 1, 30, V, &[R], Last),
        "VDB" => (222, 5, 7, V, &[V, V, V, V, V, V, V], NoTail),
        "VLOOKUP" => (102, 3, 4, V, &[V, R, R, V], NoTail),
        "WEEKDAY" => (70, 1, 2, V, &[V, V], NoTail),
        "WEIBULL" => (302, 4, 4, V, &[V, V, V, V], NoTail),
        "WINDOW.TITLE" => (263, 0, 1, V, &[V], NoTail),
        "YEAR" => (69, 1, 1, V, &[V], NoTail),
        "YEN" => (204, 1, 2, V, &[V, V], NoTail),
        "ZTEST" => (324, 2, 3, V, &[R, V, V], NoTail),
        _ => return Ok(None),
    };
    BiffFunctionMetadata::checked(id, min, max, result, parameters, tail).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_metadata_rejects_missing_or_inconsistent_classes() {
        use OperandClass::{Reference as R, Value as V};
        use RepeatedTail::{Last, None as NoTail};
        for (min, max, params, tail) in [
            (2, 1, &[V][..], NoTail),
            (0, 256, &[V][..], Last),
            (0, 1, &[][..], Last),
            (0, 0, &[V][..], NoTail),
            (1, 2, &[R][..], NoTail),
            (1, 1, &[R][..], Last),
        ] {
            assert!(BiffFunctionMetadata::checked(1, min, max, V, params, tail).is_err());
        }
        assert!(BiffFunctionMetadata::checked(0x8000, 1, 1, V, &[V], NoTail).is_err());
        let m = metadata("NPV").unwrap().unwrap();
        assert_eq!(m.parameter(0).unwrap(), V);
        assert_eq!(m.parameter(29).unwrap(), R);
        assert!(m.parameter(30).is_err());
        assert!(metadata("NOT_A_BUILTIN").unwrap().is_none());
    }
}
