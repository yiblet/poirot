/// Possible external behavior, ordered Tracked <= Volatile <= Opaque.
/// Opaque subsumes volatility: both require refresh, and opaque behavior additionally
/// lacks a complete tracked-input contract. This order does not authorize reordering calls.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ExternalBehavior {
    /// All external inputs have tracked identities.
    #[default]
    Tracked,
    /// Refresh on every evaluation despite a declared read contract.
    Volatile,
    /// Unknown external inputs or effects prevent reuse.
    Opaque,
}
