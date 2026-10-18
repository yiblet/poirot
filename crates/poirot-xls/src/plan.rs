use super::*;
use poirot_model::{CellAddress, TextRef};
use std::collections::HashMap;

/// Caller-controlled budgets. Representation checks always apply; authoring checks
/// may be relaxed for valid imported names without weakening BIFF size checks.
#[derive(Clone, Copy, Debug)]
pub struct XlsExportPolicy {
    /// Maximum worksheet count admitted for export.
    pub max_sheets: usize,
    /// Maximum stored cell count admitted for export.
    pub max_cells: usize,
    /// Maximum defined-name records admitted for export.
    pub max_defined_names: usize,
    /// Maximum unique shared strings admitted for export.
    pub max_unique_strings: usize,
    /// Maximum custom number-format records admitted for export.
    pub max_formats: usize,
    /// Maximum encoded formula-token bytes admitted per formula.
    pub max_formula_tokens: usize,
    /// Maximum planned workbook stream size.
    pub max_stream_bytes: usize,
    /// Reject unsupported name encoding rather than omitting it.
    pub strict_names: bool,
}
impl Default for XlsExportPolicy {
    fn default() -> Self {
        Self {
            max_sheets: 1024,
            max_cells: 1_000_000,
            max_defined_names: 65535,
            max_unique_strings: 1_000_000,
            max_formats: 4000,
            max_formula_tokens: 8170,
            max_stream_bytes: 256 * 1024 * 1024,
            strict_names: true,
        }
    }
}
/// Workbook-scoped add-in identity required by a formula's NameX token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BiffAddInReference {
    /// Source add-in function name retained for external-name encoding.
    pub stored_name: String,
    /// Planned external-sheet index used by formula tokens.
    pub extern_sheet_index: u16,
    /// One-based EXTERNNAME slot in the add-in SUPBOOK.
    pub name_number: u16,
}
/// Encoded BIFF formula tokens plus the add-in identities required to interpret NameX tokens.
/// Used by export planning to check record sizes before writing a workbook.
#[derive(Debug)]
pub struct BiffFormulaPlan {
    /// Checked BIFF RPN bytes retained after formula planning.
    pub(super) tokens: Vec<u8>,
    add_in_references: Vec<BiffAddInReference>,
}
impl BiffFormulaPlan {
    /// Borrow external-name identities required by this formula.
    pub fn add_in_references(&self) -> &[BiffAddInReference] {
        &self.add_in_references
    }
    /// Checked BIFF RPN bytes retained after formula planning.
    pub fn tokens(&self) -> &[u8] {
        &self.tokens
    }
    /// Size of the cell FORMULA record, excluding a cached STRING and its CONTINUE records.
    pub fn predicted_record_bytes(&self) -> usize {
        26 + self.tokens.len()
    }
}
/// Immutable borrowing prevents mutation of the workbook while a plan is usable.
pub struct XlsExportPlan<'a> {
    /// Immutable source workbook validated by the export plan.
    pub(super) workbook: &'a Workbook,
    /// Planned defined-name formula records.
    pub(super) names: Vec<BiffFormulaPlan>,
    /// Shared add-in external-book/name record plan.
    pub(super) addins: addins::AddInPlan,
    /// Planned per-sheet cell records and formula encodings.
    pub(super) cells: BTreeMap<(usize, CellAddress), BiffFormulaPlan>,
    /// Deduplicated shared-string table selected for export.
    pub(super) strings: HashMap<TextRef<'a>, u32>,
    /// Planned custom number-format records.
    pub(super) formats: BTreeMap<&'a str, u16>,
    /// Total string references, including shared duplicates.
    pub(super) total_strings: u32,
    predicted: usize,
}
fn limit(actual: usize, maximum: usize, resource: &str) -> Result<(), XlsError> {
    if actual > maximum {
        return Err(XlsError::ResourceLimit(format!(
            "{resource} exceeds export limit {maximum}"
        )));
    }
    Ok(())
}
// Mirrors encoder boundaries without allocating character buffers.
/// Checked string byte/continuation estimate used before stream allocation.
struct StringSize {
    used: usize,
    finished: usize,
}
impl StringSize {
    fn new(used: usize) -> Self {
        Self { used, finished: 0 }
    }
    fn next(&mut self, characters: bool) {
        self.finished += 4 + self.used;
        self.used = usize::from(characters);
    }
    fn string(&mut self, s: TextRef<'_>) {
        if self.used + 3 > MAX_RECORD {
            self.next(false);
        }
        self.used += 3;
        for count in strings::character_groups(s) {
            let size = count * 2;
            if self.used + size > MAX_RECORD {
                self.next(true);
            }
            self.used += size;
        }
    }
    fn bytes(&self) -> usize {
        self.finished + 4 + self.used
    }
}
impl<'a> XlsExportPlan<'a> {
    /// Validate workbook features and admit their predicted BIFF representation.
    pub fn new(workbook: &'a Workbook, policy: XlsExportPolicy) -> Result<Self, XlsError> {
        if workbook.sheets().is_empty() {
            return Err(fail("XLS output requires at least one worksheet"));
        }
        limit(
            workbook.sheets().len(),
            policy.max_sheets.min(1024),
            "worksheet count",
        )?;
        limit(
            workbook.named_ranges.len(),
            policy.max_defined_names,
            "defined names",
        )?;
        // Validate every identity before parsing or allocating token streams.
        let mut names_seen = std::collections::BTreeSet::new();
        for named in &workbook.named_ranges {
            let result = (|| {
                let units = named.name.encode_utf16().count();
                if units == 0 || units > 255 || named.name.chars().any(char::is_control) {
                    return Err(fail("invalid BIFF defined name"));
                }
                if named
                    .sheet_scope
                    .is_some_and(|s| s >= workbook.sheets().len())
                {
                    return Err(fail("defined name has invalid sheet scope"));
                }
                if !names_seen.insert((named.sheet_scope, named.name.to_lowercase())) {
                    return Err(fail("duplicate defined name identity"));
                }
                if policy.strict_names
                    && (!named.name.chars().enumerate().all(|(i, c)| {
                        c.is_alphabetic()
                            || c == '_'
                            || c == '\\'
                            || (i > 0 && (c.is_ascii_digit() || c == '.'))
                    }) || named.name.parse::<CellAddress>().is_ok()
                        || named.name.eq_ignore_ascii_case("R")
                        || named.name.eq_ignore_ascii_case("C"))
                {
                    return Err(fail("invalid authoring defined name"));
                }
                Ok(())
            })();
            result.map_err(|e| context(format!("defined name {}", named.name), e))?;
        }
        let mut identities = std::collections::BTreeSet::new();
        let mut unique = HashMap::new();
        let mut formats = BTreeMap::new();
        let mut total_strings = 0u32;
        let mut count = 0usize;
        let mut sst = StringSize::new(8);
        let mut predicted = 135 + 8 + 6 + 6 * workbook.sheets().len() + 4 + sst.bytes();
        let stream_limit = policy.max_stream_bytes.min(u32::MAX as usize);
        limit(predicted, stream_limit, "Workbook stream bytes")?;
        for sheet in workbook.sheets() {
            let name = sheet.name();
            let units = name.encode_utf16().count();
            if units == 0 || units > 31 || !identities.insert(name.to_lowercase()) {
                return Err(fail("invalid or duplicate worksheet identity"));
            }
            predicted += 12 + 2 * units + 64;
            limit(predicted, stream_limit, "Workbook stream bytes")?;
            for row in sheet.row_indices() {
                if row >= 65536 {
                    return Err(fail("row exceeds XLS 65536-row bounds"));
                }
                predicted = predicted
                    .checked_add(20)
                    .ok_or_else(|| fail("row record size overflow"))?;
                limit(predicted, stream_limit, "Workbook stream bytes")?;
            }
            for (address, cell) in sheet.cells() {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| fail("cell count overflow"))?;
                limit(count, policy.max_cells.min(u32::MAX as usize), "cell count")?;
                let result = (|| {
                    if address.row >= 65536 || address.col >= 256 {
                        return Err(fail("cell exceeds XLS 65536-row/256-column bounds"));
                    }
                    if let CellValue::Number(number) = &cell.value {
                        poirot_model::FiniteNumericRepresentationPolicy::Binary64Finite
                            .check(
                                *number,
                                poirot_model::NumericFileFormat::Xls,
                                poirot_model::NumericLocation {
                                    sheet: sheet.name().into(),
                                    address: *address,
                                    formula_cache: cell.formula.is_some(),
                                },
                            )
                            .map_err(XlsError::NumericFidelity)?;
                    }
                    cached(&cell.value)?;
                    if let Some(s) = cell.value.text_ref() {
                        limit(s.len_units(), 32767, "text UTF-16 units")?;
                        if cell.formula.is_none() {
                            let next = unique.len();
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                unique.entry(s)
                            {
                                limit(
                                    next + 1,
                                    policy.max_unique_strings.min(u32::MAX as usize),
                                    "SST strings",
                                )?;
                                let before = sst.bytes();
                                sst.string(s);
                                predicted += sst.bytes() - before;
                                limit(predicted, stream_limit, "Workbook stream bytes")?;
                                entry.insert(next as u32);
                            }
                            total_strings += 1;
                            limit(
                                unique.len(),
                                policy.max_unique_strings.min(u32::MAX as usize),
                                "SST strings",
                            )?;
                        }
                    }
                    if let Some(f) = &cell.style.number_format {
                        limit(f.encode_utf16().count(), 4000, "number format UTF-16 units")?;
                        let next = formats.len();
                        if !formats.contains_key(f.as_str()) {
                            limit(next + 1, policy.max_formats.min(4000), "number formats")?;
                            predicted += 9 + 2 * f.encode_utf16().count() + 24;
                            limit(predicted, stream_limit, "Workbook stream bytes")?;
                            formats.insert(f.as_str(), next as u16);
                        }
                        limit(
                            formats.len(),
                            policy.max_formats.min(4000),
                            "number formats",
                        )?;
                    }
                    if cell.formula.is_some() {
                        predicted += 26;
                        if let Some(s) = cell.value.text_ref() {
                            let mut size = StringSize::new(0);
                            size.string(s);
                            predicted += size.bytes();
                        }
                    } else {
                        predicted += match cell.value {
                            CellValue::Blank => 10,
                            CellValue::Number(_) => 18,
                            CellValue::Text(_) | CellValue::TextUtf16(_) => 14,
                            _ => 12,
                        };
                    }
                    limit(predicted, stream_limit, "Workbook stream bytes")?;
                    Ok(())
                })();
                result.map_err(|e| context(format!("{}!{}", sheet.name(), address), e))?;
            }
        }
        for named in &workbook.named_ranges {
            let width = if named.name.encode_utf16().any(|u| u > 255) {
                2
            } else {
                1
            };
            predicted += 19 + width * named.name.encode_utf16().count();
            limit(predicted, stream_limit, "Workbook stream bytes")?;
        }
        let mut addins = addins::AddInPlan::default();
        for named in &workbook.named_ranges {
            let expr = poirot_parsing::formula::parse_formula_with_policy(
                &named.formula,
                poirot_parsing::formula::FormulaParsePolicy::default(),
            )
            .map_err(|e| context(format!("defined name {}", named.name), fail(&e.to_string())))?;
            addins
                .visit_parsed(&expr)
                .map_err(|e| context(format!("defined name {}", named.name), e))?;
        }
        for sheet in workbook.sheets() {
            for (address, cell) in sheet.cells() {
                if let Some(source) = &cell.formula {
                    let expr = poirot_parsing::formula::parse_formula_with_policy(
                        source,
                        poirot_parsing::formula::FormulaParsePolicy::default(),
                    )
                    .map_err(|e| {
                        context(
                            format!("{}!{}", sheet.name(), address),
                            fail(&e.to_string()),
                        )
                    })?;
                    addins
                        .visit_parsed(&expr)
                        .map_err(|e| context(format!("{}!{}", sheet.name(), address), e))?;
                }
            }
        }
        predicted += addins.predicted_bytes();
        limit(predicted, stream_limit, "Workbook stream bytes")?;
        let compile = |formula: &str, sheet, class| -> Result<BiffFormulaPlan, XlsError> {
            let expr = parse_formula(formula).map_err(|e| fail(&e.to_string()))?;
            let tokens = rpn::compile_with_addins(
                workbook,
                sheet,
                &expr,
                class,
                policy.max_formula_tokens.min(8170),
                &addins,
            )?;
            limit(
                tokens.len(),
                policy.max_formula_tokens.min(8170),
                "formula token bytes",
            )?;
            let add_in_references = addins
                .referenced_indexes(&expr)
                .into_iter()
                .map(|index| {
                    Ok(BiffAddInReference {
                        stored_name: addins
                            .names()
                            .get(index)
                            .cloned()
                            .ok_or_else(|| fail("invalid add-in name index"))?,
                        extern_sheet_index: workbook.sheets().len() as u16,
                        name_number: index as u16 + 1,
                    })
                })
                .collect::<Result<Vec<_>, XlsError>>()?;
            Ok(BiffFormulaPlan {
                tokens,
                add_in_references,
            })
        };
        let mut names = Vec::new();
        for named in &workbook.named_ranges {
            let formula = compile(&named.formula, named.sheet_scope.unwrap_or(0), 0)
                .map_err(|e| context(format!("defined name {}", named.name), e))?;
            let width = if named.name.encode_utf16().any(|u| u > 255) {
                2
            } else {
                1
            };
            let bytes = 19 + width * named.name.encode_utf16().count() + formula.tokens.len();
            limit(bytes - 4, MAX_RECORD, "NAME record bytes")
                .map_err(|e| context(format!("defined name {}", named.name), e))?;
            predicted += formula.tokens.len();
            limit(
                predicted,
                policy.max_stream_bytes.min(u32::MAX as usize),
                "Workbook stream bytes",
            )
            .map_err(|e| context(format!("defined name {}", named.name), e))?;
            names.push(formula);
        }
        let mut cells = BTreeMap::new();
        for (index, sheet) in workbook.sheets().iter().enumerate() {
            for (address, cell) in sheet.cells() {
                if let Some(source) = &cell.formula {
                    let formula = compile(source, index, 0x20)
                        .map_err(|e| context(format!("{}!{}", sheet.name(), address), e))?;
                    predicted += formula.tokens.len();
                    limit(
                        predicted,
                        policy.max_stream_bytes.min(u32::MAX as usize),
                        "Workbook stream bytes",
                    )
                    .map_err(|e| context(format!("{}!{}", sheet.name(), address), e))?;
                    cells.insert((index, *address), formula);
                }
            }
        }
        Ok(Self {
            workbook,
            names,
            addins,
            cells,
            strings: unique,
            formats,
            total_strings,
            predicted,
        })
    }
    /// Exact EXTERNNAME spellings, in one-based NameX slot order.
    pub fn add_in_names(&self) -> &[String] {
        self.addins.names()
    }
    /// Return the numeric fidelity contract used by this plan.
    pub fn numeric_representation_policy(&self) -> poirot_model::FiniteNumericRepresentationPolicy {
        poirot_model::FiniteNumericRepresentationPolicy::Binary64Finite
    }
    /// Return the admitted workbook-stream size estimate.
    pub fn predicted_stream_bytes(&self) -> usize {
        self.predicted
    }
    /// Encode validated source records into the bounded BIFF workbook stream.
    pub fn encode_workbook_stream(&self) -> Result<Vec<u8>, XlsError> {
        encode_biff(self)
    }
    /// Wrap the admitted workbook stream in a compound-file byte buffer.
    pub fn to_buffer(&self) -> Result<Vec<u8>, XlsError> {
        wrap_xls_container(&self.encode_workbook_stream()?)
    }
    /// Encode completely before writing the result to a file.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), XlsError> {
        let bytes = self.to_buffer()?;
        std::fs::write(path.as_ref(), bytes).map_err(|source| XlsError::Io {
            operation: format!("write XLS {}", path.as_ref().display()),
            source,
        })
    }
}
/// Plan and encode a workbook using the supplied BIFF export policy.
pub fn write_xls_to_buffer(workbook: &Workbook) -> Result<Vec<u8>, XlsError> {
    XlsExportPlan::new(workbook, XlsExportPolicy::default())?.to_buffer()
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
