//! References occupy VM values directly; only genuine collections need heap storage.
use super::Area;

/// One inline physical reference or an owned collection preserving reference order.
#[derive(Clone)]
pub(in crate::eval) enum ReferenceAreas {
    One(Area),
    Many(Vec<Area>),
}
impl ReferenceAreas {
    /// Borrow the reference sequence without materializing a collection.
    pub(in crate::eval) fn as_slice(&self) -> &[Area] {
        match self {
            Self::One(area) => std::slice::from_ref(area),
            Self::Many(areas) => areas,
        }
    }
}
impl std::ops::Deref for ReferenceAreas {
    /// Ordered physical reference descriptors.
    type Target = [Area];
    fn deref(&self) -> &[Area] {
        self.as_slice()
    }
}
impl std::ops::DerefMut for ReferenceAreas {
    fn deref_mut(&mut self) -> &mut [Area] {
        match self {
            Self::One(area) => std::slice::from_mut(area),
            Self::Many(areas) => areas,
        }
    }
}
impl From<Vec<Area>> for ReferenceAreas {
    fn from(mut areas: Vec<Area>) -> Self {
        if areas.len() == 1
            && let Some(area) = areas.pop()
        {
            return Self::One(area);
        }
        Self::Many(areas)
    }
}
impl FromIterator<Area> for ReferenceAreas {
    fn from_iter<I: IntoIterator<Item = Area>>(values: I) -> Self {
        let mut values = values.into_iter();
        let Some(first) = values.next() else {
            return Self::Many(Vec::new());
        };
        let Some(second) = values.next() else {
            return Self::One(first);
        };
        Self::Many([first, second].into_iter().chain(values).collect())
    }
}
impl IntoIterator for ReferenceAreas {
    /// Physical reference descriptor yielded by value.
    type Item = Area;
    /// Inline single-reference iteration chained with an optional owned collection.
    type IntoIter = std::iter::Chain<std::option::IntoIter<Area>, std::vec::IntoIter<Area>>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::One(area) => Some(area).into_iter().chain(Vec::new()),
            Self::Many(areas) => None.into_iter().chain(areas),
        }
    }
}
impl<'a> IntoIterator for &'a ReferenceAreas {
    /// Borrowed physical reference descriptor.
    type Item = &'a Area;
    /// Borrowed ordered reference cursor.
    type IntoIter = std::slice::Iter<'a, Area>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
