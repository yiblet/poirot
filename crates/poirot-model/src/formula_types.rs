//! Finite possible-result types shared by source tooling and formula preparation.
use crate::CellValue;

/// A set of scalar spreadsheet kinds; an empty set denotes no possible value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct ScalarKinds(u8);
impl TryFrom<u8> for ScalarKinds {
    /// Invalid serialized scalar-kind bits.
    type Error = &'static str;
    /// Reject unknown alternatives instead of admitting invalid abstract types.
    fn try_from(bits: u8) -> Result<Self, Self::Error> {
        if bits & !31 == 0 {
            Ok(Self(bits))
        } else {
            Err("unknown scalar kind bits")
        }
    }
}
impl From<ScalarKinds> for u8 {
    /// Encode the validated set without introducing new alternatives.
    fn from(kinds: ScalarKinds) -> Self {
        kinds.0
    }
}
impl ScalarKinds {
    /// No possible scalar kind.
    pub const NONE: Self = Self(0);
    /// An explicitly blank value.
    pub const BLANK: Self = Self(1);
    /// Any binary64 number, including special numbers retained by the model.
    pub const NUMBER: Self = Self(2);
    /// Either physical text representation.
    pub const TEXT: Self = Self(4);
    /// A Boolean value.
    pub const BOOLEAN: Self = Self(8);
    /// A spreadsheet error value, distinct from an operational failure.
    pub const ERROR: Self = Self(16);
    /// Every scalar kind.
    pub const ANY: Self = Self(31);
    /// The least set containing both operands.
    pub const fn join(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Whether every kind in this set is admitted by the other set.
    pub const fn is_subtype_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }
    /// Whether this set has no members.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// Classify an actual scalar without converting its representation.
    pub fn of(value: &CellValue) -> Self {
        match value {
            CellValue::Blank => Self::BLANK,
            CellValue::Number(_) => Self::NUMBER,
            CellValue::Text(_) | CellValue::TextUtf16(_) => Self::TEXT,
            CellValue::Boolean(_) => Self::BOOLEAN,
            CellValue::Error(_) => Self::ERROR,
        }
    }
}

/// Union of scalar, rectangular-array, single-reference, multi-reference and missing alternatives.
/// Each collection/reference component describes its possible element kinds. Empty
/// components are absent alternatives, not empty arrays. Concrete dimensions stay runtime data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FormulaType {
    /// Possible direct scalar kinds.
    pub scalar: ScalarKinds,
    /// Possible materialized rectangular-array element kinds.
    pub array: ScalarKinds,
    /// Possible elements of a single-coordinate reference.
    pub single_reference: ScalarKinds,
    /// Possible elements of references that may contain multiple coordinates.
    pub multi_reference: ScalarKinds,
    /// Whether an omitted argument is possible.
    pub missing: bool,
}
impl FormulaType {
    /// No possible successful result.
    pub const BOTTOM: Self = Self {
        scalar: ScalarKinds::NONE,
        array: ScalarKinds::NONE,
        single_reference: ScalarKinds::NONE,
        multi_reference: ScalarKinds::NONE,
        missing: false,
    };
    /// Every result form and scalar kind.
    pub const TOP: Self = Self {
        scalar: ScalarKinds::ANY,
        array: ScalarKinds::ANY,
        single_reference: ScalarKinds::ANY,
        multi_reference: ScalarKinds::ANY,
        missing: true,
    };
    /// A direct scalar with the supplied possible kinds.
    pub const fn scalar(kinds: ScalarKinds) -> Self {
        Self {
            scalar: kinds,
            ..Self::BOTTOM
        }
    }
    /// A rectangular materialized array with the supplied element kinds.
    pub const fn array(kinds: ScalarKinds) -> Self {
        Self {
            array: kinds,
            ..Self::BOTTOM
        }
    }
    /// A reference containing exactly one coordinate.
    pub const fn single_reference(kinds: ScalarKinds) -> Self {
        Self {
            single_reference: kinds,
            ..Self::BOTTOM
        }
    }
    /// A reference whose cardinality is not restricted to one coordinate.
    pub const fn multi_reference(kinds: ScalarKinds) -> Self {
        Self {
            multi_reference: kinds,
            ..Self::BOTTOM
        }
    }
    /// Least upper bound in the componentwise possible-result ordering.
    pub const fn join(self, other: Self) -> Self {
        Self {
            scalar: self.scalar.join(other.scalar),
            array: self.array.join(other.array),
            single_reference: self.single_reference.join(other.single_reference),
            multi_reference: self.multi_reference.join(other.multi_reference),
            missing: self.missing || other.missing,
        }
    }
    /// Whether every result admitted by this type is admitted by the other type.
    pub const fn is_subtype_of(self, other: Self) -> bool {
        self.scalar.is_subtype_of(other.scalar)
            && self.array.is_subtype_of(other.array)
            && self.single_reference.is_subtype_of(other.single_reference)
            && self.multi_reference.is_subtype_of(other.multi_reference)
            && (!self.missing || other.missing)
    }
    /// Whether root materialization can produce more than a scalar cell.
    pub const fn may_spill(self) -> bool {
        !self.array.is_empty() || !self.multi_reference.is_empty()
    }
    /// Possible scalar elements after reading references or materializing arrays.
    pub const fn elements(self) -> ScalarKinds {
        let kinds = self
            .scalar
            .join(self.array)
            .join(self.single_reference)
            .join(self.multi_reference);
        if self.missing {
            kinds.join(ScalarKinds::BLANK)
        } else {
            kinds
        }
    }
    /// Whether a concrete direct scalar belongs to this possible type.
    pub fn admits_scalar(self, value: &CellValue) -> bool {
        ScalarKinds::of(value).is_subtype_of(self.scalar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_serialized_kind_bits_are_rejected() {
        for bits in 32u8..=255 {
            let encoded = postcard::to_allocvec(&bits).unwrap();
            assert!(postcard::from_bytes::<ScalarKinds>(&encoded).is_err());
        }
        for bits in 0u8..32 {
            let encoded = postcard::to_allocvec(&bits).unwrap();
            assert_eq!(
                u8::from(postcard::from_bytes::<ScalarKinds>(&encoded).unwrap()),
                bits
            );
        }
    }
    #[test]
    fn join_is_the_least_upper_bound_and_obeys_lattice_laws() {
        let types = [
            FormulaType::BOTTOM,
            FormulaType::TOP,
            FormulaType::scalar(ScalarKinds::NUMBER),
            FormulaType::scalar(ScalarKinds::ERROR),
            FormulaType::array(ScalarKinds::TEXT),
            FormulaType::single_reference(ScalarKinds::ANY),
            FormulaType::multi_reference(ScalarKinds::BOOLEAN),
            FormulaType {
                missing: true,
                ..FormulaType::BOTTOM
            },
        ];
        for a in types {
            for b in types {
                assert_eq!(a.join(b), b.join(a));
                assert_eq!(a.join(a), a);
                assert!(a.is_subtype_of(a.join(b)));
                assert!(b.is_subtype_of(a.join(b)));
                for c in types {
                    assert_eq!(a.join(b).join(c), a.join(b.join(c)));
                    if a.is_subtype_of(c) && b.is_subtype_of(c) {
                        assert!(a.join(b).is_subtype_of(c));
                    }
                }
            }
        }
        assert!(!FormulaType::single_reference(ScalarKinds::ANY).may_spill());
        assert!(FormulaType::multi_reference(ScalarKinds::ANY).may_spill());
    }
}
