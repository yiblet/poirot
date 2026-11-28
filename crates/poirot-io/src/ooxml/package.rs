//! Bounded ZIP entry expansion.
use super::appended_worksheets::AppendedWorksheets;
use super::bounded_buffer::BoundedBuffer;
use super::worksheet_archive::WorksheetArchive;
use super::xml::xml;
use super::zip_append_buffer::ZipAppendBuffer;
use crate::{IoError, ReadOptions};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Expanded ZIP parts indexed by normalized package entry name.
pub(super) type Package = BTreeMap<String, Vec<u8>>;

/// Import ZIP catalog with bounded, on-demand expansion of original recovery XML.
pub(crate) struct ImportPackage<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    entries: BTreeMap<String, usize>,
    max_entry_bytes: u64,
}

impl<'a> ImportPackage<'a> {
    /// Admit all entry claims and identities without allocating expanded parts.
    pub(crate) fn new(bytes: &'a [u8], options: ReadOptions) -> Result<Self, IoError> {
        if bytes.len() as u64 > options.max_input_bytes {
            return Err(IoError::quota(
                crate::IoResource::InputBytes,
                options.max_input_bytes,
                0,
                bytes.len() as u64,
                crate::IoOperation::XlsxPackage,
            ));
        }
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(xml)?;
        if zip.len() > options.max_zip_entries {
            return Err(IoError::quota(
                crate::IoResource::ZipEntries,
                options.max_zip_entries as u64,
                0,
                zip.len() as u64,
                crate::IoOperation::XlsxCatalog,
            ));
        }
        let mut total = 0u64;
        let mut entries = BTreeMap::new();
        for index in 0..zip.len() {
            let mut file = zip.by_index_raw(index).map_err(xml)?;
            // Raw access checks the local entry bounds without allocating a reader.
            // Let the normal reader enforce admission for nonstandard metadata.
            if file.encrypted()
                || !matches!(
                    file.compression(),
                    zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
                )
                || file.extra_data_fields().next().is_some()
            {
                drop(file);
                file = zip.by_index(index).map_err(xml)?;
            }
            total = total
                .checked_add(file.size())
                .ok_or(IoError::SizeOverflow)?;
            if file.size() > options.max_zip_entry_bytes {
                return Err(IoError::quota(
                    crate::IoResource::ZipEntryBytes,
                    options.max_zip_entry_bytes,
                    0,
                    file.size(),
                    crate::IoOperation::Entry {
                        path: file.name().map_err(xml)?.into_owned(),
                    },
                ));
            }
            if total > options.max_zip_total_bytes {
                return Err(IoError::quota(
                    crate::IoResource::ZipTotalBytes,
                    options.max_zip_total_bytes,
                    total.saturating_sub(file.size()),
                    file.size(),
                    crate::IoOperation::XlsxCatalog,
                ));
            }
            if file.is_dir() {
                continue;
            }
            let name = file.name().map_err(xml)?.into_owned();
            if entries.insert(name.clone(), index).is_some() {
                return Err(IoError::Unsupported(
                    crate::WorkbookFeatureError::DuplicateZipEntry { name: name.clone() },
                ));
            }
        }
        Ok(Self {
            zip,
            entries,
            max_entry_bytes: options.max_zip_entry_bytes,
        })
    }

    /// Reuse checked worksheet expansions within a bounded temporary ZIP.
    pub(crate) fn worksheet_archive(
        &mut self,
        plan: &crate::opc::OpcReadPlan,
        input_bytes: usize,
    ) -> Option<WorksheetArchive> {
        const MAX_COPY_BYTES: usize = 8 * 1024 * 1024;
        const MIN_COMPRESSED_BYTES: u64 = 8 * 1024;
        // Small packages rarely repay allocating a second archive.
        if input_bytes < 16 * 1024 {
            return None;
        }
        let expansion_limit = MAX_COPY_BYTES.checked_sub(input_bytes)? as u64;
        let mut candidates = BTreeMap::new();
        let mut compressed_size = 0u64;
        let mut expanded_size = 0u64;
        for sheet in plan.sheets.iter().filter(|s| s.supported) {
            let index = *self.entries.get(&sheet.part)?;
            let raw = self.zip.by_index_raw(index).ok()?;
            if raw.compression() == zip::CompressionMethod::Deflated {
                compressed_size = compressed_size.checked_add(raw.compressed_size())?;
                expanded_size = expanded_size.checked_add(raw.size())?;
                candidates.insert(index, sheet.part.as_str());
            }
        }
        if compressed_size < MIN_COMPRESSED_BYTES || expanded_size > expansion_limit {
            return None;
        }
        let estimate = input_bytes.checked_add(usize::try_from(expanded_size).ok()?)?;
        let discarded = std::cell::Cell::new(false);
        let mut archive = ZipWriter::new(BoundedBuffer::new(estimate, MAX_COPY_BYTES, &discarded)?);
        let mut remaining = expansion_limit;
        for index in 0..self.zip.len() {
            let raw = self.zip.by_index_raw(index).ok()?;
            // Use the original reader for nonstandard ZIP metadata. Raw copies
            // retain filenames, entry order and compressed bytes.
            if raw.encrypted()
                || raw.is_symlink()
                || !matches!(
                    raw.compression(),
                    zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
                )
                || raw.extra_data_fields().next().is_some()
            {
                return None;
            }
            if let Some(&name) = candidates.get(&index) {
                if raw.name_raw() != name.as_bytes() || !name.is_ascii() {
                    return None;
                }
                let declared = raw.size();
                drop(raw);
                let limit = self.max_entry_bytes.min(remaining);
                let mut file = self.zip.by_index(index).ok()?;
                archive
                    .start_file(
                        name,
                        SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )
                    .ok()?;
                let copied = std::io::copy(
                    &mut file.by_ref().take(limit.saturating_add(1)),
                    &mut archive,
                )
                .ok()?;
                if copied != declared || copied > limit {
                    return None;
                }
                remaining = remaining.checked_sub(copied)?;
            } else {
                archive.raw_copy_file(raw).ok()?;
            }
            if discarded.get() {
                return None;
            }
        }
        let mut stored = archive.finish_into_readable().ok()?;
        if discarded.get() {
            return None;
        }
        let mut ranges = BTreeMap::new();
        for name in candidates.into_values() {
            let part = stored.by_name(name).ok()?;
            let start = usize::try_from(part.data_start()?).ok()?;
            let end = start.checked_add(usize::try_from(part.size()).ok()?)?;
            ranges.insert(name.to_owned(), start..end);
        }
        WorksheetArchive::new(stored.into_inner().into_bytes()?, ranges)
    }

    /// Retain expanded worksheets in a new ZIP suffix, borrowing all original file records.
    pub(crate) fn appended_worksheets(
        &mut self,
        plan: &crate::opc::OpcReadPlan,
        bytes: &'a [u8],
    ) -> Option<AppendedWorksheets<'a>> {
        const TOTAL_COPY_LIMIT: usize = 8 * 1024 * 1024;
        const MIN_COMPRESSED_BYTES: u64 = 2 * 1024;
        let limit = TOTAL_COPY_LIMIT.checked_sub(bytes.len())?;
        let boundary = self.zip.central_directory_start();
        let prefix_len = usize::try_from(boundary).ok()?;
        if self.zip.offset() != 0 {
            return None;
        }
        let mut parts = BTreeMap::new();
        let mut compressed = 0u64;
        let mut estimate = bytes.len().checked_sub(prefix_len)?;
        for sheet in plan.sheets.iter().filter(|sheet| sheet.supported) {
            let index = *self.entries.get(&sheet.part)?;
            let file = self.zip.by_index_raw(index).ok()?;
            if file.compression() == zip::CompressionMethod::Deflated && !parts.contains_key(&index)
            {
                compressed = compressed.checked_add(file.compressed_size())?;
                estimate = estimate.checked_add(usize::try_from(file.size()).ok()?)?;
                // Allow room for the replaced local header; the backing buffer still checks every write.
                estimate = estimate.checked_add(30)?.checked_add(sheet.part.len())?;
                parts.insert(index, sheet.part.as_str());
            }
        }
        if compressed < MIN_COMPRESSED_BYTES || estimate > limit {
            return None;
        }
        let mut names = std::collections::BTreeSet::new();
        for index in 0..self.zip.len() {
            let raw = self.zip.by_index_raw(index).ok()?;
            let name = raw.name().ok()?;
            // Match the path identity used by Calamine 0.38.0 (MIT), revision
            // a4dba3b5309d81b2774018a13be39e68d938c3c7, src/utils.rs.
            // https://github.com/tafia/calamine/blob/a4dba3b5309d81b2774018a13be39e68d938c3c7/src/utils.rs
            // Reordering replacements is harmless only without normalized aliases.
            if !name.is_ascii()
                || name.contains('\\')
                || !names.insert(name.to_ascii_lowercase())
                || raw.encrypted()
                || raw.is_symlink()
                || !matches!(
                    raw.compression(),
                    zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
                )
                || raw.extra_data_fields().next().is_some()
                || raw.data_start()?.checked_add(raw.compressed_size())? > boundary
            {
                return None;
            }
        }

        let mut output =
            ZipWriter::new_append(ZipAppendBuffer::new(bytes, prefix_len, estimate, limit)?)
                .ok()?;
        let mut remaining = limit as u64;
        for (&index, &name) in &parts {
            let mut file = self.zip.by_index(index).ok()?;
            if file.name_raw() != name.as_bytes() {
                return None;
            }
            let declared = file.size();
            let available = self.max_entry_bytes.min(remaining);
            output.hide_file(name).ok()?;
            output
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
                )
                .ok()?;
            let copied = std::io::copy(
                &mut file.by_ref().take(available.saturating_add(1)),
                &mut output,
            )
            .ok()?;
            if copied != declared || copied > available {
                return None;
            }
            remaining = remaining.checked_sub(copied)?;
        }
        let mut stored = output.finish_into_readable().ok()?;
        let mut worksheets = BTreeMap::new();
        for name in parts.into_values() {
            let file = stored.by_name(name).ok()?;
            let start = usize::try_from(file.data_start()?)
                .ok()?
                .checked_sub(prefix_len)?;
            let end = start.checked_add(usize::try_from(file.size()).ok()?)?;
            worksheets.insert(name.to_owned(), start..end);
        }
        let (prefix, suffix) = stored.into_inner().finish()?;
        if worksheets
            .values()
            .any(|range| suffix.get(range.clone()).is_none())
        {
            return None;
        }
        Some(AppendedWorksheets {
            prefix,
            suffix,
            worksheets,
        })
    }

    /// Expand one admitted original part under the configured per-entry limit.
    pub(crate) fn read(&mut self, name: &str) -> Result<Vec<u8>, IoError> {
        let index =
            self.entries.get(name).copied().ok_or_else(|| {
                IoError::Opc(crate::opc::OpcError::MissingPart { path: name.into() })
            })?;
        let mut file = self.zip.by_index(index).map_err(xml)?;
        let mut contents = Vec::new();
        file.by_ref()
            .take(self.max_entry_bytes.saturating_add(1))
            .read_to_end(&mut contents)?;
        if contents.len() as u64 > self.max_entry_bytes {
            return Err(IoError::quota(
                crate::IoResource::ZipEntryBytes,
                self.max_entry_bytes,
                0,
                contents.len() as u64,
                crate::IoOperation::Entry { path: name.into() },
            ));
        }
        Ok(contents)
    }

    /// Resolve relationships from one original part using the package catalog.
    pub(crate) fn relationships(
        &mut self,
        owner: &str,
        warnings: &mut Vec<crate::ImportDiagnostic>,
        policy: crate::opc::OpcImportPolicy,
        selected: impl Fn(&str) -> bool,
    ) -> Result<BTreeMap<String, crate::opc::Relationship>, IoError> {
        let path = crate::opc::relationship_part(owner);
        let bytes = self.read(&path)?;
        crate::opc::parse_relationships(owner, &bytes, warnings, policy, selected, |target| {
            if self.entries.contains_key(target) {
                Ok(())
            } else {
                Err(IoError::Opc(crate::opc::OpcError::MissingPart {
                    path: target.into(),
                }))
            }
        })
    }

    /// Resolve OPC identities and return the expanded workbook XML for import.
    pub(crate) fn plan(
        &mut self,
        policy: crate::opc::OpcImportPolicy,
    ) -> Result<(crate::opc::OpcReadPlan, Vec<u8>), IoError> {
        // Empty nonmetadata entries are catalog existence markers only. The
        // planner reads XML exclusively from the three expanded metadata parts.
        let mut inventory = crate::opc::PackageInventory {
            parts: self
                .entries
                .keys()
                .map(|name| (name.clone(), Vec::new()))
                .collect(),
        };
        let root = crate::opc::relationship_part("");
        inventory.parts.insert(root.clone(), self.read(&root)?);
        let mut warnings = Vec::new();
        let workbook = inventory.workbook_part(policy, &mut warnings)?;
        let relationships = crate::opc::relationship_part(&workbook);
        inventory
            .parts
            .insert(workbook.clone(), self.read(&workbook)?);
        inventory
            .parts
            .insert(relationships.clone(), self.read(&relationships)?);
        let plan = inventory.plan_from_workbook(policy, workbook.clone(), warnings)?;
        let workbook_xml = inventory.parts.remove(&workbook).ok_or(IoError::Opc(
            crate::opc::OpcError::MissingPart { path: workbook },
        ))?;
        Ok((plan, workbook_xml))
    }
}

/// Expand XLSX entries under explicit compressed/uncompressed resource limits.
pub(crate) fn read_package_with_options(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<Package, IoError> {
    if bytes.len() as u64 > options.max_input_bytes {
        return Err(IoError::quota(
            crate::IoResource::InputBytes,
            options.max_input_bytes,
            0,
            bytes.len() as u64,
            crate::IoOperation::XlsxPackage,
        ));
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(xml)?;
    if zip.len() > options.max_zip_entries {
        return Err(IoError::quota(
            crate::IoResource::ZipEntries,
            options.max_zip_entries as u64,
            0,
            zip.len() as u64,
            crate::IoOperation::XlsxCatalog,
        ));
    }
    let mut total = 0u64;
    let mut result = Package::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(xml)?;
        total = total
            .checked_add(file.size())
            .ok_or(IoError::SizeOverflow)?;
        if file.size() > options.max_zip_entry_bytes {
            return Err(IoError::quota(
                crate::IoResource::ZipEntryBytes,
                options.max_zip_entry_bytes,
                0,
                file.size(),
                crate::IoOperation::Entry {
                    path: file.name().map_err(xml)?.into_owned(),
                },
            ));
        }
        if total > options.max_zip_total_bytes {
            return Err(IoError::quota(
                crate::IoResource::ZipTotalBytes,
                options.max_zip_total_bytes,
                total.saturating_sub(file.size()),
                file.size(),
                crate::IoOperation::XlsxCatalog,
            ));
        }
        if file.is_dir() {
            continue;
        }
        let name = file
            .name()
            .map_err(|e| IoError::Xml(e.into()))?
            .into_owned();
        let mut contents = Vec::new();
        file.by_ref()
            .take(options.max_zip_entry_bytes.saturating_add(1))
            .read_to_end(&mut contents)?;
        if contents.len() as u64 > options.max_zip_entry_bytes {
            return Err(IoError::quota(
                crate::IoResource::ZipEntryBytes,
                options.max_zip_entry_bytes,
                0,
                contents.len() as u64,
                crate::IoOperation::Entry { path: name.clone() },
            ));
        }
        if result.insert(name.clone(), contents).is_some() {
            return Err(IoError::Unsupported(
                crate::WorkbookFeatureError::DuplicateZipEntry { name: name.clone() },
            ));
        }
    }
    Ok(result)
}

/// Reject generated claims before allocating expanded entries.
pub(super) fn check_generated_claims(
    bytes: &[u8],
    options: crate::WriteOptions,
) -> Result<(), IoError> {
    // Inspect every generated entry claim before allocating expanded parts.
    let mut generated_zip = ZipArchive::new(Cursor::new(bytes)).map_err(xml)?;
    if generated_zip.len() > options.max_zip_entries {
        return Err(IoError::Limit(crate::ExportLimitError::EntryCount {
            requested: generated_zip.len(),
            limit: options.max_zip_entries,
        }));
    }
    let mut generated_total = 0u64;
    for index in 0..generated_zip.len() {
        let entry = generated_zip.by_index(index).map_err(xml)?;
        generated_total = generated_total
            .checked_add(entry.size())
            .ok_or(IoError::Limit(
                crate::ExportLimitError::ExportPackageSizeOverflow,
            ))?;
        if entry.size() > options.max_zip_entry_bytes
            || generated_total > options.max_zip_total_bytes
        {
            return Err(IoError::Limit(crate::ExportLimitError::PackageBytes {
                entry_bytes: entry.size(),
                entry_limit: options.max_zip_entry_bytes,
                total_bytes: generated_total,
                total_limit: options.max_zip_total_bytes,
            }));
        }
    }
    Ok(())
}

/// Validate expanded parts and serialize the final package in lexical order.
pub(super) fn write_export(
    package: Package,
    options: crate::WriteOptions,
) -> Result<Vec<u8>, IoError> {
    if package.len() > options.max_zip_entries {
        return Err(IoError::Limit(crate::ExportLimitError::EntryCount {
            requested: package.len(),
            limit: options.max_zip_entries,
        }));
    }
    let mut total = 0u64;
    for part in package.values() {
        let size = part.len() as u64;
        total = total.checked_add(size).ok_or(IoError::Limit(
            crate::ExportLimitError::ExportPackageSizeOverflow,
        ))?;
        if size > options.max_zip_entry_bytes || total > options.max_zip_total_bytes {
            return Err(IoError::Limit(crate::ExportLimitError::PackageBytes {
                entry_bytes: size,
                entry_limit: options.max_zip_entry_bytes,
                total_bytes: total,
                total_limit: options.max_zip_total_bytes,
            }));
        }
    }
    let max_output_bytes = options.max_output_bytes;
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in package {
        zip.start_file(name, options).map_err(xml)?;
        zip.write_all(&bytes)?;
    }
    let output = zip.finish().map_err(xml)?.into_inner();
    if output.len() as u64 > max_output_bytes {
        return Err(IoError::Limit(crate::ExportLimitError::OutputBytes {
            requested: output.len() as u64,
            limit: max_output_bytes,
        }));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an original test package with nonstandard worksheet part names.
    fn fixture() -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in [
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="meta/book.xml"/></Relationships>"#,
            ),
            (
                "meta/book.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="data"/></sheets></workbook>"#,
            ),
            (
                "meta/_rels/book.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="data" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../cells/data.xml"/></Relationships>"#,
            ),
            ("cells/data.xml", "original worksheet payload"),
            (
                "unused.bin",
                "unneeded payload with deliberately invalid CRC",
            ),
        ] {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn planning_uses_catalog_without_expanding_nonmetadata_parts() {
        let mut bytes = fixture();
        let payload = b"unneeded payload with deliberately invalid CRC";
        let offset = bytes
            .windows(payload.len())
            .position(|chunk| chunk == payload)
            .unwrap();
        bytes[offset] ^= 1;
        let mut package = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        let (plan, workbook_xml) = package
            .plan(crate::opc::OpcImportPolicy::default())
            .unwrap();
        assert_eq!(plan.workbook_part, "meta/book.xml");
        assert_eq!(workbook_xml, package.read(&plan.workbook_part).unwrap());
        assert_eq!(plan.sheets[0].part, "cells/data.xml");
        assert_eq!(
            package.read(&plan.sheets[0].part).unwrap(),
            b"original worksheet payload"
        );
        assert!(package.read("unused.bin").is_err());
    }

    #[test]
    fn catalog_still_rejects_claims_for_parts_that_would_not_be_read() {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("unused.bin", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&[0; 4096]).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(matches!(
            ImportPackage::new(
                &bytes,
                ReadOptions {
                    max_zip_entry_bytes: 1024,
                    ..ReadOptions::default()
                }
            ),
            Err(IoError::Quota(_))
        ));
        assert!(matches!(
            ImportPackage::new(
                &fixture(),
                ReadOptions {
                    max_zip_entries: 4,
                    ..ReadOptions::default()
                }
            ),
            Err(IoError::Quota(_))
        ));
    }

    #[test]
    fn catalog_rejects_an_encrypted_entry_before_planning() {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("unused.bin", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"unused").unwrap();
        let mut bytes = zip.finish().unwrap().into_inner();
        // Set the ZIP encryption flag on both headers of the one unused part.
        for (signature, flag_offset) in [(b"PK\x03\x04", 6), (b"PK\x01\x02", 8)] {
            let offset = bytes.windows(4).position(|word| word == signature).unwrap();
            bytes[offset + flag_offset] |= 1;
        }
        assert!(matches!(
            ImportPackage::new(&bytes, ReadOptions::default()),
            Err(IoError::Xml(crate::XmlCodecError::Zip(
                zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED,)
            ))),
        ));
    }

    #[test]
    fn planning_retains_missing_relationship_target_errors() {
        let bytes = fixture();
        let mut package = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        package.entries.remove("cells/data.xml");
        assert!(
            matches!(package.plan(crate::opc::OpcImportPolicy::default()), Err(IoError::Opc(crate::opc::OpcError::MissingPart {path:part})) if part == "cells/data.xml")
        );
    }
}

#[cfg(test)]
mod worksheet_archive_tests {
    use super::ImportPackage;
    use crate::ReadOptions;
    use crate::opc::{OpcReadPlan, SheetIdentity};
    use std::io::{Cursor, Read, Write};
    use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

    const PART: &str = "xl/worksheets/sheet1.xml";
    const OTHER: &str = "xl/other.xml";

    /// Build a checked-identity fixture with a worksheet large enough for reuse.
    fn fixture() -> (Vec<u8>, Vec<u8>, OpcReadPlan) {
        let mut state = 0xda093f84u32;
        let sheet = (0..32 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect::<Vec<_>>();
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file(PART, options).unwrap();
        archive.write_all(&sheet).unwrap();
        archive.start_file(OTHER, options).unwrap();
        archive.write_all(b"metadata").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let plan = OpcReadPlan {
            workbook_part: "xl/workbook.xml".to_owned(),
            styles_part: None,
            theme_part: None,
            shared_strings_part: None,
            sheets: vec![SheetIdentity {
                source_index: 0,
                name: "Data".to_owned(),
                sheet_id: "1".to_owned(),
                relationship_id: "rId1".to_owned(),
                part: PART.to_owned(),
                supported: true,
            }],
            warnings: Vec::new(),
        };
        (bytes, sheet, plan)
    }

    #[test]
    fn reuses_checked_payload_and_preserves_other_part_bytes() {
        let (bytes, sheet, plan) = fixture();
        let mut source = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        assert!(source.worksheet_archive(&plan, 8 * 1024 * 1024).is_none());
        let archive = source.worksheet_archive(&plan, bytes.len()).unwrap();
        assert_eq!(archive.worksheet(PART), Some(sheet.as_slice()));
        assert!(archive.worksheet(OTHER).is_none());
        let mut rebuilt = ZipArchive::new(Cursor::new(archive.bytes())).unwrap();
        assert_eq!(
            rebuilt.by_name(PART).unwrap().compression(),
            CompressionMethod::Stored
        );
        let mut other = rebuilt.by_name(OTHER).unwrap();
        assert_eq!(other.compression(), CompressionMethod::Deflated);
        let mut text = String::new();
        other.read_to_string(&mut text).unwrap();
        assert_eq!(text, "metadata");
    }

    #[test]
    fn appended_records_preserve_original_prefix_and_checked_payload() {
        let (bytes, sheet, plan) = fixture();
        let mut source = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        let archive = source.appended_worksheets(&plan, &bytes).unwrap();
        assert!(std::ptr::eq(archive.prefix.as_ptr(), bytes.as_ptr()));
        assert_eq!(
            archive.prefix.len() as u64,
            source.zip.central_directory_start()
        );
        assert_eq!(archive.worksheet(PART), Some(sheet.as_slice()));
        let mut rebuilt = ZipArchive::new(archive.reader()).unwrap();
        let mut value = Vec::new();
        let mut saved = rebuilt.by_name(PART).unwrap();
        assert_eq!(saved.compression(), CompressionMethod::Stored);
        saved.read_to_end(&mut value).unwrap();
        assert_eq!(value, sheet);
        drop(saved);
        let mut other = rebuilt.by_name(OTHER).unwrap();
        assert_eq!(other.compression(), CompressionMethod::Deflated);
        let mut value = String::new();
        other.read_to_string(&mut value).unwrap();
        assert_eq!(value, "metadata");
    }

    #[test]
    fn appending_does_not_reorder_backend_path_aliases() {
        let (bytes, _, plan) = fixture();
        let mut zip = ZipWriter::new_append(Cursor::new(bytes)).unwrap();
        zip.start_file("xl/WORKSHEETS/SHEET1.xml", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"Different worksheet bytes").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let mut source = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        assert!(source.appended_worksheets(&plan, &bytes).is_none());
    }

    #[test]
    fn corrupt_worksheet_is_left_to_the_original_reader() {
        let (mut bytes, _, plan) = fixture();
        let start = {
            let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
            archive.by_name(PART).unwrap().data_start().unwrap() as usize
        };
        bytes[start] = 0xff;
        let mut source = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        assert!(source.worksheet_archive(&plan, bytes.len()).is_none());
        assert!(source.appended_worksheets(&plan, &bytes).is_none());
        assert!(source.read(PART).is_err());
    }
}
