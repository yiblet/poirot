use super::mappings;

/// Japanese width conversion for ASCII, spaces, punctuation and katakana.
/// Units outside the explicit ASCII/kana mappings, including unpaired UTF-16
/// surrogates, are preserved. This is a fixed Japanese mapping, not general Unicode
/// normalization or a locale-selected Chinese/Korean conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidthConversion {
    /// Convert fullwidth ASCII and Japanese kana to their halfwidth forms.
    Narrow,
    /// Convert ASCII and halfwidth Japanese kana to fullwidth forms.
    Wide,
}

impl WidthConversion {
    /// Yield converted UTF-16 units without allocation so callers can measure output.
    /// Narrowing splits voiced kana into a base and mark. Widening combines supported
    /// halfwidth base/mark pairs into one kana.
    /// Standalone halfwidth voicing marks widen to spacing marks U+309B/U+309C.
    pub fn units(self, text: &[u16]) -> impl Iterator<Item = u16> + Clone + '_ {
        let mut input = text.iter().copied().peekable();
        let mut pending = None;
        std::iter::from_fn(move || {
            if let Some(unit) = pending.take() {
                return Some(unit);
            }
            let unit = input.next()?;
            Some(match self {
                Self::Narrow => match unit {
                    // SAFETY: the matched range is above the subtracted offset.
                    0xff01..=0xff5e => unit - 0xfee0,
                    0x3000 => 0x20,
                    _ => match mappings::narrow(unit) {
                        Some((base, mark)) => {
                            pending = mark;
                            base
                        }
                        None => unit,
                    },
                },
                Self::Wide => match unit {
                    // SAFETY: the matched range plus the offset is at most FF5E.
                    0x21..=0x7e => unit + 0xfee0,
                    0x20 => 0x3000,
                    _ => {
                        if let Some(composed) =
                            input.peek().and_then(|mark| mappings::compose(unit, *mark))
                        {
                            input.next();
                            composed
                        } else {
                            mappings::wide(unit).unwrap_or(unit)
                        }
                    }
                },
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WidthConversion;

    #[test]
    fn halfwidth_japanese_units_roundtrip_individually() {
        // The FF61..FF9F block is exactly the Japanese narrow-decomposition
        // subset of Unicode 17.0.0 used by this module, including punctuation.
        for unit in 0xff61..=0xff9f {
            let wide: Vec<_> = WidthConversion::Wide.units(&[unit]).collect();
            let narrow: Vec<_> = WidthConversion::Narrow.units(&wide).collect();
            assert_eq!(narrow, [unit], "{unit:04x}");
        }
        for unit in 0x20..=0x7e {
            let wide: Vec<_> = WidthConversion::Wide.units(&[unit]).collect();
            let narrow: Vec<_> = WidthConversion::Narrow.units(&wide).collect();
            assert_eq!(narrow, [unit], "{unit:04x}");
        }
    }

    #[test]
    fn canonical_katakana_voicing_roundtrips() {
        for text in [
            "ガギグゲゴ",
            "ザジズゼゾ",
            "ダヂヅデド",
            "バビブベボ",
            "パピプペポ",
            "ヴヷヺ",
        ] {
            let input: Vec<_> = text.encode_utf16().collect();
            let narrow: Vec<_> = WidthConversion::Narrow.units(&input).collect();
            assert_eq!(narrow.len(), input.len() * 2);
            let wide: Vec<_> = WidthConversion::Wide.units(&narrow).collect();
            assert_eq!(wide, input);
        }
    }

    #[test]
    fn preserves_units_outside_the_selected_width_contract() {
        let input = [
            0xd800, 0x05d0, 0x00e9, 0x0301, 0x2460, 0xfb03, 0xdc00, 0x3099, 0x309a,
        ];
        assert_eq!(
            WidthConversion::Wide.units(&input).collect::<Vec<_>>(),
            input
        );
        let unchanged = &input[..7];
        assert_eq!(
            WidthConversion::Narrow.units(unchanged).collect::<Vec<_>>(),
            unchanged
        );
        // Widening only composes halfwidth pairs, not arbitrary canonical
        // sequences. Unrelated combining marks remain in their source order.
        assert_eq!(
            WidthConversion::Wide
                .units(&[0xff76, 0x3099, 0x0301])
                .collect::<Vec<_>>(),
            [0x30ab, 0x3099, 0x0301]
        );
    }
}
