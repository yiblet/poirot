/// Metadata describing a builtin's operand/arity evaluation protocol.
/// The family executor enforces the actual function-specific ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvaluationStrategy {
    /// Evaluate only operands selected by the handler.
    Lazy,
    /// Inspect eager operand errors before checking argument count.
    EagerErrorsBeforeArity,
    /// Let the family executor determine operand and error ordering.
    FamilyControlled,
}
/// Conservative flags for settings or workbook services a selected handler may use.
/// These flags are not a trace of actual argument or cell reads.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct DependencyPolicy(u16);
impl DependencyPolicy {
    /// No possible host dependency.
    pub const NONE: Self = Self(0);
    /// May consume execution policy, text/number coercion, or formatting settings.
    pub const HOST_POLICY: Self = Self(512);
    /// May consume argument values.
    pub const ARGUMENTS: Self = Self(1);
    /// May consume the formula's coordinates.
    pub const POSITION: Self = Self(2);
    /// May consume the evaluation clock.
    pub const TIME: Self = Self(4);
    /// May consume localization settings.
    pub const LOCALIZATION: Self = Self(8);
    /// May consume the workbook date system.
    pub const DATE_SYSTEM: Self = Self(16);
    /// May select or inspect references dynamically.
    pub const REFERENCE_SELECTION: Self = Self(32);
    /// May consume currency formatting settings.
    pub const CURRENCY: Self = Self(256);
    /// May resolve workbook names or sheet identities.
    pub const NAMESPACE: Self = Self(64);
    /// May consume the random seed.
    pub const SEED: Self = Self(128);
    /// Combined dependencies of cell-specific random handlers.
    pub const RANDOM_READS: Self = Self::REFERENCE_READS
        .union(Self::SEED)
        .union(Self::POSITION);
    /// Random draws plus locale/time-sensitive numeric option coercion.
    pub const RANDOM_NUMERIC: Self = Self::RANDOM_READS.union(Self::NUMERIC_COERCION);
    /// Combine possible dependency categories.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Test whether all categories in `other` are present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    // Possible reads guard a selected handler conservatively. DependencySink records
    // actual reads during execution; this metadata does not establish consumption.
    /// Arguments plus formula coordinates.
    pub const POSITION_READS: Self = Self::ARGUMENTS.union(Self::POSITION);
    /// Clock conversion dependencies.
    pub const TIME_READS: Self = Self::TIME
        .union(Self::LOCALIZATION)
        .union(Self::DATE_SYSTEM);
    /// Conservative dependencies shared by date handlers.
    pub const DATE_FAMILY: Self = Self::ARGUMENTS
        .union(Self::TIME)
        .union(Self::LOCALIZATION)
        .union(Self::DATE_SYSTEM);
    /// Argument, reference-selection, position, and namespace dependencies.
    pub const REFERENCE_READS: Self = Self::ARGUMENTS
        .union(Self::REFERENCE_SELECTION)
        .union(Self::POSITION)
        .union(Self::NAMESPACE);
    /// Reference reads plus currency formatting settings.
    pub const CURRENCY_FORMATTING: Self = Self::REFERENCE_READS.union(Self::CURRENCY);
    // OperandResolver numeric datetime fallback uses source-1900 serials,
    // independently of the workbook date system. Actual reads are host-recorded.
    /// Reference reads plus clock and localization used by numeric date coercion.
    pub const NUMERIC_COERCION: Self = Self::REFERENCE_READS
        .union(Self::TIME)
        .union(Self::LOCALIZATION);
    /// Numeric scalar coercion plus the date system used to prepare calendar grids.
    pub const FORECAST_READS: Self = Self::NUMERIC_COERCION.union(Self::DATE_SYSTEM);
    /// Numeric text admission and the date system used by text formatting.
    pub const TEXT_FORMATTING: Self = Self::NUMERIC_COERCION.union(Self::DATE_SYSTEM);
}
/// Inclusive minimum and maximum argument counts accepted by a function contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Arity {
    /// Smallest accepted argument count.
    pub minimum: usize,
    /// Largest accepted argument count.
    pub maximum: usize,
}
impl Arity {
    /// Check the inclusive argument-count bounds.
    pub fn accepts(self, count: usize) -> bool {
        (self.minimum..=self.maximum).contains(&count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_dependency_categories_have_distinct_bits() {
        let categories = [
            DependencyPolicy::ARGUMENTS,
            DependencyPolicy::POSITION,
            DependencyPolicy::TIME,
            DependencyPolicy::LOCALIZATION,
            DependencyPolicy::DATE_SYSTEM,
            DependencyPolicy::REFERENCE_SELECTION,
            DependencyPolicy::NAMESPACE,
            DependencyPolicy::SEED,
            DependencyPolicy::CURRENCY,
            DependencyPolicy::HOST_POLICY,
        ];
        for (index, category) in categories.iter().enumerate() {
            assert_eq!(category.0.count_ones(), 1);
            for other in &categories[index + 1..] {
                assert_eq!(category.0 & other.0, 0);
            }
        }
    }
}
