//! Fixed historical rate facts and independently calculated conversion identities.
use poirot_functions_financial::euro::{EuroCurrency as C, euro_convert};
#[test]
fn historical_currencies_and_rounding_are_fixed_and_bounded() {
    for (code, rate) in [
        ("ATS", 13.7603),
        ("BEF", 40.3399),
        ("DEM", 1.95583),
        ("ESP", 166.386),
        ("FIM", 5.94573),
        ("FRF", 6.55957),
        ("GRD", 340.750),
        ("IEP", 0.787564),
        ("ITL", 1936.27),
        ("LUF", 40.3399),
        ("NLG", 2.20371),
        ("PTE", 200.482),
        ("SIT", 239.640),
    ] {
        let c = C::parse(code).unwrap();
        assert_eq!(c.code(), code);
        assert_eq!(euro_convert(1., C::Eur, c, true, None).unwrap(), rate);
        assert!((euro_convert(rate, c, C::Eur, true, None).unwrap() - 1.).abs() < 1e-14);
    }
    assert!(C::parse("USD").is_none());
    assert!(C::parse("EUR ").is_none());
    assert_eq!(
        euro_convert(1.234, C::Dem, C::Dem, false, None).unwrap(),
        1.234
    );
    assert_eq!(euro_convert(1., C::Eur, C::Bef, false, None).unwrap(), 40.);
    assert!((euro_convert(1., C::Frf, C::Dem, true, Some(3.)).unwrap() - 0.29728616).abs() < 1e-13);
    for precision in [0., 2., 3.5, f64::INFINITY] {
        assert!(euro_convert(1., C::Frf, C::Eur, true, Some(precision)).is_err());
    }
}
