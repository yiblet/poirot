//! Numeric text is a source operation, separate from character-unit policy.
//! Pinned POI7321209 NumberToTextConverter.toText delegates to the shared
//! NormalisedDecimal / MutableFPNumber kernel; arithmetic values remain untouched.

/// Numeric-to-text source contract selected independently of display formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceVersion {
    /// Legacy ROI numeric-text conversion contract.
    PoirotF6c572d,
    /// Pinned POI number-to-text conversion contract.
    Poi7321209,
}

/// Admit this bound before rendering. It preserves the evaluator's existing
/// conservative numeric-text admission; the kernel itself has bounded precision.
pub const ALLOCATION_BOUND: usize = 512;

/// Render binary64 input using the selected numeric-text source contract.
pub fn render(value: f64, source: SourceVersion) -> String {
    match source {
        SourceVersion::PoirotF6c572d => value.to_string(),
        SourceVersion::Poi7321209 => super::to_text(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_selection_preserves_value_and_legacy_projection() {
        let value = -14.000000000000002_f64;
        let bits = value.to_bits();
        assert_eq!(render(value, SourceVersion::Poi7321209), "-14");
        assert_eq!(
            render(value, SourceVersion::PoirotF6c572d),
            "-14.000000000000002"
        );
        assert_eq!(value.to_bits(), bits);
    }
    #[test]
    fn poi_signed_zero_denormals_and_nonfinite_entry_points() {
        for (bits, expected) in [
            (0u64, "0"),
            (1 << 63, "-0"),
            (1, "0"),
            ((1 << 63) | 1, "-0"),
            (0x000f_ffff_ffff_ffff, "0"),
            (0x800f_ffff_ffff_ffff, "-0"),
            (0x0010_0000_0000_0000, "2.2250738585072E-308"),
            (0x7ff0_0000_0000_0000, "1.7976931348623E+308"),
            (0xfff0_0000_0000_0000, "1.7976931348623E+308"),
            (0xffff_ffff_ffff_ffff, "3.5953862697246E+308"),
            (0x7ff8_0000_0000_0000, "2.6965397022935E+308"),
        ] {
            assert_eq!(super::super::raw_bits_to_text(bits), expected);
        }
        for bits in [
            0xffff_ffff_ffff_ffff,
            0x7ff0_0000_0000_0001,
            0xfff8_0000_0000_0000,
        ] {
            assert_eq!(
                render(f64::from_bits(bits), SourceVersion::Poi7321209),
                "2.6965397022935E+308"
            );
        }
    }
}
