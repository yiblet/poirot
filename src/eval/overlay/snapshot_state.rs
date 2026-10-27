use super::borrowed_source::BorrowedSource;
use super::snapshot_payload::SnapshotPayload;

/// Borrowed runtime view or independently owned wire payload.
pub(super) enum SnapshotState<'a> {
    Borrowed(BorrowedSource<'a>),
    Owned(Box<SnapshotPayload<'static>>),
}
