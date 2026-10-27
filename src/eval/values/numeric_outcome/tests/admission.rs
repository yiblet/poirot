#[test]
fn ieee_classification_identity_is_payload_exact_without_source_production_claim() {
    use crate::model::{IeeeNumberBits, IeeeNumberClass};
    for (raw, class) in [
        (0, IeeeNumberClass::PositiveZero),
        (0x8000_0000_0000_0000, IeeeNumberClass::NegativeZero),
        (1, IeeeNumberClass::PositiveSubnormal),
        (0x800f_ffff_ffff_ffff, IeeeNumberClass::NegativeSubnormal),
        (0x0010_0000_0000_0000, IeeeNumberClass::Finite),
        (0x7fef_ffff_ffff_ffff, IeeeNumberClass::Finite),
        (0x7ff0_0000_0000_0000, IeeeNumberClass::PositiveInfinity),
        (0xfff0_0000_0000_0000, IeeeNumberClass::NegativeInfinity),
        (0x7ff0_0000_0000_0001, IeeeNumberClass::Nan),
        (0xfff8_0000_0000_0099, IeeeNumberClass::Nan),
    ] {
        let bits = IeeeNumberBits::from_bits(raw);
        assert_eq!(bits.class(), class);
        assert_eq!(IeeeNumberBits::from_number(bits.number()).bits(), raw);
    }
    let a = crate::model::IeeeNumberBits::from_number(f64::from_bits(0x7ff8_0000_0000_0042));
    let b = crate::model::IeeeNumberBits::from_number(f64::from_bits(0x7ff8_0000_0000_0043));
    assert_ne!(a, b);
}
