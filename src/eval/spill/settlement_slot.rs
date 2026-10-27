use super::settlement_attempt::SettlementAttempt;
use crate::eval::limits::MemoRetentionUsage;

/// One lazily allocated component workspace, active only during private settlement.
#[derive(Default)]
pub(super) struct SettlementSlot {
    pub(super) workspace: Option<Box<SettlementAttempt>>,
    pub(super) running: bool,
}

impl SettlementSlot {
    /// Read semantic attempt state only while its component is running.
    pub(super) fn active(&self) -> Option<&SettlementAttempt> {
        self.workspace.as_deref().filter(|_| self.running)
    }

    /// Mutate semantic attempt state without exposing retained inactive buffers.
    pub(super) fn active_mut(&mut self) -> Option<&mut SettlementAttempt> {
        self.workspace.as_deref_mut().filter(|_| self.running)
    }

    /// Begin a new component using empty collections left by its predecessor.
    pub(super) fn begin(
        &mut self,
        memo_usage: MemoRetentionUsage,
        journal_len: usize,
        admitted_len: usize,
    ) {
        let state = self.workspace.get_or_insert_with(|| {
            // SAFETY: One fixed-size SettlementAttempt cannot overflow an allocation layout.
            Box::new(SettlementAttempt {
                selected: Default::default(),
                blocked: Default::default(),
                footprints: Default::default(),
                admitted: Default::default(),
                consumers: Default::default(),
                random_touched: Default::default(),
                memo_usage: Default::default(),
                journal_len: 0,
                admitted_len: 0,
                notifications: Vec::new(),
                non_repeatable: false,
            })
        });
        state.memo_usage = memo_usage;
        state.journal_len = journal_len;
        state.admitted_len = admitted_len;
        self.running = true;
    }

    /// Discard attempt contents on success or failure while retaining reusable storage.
    pub(super) fn finish(&mut self) {
        self.running = false;
        if let Some(state) = self.workspace.as_deref_mut() {
            // Hash tables and the notification vector retain capacities; BTreeSet
            // clears its nodes. No selected owner or old replay effect survives.
            state.selected.clear();
            state.blocked.clear();
            state.footprints.clear();
            state.admitted.clear();
            state.consumers.clear();
            state.random_touched.clear();
            state.notifications.clear();
            state.memo_usage = Default::default();
            state.journal_len = 0;
            state.admitted_len = 0;
            state.non_repeatable = false;
        }
    }
}
