//! Metered wildcard normalization and dynamic-programming matching.
use super::*;

// Bounded dynamic programming; '~' escapes wildcards and matching is case-insensitive.
/// Match the entire value with case-insensitive, tilde-escaped wildcard semantics.
pub(super) fn wildcard(pattern: &str, value: &str, budget: &mut BudgetMeter) -> EvalResult<bool> {
    wildcard_impl(pattern, value, false, budget)
}
/// Match a value prefix under the same metered wildcard rules.
pub(super) fn wildcard_prefix(
    pattern: &str,
    value: &str,
    budget: &mut BudgetMeter,
) -> EvalResult<bool> {
    wildcard_impl(pattern, value, true, budget)
}
fn wildcard_impl(
    pattern: &str,
    value: &str,
    prefix: bool,
    budget: &mut BudgetMeter,
) -> EvalResult<bool> {
    budget.text(pattern.len())?;
    budget.text(value.len())?;
    budget.check_allocation(pattern.chars().flat_map(char::to_lowercase).count())?;
    budget.check_allocation(
        value
            .chars()
            .flat_map(char::to_lowercase)
            .count()
            .saturating_add(1),
    )?;
    budget.text(
        pattern
            .chars()
            .flat_map(char::to_lowercase)
            .map(char::len_utf8)
            .sum(),
    )?;
    budget.text(
        value
            .chars()
            .flat_map(char::to_lowercase)
            .map(char::len_utf8)
            .sum(),
    )?;
    let mut tokens = Vec::new();
    let mut p = pattern
        .to_lowercase()
        .chars()
        .collect::<Vec<_>>()
        .into_iter();
    while let Some(c) = p.next() {
        if c == '~' {
            tokens.push((p.next().unwrap_or('~'), false))
        } else {
            tokens.push((c, c == '*' || c == '?'))
        }
    }
    let value: Vec<_> = value.to_lowercase().chars().collect();
    budget.wildcard((tokens.len() as u64).saturating_mul(value.len() as u64 + 1))?;
    budget.allocation(value.len() + 1)?;
    let mut dp = vec![false; value.len() + 1];
    // SAFETY: Both local state vectors have value.len() + 1 entries; j ranges from one through value.len(), so j/j-1 and the terminal index are in bounds.
    dp[0] = true;
    for (c, special) in tokens {
        budget.allocation(value.len() + 1)?;
        let mut next = vec![false; value.len() + 1];
        if special && c == '*' {
            // SAFETY: Both local state vectors have value.len() + 1 entries; j ranges from one through value.len(), so j/j-1 and the terminal index are in bounds.
            next[0] = dp[0];
            for j in 1..=value.len() {
                // SAFETY: Both local state vectors have value.len() + 1 entries; j ranges from one through value.len(), so j/j-1 and the terminal index are in bounds.
                next[j] = dp[j] || next[j - 1]
            }
        } else {
            for j in 1..=value.len() {
                // SAFETY: Both local state vectors have value.len() + 1 entries; j ranges from one through value.len(), so j/j-1 and the terminal index are in bounds.
                next[j] = dp[j - 1] && ((special && c == '?') || c == value[j - 1])
            }
        }
        dp = next
    }
    Ok(if prefix {
        dp.iter().any(|b| *b)
    } else {
        // SAFETY: Both local state vectors have value.len() + 1 entries; j ranges from one through value.len(), so j/j-1 and the terminal index are in bounds.
        dp[value.len()]
    })
}
