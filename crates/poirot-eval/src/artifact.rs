//! Versioned on-disk snapshots with independently recoverable source and optional prepared state.
//!
//! Source schema revisions are independent of engine-prepared compatibility revisions.
//! Unknown source schemas are rejected; incompatible optional prepared state is discarded
//! explicitly, preserving authored edits. These are trusted application artifacts, not an
//! untrusted bytecode interchange format. Decode budgets bound logical values, nesting and
//! copied text/bytes, not allocator-exact peak memory or subsequent engine restoration work.
mod bounded;

use crate::overlay::snapshot::{Snapshot as OverlaySnapshot, WorkspaceState};
use crate::overlay::{Evaluation, EvaluationContext, Overlay, OverlayPolicy};
use poirot_model::{Workbook, snapshot::Snapshot as WorkbookSnapshot};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Read, path::Path};

const MAGIC: &[u8; 8] = b"ROISNAP\0";
const FILE_VERSION: u16 = 1;
const SOURCE_SCHEMA: u16 = poirot_model::snapshot::SCHEMA_VERSION;
const HEADER: usize = 40;

/// Optional snapshot loading quotas checked before admission; integer maxima mean no quota.
/// The default has no size quotas and retains a 256-level recursive decoder safety ceiling.
/// Save builders independently use [`Self::for_writing`] to preserve export defaults.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotLimits {
    /// Maximum complete file bytes, including envelope and optional prepared section.
    pub max_file_bytes: usize,
    /// Maximum number of typed Serde values across both sections, including scalar fields.
    pub max_decoded_values: usize,
    /// Maximum aggregate string/byte payload copied by decoding, across both sections.
    pub max_decoded_bytes: usize,
    /// Maximum nested typed values, including enums, structs and collection elements.
    pub max_depth: usize,
}
impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: usize::MAX,
            max_decoded_values: usize::MAX,
            max_decoded_bytes: usize::MAX,
            max_depth: 256,
        }
    }
}

impl SnapshotLimits {
    /// Historical snapshot output quotas, retained independently of unlimited loading defaults.
    pub fn for_writing() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            max_decoded_values: 8_000_000,
            max_decoded_bytes: 64 * 1024 * 1024,
            max_depth: 256,
        }
    }
}
/// Whether loading may reuse engine-prepared data or must recover only durable source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreparedPolicy {
    /// Reuse compatible state, otherwise return source and an explicit discard status.
    #[default]
    ReuseCompatible,
    /// Do not decode optional prepared data, even if its engine revision matches.
    /// Recovered source needs an explicit workspace policy; prepared policies are discarded.
    SourceOnly,
    /// Refuse a prepared artifact whose engine compatibility revision differs.
    RequireCompatible,
}

/// Outcome for the optional prepared section; source is always restored and validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedStatus {
    /// The file contained only source.
    Absent,
    /// Prepared state was validated; providers still need normal registry attachment.
    Restored,
    /// Source and authored edits recovered; engine-prepared state and policies were incompatible.
    DiscardedIncompatible,
    /// Source and authored edits recovered as explicitly requested.
    DiscardedByRequest,
}

/// Validated workbook and optional detached workspace, ready for ordinary evaluation.
/// With a workspace, `workbook` is its exact baseline; otherwise it is effective authored
/// source including all persisted edits. Attach the workspace using its existing APIs.
pub struct RestoredArtifact {
    /// Baseline for `workspace`, or effective source when no prepared state was retained.
    pub workbook: Workbook,
    /// Validated compiled/direct or incremental state; runtime providers are never stored.
    pub workspace: Option<WorkspaceState>,
    /// Explicit disposition of optional prepared state.
    pub prepared_status: PreparedStatus,
}

/// Resource admitted during native snapshot loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotResource {
    /// Entire source file bytes.
    FileBytes,
    /// Typed values across source and prepared sections.
    DecodedValues,
    /// Copied text and byte payload across sections.
    DecodedBytes,
    /// Caller-selected recursive nesting below the engine ceiling.
    Depth,
}
/// A configured native-snapshot quota refusal with actual decoder admission counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotQuotaFailure {
    /// File bytes, decoded values, decoded bytes, or caller-selected depth.
    pub resource: SnapshotResource,
    /// Configured maximum.
    pub limit: usize,
    /// Amount admitted before this refused operation.
    pub consumed: usize,
    /// Additional amount requested.
    pub requested: usize,
}
/// File-envelope, codec, resource, or semantic restoration failure.
#[derive(Debug)]
pub enum SnapshotError {
    /// Caller-configured file or decoded-resource quota, separate from malformed input.
    Quota(SnapshotQuotaFailure),
    /// Recursive decoder safety guard or an invalid requested nesting ceiling.
    EngineConstraint(&'static str),
    /// A fallible file-buffer allocation failed; no invented byte count is reported.
    Allocation(&'static str),
    /// Filesystem ingress failure.
    Io(std::io::Error),
    /// Unsupported, inconsistent, truncated, or oversized envelope.
    Envelope(&'static str),
    /// Bounded payload encoding or decoding failed.
    Codec(postcard::Error),
    /// Workbook source invariants were invalid.
    Source(poirot_model::ModelError),
    /// Prepared state failed the evaluator's semantic validation.
    Workspace(crate::overlay::OverlayError),
}
impl SnapshotError {
    /// Preserve fallible file-buffer allocation failures independently of filesystem failures.
    fn from_io(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::OutOfMemory {
            Self::Allocation("snapshot file buffer")
        } else {
            Self::Io(error)
        }
    }
}
impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quota(e) => write!(
                f,
                "snapshot {:?} quota {} exceeded (consumed {}, requested {})",
                e.resource, e.limit, e.consumed, e.requested
            ),
            Self::EngineConstraint(e) => write!(f, "snapshot engine constraint: {e}"),
            Self::Allocation(e) => write!(f, "snapshot allocation refused: {e}"),
            Self::Io(e) => write!(f, "snapshot file: {e}"),
            Self::Envelope(e) => write!(f, "snapshot envelope: {e}"),
            Self::Codec(e) => write!(f, "snapshot codec or decode budget: {e}"),
            Self::Source(e) => write!(f, "snapshot source: {e}"),
            Self::Workspace(e) => write!(f, "snapshot workspace: {e}"),
        }
    }
}
impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Codec(e) => Some(e),
            Self::Source(e) => Some(e),
            Self::Workspace(e) => Some(e),
            Self::Envelope(_)
            | Self::Quota(_)
            | Self::EngineConstraint(_)
            | Self::Allocation(_) => None,
        }
    }
}

/// Durable source payload independent of optional evaluator preparation.
// This source schema is a maintained file contract. Bump SOURCE_SCHEMA when changing it
// or the model snapshot wire layout; prepared engine changes need not invalidate source.
#[derive(Serialize, Deserialize)]
struct SourcePayload<'a> {
    model: WorkbookSnapshot<'a>,
}

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
pub fn encode_workspace(
    snapshot: OverlaySnapshot<'_>,
    limits: SnapshotLimits,
) -> Result<Vec<u8>, SnapshotError> {
    let prepared = serialize(&snapshot, limits.max_file_bytes.saturating_sub(HEADER))?;
    let (baseline, state) = Overlay::from_snapshot(snapshot).map_err(SnapshotError::Workspace)?;
    let source = state
        .source_workbook(&baseline)
        .map_err(SnapshotError::Workspace)?;
    encode(
        &source,
        &prepared,
        crate::overlay::snapshot::COMPATIBILITY_REVISION,
        limits,
    )
}

fn encode(
    workbook: &Workbook,
    prepared: &[u8],
    revision: u64,
    limits: SnapshotLimits,
) -> Result<Vec<u8>, SnapshotError> {
    let source_limit = limits
        .max_file_bytes
        .checked_sub(HEADER)
        .and_then(|n| n.checked_sub(prepared.len()))
        .ok_or(SnapshotError::Envelope("file byte limit"))?;
    let source = serialize(
        &SourcePayload {
            model: workbook.snapshot(),
        },
        source_limit,
    )?;
    let mut file = Vec::with_capacity(HEADER + source.len() + prepared.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&FILE_VERSION.to_le_bytes());
    file.extend_from_slice(&SOURCE_SCHEMA.to_le_bytes());
    file.push(u8::from(!prepared.is_empty()));
    file.extend_from_slice(&[0; 3]);
    file.extend_from_slice(&revision.to_le_bytes());
    file.extend_from_slice(&(source.len() as u64).to_le_bytes());
    file.extend_from_slice(&(prepared.len() as u64).to_le_bytes());
    file.extend_from_slice(&source);
    file.extend_from_slice(prepared);
    Ok(file)
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
    let (source, prepared, revision) = sections(bytes, limits.max_file_bytes)?;
    let schema = bytes
        .get(10..12)
        .and_then(|b| <[u8; 2]>::try_from(b).ok())
        .map(u16::from_le_bytes)
        .ok_or(SnapshotError::Envelope("source schema missing"))?;
    let compatible =
        schema == SOURCE_SCHEMA && revision == crate::overlay::snapshot::COMPATIBILITY_REVISION;
    if !prepared.is_empty() && policy == PreparedPolicy::RequireCompatible && !compatible {
        return Err(SnapshotError::Envelope(
            "incompatible prepared engine revision",
        ));
    }
    let budget = bounded::Budget::new(
        limits.max_decoded_values,
        limits.max_decoded_bytes,
        limits.max_depth,
    );
    let source: SourcePayload<'static> = deserialize(source, &budget)?;
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
    let snapshot: OverlaySnapshot<'static> = deserialize(prepared, &budget)?;
    let (baseline, state) = Overlay::from_snapshot(snapshot).map_err(SnapshotError::Workspace)?;
    let effective = state
        .source_workbook(&baseline)
        .map_err(SnapshotError::Workspace)?;
    if !workbook.snapshot().matches(&effective) || workbook.file_metadata != effective.file_metadata
    {
        return Err(SnapshotError::Envelope(
            "source and prepared state disagree",
        ));
    }
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

fn sections(bytes: &[u8], max: usize) -> Result<(&[u8], &[u8], u64), SnapshotError> {
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

fn deserialize<T: serde::de::DeserializeOwned>(
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

/// Capped serializer output that rejects oversized snapshots during encoding.
struct Output {
    bytes: Vec<u8>,
    limit: usize,
}
impl postcard::ser_flavors::Flavor for Output {
    /// Encoded native snapshot bytes.
    type Output = Vec<u8>;
    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.try_extend(&[byte])
    }
    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn finalize(self) -> postcard::Result<Vec<u8>> {
        Ok(self.bytes)
    }
}
fn serialize(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, SnapshotError> {
    postcard::serialize_with_flavor(
        value,
        Output {
            bytes: Vec::new(),
            limit,
        },
    )
    .map_err(SnapshotError::Codec)
}

/// Borrowed source or prepared checkpoint selected for native persistence.
enum SnapshotSource<'a> {
    Workbook(&'a Workbook),
    Checkpoint(OverlaySnapshot<'a>),
}
/// Configure bounded native snapshot output without recalculating source.
#[must_use]
pub struct SnapshotSaveBuilder<'a> {
    source: SnapshotSource<'a>,
    limits: SnapshotLimits,
}
impl<'a> SnapshotSaveBuilder<'a> {
    /// Persist canonical source and retained file metadata.
    pub fn workbook(workbook: &'a Workbook) -> Self {
        Self {
            source: SnapshotSource::Workbook(workbook),
            limits: SnapshotLimits::for_writing(),
        }
    }
    /// Persist authored edits, preparation, and any retained incremental results.
    pub fn checkpoint(snapshot: OverlaySnapshot<'a>) -> Self {
        Self {
            source: SnapshotSource::Checkpoint(snapshot),
            limits: SnapshotLimits::for_writing(),
        }
    }
    /// Set encoding and file resource bounds.
    pub fn limits(mut self, limits: SnapshotLimits) -> Self {
        self.limits = limits;
        self
    }
    /// Serialize the complete snapshot into an owned buffer.
    pub fn to_buffer(self) -> Result<Vec<u8>, SnapshotError> {
        match self.source {
            SnapshotSource::Workbook(w) => encode_workbook(w, self.limits),
            SnapshotSource::Checkpoint(s) => encode_workspace(s, self.limits),
        }
    }
    /// Finish serialization before opening the output file.
    pub fn write_file(self, path: impl AsRef<Path>) -> Result<(), SnapshotError> {
        std::fs::write(path, self.to_buffer()?).map_err(SnapshotError::from_io)
    }
}
/// Configure native snapshot decoding and prepared-state recovery.
#[derive(Debug, Default)]
#[must_use]
pub struct SnapshotLoadBuilder {
    limits: SnapshotLimits,
    policy: PreparedPolicy,
}
impl SnapshotLoadBuilder {
    /// Set resource limits before reading or decoding source.
    pub fn limits(mut self, limits: SnapshotLimits) -> Self {
        self.limits = limits;
        self
    }
    /// Choose whether compatible prepared state may be reused.
    pub fn prepared_policy(mut self, policy: PreparedPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Decode a bounded native snapshot buffer.
    pub fn from_bytes(self, bytes: &[u8]) -> Result<RestoredArtifact, SnapshotError> {
        decode(bytes, self.limits, self.policy)
    }
    /// Read and decode a bounded native snapshot file.
    pub fn open(self, path: impl AsRef<Path>) -> Result<RestoredArtifact, SnapshotError> {
        read_file(path, self.limits, self.policy)
    }
}
/// Direct calculation restored with an explicit disposition of persisted preparation.
pub struct RestoredEvaluation {
    /// Direct execution with source and compilation ready to use.
    pub evaluation: Evaluation<'static, Workbook>,
    /// Whether preparation was reused, absent, or discarded.
    pub prepared_status: PreparedStatus,
}
/// Restored preparation or independently recovered source awaiting an explicit policy.
pub enum RestoredCheckpoint {
    /// Validated preparation attached to its owned original baseline.
    Prepared(Box<Overlay<'static, Workbook>>),
    /// Durable source recovered after preparation was absent or discarded.
    Source {
        /// Independently recovered authored source and file metadata.
        workbook: Workbook,
        /// Why preparation was absent or discarded.
        status: PreparedStatus,
    },
}
impl RestoredArtifact {
    /// Attach reusable preparation, or expose recovered source without choosing new policies.
    pub fn into_checkpoint(self) -> Result<RestoredCheckpoint, SnapshotError> {
        match self.workspace {
            Some(state) => Ok(RestoredCheckpoint::Prepared(Box::new(
                state
                    .attach(self.workbook)
                    .map_err(SnapshotError::Workspace)?,
            ))),
            None => Ok(RestoredCheckpoint::Source {
                workbook: self.workbook,
                status: self.prepared_status,
            }),
        }
    }
    /// Restore direct evaluation and report whether preparation survived decoding.
    pub fn into_evaluation(
        self,
        context: EvaluationContext,
        recovery_policy: OverlayPolicy,
    ) -> Result<RestoredEvaluation, SnapshotError> {
        let prepared_status = self.prepared_status;
        let evaluation = match self.workspace {
            Some(state) => state
                .into_evaluation(self.workbook, context)
                .map_err(SnapshotError::Workspace)?,
            None => Evaluation::with_workbook(self.workbook)
                .with_policy(recovery_policy)
                .build(context)
                .map_err(SnapshotError::Workspace)?,
        };
        Ok(RestoredEvaluation {
            evaluation,
            prepared_status,
        })
    }
}

impl SnapshotLimits {
    /// Set the maximum file bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_file_bytes(mut self, maximum: usize) -> Self {
        self.max_file_bytes = maximum;
        self
    }
    /// Remove the file bytes quota. Structural and representation checks remain.
    pub fn without_file_bytes_limit(mut self) -> Self {
        self.max_file_bytes = usize::MAX;
        self
    }
    /// Inspect the file bytes quota; `None` means no configured maximum.
    pub fn file_bytes_limit(&self) -> Option<usize> {
        (self.max_file_bytes != usize::MAX).then_some(self.max_file_bytes)
    }
    /// Set the maximum decoded values before decoding; zero admits none. Replaces only this setting.
    pub fn max_decoded_values(mut self, maximum: usize) -> Self {
        self.max_decoded_values = maximum;
        self
    }
    /// Remove the decoded values quota. Structural and representation checks remain.
    pub fn without_decoded_values_limit(mut self) -> Self {
        self.max_decoded_values = usize::MAX;
        self
    }
    /// Inspect the decoded values quota; `None` means no configured maximum.
    pub fn decoded_values_limit(&self) -> Option<usize> {
        (self.max_decoded_values != usize::MAX).then_some(self.max_decoded_values)
    }
    /// Set the maximum decoded bytes before decoding; zero admits none. Replaces only this setting.
    pub fn max_decoded_bytes(mut self, maximum: usize) -> Self {
        self.max_decoded_bytes = maximum;
        self
    }
    /// Remove the decoded bytes quota. Structural and representation checks remain.
    pub fn without_decoded_bytes_limit(mut self) -> Self {
        self.max_decoded_bytes = usize::MAX;
        self
    }
    /// Inspect the decoded bytes quota; `None` means no configured maximum.
    pub fn decoded_bytes_limit(&self) -> Option<usize> {
        (self.max_decoded_bytes != usize::MAX).then_some(self.max_decoded_bytes)
    }
}

#[cfg(test)]
mod tests;
