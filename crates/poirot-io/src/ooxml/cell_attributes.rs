//! Read the small set of cell metadata shared by import recovery.

use super::xml::{attr, xml};
use crate::{CellAddress, IoError};
use quick_xml::{XmlVersion, events::BytesStart};
use std::borrow::Cow;

/// Coordinate, style index and stored value kind of one worksheet cell.
pub(super) struct CellAttributes<'a> {
    /// Explicit address, or the next sequential column in the current row.
    pub(super) address: CellAddress,
    /// Style table index, defaulting to the workbook's first style.
    pub(super) style: usize,
    /// OOXML storage kind. Its default is a numeric cell.
    pub(super) kind: Option<Cow<'a, str>>,
}

impl<'a> CellAttributes<'a> {
    /// Read the three attributes together. Preserve diagnostic priority on malformed input.
    pub(super) fn read(
        event: &'a BytesStart<'_>,
        row: u32,
        next_col: u32,
    ) -> Result<Self, IoError> {
        if let Some(attributes) = Self::scan(event, row, next_col) {
            return Ok(attributes);
        }
        let address = match attr(event, "r")? {
            Some(address) => address.parse()?,
            None => CellAddress::new(row, next_col)?,
        };
        let style = attr(event, "s")?
            .map(|style| style.parse::<usize>().map_err(xml))
            .transpose()?
            .unwrap_or(0);
        let kind = attr(event, "t")?.map(Cow::Owned);
        Ok(Self {
            address,
            style,
            kind,
        })
    }

    /// Scan well-formed attributes once with the original first-match rules.
    fn scan(event: &'a BytesStart<'_>, row: u32, next_col: u32) -> Option<Self> {
        let mut address = None;
        let mut style = None;
        let mut kind = None;
        for item in event.attributes().with_checks(false) {
            let attribute = item.ok()?;
            let field = match attribute.key.as_ref() {
                "r" => &mut address,
                "s" => &mut style,
                "t" => &mut kind,
                _ => continue,
            };
            if field.is_none() {
                *field = Some(attribute.normalized_value(XmlVersion::Implicit1_0).ok()?);
                if address.is_some() && style.is_some() && kind.is_some() {
                    break;
                }
            }
        }
        let address = match address {
            Some(address) => address.parse().ok()?,
            None => CellAddress::new(row, next_col).ok()?,
        };
        let style = style.map_or(Some(0), |style| style.parse().ok())?;
        Some(Self {
            address,
            style,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CellAttributes;
    use quick_xml::events::BytesStart;

    #[test]
    fn first_attributes_and_decoded_entities_drive_cell_metadata() {
        let event = BytesStart::from_content(r#"c s="2" t="in&#108;ineStr" r="C5" s="7""#, 1);
        let cell = CellAttributes::read(&event, 0, 0).unwrap();
        assert_eq!(cell.address.to_string(), "C5");
        assert_eq!(cell.style, 2);
        assert_eq!(cell.kind.as_deref(), Some("inlineStr"));
    }
}
