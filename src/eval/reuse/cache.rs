//! Persistent results with reverse edges and range indexes for transitive invalidation.
//!
//! Missing cells and scanned ranges are dependencies too: inserting a value there
//! must invalidate a formula that previously observed a blank or an empty range.
use super::cached::Cached;
use super::key::Key;
use super::range_index::RangeIndex;
use super::range_purpose::RangePurpose;
use super::validity::Validity;
use crate::eval::config::ContextKey;
use crate::eval::coordinates::{CellMap, CellSet, EvaluationTarget};
use crate::eval::limits::EvaluationError;
use std::collections::BTreeSet;

/// Inline retained entries keyed directly by full worksheet coordinates.
type Entries = CellMap<Cached>;

/// Persistent overlay results with reverse edges for transitive invalidation.
#[derive(Default)]
pub(in crate::eval) struct Cache {
    /// Live results keyed without worksheet spatial indexes.
    pub entries: Entries,
    /// Context used by the latest evaluation.
    pub context: Option<ContextKey>,
    /// Estimated bytes charged against persistent retention limits.
    pub retained_bytes: usize,
    /// Spill anchors whose previously exported children must be cleared.
    pub spill_resets: BTreeSet<Key>,
    /// Bounded evaluated exact-search evidence preserved by prepared snapshots.
    pub lookup_prefixes: super::lookup_prefixes::LookupPrefixes,
    pub(super) reverse: CellMap<CellSet>,
    pub(super) always_dirty: CellSet,
    pub(super) ranges: RangeIndex,
    pub(super) spill_ranges: RangeIndex,
    pub(super) ranges_dirty: bool,
    pub(super) next_version: u64,
}

impl Cache {
    /// Admit a complete batch before publishing values, indexes, and spill cleanup together.
    pub fn commit(
        &mut self,
        staged: Vec<(Key, Cached)>,
        retention: crate::eval::limits::MemoRetentionLimits,
        consumer: Option<EvaluationTarget>,
        spill_resets: impl IntoIterator<Item = Key>,
    ) -> Result<(), EvaluationError> {
        let mut cells = self.len();
        let mut bytes = self.retained_bytes;
        let mut overflow = false;
        for (key, entry) in &staged {
            if let Some(old) = self.retained(*key) {
                let remaining = bytes.checked_sub(old.retained_bytes());
                overflow |= remaining.is_none();
                bytes = remaining.unwrap_or(usize::MAX);
            } else {
                let next = cells.checked_add(1);
                overflow |= next.is_none();
                cells = next.unwrap_or(usize::MAX);
            }
            let next = bytes.checked_add(entry.retained_bytes());
            overflow |= next.is_none();
            bytes = next.unwrap_or(usize::MAX);
        }
        if overflow || cells > retention.cells || bytes > retention.bytes {
            return Err(EvaluationError::MemoRetentionLimit {
                limits: retention,
                consumed: crate::eval::limits::MemoRetentionUsage {
                    cells: self.len(),
                    bytes: self.retained_bytes,
                },
                requested: crate::eval::limits::MemoRetentionUsage {
                    cells: cells.saturating_sub(self.len()),
                    bytes: bytes.saturating_sub(self.retained_bytes),
                },
                target: consumer,
            });
        }
        self.next_version
            .checked_add(staged.len() as u64)
            .ok_or(EvaluationError::StalePlan)?;
        for (key, _) in &staged {
            key.1.validate().map_err(|_| EvaluationError::StalePlan)?;
        }
        for (key, entry) in staged {
            self.insert(key, entry)
                .map_err(|_| EvaluationError::StalePlan)?;
        }
        self.spill_resets.extend(spill_resets);
        Ok(())
    }

    /// Look up a current result without evaluating or exposing dirty evidence.
    pub fn get(&self, key: Key) -> Option<&Cached> {
        self.retained(key).filter(|entry| entry.validity.is_clean())
    }

    /// Read retained evidence, including results that must not be exposed as current.
    pub fn retained(&self, key: Key) -> Option<&Cached> {
        self.entries.get(&key)
    }

    /// Count only values whose validity has already been established.
    pub fn valid_len(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.validity.is_clean())
            .count()
    }

    /// Count retained results across all sheets.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Remove a result and its point edges; range indices are rebuilt separately.
    pub fn remove(&mut self, key: Key) {
        self.always_dirty.remove(&key);
        if let Some(entry) = self.entries.remove(&key) {
            self.ranges_dirty |= !entry.dependencies.ranges.is_empty()
                || !entry.dependencies.point_runs.is_empty()
                || !entry.dependencies.spill_ranges.is_empty();
            self.retained_bytes -= entry.retained_bytes();
            for dependency in &entry.dependencies.cells {
                if let Some(readers) = self.reverse.get_mut(dependency) {
                    readers.remove(&key);
                    if readers.is_empty() {
                        self.reverse.remove(dependency);
                    }
                }
            }
        }
    }

    /// Replace a result and register its point edges and always-dirty status.
    pub fn insert(
        &mut self,
        key: Key,
        mut entry: Cached,
    ) -> Result<(), super::cache_error::CacheError> {
        key.1.validate()?;
        let previous = self.retained(key);
        let version = match previous {
            Some(old) if crate::eval::ports::same_result_identity(&old.value, &entry.value) => {
                old.version
            }
            _ => self
                .next_version
                .checked_add(1)
                .ok_or(super::cache_error::CacheError::GenerationOverflow)?,
        };
        entry.version = version;
        entry.validity = Validity::Clean;
        let retained_bytes = self
            .retained_bytes
            .checked_sub(previous.map_or(0, Cached::retained_bytes))
            .and_then(|bytes| bytes.checked_add(entry.retained_bytes()))
            .ok_or(super::cache_error::CacheError::RetentionOverflow)?;
        let same_dependencies = previous.is_some_and(|old| old.dependencies == entry.dependencies);
        self.next_version = self.next_version.max(version);
        // Value edits commonly leave the consumed graph unchanged. Keep its
        // reverse edges, range indexes, and retained entry in place.
        if same_dependencies {
            let retained = self
                .entries
                .get_mut(&key)
                .ok_or(super::cache_error::CacheError::MissingRetainedResult)?;
            *retained = entry;
            self.retained_bytes = retained_bytes;
            return Ok(());
        }
        self.remove(key);
        self.ranges_dirty |= !entry.dependencies.ranges.is_empty()
            || !entry.dependencies.point_runs.is_empty()
            || !entry.dependencies.spill_ranges.is_empty();
        self.retained_bytes = retained_bytes;
        if entry.dependencies.opaque || entry.dependencies.volatile {
            self.always_dirty.insert(key);
        }
        for dependency in &entry.dependencies.cells {
            self.reverse.entry(*dependency).or_default().insert(key);
        }
        self.entries.insert(key, entry);
        Ok(())
    }

    /// Force spatial membership consumers after an authored occupancy/formula edit.
    pub fn invalidate_geometry(&mut self, seeds: impl IntoIterator<Item = Key>) {
        let seeds: Vec<_> = seeds.into_iter().collect();
        for key in &seeds {
            self.remove(*key);
        }
        let geometry = seeds.iter().copied().collect();
        self.invalidate_context_with_geometry(seeds, &geometry);
    }

    /// Retain scalar evidence while propagating only immediate invalidation causes.
    pub fn invalidate_values(&mut self, seeds: impl IntoIterator<Item = Key>) {
        let seeds: Vec<_> = seeds.into_iter().collect();
        // Authored literals are sources, never generation certificates. Even a
        // retained numeric-coercion result must force its immediate readers.
        for key in &seeds {
            self.remove(*key);
        }
        self.invalidate_context(seeds);
    }

    pub(super) fn invalidate_context(&mut self, seeds: impl IntoIterator<Item = Key>) {
        self.invalidate_context_with_geometry(seeds, &CellSet::default());
    }

    fn invalidate_context_with_geometry(
        &mut self,
        seeds: impl IntoIterator<Item = Key>,
        geometry_seeds: &CellSet,
    ) {
        let seeds: Vec<_> = seeds.into_iter().collect();
        if seeds.is_empty() {
            return;
        }
        if self.ranges_dirty {
            self.rebuild_ranges();
        }
        let mut pending = seeds.clone();
        let mut seen = CellSet::default();
        for key in &seeds {
            self.force(*key);
        }
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                continue;
            }
            let mut readers = Vec::new();
            if let Some(points) = self.reverse.get(&key) {
                readers.extend(points.iter().copied());
            }
            self.ranges.readers(key, &mut readers);
            readers.sort_unstable();
            readers.dedup();
            // Geometry changes force the coordinate/producer that consumed the
            // membership guard. Its value readers remain conditionally valid.
            let mut geometry = Vec::new();
            if geometry_seeds.contains(&key)
                || self.retained(key).is_some_and(|entry| {
                    entry.dependencies.spill_sensitive
                        || matches!(
                            entry.dependencies.spill,
                            super::spill_role::SpillRole::Anchor { .. }
                        )
                })
            {
                self.spill_ranges.readers(key, &mut geometry);
            }
            for reader in geometry {
                if reader != key {
                    self.force(reader);
                    readers.push(reader);
                }
            }
            let version = self.retained(key).map(|entry| entry.version);
            for reader in readers {
                if !seen.contains(&reader) {
                    pending.push(reader);
                }
                let Some(entry) = self.entries.get_mut(&reader) else {
                    continue;
                };
                let prior_bytes = entry.validity.retained_bytes();
                // A spill child must resolve its owner again: the anchor's
                // top-left scalar cannot certify the child's contents or extent.
                if matches!(entry.dependencies.spill,
                    super::spill_role::SpillRole::Child { anchor } if (reader.0, anchor) == key)
                {
                    entry.validity = Validity::Forced;
                }
                match (version, &mut entry.validity) {
                    (_, Validity::Forced) => {}
                    (None, _) => entry.validity = Validity::Forced,
                    (Some(version), Validity::Clean) => {
                        entry.validity = Validity::Cause { key, version }
                    }
                    (Some(_), Validity::Cause { key: prior, .. }) if *prior == key => {}
                    (
                        Some(version),
                        Validity::Cause {
                            key: prior,
                            version: old,
                        },
                    ) => {
                        let mut causes = CellMap::default();
                        causes.insert(*prior, *old);
                        causes.insert(key, version);
                        entry.validity = Validity::Causes(causes);
                    }
                    (Some(version), Validity::Causes(causes)) => {
                        causes.entry(key).or_insert(version);
                    }
                }
                self.retained_bytes = self
                    .retained_bytes
                    .saturating_sub(prior_bytes)
                    .saturating_add(entry.validity.retained_bytes());
            }
        }
        self.lookup_prefixes.invalidate(&seen);
    }

    pub(super) fn force(&mut self, key: Key) {
        if let Some(entry) = self.entries.get_mut(&key) {
            let prior_bytes = entry.validity.retained_bytes();
            entry.validity = Validity::Forced;
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(prior_bytes)
                .saturating_add(entry.validity.retained_bytes());
        }
    }

    /// Invalidate volatile, opaque, and context-sensitive results for a new evaluation.
    pub fn begin(&mut self, context: ContextKey) {
        if self.context.as_ref() == Some(&context) {
            // The common unchanged-context path only needs volatile/opaque roots.
            // insert/remove maintain this derived index, including snapshot restore.
            let seeds = self.always_dirty.iter().copied().collect::<Vec<_>>();
            self.invalidate_context(seeds);
            return;
        }
        let seeds = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                let trace = &entry.dependencies;
                trace.opaque
                    || trace.volatile
                    || self.context.as_ref().is_none_or(|previous| {
                        !trace
                            .context
                            .changed_fields(previous.inputs, context.inputs, false, false)
                            .is_empty()
                            || (trace.formatting && previous.formatting != context.formatting)
                            || (trace.number_locale
                                && previous.number_locale != context.number_locale)
                    })
            })
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        self.invalidate_context(seeds);
        self.context = Some(context);
    }

    /// Invalidate cached cells that read the byte-text language and their dependents.
    pub fn invalidate_byte_text_language(&mut self) {
        let seeds = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.dependencies.byte_text_language)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        self.invalidate_context(seeds);
    }

    /// Rebuild the rectangle lookup after dependency traces have changed.
    pub fn rebuild_ranges(&mut self) {
        self.ranges = RangeIndex::new(&self.entries);
        self.spill_ranges = RangeIndex::build(&self.entries, RangePurpose::SpillGeometry);
        self.ranges_dirty = false;
    }
}
