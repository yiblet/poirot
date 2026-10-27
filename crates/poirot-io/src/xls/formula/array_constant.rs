//! PtgExtraArray decoding for BIFF8 array constants.
use super::decompile::error_text;
use crate::xls::preflight::{PreflightError, ResourceBudget};
use crate::xls::result::Result;
use crate::xls::text::{take, text, word};

/// Render the next [MS-XLS] PtgExtraArray as `{a,b;c,d}` source
/// and advance `extra` past it.
///
/// The header stores columns - 1 (one byte) then rows - 1 (two bytes); SerAr
/// elements (SerNil, SerNum, SerStr, SerBool, SerErr) follow in row-major order. Each element is
/// charged to `tokens` against `max_metadata_entries` before any is read, and
/// the rendered text is bounded by `max_string_bytes`.
pub(super) fn array_constant(
    extra: &mut &[u8],
    tokens: &mut usize,
    budget: ResourceBudget,
) -> Result<String> {
    let columns = take(extra, 1)?
        .first()
        .map(|&columns| usize::from(columns))
        .ok_or(PreflightError::Malformed("missing byte"))?
        .checked_add(1)
        .ok_or(PreflightError::Representation(
            "array constant size overflow",
        ))?;
    let count = usize::from(word(extra)?)
        .checked_add(1)
        .and_then(|rows| rows.checked_mul(columns))
        .ok_or(PreflightError::Representation(
            "array constant size overflow",
        ))?;
    *tokens = tokens
        .checked_add(count)
        .ok_or(PreflightError::Representation(
            "formula token count overflow",
        ))?;
    if *tokens > budget.max_metadata_entries {
        return Err(PreflightError::Resource {
            resource: "BIFF formula tokens",
            requested: *tokens as u64,
            limit: budget.max_metadata_entries as u64,
        });
    }
    let mut out = String::from("{");
    for index in 0..count {
        if index > 0 {
            // SAFETY: columns is a checked_add(1) result, so it is never zero.
            out.push(if index % columns == 0 { ';' } else { ',' });
        }
        let kind = *take(extra, 1)?
            .first()
            .ok_or(PreflightError::Malformed("missing byte"))?;
        match kind {
            // SerNil: eight reserved bytes; renders as an omitted element.
            0x00 => {
                take(extra, 8)?;
            }
            // SerNum: Xnum.
            0x01 => {
                let value = take(extra, 8)?;
                let value = f64::from_le_bytes(
                    value
                        .try_into()
                        .map_err(|_| PreflightError::Malformed("array number"))?,
                );
                out.push_str(&value.to_string());
            }
            // SerStr: XLUnicodeString (cch, fHighByte, characters).
            0x02 => {
                let length = usize::from(word(extra)?);
                let value = text(extra, length)?;
                out.push('"');
                out.push_str(&value.replace('"', "\"\""));
                out.push('"');
            }
            // SerBool: fBool then seven reserved bytes.
            0x04 => {
                let value = take(extra, 8)?;
                out.push_str(if value.first() == Some(&0) {
                    "FALSE"
                } else {
                    "TRUE"
                });
            }
            // SerErr: BErr then seven reserved bytes.
            0x10 => {
                let value = take(extra, 8)?;
                let code = *value
                    .first()
                    .ok_or(PreflightError::Malformed("missing byte"))?;
                out.push_str(error_text(code)?);
            }
            _ => return Err(PreflightError::Malformed("unknown array constant type")),
        }
        if out.len() > budget.max_string_bytes {
            return Err(PreflightError::Resource {
                resource: "BIFF formula bytes",
                requested: out.len() as u64,
                limit: budget.max_string_bytes as u64,
            });
        }
    }
    out.push('}');
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Original synthetic PtgArray/RgbExtra bytes; layouts follow [MS-XLS] Ptg structures.
    use super::super::add_ins::AddIns;
    use super::super::decompile::formula_with_addins;
    use super::super::parsed_formula::ParsedFormula;
    use crate::xls::preflight::{PreflightError, ResourceBudget};

    /// PtgArray (value class) with its seven unused bytes.
    const ARRAY: [u8; 8] = [0x40, 0, 0, 0, 0, 0, 0, 0];

    fn decode(
        rgce: &[u8],
        rgcb: &[u8],
        budget: ResourceBudget,
    ) -> Result<Option<String>, PreflightError> {
        formula_with_addins(
            ParsedFormula { rgce, rgcb },
            &[],
            &[],
            &[],
            &AddIns::default(),
            None,
            budget,
        )
    }

    fn number(value: f64) -> Vec<u8> {
        let mut out = vec![0x01];
        out.extend(value.to_le_bytes());
        out
    }

    #[test]
    fn nil_elements_render_empty_and_strings_double_quotes() {
        // Two columns, one row: SerNil then SerStr `a"b` (compressed).
        let mut rgcb = vec![1, 0, 0, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        rgcb.extend([0x02, 3, 0, 0, b'a', b'"', b'b']);
        assert_eq!(
            decode(&ARRAY, &rgcb, ResourceBudget::default()).unwrap(),
            Some("{,\"a\"\"b\"}".into())
        );
    }

    #[test]
    fn memory_area_extra_data_precedes_a_later_array() {
        // PtgMemArea (cce 3) over PtgInt 7, then PtgArray and PtgAdd. RgbExtra
        // holds PtgExtraMem (one Ref8U) before the PtgExtraArray.
        let mut rgce = vec![0x26, 0, 0, 0, 0, 3, 0, 0x1e, 7, 0];
        rgce.extend(ARRAY);
        rgce.push(0x03);
        let mut rgcb = vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        rgcb.extend([0, 0, 0]);
        rgcb.extend(number(1.5));
        assert_eq!(
            decode(&rgce, &rgcb, ResourceBudget::default()).unwrap(),
            Some("7+{1.5}".into())
        );
    }

    #[test]
    fn array_elements_and_text_are_budgeted() {
        let mut rgcb = vec![1, 0, 0];
        rgcb.extend(number(1.));
        rgcb.extend(number(2.));
        let budget = ResourceBudget::default();
        assert_eq!(
            decode(
                &ARRAY,
                &rgcb,
                ResourceBudget {
                    max_metadata_entries: 2,
                    ..budget
                }
            ),
            Err(PreflightError::Resource {
                resource: "BIFF formula tokens",
                requested: 3,
                limit: 2,
            })
        );
        assert_eq!(
            decode(
                &ARRAY,
                &rgcb,
                ResourceBudget {
                    max_metadata_entries: 3,
                    ..budget
                }
            )
            .unwrap(),
            Some("{1,2}".into())
        );
        assert!(matches!(
            decode(
                &ARRAY,
                &rgcb,
                ResourceBudget {
                    max_string_bytes: 3,
                    ..budget
                }
            ),
            Err(PreflightError::Resource {
                resource: "BIFF formula bytes",
                ..
            })
        ));
        // The largest declared shape (256 x 65,536) is refused before any element is read.
        assert!(matches!(
            decode(
                &ARRAY,
                &[0xff, 0xff, 0xff],
                ResourceBudget {
                    max_metadata_entries: 1_000_000,
                    ..budget
                }
            ),
            Err(PreflightError::Resource {
                requested: 16_777_217,
                ..
            })
        ));
    }

    #[test]
    fn truncated_or_unknown_array_data_is_malformed() {
        let mut rgcb = vec![1, 0, 0];
        rgcb.extend(number(1.));
        rgcb.extend(number(2.));
        for length in 0..rgcb.len() {
            assert!(
                matches!(
                    decode(&ARRAY, &rgcb[..length], ResourceBudget::default()),
                    Err(PreflightError::Malformed(_))
                ),
                "{length}"
            );
        }
        for length in 1..ARRAY.len() {
            assert!(matches!(
                decode(&ARRAY[..length], &rgcb, ResourceBudget::default()),
                Err(PreflightError::Malformed(_))
            ));
        }
        for kind in [0x03, 0x08, 0xff] {
            let mut bad = vec![0, 0, 0, kind];
            bad.extend([0; 8]);
            assert_eq!(
                decode(&ARRAY, &bad, ResourceBudget::default()),
                Err(PreflightError::Malformed("unknown array constant type"))
            );
        }
        // Unknown SerErr codes keep the BErr validation.
        assert_eq!(
            decode(
                &ARRAY,
                &[0, 0, 0, 0x10, 0x01, 0, 0, 0, 0, 0, 0, 0],
                ResourceBudget::default()
            ),
            Err(PreflightError::Malformed("unknown BIFF error code"))
        );
    }
}
