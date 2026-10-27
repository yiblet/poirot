//! Bounded ZIP entry expansion.
use super::xml::xml;
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
            let file = zip.by_index(index).map_err(xml)?;
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

    /// Resolve OPC identities using catalog names and only relationship/workbook XML.
    pub(crate) fn plan(
        &mut self,
        policy: crate::opc::OpcImportPolicy,
    ) -> Result<crate::opc::OpcReadPlan, IoError> {
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
        let workbook = inventory.workbook_part(policy)?;
        let relationships = crate::opc::relationship_part(&workbook);
        inventory
            .parts
            .insert(workbook.clone(), self.read(&workbook)?);
        inventory
            .parts
            .insert(relationships.clone(), self.read(&relationships)?);
        inventory.plan_with_policy(policy)
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
        let plan = package
            .plan(crate::opc::OpcImportPolicy::default())
            .unwrap();
        assert_eq!(plan.workbook_part, "meta/book.xml");
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
    fn planning_retains_missing_relationship_target_errors() {
        let bytes = fixture();
        let mut package = ImportPackage::new(&bytes, ReadOptions::default()).unwrap();
        package.entries.remove("cells/data.xml");
        assert!(
            matches!(package.plan(crate::opc::OpcImportPolicy::default()), Err(IoError::Opc(crate::opc::OpcError::MissingPart {path:part})) if part == "cells/data.xml")
        );
    }
}
