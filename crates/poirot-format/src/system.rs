/// Special Excel system formats, distinct from concrete locale identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemDateTime {
    /// System long-date selector resolved using the pinned regional contract.
    LongDate,
    /// System long-time selector resolved using the pinned regional contract.
    LongTime,
}
/// Recognize a supported system date/time selector in format tokens.
pub(super) fn selector(directive: &str) -> Option<SystemDateTime> {
    match directive {
        "$-f800" => Some(SystemDateTime::LongDate),
        "$-f400" => Some(SystemDateTime::LongTime),
        _ => None,
    }
}
/// Fixed bounded policy tokens are allocated during pure plan compilation only.
pub(super) fn configured_tokens(selector: SystemDateTime) -> Vec<super::Token> {
    use super::Token;
    match selector {
        SystemDateTime::LongDate => vec![
            Token::Date('d', 4),
            Token::Literal(", ".into()),
            Token::Date('m', 4),
            Token::Literal(" ".into()),
            Token::Date('d', 1),
            Token::Literal(", ".into()),
            Token::Date('y', 4),
        ],
        SystemDateTime::LongTime => vec![
            Token::Date('h', 1),
            Token::Literal(":".into()),
            Token::Date('m', 2),
            Token::Literal(":".into()),
            Token::Date('s', 2),
            Token::Literal(" ".into()),
            Token::AmPm(true),
        ],
    }
}
