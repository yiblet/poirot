//! Allocation-free UTF-8 URL component encoding for ENCODEURL.
//!
//! Independently implemented from Microsoft’s ENCODEURL example and RFC 3986
//! sections 2.1, 2.3 and 2.5; no third-party code or fixtures were reused.
//! <https://support.microsoft.com/en-us/excel/functions/encodeurl-function>
//! <https://www.rfc-editor.org/rfc/rfc3986.html#section-2>

/// Encode a URL component as ASCII characters.
/// Leave letters, digits and `-._~` unchanged; percent-encode every other UTF-8 byte.
/// Callers can clone the iterator to measure output before allocation.
pub fn encode_url(text: &str) -> impl Iterator<Item = char> + Clone + '_ {
    text.bytes().flat_map(|byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            [Some(char::from(byte)), None, None]
        } else {
            [
                Some('%'),
                Some(hex_digit(byte >> 4)),
                Some(hex_digit(byte & 15)),
            ]
        }
        .into_iter()
        .flatten()
    })
}

/// Render a byte's hexadecimal nibble using uppercase ASCII.
fn hex_digit(nibble: u8) -> char {
    let nibble = nibble & 15;
    if nibble < 10 {
        // SAFETY: nibble < 10, so adding ASCII zero stays below 58.
        char::from(b'0' + nibble)
    } else {
        // SAFETY: masking bounds nibble to 10..=15, so the result is 65..=70.
        char::from(b'A' + (nibble - 10))
    }
}
