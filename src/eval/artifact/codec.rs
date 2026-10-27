use super::bounded;
use super::output::Output;
use super::prepared_policy::PreparedPolicy;
use super::prepared_status::PreparedStatus;
use super::restored_artifact::RestoredArtifact;
use super::snapshot_error::SnapshotError;
use super::snapshot_limits::SnapshotLimits;
use super::snapshot_quota_failure::SnapshotQuotaFailure;
use super::snapshot_resource::SnapshotResource;
use super::source_payload::SourcePayload;
use crate::eval::overlay::snapshot::Snapshot;
use crate::model::Workbook;
use serde::Serialize;
use std::{io::Read, path::Path};

pub(super) const MAGIC: &[u8; 8] = b"ROISNAP\0";

pub(super) const FILE_VERSION: u16 = 1;

pub(super) const SOURCE_SCHEMA: u16 = crate::model::snapshot::SCHEMA_VERSION;

pub(super) const HEADER: usize = 40;

/// Bytes of the CRC-32 that ends a prepared section, identifying the source section it was
/// written beside. Prepared state may rely on that source as its baseline workbook.
pub(super) const SOURCE_CHECKSUM: usize = 4;

/// Encode workbook source and its file metadata without engine preparation.
pub fn encode_workbook(
    workbook: &Workbook,
    limits: SnapshotLimits,
) -> Result<Vec<u8>, SnapshotError> {
    encode(workbook, &[], 0, limits)
}

/// Encode a direct or overlay snapshot with independent effective-source recovery.
/// Authored edits survive discarding incompatible preparation; temporary results are not
/// promoted to inputs. Runtime extension implementations must be reattached after load.
/// Preparing the fallback materializes a workbook; limits are not a peak-memory cap.
/// Unedited preparation omits its baseline model: it restores against the source section,
/// whose CRC-32 ends the prepared section.
pub fn encode_workspace(
    snapshot: Snapshot<'_>,
    limits: SnapshotLimits,
) -> Result<Vec<u8>, SnapshotError> {
    let mut serializer = postcard::Serializer {
        output: Output {
            bytes: Vec::new(),
            limit: limits
                .max_file_bytes
                .saturating_sub(HEADER + SOURCE_CHECKSUM),
        },
    };
    snapshot
        .serialize_prepared(&mut serializer)
        .map_err(SnapshotError::Codec)?;
    let mut prepared = serializer.output.bytes;
    let workbook = snapshot
        .effective_source()
        .map_err(SnapshotError::Workspace)?;
    let source = encode_source(
        &workbook,
        prepared.len().saturating_add(SOURCE_CHECKSUM),
        limits,
    )?;
    prepared.extend_from_slice(&crc32fast::hash(&source).to_le_bytes());
    Ok(frame(
        &source,
        &prepared,
        crate::eval::overlay::snapshot::COMPATIBILITY_REVISION,
    ))
}

/// Frame `workbook` as the source section beside an already encoded prepared section.
pub(super) fn encode(
    workbook: &Workbook,
    prepared: &[u8],
    revision: u64,
    limits: SnapshotLimits,
) -> Result<Vec<u8>, SnapshotError> {
    let source = encode_source(workbook, prepared.len(), limits)?;
    Ok(frame(&source, prepared, revision))
}

/// Serialize the source section within the file bound left by a prepared section.
fn encode_source(
    workbook: &Workbook,
    prepared_len: usize,
    limits: SnapshotLimits,
) -> Result<Vec<u8>, SnapshotError> {
    let source_limit = limits
        .max_file_bytes
        .checked_sub(HEADER)
        .and_then(|n| n.checked_sub(prepared_len))
        .ok_or(SnapshotError::Envelope("file byte limit"))?;
    serialize(
        &SourcePayload {
            model: workbook.snapshot(),
        },
        source_limit,
    )
}

/// Write the fixed header followed by the source and prepared sections.
fn frame(source: &[u8], prepared: &[u8], revision: u64) -> Vec<u8> {
    let mut file = Vec::with_capacity(HEADER + source.len() + prepared.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&FILE_VERSION.to_le_bytes());
    file.extend_from_slice(&SOURCE_SCHEMA.to_le_bytes());
    file.push(u8::from(!prepared.is_empty()));
    file.extend_from_slice(&[0; 3]);
    file.extend_from_slice(&revision.to_le_bytes());
    file.extend_from_slice(&(source.len() as u64).to_le_bytes());
    file.extend_from_slice(&(prepared.len() as u64).to_le_bytes());
    file.extend_from_slice(source);
    file.extend_from_slice(prepared);
    file
}

/// Inspect the fixed envelope before any payload decode; then bound both sections with
/// one cumulative budget. Trailing data, unknown kinds and unknown source versions fail.
/// Compatible but malformed preparation fails rather than silently hiding corruption.
pub fn decode(
    bytes: &[u8],
    limits: SnapshotLimits,
    policy: PreparedPolicy,
) -> Result<RestoredArtifact, SnapshotError> {
    if limits.max_depth > 256 {
        return Err(SnapshotError::EngineConstraint(
            "recursive decoder nesting ceiling is 256",
        ));
    }
    let (encoded_source, prepared, revision) = sections(bytes, limits.max_file_bytes)?;
    let schema = bytes
        .get(10..12)
        .and_then(|b| <[u8; 2]>::try_from(b).ok())
        .map(u16::from_le_bytes)
        .ok_or(SnapshotError::Envelope("source schema missing"))?;
    let compatible = schema == SOURCE_SCHEMA
        && revision == crate::eval::overlay::snapshot::COMPATIBILITY_REVISION;
    if policy == PreparedPolicy::RequireCompatible && prepared.is_empty() {
        return Err(SnapshotError::Envelope(
            "compatible prepared state required but absent",
        ));
    }
    if policy == PreparedPolicy::RequireCompatible && !compatible {
        return Err(SnapshotError::Envelope(
            "incompatible prepared engine revision",
        ));
    }
    let budget = bounded::Budget::new(
        limits.max_decoded_values,
        limits.max_decoded_bytes,
        limits.max_depth,
    );
    let source: SourcePayload<'static> = deserialize(encoded_source, &budget)?;
    let workbook = Workbook::from_snapshot(source.model).map_err(SnapshotError::Source)?;
    let status = if prepared.is_empty() {
        PreparedStatus::Absent
    } else if policy == PreparedPolicy::SourceOnly {
        PreparedStatus::DiscardedByRequest
    } else if !compatible {
        PreparedStatus::DiscardedIncompatible
    } else {
        PreparedStatus::Restored
    };
    if status != PreparedStatus::Restored {
        return Ok(RestoredArtifact {
            workbook,
            workspace: None,
            prepared_status: status,
        });
    }
    let (prepared, checksum) = prepared
        .split_last_chunk::<SOURCE_CHECKSUM>()
        .ok_or(SnapshotError::Envelope("truncated prepared section"))?;
    if u32::from_le_bytes(*checksum) != crc32fast::hash(encoded_source) {
        return Err(SnapshotError::Envelope(
            "source and prepared state disagree",
        ));
    }
    let snapshot: Snapshot<'static> = deserialize(prepared, &budget)?;
    // The source section is the postcard model snapshot the workbook was decoded from:
    // a single-field struct encodes exactly as its field.
    let (baseline, state) = snapshot
        .restore_with_source(workbook, encoded_source)
        .map_err(SnapshotError::Workspace)?;
    Ok(RestoredArtifact {
        workbook: baseline,
        workspace: Some(state),
        prepared_status: status,
    })
}

/// Read at most the configured file bound plus one sentinel byte. The bound is checked
/// again after reading, so growing files cannot bypass a metadata-only size check.
pub fn read_file(
    path: impl AsRef<Path>,
    limits: SnapshotLimits,
    policy: PreparedPolicy,
) -> Result<RestoredArtifact, SnapshotError> {
    if limits.max_depth > 256 {
        return Err(SnapshotError::EngineConstraint(
            "recursive decoder nesting ceiling is 256",
        ));
    }
    let file = std::fs::File::open(path).map_err(SnapshotError::from_io)?;
    if file.metadata().map_err(SnapshotError::from_io)?.len() > limits.max_file_bytes as u64 {
        return Err(SnapshotError::Quota(SnapshotQuotaFailure {
            resource: SnapshotResource::FileBytes,
            limit: limits.max_file_bytes,
            consumed: 0,
            requested: usize::try_from(file.metadata().map_err(SnapshotError::from_io)?.len())
                .unwrap_or(usize::MAX),
        }));
    }
    let mut bytes = Vec::new();
    file.take((limits.max_file_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(SnapshotError::from_io)?;
    decode(&bytes, limits, policy)
}

pub(super) fn sections(bytes: &[u8], max: usize) -> Result<(&[u8], &[u8], u64), SnapshotError> {
    let invalid = SnapshotError::Envelope;
    if bytes.len() > max {
        return Err(SnapshotError::Quota(SnapshotQuotaFailure {
            resource: SnapshotResource::FileBytes,
            limit: max,
            consumed: 0,
            requested: bytes.len(),
        }));
    }
    // SAFETY: The header guard requires at least HEADER (40) bytes; section lengths are checked against bytes.len() before slicing.
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(invalid("invalid magic or truncated header"));
    }
    // SAFETY: The header guard requires at least HEADER (40) bytes; section lengths are checked against bytes.len() before slicing.
    if u16::from_le_bytes([bytes[8], bytes[9]]) != FILE_VERSION {
        return Err(invalid("unsupported file version"));
    }
    // SAFETY: The header guard requires at least HEADER (40) bytes; section lengths are checked against bytes.len() before slicing.
    if u16::from_le_bytes([bytes[10], bytes[11]]) != SOURCE_SCHEMA {
        return Err(invalid("unsupported source schema"));
    }
    // SAFETY: The header guard requires at least HEADER (40) bytes; section lengths are checked against bytes.len() before slicing.
    let kind = bytes[12];
    // SAFETY: The header guard requires at least HEADER (40) bytes; section lengths are checked against bytes.len() before slicing.
    if kind > 1 || bytes[13..16] != [0; 3] {
        return Err(invalid("unknown kind or header flags"));
    }
    let number = |at: usize| -> Result<u64, SnapshotError> {
        let end = at.checked_add(8).ok_or(invalid("header offset overflow"))?;
        let word = bytes.get(at..end).ok_or(invalid("truncated header word"))?;
        let word = word.try_into().map_err(|_| invalid("header word width"))?;
        Ok(u64::from_le_bytes(word))
    };
    let revision = number(16)?;
    let source_len = usize::try_from(number(24)?).map_err(|_| invalid("source length overflow"))?;
    let prepared_len =
        usize::try_from(number(32)?).map_err(|_| invalid("prepared length overflow"))?;
    let split = HEADER
        .checked_add(source_len)
        .ok_or(invalid("source length overflow"))?;
    if split.checked_add(prepared_len) != Some(bytes.len()) || source_len == 0 {
        return Err(invalid("section lengths or trailing data"));
    }
    if (kind == 0 && (prepared_len != 0 || revision != 0))
        || (kind == 1 && (prepared_len == 0 || revision == 0))
    {
        return Err(invalid("kind does not match prepared section"));
    }
    // SAFETY: The header guard requires at least HEADER (40) bytes; section lengths are checked against bytes.len() before slicing.
    Ok((&bytes[HEADER..split], &bytes[split..], revision))
}

pub(super) fn deserialize<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    budget: &bounded::Budget,
) -> Result<T, SnapshotError> {
    let mut decoder = postcard::Deserializer::from_bytes(bytes);
    let value = T::deserialize(bounded::Decoder {
        inner: &mut decoder,
        budget,
        depth: 0,
    })
    .map_err(|error| budget.failure().unwrap_or(SnapshotError::Codec(error)))?;
    if !decoder.finalize().map_err(SnapshotError::Codec)?.is_empty() {
        return Err(SnapshotError::Envelope("trailing section bytes"));
    }
    Ok(value)
}

pub(super) fn serialize(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, SnapshotError> {
    postcard::serialize_with_flavor(
        value,
        Output {
            bytes: Vec::new(),
            limit,
        },
    )
    .map_err(SnapshotError::Codec)
}
