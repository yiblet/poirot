//! Thai currency spelling, independently implemented without third-party code or data.
//!
//! Microsoft establishes the 1234 example:
//! <https://support.microsoft.com/en-us/excel/functions/bahttext-function>
//! Thai numeral words are language vocabulary, not a copied mapping table.
//! This implementation uses เอ็ด for a final one only when the tens digit is
//! nonzero. This differs from the Royal Institute's broader modern Thai reading.
//! The Royal Institute documents the broader reading (semantic reference only):
//! <https://www.orst.go.th/iwfm_list.asp?i=0040002104011001%2F63ERK0020019%24%400001>
//! Excel edge parity beyond Microsoft's example has not been independently measured.

/// Yield Thai currency words from separate integer baht and satang amounts.
/// Return `None` if satang is 100 or more; the caller performs decimal rounding.
/// Zero baht is omitted when satang is nonzero, and zero has no negative sign.
/// The cloneable iterator lets callers measure output before allocation.
pub fn words(
    baht: u64,
    satang: u8,
    negative: bool,
) -> Option<impl Iterator<Item = &'static str> + Clone> {
    if satang >= 100 {
        return None;
    }
    let sign = (negative && (baht != 0 || satang != 0)).then_some("ลบ");
    let zero = (baht == 0 && satang == 0).then_some("ศูนย์");
    let baht_suffix = (baht != 0 || satang == 0).then_some("บาท");
    let ending = if satang == 0 {
        "ถ้วน"
    } else {
        "สตางค์"
    };
    Some(
        sign.into_iter()
            .chain(zero)
            .chain(integer(baht))
            .chain(baht_suffix)
            .chain(integer(u64::from(satang)))
            .chain([ending]),
    )
}

/// Spell groups of six digits, repeating the million marker across empty groups.
fn integer(number: u64) -> impl Iterator<Item = &'static str> + Clone {
    let mut started = false;
    [1_000_000_000_000_000_000, 1_000_000_000_000, 1_000_000, 1]
        .into_iter()
        .flat_map(move |divisor| {
            // SAFETY: Every divisor in the local array is nonzero.
            let group = number / divisor % 1_000_000;
            started |= group != 0;
            group_words(group).chain((started && divisor > 1).then_some("ล้าน"))
        })
}

/// Spell a six-digit group using the special ten, twenty, and trailing-one forms.
fn group_words(number: u64) -> impl Iterator<Item = &'static str> + Clone {
    [
        (100_000, "แสน"),
        (10_000, "หมื่น"),
        (1_000, "พัน"),
        (100, "ร้อย"),
        (10, "สิบ"),
        (1, ""),
    ]
    .into_iter()
    .flat_map(move |(divisor, place)| {
        let digit = number / divisor % 10;
        let word = match digit {
            0 => None,
            1 if divisor == 10 => None,
            1 if divisor == 1 && !(number / 10).is_multiple_of(10) => Some("เอ็ด"),
            1 => Some("หนึ่ง"),
            2 if divisor == 10 => Some("ยี่"),
            2 => Some("สอง"),
            3 => Some("สาม"),
            4 => Some("สี่"),
            5 => Some("ห้า"),
            6 => Some("หก"),
            7 => Some("เจ็ด"),
            8 => Some("แปด"),
            _ => Some("เก้า"),
        };
        word.into_iter()
            .chain((digit != 0 && divisor != 1).then_some(place))
    })
}

#[cfg(test)]
mod tests {
    use super::words;

    #[test]
    fn independently_derived_currency_and_group_rules() {
        // The first example is Microsoft's published contract. Remaining
        // expectations derive from the explicit Thai positional rules above;
        // they are not claims of observed Excel edge-case parity.
        for (baht, satang, negative, expected) in [
            (1234, 0, false, "หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน"),
            (0, 0, true, "ศูนย์บาทถ้วน"),
            (0, 1, false, "หนึ่งสตางค์"),
            (0, 11, true, "ลบสิบเอ็ดสตางค์"),
            (1, 0, false, "หนึ่งบาทถ้วน"),
            (10, 0, false, "สิบบาทถ้วน"),
            (21, 25, false, "ยี่สิบเอ็ดบาทยี่สิบห้าสตางค์"),
            (101, 0, false, "หนึ่งร้อยหนึ่งบาทถ้วน"),
            (987654, 0, false, "เก้าแสนแปดหมื่นเจ็ดพันหกร้อยห้าสิบสี่บาทถ้วน"),
            (1_000_001, 0, false, "หนึ่งล้านหนึ่งบาทถ้วน"),
            (21_000_000, 0, false, "ยี่สิบเอ็ดล้านบาทถ้วน"),
            (1_000_000_000_000, 0, false, "หนึ่งล้านล้านบาทถ้วน"),
            (1_000_000_000_000_000_000, 0, false, "หนึ่งล้านล้านล้านบาทถ้วน"),
        ] {
            assert_eq!(
                words(baht, satang, negative).unwrap().collect::<String>(),
                expected
            );
        }
        assert!(words(1, 100, false).is_none());
    }
}
