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

/// Expand XLSX entries under explicit compressed/uncompressed resource limits.
pub(crate) fn read_package_with_options(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<Package, IoError> {
    if bytes.len() as u64 > options.max_input_bytes {
        return Err(IoError::Limit(
            "compressed package exceeds configured input budget".into(),
        ));
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(xml)?;
    if zip.len() > options.max_zip_entries {
        return Err(IoError::Limit(
            "package exceeds configured entry-count budget".into(),
        ));
    }
    let mut total = 0u64;
    let mut result = Package::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(xml)?;
        total = total
            .checked_add(file.size())
            .ok_or_else(|| IoError::Limit("package size overflow".into()))?;
        if file.size() > options.max_zip_entry_bytes || total > options.max_zip_total_bytes {
            return Err(IoError::Limit(
                "uncompressed package exceeds configured entry/total budget".into(),
            ));
        }
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_owned();
        let mut contents = Vec::new();
        file.by_ref()
            .take(options.max_zip_entry_bytes.saturating_add(1))
            .read_to_end(&mut contents)?;
        if contents.len() as u64 > options.max_zip_entry_bytes {
            return Err(IoError::Limit(
                "entry expanded past configured budget".into(),
            ));
        }
        if result.insert(name.clone(), contents).is_some() {
            return Err(IoError::Unsupported(format!(
                "duplicate ZIP entry {name}: ambiguous import not yet covered"
            )));
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
        return Err(IoError::Limit(
            "export package exceeds configured entry-count budget".into(),
        ));
    }
    let mut generated_total = 0u64;
    for index in 0..generated_zip.len() {
        let entry = generated_zip.by_index(index).map_err(xml)?;
        generated_total = generated_total
            .checked_add(entry.size())
            .ok_or_else(|| IoError::Limit("export package size overflow".into()))?;
        if entry.size() > options.max_zip_entry_bytes
            || generated_total > options.max_zip_total_bytes
        {
            return Err(IoError::Limit(
                "generated export ZIP exceeds entry/total budget before expansion".into(),
            ));
        }
    }
    Ok(())
}

/// Validate expanded parts and serialize the final package in lexical order.
pub(super) fn write_export(
    package: Package,
    options: crate::WriteOptions,
) -> Result<Vec<u8>, IoError> {
    let mut total = 0u64;
    for (name, part) in &package {
        let size = part.len() as u64;
        total = total
            .checked_add(size)
            .ok_or_else(|| IoError::Limit("export package size overflow".into()))?;
        if size > options.max_zip_entry_bytes || total > options.max_zip_total_bytes {
            return Err(IoError::Limit(format!(
                "export part {name} exceeds configured entry/total budget"
            )));
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
        return Err(IoError::Limit(format!(
            "export ZIP exceeds max_output_bytes {max_output_bytes}"
        )));
    }
    Ok(output)
}
