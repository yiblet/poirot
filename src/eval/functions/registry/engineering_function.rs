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
