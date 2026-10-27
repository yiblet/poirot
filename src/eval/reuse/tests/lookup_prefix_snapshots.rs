use super::super::lookup_prefixes::{LookupPrefixes, MAX_ENTRIES};
use super::super::prefix_axis::PrefixAxis;
use super::super::prefix_key::PrefixKey;
use super::super::search_prefix::SearchPrefix;
use crate::model::Workbook;
use std::collections::BTreeMap;

/// A bounded two-cell numeric prefix with an independently retained text selection.
fn prepared() -> LookupPrefixes {
    let mut prefix = SearchPrefix {
        next: 2,
        ..Default::default()
    };
    prefix.first.insert(PrefixKey::Number(2.0f64.to_bits()), 1);
    prefix.text.insert(vec![65, 0xd800], Some(0));
    let mut prefixes = LookupPrefixes::default();
    prefixes.insert(
        PrefixAxis {
            sheet: 0,
            first: "A1".parse().unwrap(),
            last: "A3".parse().unwrap(),
        },
        prefix,
    );
    prefixes
}

#[test]
fn decoded_lookup_prefixes_rebuild_admission_and_preserve_exact_text_units() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let bytes = postcard::to_allocvec(&prepared()).unwrap();
    let decoded: LookupPrefixes = postcard::from_bytes(&bytes).unwrap();
    let mut restored = decoded.restore(&book).unwrap();
    assert_eq!(restored.reserved, 5);
    let prefix = restored
        .axes
        .values()
        .next()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(prefix.text.get(&vec![65, 0xd800]), Some(&Some(0)));
    assert_eq!(
        prefix.first.get(&PrefixKey::Number(2.0f64.to_bits())),
        Some(&1)
    );
    restored.admit(4);
    assert!(restored.is_empty());
}

#[test]
fn invalid_lookup_snapshot_geometry_offsets_keys_and_storage_are_refused() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for malformed in 0..7 {
        let mut prefixes = prepared();
        let prefix = prefixes
            .axes
            .values_mut()
            .next()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        match malformed {
            0 => prefix.next = 4,
            1 => {
                prefix.first.insert(PrefixKey::Number(3.0f64.to_bits()), 2);
            }
            2 => {
                prefix
                    .first
                    .insert(PrefixKey::Number(f64::NAN.to_bits()), 0);
            }
            3 => {
                prefix
                    .first
                    .insert(PrefixKey::Number((-0.0f64).to_bits()), 0);
            }
            4 => {
                prefix.text.insert(vec![65], Some(2));
            }
            5 => {
                prefix.text.insert(vec![65; MAX_ENTRIES], None);
            }
            _ => {
                prefixes.axes.insert((1, true, 0), BTreeMap::new());
            }
        }
        assert!(prefixes.restore(&book).is_err(), "case {malformed}");
    }
    let mut prefixes = LookupPrefixes::default();
    prefixes
        .axes
        .insert((0, true, 0), [((2, 1), SearchPrefix::default())].into());
    assert!(prefixes.restore(&book).is_err());
}
