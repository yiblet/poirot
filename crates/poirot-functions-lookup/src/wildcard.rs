use crate::operands::{charge, checked_product};
use crate::{Failure, OperationalFailure, Work, Workbook};
/// Match supported wildcard syntax with host-metered matching states.
pub(super) fn wildcard_matches(
    pattern: &str,
    text: &str,
    w: &mut impl Workbook,
) -> Result<bool, Failure> {
    let rows = pattern
        .len()
        .checked_add(1)
        .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))?;
    let cols = text
        .len()
        .checked_add(1)
        .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))?;
    charge(
        w,
        Work::SearchText,
        pattern
            .len()
            .checked_add(text.len())
            .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))?,
    )?;
    charge(w, Work::WildcardStates, checked_product(rows, cols)?)?;
    Ok(wildcard_kernel(pattern, text))
}
/// Dynamic-programming matching after the original text/state admissions.
fn wildcard_kernel(pattern: &str, text: &str) -> bool {
    /// Compiled literal, wildcard, or escape atom used by the matcher.
    #[derive(Clone, Copy)]
    enum Token {
        Star,
        Any,
        Literal(char),
    }
    let mut tokens = Vec::new();
    let lowercase = pattern.to_lowercase();
    let mut chars = lowercase.chars();
    while let Some(ch) = chars.next() {
        tokens.push(match ch {
            '*' => Token::Star,
            '?' => Token::Any,
            '~' => Token::Literal(chars.next().unwrap_or('~')),
            c => Token::Literal(c),
        });
    }
    let letters: Vec<char> = text.to_lowercase().chars().collect();
    let mut previous = vec![false; letters.len() + 1];
    // SAFETY: The state buffers are allocated with letters.len() + 1 entries, so zero and letters.len() are valid indices.
    previous[0] = true;
    for token in tokens {
        let mut next = vec![false; letters.len() + 1];
        if matches!(token, Token::Star) {
            // SAFETY: The state buffers are allocated with letters.len() + 1 entries, so zero and letters.len() are valid indices.
            next[0] = previous[0];
        }
        for j in 1..=letters.len() {
            // SAFETY: Both state buffers have letters.len() + 1 elements; j ranges from 1 through letters.len(), so j and j - 1 are in bounds.
            next[j] = match token {
                // SAFETY: Both state buffers have letters.len() + 1 elements; j ranges from 1 through letters.len(), so j and j - 1 are in bounds.
                Token::Star => previous[j] || next[j - 1],
                // SAFETY: Both state buffers have letters.len() + 1 elements; j ranges from 1 through letters.len(), so j and j - 1 are in bounds.
                Token::Any => previous[j - 1],
                // SAFETY: Both state buffers have letters.len() + 1 elements; j ranges from 1 through letters.len(), so j and j - 1 are in bounds.
                Token::Literal(c) => previous[j - 1] && c == letters[j - 1],
            };
        }
        previous = next;
    }
    // SAFETY: The state buffers are allocated with letters.len() + 1 entries, so zero and letters.len() are valid indices.
    previous[letters.len()]
}
