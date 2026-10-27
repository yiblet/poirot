/// Advisory locality hint whose admission was checked during the existing key pass.
#[derive(Clone, Copy)]
pub(super) enum ScalarLocality {
    /// Existing storage limits exclude a matcher; retain ordinary scalar proof reuse.
    Ineligible,
    /// Hash selects promotion only; exact spans and decoded cells authorize reuse.
    Eligible(u64),
}
