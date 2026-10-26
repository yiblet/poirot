//! Finite possible-result types shared by source tooling and formula preparation.
use crate::model::CellValue;

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
/// components are absent alternatives, not empty arrays. Concrete dimensions stay runtime data; ShapeBounds describes their static upper bounds.
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

/// Independent upper bounds on successful rectangular result dimensions.
/// `None` means that no finite bound has been proved for that axis. These
/// bounds include scalar outcomes as 1 by 1, and never describe ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShapeBounds {
    /// Maximum row count, when statically proved.
    pub rows: Option<std::num::NonZeroU32>,
    /// Maximum column count, when statically proved.
    pub columns: Option<std::num::NonZeroU32>,
}
impl Default for ShapeBounds {
    /// Unknown geometry is the conservative default for unavailable analysis.
    fn default() -> Self {
        Self::UNKNOWN
    }
}
impl ShapeBounds {
    /// No finite proof on either axis.
    pub const UNKNOWN: Self = Self {
        rows: None,
        columns: None,
    };
    /// Scalar or single-coordinate geometry.
    pub const SCALAR: Self = Self {
        rows: std::num::NonZeroU32::new(1),
        columns: std::num::NonZeroU32::new(1),
    };
    /// Construct bounds; zero counts cannot prove nonempty result geometry.
    pub fn new(rows: u32, columns: u32) -> Self {
        Self {
            rows: std::num::NonZeroU32::new(rows),
            columns: std::num::NonZeroU32::new(columns),
        }
    }
    /// Least upper bound in the independent dimension ordering.
    pub fn join(self, other: Self) -> Self {
        let axis = |a: Option<std::num::NonZeroU32>, b: Option<std::num::NonZeroU32>| match (a, b) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        };
        Self {
            rows: axis(self.rows, other.rows),
            columns: axis(self.columns, other.columns),
        }
    }
    /// Whether this rectangle bound is included in the other bound.
    pub fn is_subtype_of(self, other: Self) -> bool {
        let axis = |a: Option<std::num::NonZeroU32>, b: Option<std::num::NonZeroU32>| match (a, b) {
            (_, None) => true,
            (Some(a), Some(b)) => a <= b,
            _ => false,
        };
        axis(self.rows, other.rows) && axis(self.columns, other.columns)
    }
    /// Whether successful nonempty dimensions satisfy both independent proofs.
    pub fn admits(self, rows: u32, columns: u32) -> bool {
        rows > 0
            && columns > 0
            && self.rows.is_none_or(|n| rows <= n.get())
            && self.columns.is_none_or(|n| columns <= n.get())
    }
    /// Exchange axes when a function transposes rectangular values.
    pub fn transpose(self) -> Self {
        Self {
            rows: self.columns,
            columns: self.rows,
        }
    }
}

#[cfg(test)]
mod shape_tests {
    use super::ShapeBounds as S;
    #[test]
    fn independent_dimension_joins_have_the_expected_order_and_lattice_laws() {
        let bounds = [
            S::SCALAR,
            S::new(2, 3),
            S::new(5, 1),
            S::new(0, 4),
            S::new(3, 0),
            S::UNKNOWN,
        ];
        for a in bounds {
            for b in bounds {
                assert_eq!(a.join(b), b.join(a));
                assert_eq!(a.join(a), a);
                assert!(a.is_subtype_of(a.join(b)));
                assert!(b.is_subtype_of(a.join(b)));
                for c in bounds {
                    assert_eq!(a.join(b).join(c), a.join(b.join(c)));
                    if a.is_subtype_of(c) && b.is_subtype_of(c) {
                        assert!(a.join(b).is_subtype_of(c));
                    }
                }
            }
        }
        assert_eq!(S::new(2, 3).join(S::new(5, 1)), S::new(5, 3));
        assert_eq!(S::new(0, 4).join(S::new(3, 2)), S::new(0, 4));
        assert!(S::new(0, 4).admits(1_000_000, 4));
        assert!(!S::new(0, 4).admits(1, 5));
        assert!(!S::UNKNOWN.admits(0, 1));
        assert!(!S::UNKNOWN.admits(1, 0));
    }
    #[test]
    fn serialized_zero_dimension_proofs_are_rejected() {
        let malformed = postcard::to_allocvec(&(Some(0u32), Some(1u32))).unwrap();
        assert!(postcard::from_bytes::<S>(&malformed).is_err());
        for shape in [S::SCALAR, S::UNKNOWN, S::new(3, 0)] {
            assert_eq!(
                postcard::from_bytes::<S>(&postcard::to_allocvec(&shape).unwrap()).unwrap(),
                shape
            );
        }
    }
}
