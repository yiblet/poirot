//! Read worksheet row geometry and presentation together.

use quick_xml::{XmlVersion, events::BytesStart};

/// Checked row metadata used by the worksheet recovery pass.
pub(super) struct RowAttributes {
    pub(super) number: u32,
    pub(super) styled: bool,
    pub(super) hidden: bool,
}

impl RowAttributes {
    /// Read each relevant first occurrence. Malformed metadata uses the ordered caller path.
    pub(super) fn scan(event: &BytesStart<'_>, next_row: u32) -> Option<Self> {
        let mut number = None;
        let mut style_seen = false;
        let mut hidden = None;
        for item in event.attributes().with_checks(false) {
            let attribute = item.ok()?;
            match attribute.key.as_ref() {
                "r" if number.is_none() => {
                    let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
                    number = Some(value.parse::<u32>().ok()?.checked_sub(1)?);
                }
                "s" if !style_seen => {
                    attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
                    style_seen = true;
                }
                "hidden" if hidden.is_none() => {
                    let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
                    hidden = Some(matches!(value.as_ref(), "1" | "true"));
                }
                _ => continue,
            }
            if number.is_some() && style_seen && hidden.is_some() {
                break;
            }
        }
        Some(Self {
            number: number.unwrap_or(next_row),
            styled: style_seen,
            hidden: hidden.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::RowAttributes;
    use quick_xml::events::BytesStart;

    #[test]
    fn first_values_and_entities_drive_row_metadata() {
        let row = BytesStart::from_content(r#"row hidden="tru&#101;" s="2" r="7" r="8""#, 3);
        let attributes = RowAttributes::scan(&row, 2).unwrap();
        assert_eq!(attributes.number, 6);
        assert!(attributes.styled);
        assert!(attributes.hidden);

        let row = BytesStart::from_content(r#"row hidden="0" hidden="1""#, 3);
        let attributes = RowAttributes::scan(&row, 2).unwrap();
        assert_eq!(attributes.number, 2);
        assert!(!attributes.styled);
        assert!(!attributes.hidden);

        let row = BytesStart::from_content(r#"row hidden="&#0;" r="0""#, 3);
        assert!(RowAttributes::scan(&row, 2).is_none());
    }
}
