//! Spelling indexes shared by the default catalog and composed adapter registries.
use poirot_parsing::formula::{CommonFunction, FunctionName};
use std::collections::HashMap;

/// Name index for one registry namespace, mapping spellings to registration positions.
/// Common function names use direct slots; other names use a case-normalized map.
#[derive(Clone)]
struct Names {
    core: Vec<Option<usize>>,
    other: HashMap<String, usize>,
}

impl Names {
    fn new() -> Self {
        Self {
            core: vec![None; CommonFunction::COUNT],
            other: HashMap::new(),
        }
    }

    fn insert(&mut self, name: &str, entry: usize) {
        if let Some(name) = CommonFunction::parse(name) {
            if let Some(slot) = self.core.get_mut(name.index()) {
                slot.get_or_insert(entry);
            }
        } else {
            self.other.entry(name.to_ascii_uppercase()).or_insert(entry);
        }
    }

    fn get(&self, name: &FunctionName) -> Option<usize> {
        match name {
            FunctionName::Known(name) => self.core.get(name.index()).copied().flatten(),
            // Also admit manually constructed Unknown values whose spelling is common.
            FunctionName::Unknown(name) => self.get_spelling(name),
        }
    }

    /// Look up a borrowed spelling without allocating a temporary parsed function name.
    fn get_spelling(&self, name: &str) -> Option<usize> {
        if let Some(common) = CommonFunction::parse(name) {
            return self.core.get(common.index()).copied().flatten();
        }
        let mut folded = [0u8; 64];
        if let Some(bytes) = folded.get_mut(..name.len()) {
            for (out, byte) in bytes.iter_mut().zip(name.bytes()) {
                *out = byte.to_ascii_uppercase();
            }
            self.other.get(std::str::from_utf8(bytes).ok()?).copied()
        } else {
            self.other.get(&name.to_ascii_uppercase()).copied()
        }
    }
}

/// Spelling indexes of one builtin registry: ordinary and profile-only namespaces, each
/// with the `_xlfn.`-prefixed spellings it admits. Earlier positions win duplicate
/// spellings, so callers insert registrations in precedence order.
#[derive(Clone)]
pub(in crate::eval) struct FunctionNames {
    names: Names,
    prefixed: Names,
    profile: Names,
    profile_prefixed: Names,
}

impl FunctionNames {
    /// Create empty ordinary and profile-only indexes.
    pub(in crate::eval) fn new() -> Self {
        Self {
            names: Names::new(),
            prefixed: Names::new(),
            profile: Names::new(),
            profile_prefixed: Names::new(),
        }
    }

    /// Index a registration position under its spelling. Ordinary names admit the
    /// `_xlfn.` prefix only for pinned Analysis ToolPak functions; profile-only names
    /// always admit it.
    pub(in crate::eval) fn insert(&mut self, name: &str, profile: bool, position: usize) {
        if profile {
            self.profile.insert(name, position);
            self.profile_prefixed.insert(name, position);
        } else {
            self.names.insert(name, position);
            if crate::eval::functions::atp::PINNED_ATP_FUNCTIONS
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(name))
            {
                self.prefixed.insert(name, position);
            }
        }
    }

    /// Resolve an ordinary spelling to its registration position.
    pub(in crate::eval) fn resolve(&self, name: &FunctionName) -> Option<usize> {
        Self::lookup(name, &self.names, &self.prefixed)
    }

    /// Resolve a profile-only spelling to its registration position.
    pub(in crate::eval) fn resolve_profile(&self, name: &FunctionName) -> Option<usize> {
        Self::lookup(name, &self.profile, &self.profile_prefixed)
    }

    /// Ordinary position held in the direct slot of a common spelling.
    #[cfg(test)]
    pub(in crate::eval) fn common_slot(&self, name: CommonFunction) -> Option<usize> {
        self.names.core.get(name.index()).copied().flatten()
    }

    /// Number of ordinary spellings indexed through the case-normalized map.
    #[cfg(test)]
    pub(in crate::eval) fn uncommon_count(&self) -> usize {
        self.names.other.len()
    }

    fn lookup(name: &FunctionName, names: &Names, prefixed: &Names) -> Option<usize> {
        match name {
            FunctionName::Unknown(name) if name.starts_with("_xlfn.") => {
                let suffix = name.strip_prefix("_xlfn.")?;
                // MS-XLSX §2.2.3 assigns this additional worksheet qualifier to
                // FILTER, SORT and PY. PY has no executable registration here.
                // https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf
                // Normative spellings only; no upstream code or tests reused.
                let suffix = if let Some(worksheet) = suffix.strip_prefix("_xlws.") {
                    if !["FILTER", "SORT"]
                        .iter()
                        .any(|name| worksheet.eq_ignore_ascii_case(name))
                    {
                        return None;
                    }
                    worksheet
                } else {
                    suffix
                };
                prefixed.get_spelling(suffix)
            }
            _ => names.get(name),
        }
    }
}
