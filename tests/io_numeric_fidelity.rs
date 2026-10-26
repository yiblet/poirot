//! Numeric serialization and import checks. Raw NaN fixtures test file
//! representation; they do not assert that spreadsheet evaluation produces NaN.
use poirot::model::{
    CellValue, FiniteNumericRepresentationPolicy, NumericFidelityReason, NumericFileFormat,
    Workbook,
};
use poirot_io::{
    IoError as Error, WriteOptions, plan_xlsx_export, read_workbook_bytes as read,
    write_xlsx as write, write_xlsx_to_buffer as buffer,
};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn book(bits: u64, cache: bool) -> Workbook {
    let mut w = Workbook::new();
    let s = w.add_sheet("Data").unwrap();
    let value = CellValue::Number(f64::from_bits(bits));
    if cache {
        s.set_formula_with_cached_value("B2".parse().unwrap(), "1", value)
            .unwrap();
    } else {
        s.set_value("B2".parse().unwrap(), value).unwrap();
    }
    w
}
fn refusal(error: Error, bits: u64, cache: bool) {
    let Error::NumericFidelity(r) = error else {
        panic!("expected typed refusal: {error}")
    };
    assert_eq!(r.bits.bits(), bits);
    assert_eq!(r.format, NumericFileFormat::Xlsx);
    assert_eq!(
        r.policy,
        FiniteNumericRepresentationPolicy::DecimalRoundTripFinite
    );
    assert_eq!(r.location.sheet, "Data");
    assert_eq!(r.location.address, "B2".parse().unwrap());
    assert_eq!(r.location.formula_cache, cache);
    assert_eq!(r.reason, NumericFidelityReason::Nonfinite);
}
#[test]
fn specials_refuse_plan_buffer_and_file_before_create_or_overwrite() {
    for bits in [
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000042,
        0xfff8000000000043,
        0x7ff0000000000001,
    ] {
        for cache in [false, true] {
            let w = book(bits, cache);
            refusal(
                plan_xlsx_export(&w, WriteOptions::default()).expect_err("must refuse"),
                bits,
                cache,
            );
            refusal(buffer(&w).unwrap_err(), bits, cache);
            let path = std::env::temp_dir().join(format!(
                "poirot152-xlsx-{}-{}.xlsx",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            assert!(!path.exists());
            refusal(write(&w, &path).unwrap_err(), bits, cache);
            assert!(!path.exists());
            std::fs::write(&path, b"preexisting sentinel").unwrap();
            refusal(write(&w, &path).unwrap_err(), bits, cache);
            assert_eq!(std::fs::read(&path).unwrap(), b"preexisting sentinel");
            refusal(
                poirot_io::write_xlsx_with_options(&w, &path, WriteOptions::default()).unwrap_err(),
                bits,
                cache,
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"preexisting sentinel");
            refusal(
                poirot_io::write_xlsx_to_buffer_with_options(&w, WriteOptions::default())
                    .unwrap_err(),
                bits,
                cache,
            );
            std::fs::remove_file(path).unwrap();
        }
    }
}
#[test]
fn finite_literal_and_cache_public_backend_roundtrip_preserves_bits() {
    for bits in [
        0,
        0x8000000000000000,
        1,
        0x8000000000000001,
        0x000fffffffffffff,
        0x800fffffffffffff,
        0x0010000000000000,
        0x3fb999999999999a,
        0x3ff0000000000001,
        0x7fefffffffffffff,
    ] {
        for cache in [false, true] {
            let w = book(bits, cache);
            let plan = plan_xlsx_export(&w, WriteOptions::default()).unwrap();
            assert_eq!(
                plan.numeric_representation_policy(),
                FiniteNumericRepresentationPolicy::DecimalRoundTripFinite
            );
            for bytes in [plan.execute().unwrap(), buffer(&w).unwrap()] {
                let decoded = read::<Workbook>(&bytes).unwrap();
                let CellValue::Number(number) = &decoded
                    .sheet(0)
                    .unwrap()
                    .get("B2".parse().unwrap())
                    .unwrap()
                    .value
                else {
                    panic!("numeric type lost")
                };
                assert_eq!(number.to_bits(), bits, "cache={cache}");
            }
        }
    }
}
