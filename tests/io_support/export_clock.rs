//! Export comparison permits only the backend's independent creation clocks.
use std::ops::Range;
fn timestamp(bytes: &[u8]) -> bool {
    if bytes.len() != 20 {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        let expected = match i {
            4 | 7 => Some(b'-'),
            10 => Some(b'T'),
            13 | 16 => Some(b':'),
            19 => Some(b'Z'),
            _ => None,
        };
        if expected.map_or(!b.is_ascii_digit(), |e| b != e) {
            return false;
        }
    }
    let number = |start: usize, end: usize| {
        bytes[start..end]
            .iter()
            .fold(0u32, |n, b| n * 10 + u32::from(b - b'0'))
    };
    let year = number(0, 4);
    let month = number(5, 7);
    let day = number(8, 10);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return false,
    };
    year > 0
        && day >= 1
        && day <= days
        && number(11, 13) < 24
        && number(14, 16) < 60
        && number(17, 19) < 60
}
fn text_range(xml: &str, name: &str) -> Result<Range<usize>, &'static str> {
    let prefix = format!("<{name}");
    let close = format!("</{name}>");
    let mut matches = xml.match_indices(&prefix);
    let (start, _) = matches.next().ok_or("missing timestamp element")?;
    if matches.next().is_some() || xml.matches(&close).count() != 1 {
        return Err("duplicate or missing timestamp element");
    }
    let tail = &xml[start + prefix.len()..];
    if !tail.starts_with('>') && !tail.starts_with(char::is_whitespace) {
        return Err("invalid timestamp element name");
    }
    let begin = start + prefix.len() + tail.find('>').ok_or("missing timestamp opening end")? + 1;
    let end = begin
        + xml[begin..]
            .find(&close)
            .ok_or("missing timestamp closing end")?;
    if !timestamp(&xml.as_bytes()[begin..end]) {
        return Err("malformed UTC creation timestamp");
    }
    Ok(begin..end)
}
/// Normalize generated creation-clock metadata for deterministic export comparisons.
pub(super) fn core_without_creation_clock(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let xml = std::str::from_utf8(bytes).map_err(|_| "non UTF8 core XML")?;
    let created = text_range(xml, "dcterms:created")?;
    let modified = text_range(xml, "dcterms:modified")?;
    if xml[created.clone()] != xml[modified.clone()] {
        return Err("created and modified clocks differ within export");
    }
    let mut result = bytes.to_vec();
    // Replace text only after validating the complete exact20-byte UTC values.
    // Tags, attributes, namespaces, creator and every other byte remain exact.
    for range in [created, modified] {
        result[range].copy_from_slice(b"0000-00-00T00:00:00Z")
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn core(date: &str) -> String {
        format!(
            "<cp:coreProperties xmlns:cp=\"urn:core\"><dc:creator>Ada</dc:creator><dcterms:created xsi:type=\"dcterms:W3CDTF\">{date}</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">{date}</dcterms:modified><payload>keep</payload></cp:coreProperties>"
        )
    }
    #[test]
    fn independent_valid_clocks_preserve_every_other_byte() {
        let a = core("2026-10-04T22:22:51Z");
        let b = core("2026-10-04T22:22:52Z");
        let normalized = core_without_creation_clock(a.as_bytes()).unwrap();
        assert_eq!(
            normalized,
            core_without_creation_clock(b.as_bytes()).unwrap()
        );
        for altered in [
            b.replace("Ada", "Grace"),
            b.replace("W3CDTF", "changed"),
            b.replace("keep", "other"),
            b.replace("urn:core", "urn:other"),
        ] {
            assert_ne!(
                normalized,
                core_without_creation_clock(altered.as_bytes()).unwrap()
            );
        }
    }
    #[test]
    fn missing_duplicate_or_malformed_clocks_are_rejected() {
        let valid = core("2026-10-04T22:22:51Z");
        for malformed in [
            valid.replace("dcterms:created", "other:created"),
            valid.replace("dcterms:modified", "other:modified"),
            valid.replace(
                "</cp:coreProperties>",
                "<dcterms:created>2026-10-04T22:22:51Z</dcterms:created></cp:coreProperties>",
            ),
            valid.replace(
                "</cp:coreProperties>",
                "<dcterms:modified>2026-10-04T22:22:51Z</dcterms:modified></cp:coreProperties>",
            ),
            valid.replacen("2026-10-04T22:22:51Z", "2026-10-04T22:22:52Z", 1),
            core("2026-10-04T22:22:51+00:00"),
            core("2026-10-04T22:22:51z"),
            core("2026-10-04 22:22:51Z"),
            core("2026-02-30T22:22:51Z"),
            core("2026-10-04T25:22:51Z"),
            core("2026-10-0xT22:22:51Z"),
        ] {
            assert!(
                core_without_creation_clock(malformed.as_bytes()).is_err(),
                "{malformed}"
            );
        }
    }
}
