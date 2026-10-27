//! Deterministic encoding of complete syntax and raw compilation policy.
use super::{Condition, Format, FormatCompilePolicy, Token};
use std::hash::Hasher;

/// Accumulates a deterministic fingerprint of compiled format settings for cache diagnostics.
/// The result is noncryptographic and does not replace full program equality.
struct Fingerprint(u64);
impl Hasher for Fingerprint {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write_u64(&mut self, n: u64) {
        self.write(&n.to_le_bytes());
    }
    fn write_u32(&mut self, n: u32) {
        self.write(&n.to_le_bytes());
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}
fn string(h: &mut Fingerprint, s: &str) {
    h.write_u64(s.len() as u64);
    h.write(s.as_bytes());
}
/// Hash parsed format structure and compilation policies for program identity.
pub(super) fn program_fingerprint(format: &Format, p: FormatCompilePolicy) -> u64 {
    let mut h = Fingerprint(0xcbf29ce484222325);
    for n in [
        p.max_tokens_per_section,
        p.max_literal_characters,
        p.max_output_characters,
        format.sections.len(),
    ] {
        h.write_u64(n as u64);
    }
    for s in &format.sections {
        h.write_u8(u8::from(s.condition.is_some()));
        if let Some(c) = s.condition {
            let (tag, n) = match c {
                Condition::Less(n) => (0, n),
                Condition::LessEqual(n) => (1, n),
                Condition::Equal(n) => (2, n),
                Condition::NotEqual(n) => (3, n),
                Condition::Greater(n) => (4, n),
                Condition::GreaterEqual(n) => (5, n),
            };
            h.write_u8(tag);
            h.write_u64(n.to_bits());
        }
        h.write_u8(u8::from(s.color.is_some()));
        if let Some(c) = &s.color {
            string(&mut h, c);
        }
        h.write_u64(s.tokens.len() as u64);
        for t in &s.tokens {
            match t {
                Token::SystemDateTime(selector) => {
                    h.write_u8(14);
                    h.write_u8(match selector {
                        super::super::SystemDateTime::LongDate => 0,
                        super::super::SystemDateTime::LongTime => 1,
                    });
                }
                Token::Literal(s) => {
                    h.write_u8(0);
                    string(&mut h, s);
                }
                Token::Digit(c) => {
                    h.write_u8(1);
                    h.write_u32(*c as u32);
                }
                Token::Decimal => h.write_u8(2),
                Token::Comma => h.write_u8(3),
                Token::Percent => h.write_u8(4),
                Token::Text => h.write_u8(5),
                Token::General => h.write_u8(6),
                Token::Date(c, w) => {
                    h.write_u8(7);
                    h.write_u32(*c as u32);
                    h.write_u64(*w as u64);
                }
                Token::AmPm(full) => {
                    h.write_u8(8);
                    h.write_u8(u8::from(*full));
                }
                Token::Elapsed(c, w) => {
                    h.write_u8(9);
                    h.write_u32(*c as u32);
                    h.write_u64(*w as u64);
                }
                Token::Padding(c) => {
                    h.write_u8(10);
                    h.write_u32(*c as u32);
                }
                Token::Fill(c) => {
                    h.write_u8(11);
                    h.write_u32(*c as u32);
                }
                Token::Slash => h.write_u8(12),
                Token::Exponent {
                    positive_sign,
                    digits,
                } => {
                    h.write_u8(13);
                    h.write_u8(u8::from(*positive_sign));
                    h.write_u64(*digits as u64);
                }
            }
        }
    }
    h.finish()
}
