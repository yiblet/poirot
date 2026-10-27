//! Cumulative decode budget for values, payload bytes and nesting.
use serde::de::{self};
use std::cell::{Cell, RefCell};

/// Shared budget for decoded values, payload bytes, and nesting.
pub(in crate::eval::artifact) struct Budget {
    values: Cell<usize>,
    bytes: Cell<usize>,
    depth: usize,
    limits: (usize, usize),
    failure: RefCell<Option<super::super::snapshot_error::SnapshotError>>,
}

impl Budget {
    /// Create one cumulative decoding budget.
    pub fn new(values: usize, bytes: usize, depth: usize) -> Self {
        Self {
            values: Cell::new(values),
            bytes: Cell::new(bytes),
            depth,
            limits: (values, bytes),
            failure: RefCell::new(None),
        }
    }

    /// Recover structured refusal details instead of presenting a codec's custom-error sentinel.
    pub fn failure(&self) -> Option<super::super::snapshot_error::SnapshotError> {
        self.failure
            .try_borrow_mut()
            .ok()
            .and_then(|mut failure| failure.take())
    }

    #[cold]
    #[inline(never)]
    fn refuse<E: de::Error>(
        &self,
        resource: super::super::snapshot_resource::SnapshotResource,
        limit: usize,
        consumed: usize,
        requested: usize,
    ) -> E {
        if let Ok(mut failure) = self.failure.try_borrow_mut() {
            *failure = Some(super::super::snapshot_error::SnapshotError::Quota(
                super::super::snapshot_quota_error::SnapshotQuotaError {
                    resource,
                    limit,
                    consumed,
                    requested,
                },
            ));
        }
        E::custom("snapshot quota refused")
    }

    #[inline]
    pub(super) fn charge<E: de::Error>(&self, bytes: usize) -> Result<(), E> {
        let remaining = self.bytes.get().checked_sub(bytes).ok_or_else(|| {
            self.refuse(
                super::super::snapshot_resource::SnapshotResource::DecodedBytes,
                self.limits.1,
                self.limits.1.saturating_sub(self.bytes.get()),
                bytes,
            )
        })?;
        self.bytes.set(remaining);
        Ok(())
    }

    #[inline]
    pub(super) fn enter<E: de::Error>(&self, depth: usize) -> Result<(), E> {
        if depth > self.depth {
            if self.depth == 256 {
                if let Ok(mut failure) = self.failure.try_borrow_mut() {
                    *failure = Some(
                        super::super::snapshot_error::SnapshotError::EngineConstraint(
                            crate::eval::artifact::snapshot_constraint_error::SnapshotConstraintError::RecursiveDecoderNestingCeilingIs256,
                        ),
                    );
                }
                return Err(E::custom("snapshot decoder nesting ceiling"));
            }
            return Err(self.refuse(
                super::super::snapshot_resource::SnapshotResource::Depth,
                self.depth,
                depth.saturating_sub(1),
                1,
            ));
        }
        let remaining = self.values.get().checked_sub(1).ok_or_else(|| {
            self.refuse(
                super::super::snapshot_resource::SnapshotResource::DecodedValues,
                self.limits.0,
                self.limits.0.saturating_sub(self.values.get()),
                1,
            )
        })?;
        self.values.set(remaining);
        Ok(())
    }

    #[inline]
    pub(super) fn sequence<E: de::Error>(&self, len: Option<usize>) -> Result<(), E> {
        if let Some(len) = len
            && len > self.values.get()
        {
            return Err(self.refuse(
                super::super::snapshot_resource::SnapshotResource::DecodedValues,
                self.limits.0,
                self.limits.0.saturating_sub(self.values.get()),
                len,
            ));
        }
        Ok(())
    }
}
