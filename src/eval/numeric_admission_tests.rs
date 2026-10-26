#[test]
fn ieee_classification_identity_is_payload_exact_without_source_production_claim() {
    use crate::model::{IeeeNumberBits as Bits, IeeeNumberClass as Class};
    for (raw, class) in [
        (0, Class::PositiveZero),
        (0x8000_0000_0000_0000, Class::NegativeZero),
        (1, Class::PositiveSubnormal),
        (0x800f_ffff_ffff_ffff, Class::NegativeSubnormal),
        (0x0010_0000_0000_0000, Class::Finite),
        (0x7fef_ffff_ffff_ffff, Class::Finite),
        (0x7ff0_0000_0000_0000, Class::PositiveInfinity),
        (0xfff0_0000_0000_0000, Class::NegativeInfinity),
        (0x7ff0_0000_0000_0001, Class::Nan),
        (0xfff8_0000_0000_0099, Class::Nan),
    ] {
        let bits = Bits::from_bits(raw);
        assert_eq!(bits.class(), class);
        assert_eq!(Bits::from_number(bits.number()).bits(), raw);
    }
    let a = crate::model::IeeeNumberBits::from_number(f64::from_bits(0x7ff8_0000_0000_0042));
    let b = crate::model::IeeeNumberBits::from_number(f64::from_bits(0x7ff8_0000_0000_0043));
    assert_ne!(a, b);
}
