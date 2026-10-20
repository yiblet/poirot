//! Pure checked OPC identity planning. ZIP execution lives in the OOXML adapter.
use crate::IoError;
use poirot_model::{ImportDiagnostic, ImportDiagnosticKind};
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::collections::{BTreeMap, BTreeSet};
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// Invalid or unsupported XLSX package structure, relationships, or part identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpcError {
    /// A required package entry is absent.
    MissingPart(String),
    /// Relationship target cannot be safely resolved within the package.
    InvalidTarget {
        /// Owning part used as the relative URI base.
        owner: String,
        /// Rejected relationship target spelling.
        target: String,
    },
    /// Package contains conflicting part, sheet, or relationship identities.
    DuplicateIdentity(String),
    /// Relationship metadata violates the supported package contract.
    InvalidRelationship(String),
    /// External link is rejected by import policy.
    ExternalRelationship(String),
    /// Declared sheet inventory disagrees with decoded worksheet identity.
    SheetMismatch(String),
    /// XML namespace is not supported for the selected package part.
    InvalidNamespace(String),
}
impl std::fmt::Display for OpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OpcError {}
/// External targets are never fetched. Select whether unrepresented links are
/// reported as loss or make the entire import fail before backend execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExternalRelationshipPolicy {
    /// Preserve a warning and omit unsupported external relationships.
    #[default]
    ReportAndOmit,
    /// Refuse packages requiring external relationships.
    Reject,
}
/// Import policy for package relationships. External targets are never fetched;
/// the default reports and omits them rather than rejecting the entire workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpcImportPolicy {
    /// Whether external links produce warnings (default) or reject import.
    pub external_relationships: ExternalRelationshipPolicy,
}
/// A workbook sheet mapped to its relationship and package part.
/// `source_index` preserves workbook order, including unsupported sheet kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetIdentity {
    /// Worksheet index in original workbook order.
    pub source_index: usize,
    /// Declared worksheet spelling.
    pub name: String,
    /// Workbook XML sheet identifier.
    pub sheet_id: String,
    /// Workbook relationship selecting the worksheet part.
    pub relationship_id: String,
    /// Resolved internal worksheet part name.
    pub part: String,
    /// Whether the selected sheet part is supported for decoding.
    pub supported: bool,
}
/// Resolved XLSX part and sheet identities plus fidelity warnings, ready for decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcReadPlan {
    /// Resolved workbook metadata part.
    pub workbook_part: String,
    /// Optional resolved style-table part.
    pub styles_part: Option<String>,
    /// Optional resolved shared-string part.
    pub shared_strings_part: Option<String>,
    /// Checked source worksheet identities in workbook order.
    pub sheets: Vec<SheetIdentity>,
    /// External or unsupported relationships omitted under policy.
    pub warnings: Vec<ImportDiagnostic>,
}
/// Expanded XLSX package entries keyed by their checked ZIP part names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInventory {
    /// Expanded package bytes keyed by normalized part name.
    pub parts: BTreeMap<String, Vec<u8>>,
}
/// Parsed relationship kind and admitted internal target for OPC planning.
#[derive(Debug)]
struct Relationship {
    kind: String,
    target: Option<String>,
}
fn fail(e: OpcError) -> IoError {
    IoError::Opc(e)
}
fn xml(e: impl std::fmt::Display) -> IoError {
    IoError::Xml(e.to_string())
}
/// Resolve an internal URI relative to its owning part, rejecting escape and ambiguous URI syntax.
pub fn resolve_target(owner: &str, target: &str) -> Result<String, OpcError> {
    if target.starts_with("//")
        || target.is_empty()
        || target.contains(['\\', '?', '#'])
        || (!target.starts_with('/') && target.split('/').next().is_some_and(|s| s.contains(':')))
        || target.chars().any(|c| c.is_control())
    {
        return Err(OpcError::InvalidTarget {
            owner: owner.into(),
            target: target.into(),
        });
    }
    let mut segments: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        owner
            .rsplit_once('/')
            .map_or(Vec::new(), |(dir, _)| dir.split('/').collect())
    };
    for s in target.trim_start_matches('/').split('/') {
        match s {
            "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(OpcError::InvalidTarget {
                        owner: owner.into(),
                        target: target.into(),
                    });
                }
            }
            "" => {
                return Err(OpcError::InvalidTarget {
                    owner: owner.into(),
                    target: target.into(),
                });
            }
            _ => {
                // POI PackagePartName M1.6-M1.8 validates raw URI pchars;
                // ZipHelper maps that raw URI spelling to the ZIP name. Never
                // decode percent bytes into separators or a different entry.
                if !valid_segment(s) || s.ends_with('.') || s.bytes().all(|b| b == b'.') {
                    return Err(OpcError::InvalidTarget {
                        owner: owner.into(),
                        target: target.into(),
                    });
                }
                segments.push(s);
            }
        }
    }
    if segments.is_empty() {
        return Err(OpcError::InvalidTarget {
            owner: owner.into(),
            target: target.into(),
        });
    }
    Ok(segments.join("/"))
}
fn valid_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // SAFETY: The loop bounds i; get(i+1..i+3) supplies exactly two bytes before decoding hex[0] and hex[1].
        let b = bytes[i];
        if b == b'%' {
            let Some(hex) = bytes.get(i + 1..i + 3) else {
                return false;
            };
            // SAFETY: The radix is the literal 16, within to_digit's supported range.
            let digit = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
            // SAFETY: The loop bounds i; get(i+1..i+3) supplies exactly two bytes before decoding hex[0] and hex[1].
            let (Some(high), Some(low)) = (digit(hex[0]), digit(hex[1])) else {
                return false;
            };
            let decoded = high * 16 + low;
            if decoded == b'/'
                || decoded == b'\\'
                || decoded.is_ascii_alphanumeric()
                || b"-._~".contains(&decoded)
            {
                return false;
            }
            i += 3;
        } else if b.is_ascii_alphanumeric() || b"-._~:@!$&'()*+,;=".contains(&b) {
            i += 1;
        } else {
            return false;
        }
    }
    true
}
fn relationship_part(owner: &str) -> String {
    if owner.is_empty() {
        "_rels/.rels".into()
    } else {
        let (dir, file) = owner.rsplit_once('/').unwrap_or(("", owner));
        format!(
            "{}{}_rels/{file}.rels",
            dir,
            if dir.is_empty() { "" } else { "/" }
        )
    }
}
fn attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<BTreeMap<String, String>, IoError> {
    let mut result = BTreeMap::new();
    for a in e.attributes() {
        let a = a.map_err(xml)?;
        let key = a.key.as_ref().to_owned();
        let value = a
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(xml)?
            .into_owned();
        result.insert(key, value);
    }
    Ok(result)
}
impl PackageInventory {
    fn part(&self, path: &str) -> Result<&[u8], IoError> {
        self.parts
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| fail(OpcError::MissingPart(path.into())))
    }
    fn relationships(
        &self,
        owner: &str,
        warnings: &mut Vec<ImportDiagnostic>,
        policy: OpcImportPolicy,
    ) -> Result<BTreeMap<String, Relationship>, IoError> {
        let path = relationship_part(owner);
        let mut reader = NsReader::from_reader(self.part(&path)?);
        reader.config_mut().expand_empty_elements = true;
        let mut result = BTreeMap::new();
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == "Relationship" => {
                    let (ns, _) = reader.resolver().resolve_element(e.name());
                    if !matches!(ns, ResolveResult::Bound(n) if n.as_ref() == PKG) {
                        return Err(fail(OpcError::InvalidNamespace(path.clone())));
                    }
                    let a = attrs(&e)?;
                    let required = |key: &str| {
                        a.get(key)
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .ok_or_else(|| {
                                fail(OpcError::InvalidRelationship(format!(
                                    "{path}: missing {key}"
                                )))
                            })
                    };
                    let id = required("Id")?;
                    let kind = required("Type")?;
                    let raw = required("Target")?;
                    let target = match a.get("TargetMode").map(String::as_str) {
                        None | Some("Internal") => {
                            let target = resolve_target(owner, &raw).map_err(fail)?;
                            self.part(&target)?;
                            Some(target)
                        }
                        Some("External") => {
                            if policy.external_relationships == ExternalRelationshipPolicy::Reject {
                                return Err(fail(OpcError::ExternalRelationship(format!(
                                    "{owner}#{id}"
                                ))));
                            }
                            warnings.push(ImportDiagnostic { kind: ImportDiagnosticKind::ExternalLink, detail: format!("external relationship {owner}#{id} ({kind}) is not followed or preserved") });
                            None
                        }
                        _ => {
                            return Err(fail(OpcError::InvalidRelationship(format!(
                                "{path}#{id}: invalid TargetMode"
                            ))));
                        }
                    };
                    if result
                        .insert(id.clone(), Relationship { kind, target })
                        .is_some()
                    {
                        return Err(fail(OpcError::DuplicateIdentity(format!("{path}#{id}"))));
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(result)
    }
    /// Validate package identity using the default external-relationship policy.
    pub fn plan(&self) -> Result<OpcReadPlan, IoError> {
        self.plan_with_policy(OpcImportPolicy::default())
    }
    /// Resolve checked package topology under an explicit external-link policy.
    pub fn plan_with_policy(&self, policy: OpcImportPolicy) -> Result<OpcReadPlan, IoError> {
        let mut warnings = Vec::new();
        let roots = self.relationships("", &mut warnings, policy)?;
        let select = |rels: &BTreeMap<String, Relationship>,
                      kind: &str,
                      required: bool|
         -> Result<Option<String>, IoError> {
            let found: Vec<_> = rels
                .iter()
                .filter(|(_, r)| r.kind == format!("{REL}{kind}"))
                .collect();
            if found.len() > 1 || (required && found.is_empty()) {
                return Err(fail(OpcError::InvalidRelationship(format!(
                    "expected {} {kind} relationship",
                    if required {
                        "exactly one"
                    } else {
                        "at most one"
                    }
                ))));
            }
            found
                .first()
                .map(|(id, r)| {
                    r.target
                        .clone()
                        .ok_or_else(|| fail(OpcError::ExternalRelationship((*id).clone())))
                })
                .transpose()
        };
        let workbook_part = select(&roots, "officeDocument", true)?.ok_or_else(|| {
            fail(OpcError::InvalidRelationship(
                "missing officeDocument relationship".into(),
            ))
        })?;
        let rels = self.relationships(&workbook_part, &mut warnings, policy)?;
        let styles_part = select(&rels, "styles", false)?;
        let shared_strings_part = select(&rels, "sharedStrings", false)?;
        let mut reader = NsReader::from_reader(self.part(&workbook_part)?);
        reader.config_mut().expand_empty_elements = true;
        let mut sheets = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut ids = BTreeSet::new();
        let mut used = BTreeSet::new();
        let mut targets = BTreeSet::new();
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == "sheet" => {
                    let (ns, _) = reader.resolver().resolve_element(e.name());
                    if !matches!(ns, ResolveResult::Bound(n) if n.as_ref() == MAIN) {
                        return Err(fail(OpcError::InvalidNamespace(workbook_part.clone())));
                    }
                    let a = attrs(&e)?;
                    let name = a
                        .get("name")
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .ok_or_else(|| {
                            fail(OpcError::SheetMismatch("missing sheet name".into()))
                        })?;
                    let sheet_id = a
                        .get("sheetId")
                        .filter(|s| s.parse::<u32>().is_ok_and(|id| id > 0))
                        .cloned()
                        .ok_or_else(|| fail(OpcError::SheetMismatch("invalid sheetId".into())))?;
                    let mut relationship_id = None;
                    for attribute in e.attributes() {
                        let attribute = attribute.map_err(xml)?;
                        let (ns, local) = reader.resolver().resolve_attribute(attribute.key);
                        if local.as_ref() == "id"
                            && matches!(ns, ResolveResult::Bound(n) if n.as_ref() == REL.trim_end_matches('/'))
                        {
                            relationship_id = Some(
                                attribute
                                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                    .map_err(xml)?
                                    .into_owned(),
                            );
                        }
                    }
                    let relationship_id = relationship_id.ok_or_else(|| {
                        fail(OpcError::SheetMismatch(format!(
                            "{name}: missing namespace-qualified relationship id"
                        )))
                    })?;
                    let r = rels.get(&relationship_id).ok_or_else(|| {
                        fail(OpcError::InvalidRelationship(relationship_id.clone()))
                    })?;
                    let supported = r.kind == format!("{REL}worksheet");
                    if !supported
                        && r.kind != format!("{REL}chartsheet")
                        && r.kind != format!("{REL}dialogsheet")
                    {
                        return Err(fail(OpcError::InvalidRelationship(format!(
                            "{name}: invalid sheet relationship type {}",
                            r.kind
                        ))));
                    }
                    let part = r.target.clone().ok_or_else(|| {
                        fail(OpcError::ExternalRelationship(relationship_id.clone()))
                    })?;
                    let mut duplicate_name = false;
                    for previous in &names {
                        match poirot_model::identifiers::compare(previous, &name) {
                            poirot_model::identifiers::IdentifierComparison::Equal => duplicate_name = true,
                            poirot_model::identifiers::IdentifierComparison::UnsupportedMapping => warnings.push(ImportDiagnostic { kind: ImportDiagnosticKind::IdentifierIdentity, detail: format!(
                                "identifier comparison unsupported: sheet {previous:?} versus {name:?}; preserving raw identities"
                            ) }),
                            poirot_model::identifiers::IdentifierComparison::Different => {},
                        }
                    }
                    names.push(name.clone());
                    if duplicate_name
                        // SAFETY: sheet_id is retained only after the local parse::<u32>() filter succeeds and is not modified before parsing it again.
                        || !ids.insert(sheet_id.parse::<u32>().expect("validated sheetId"))
                        || !used.insert(relationship_id.clone())
                        || !targets.insert(part.clone())
                    {
                        return Err(fail(OpcError::DuplicateIdentity(name)));
                    }
                    if !supported {
                        warnings.push(ImportDiagnostic {
                            kind: ImportDiagnosticKind::RepresentationLoss,
                            detail: format!("sheet {name} ({}) is unsupported and omitted", r.kind),
                        });
                    }
                    sheets.push(SheetIdentity {
                        source_index: sheets.len(),
                        name,
                        sheet_id,
                        relationship_id,
                        part,
                        supported,
                    });
                }
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(OpcReadPlan {
            workbook_part,
            styles_part,
            shared_strings_part,
            sheets,
            warnings,
        })
    }
}
