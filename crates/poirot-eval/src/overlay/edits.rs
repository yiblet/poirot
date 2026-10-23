//! Apply write-only source changes before one combined invalidation pass.
use super::*;
use crate::coordinate_hash::CellSet;

/// Write-only source operations within [`Overlay::edit`].
///
/// Edits take effect in order, including repeated writes to the same target.
/// Earlier successful edits survive a later failed operation. The enclosing
/// closure always invalidates their calculated dependents before returning.
/// The view cannot escape the closure and keep calculated state inaccessible:
///
/// ```compile_fail
/// use poirot_eval::overlay::Overlay;
/// use poirot_model::Workbook;
/// let book = Workbook::new();
/// let mut overlay = Overlay::with_workbook(&book).build().unwrap();
/// let escaped = overlay.edit(|edits| edits);
/// ```
pub struct OverlayEdits<'e, 'a, W: Borrow<Workbook>> {
    source: &'e mut Source<'a, W>,
    cache: &'e mut Cache,
    changed: CellSet,
    values: CellSet,
}
impl<'e, 'a, W: Borrow<Workbook>> OverlayEdits<'e, 'a, W> {
    /// Borrow source and retained state exclusively until deferred invalidation completes.
    pub(super) fn new(source: &'e mut Source<'a, W>, cache: &'e mut Cache) -> Self {
        Self {
            source,
            cache,
            changed: CellSet::default(),
            values: CellSet::default(),
        }
    }
    source::edit_methods!();

    fn source_changed(&mut self, target: EvaluationTarget, change: source::SourceChange) {
        let key = (target.sheet, target.address);
        if change == source::SourceChange::Metadata {
            self.values.remove(&key);
            self.changed.insert(key);
        } else if !self.changed.contains(&key) {
            self.values.insert(key);
        }
    }
}
impl<W: Borrow<Workbook>> Drop for OverlayEdits<'_, '_, W> {
    fn drop(&mut self) {
        if !self.changed.is_empty() {
            self.cache.invalidate_geometry(self.changed.drain());
        }
        self.cache.invalidate_values(self.values.drain());
    }
}
