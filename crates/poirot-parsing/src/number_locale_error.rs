use crate::number_locale_token::NumberLocaleToken;

/// Failure to validate numeric-locale separators and affixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NumberLocaleError {
    /// An enabled token contains no characters.
    #[error("numeric locale {token:?} token is empty")]
    EmptyToken {
        /// Role of the rejected token.
        token: NumberLocaleToken,
    },
    /// A token contains a control character.
    #[error("numeric locale {token:?} token contains control character {character:?}")]
    ControlCharacter {
        /// Role of the rejected token.
        token: NumberLocaleToken,
        /// Rejected input character.
        character: char,
    },
    /// A separator contains a character reserved by the numeric grammar.
    #[error("numeric locale {token:?} token contains numeric syntax {character:?}")]
    NumericSyntax {
        /// Role of the rejected token.
        token: NumberLocaleToken,
        /// Reserved input character.
        character: char,
    },
    /// A non-grouping token contains whitespace.
    #[error("numeric locale {token:?} token contains whitespace {character:?}")]
    Whitespace {
        /// Role of the rejected token.
        token: NumberLocaleToken,
        /// Rejected whitespace character.
        character: char,
    },
    /// Two tokens share a prefix and cannot be distinguished by the grammar.
    #[error("numeric locale {first:?} and {second:?} tokens overlap")]
    OverlappingTokens {
        /// Earlier token in locale validation order.
        first: NumberLocaleToken,
        /// Later token sharing the prefix.
        second: NumberLocaleToken,
    },
}
