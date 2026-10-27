//! Builtin, text and regex policy reads with their recorded policy dependencies.
use super::call_host::CallHost;
use crate::eval::config::BuiltinSemanticsPolicy;
use crate::eval::ports::DependencyRead;
use crate::eval::session::ReadRecorder;

/// Reads the run's compatibility, text and regex policies for one call.
/// Recorded reads add the policy dependency; snapshots inspect a policy whose dependency
/// the caller records separately or that has no dependency.
pub(in crate::eval) struct PolicyReads<'h, 'call, 'workbook> {
    /// The call whose session records the policy reads.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> PolicyReads<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's reads.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Read the compatibility policy and record the invocation's dependency on it.
    pub(in crate::eval) fn builtin_policy(&mut self) -> BuiltinSemanticsPolicy {
        ReadRecorder::new(self.host.session).read(DependencyRead::BuiltinPolicy);
        self.host.session.policies.builtin_policy
    }

    /// Return the run's builtin compatibility contract without recording a read.
    pub(in crate::eval) fn builtin_policy_snapshot(&self) -> BuiltinSemanticsPolicy {
        self.host.session.policies.builtin_policy
    }

    /// Read the selected text policy and record its dependency.
    pub(in crate::eval) fn text_policy(
        &mut self,
    ) -> poirot_functions::text_units::TextSemanticsPolicy {
        ReadRecorder::new(self.host.session).read(DependencyRead::TextPolicy);
        self.host.session.policies.text_policy
    }

    /// Inspect text configuration after the caller has recorded its policy dependency.
    pub(in crate::eval) fn text_policy_snapshot(
        &self,
    ) -> poirot_functions::text_units::TextSemanticsPolicy {
        self.host.session.policies.text_policy
    }

    /// Native regex execution guards selected independently of calculation work.
    pub(in crate::eval) fn regex_settings(&self) -> poirot_functions::text::regex::ExecutionLimits {
        self.host.session.policies.regex
    }
}
