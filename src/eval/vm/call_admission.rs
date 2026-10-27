//! Resolved builtin call admission shared by expression, VM, and direct adapter entry.
//! The two entry orders are explicit because observer refusal changes which reads occur.

use super::admission_order::AdmissionOrder;
use super::resolved_call::ResolvedCall;
use crate::eval::limits::EvalResult;
use crate::eval::ports::DependencyRead;
use crate::eval::session::{ReadRecorder, Session};

/// Observes and admits resolved builtin calls before their handlers run.
pub(in crate::eval) struct CallAdmission<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> CallAdmission<'s, 'a> {
    /// Borrow the session whose observers and dependency reads admission uses.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Observe and admit a call; operand and handler execution record actual context reads.
    pub(in crate::eval) fn admit(
        &mut self,
        call: ResolvedCall<'_>,
        order: AdmissionOrder,
    ) -> EvalResult<()> {
        match order {
            AdmissionOrder::ObserverBeforePolicy => {
                self.session.ports.observe_resolved_call(call)?;
                if call.read_builtin_policy {
                    ReadRecorder::new(self.session).read(DependencyRead::BuiltinPolicy);
                }
            }
            AdmissionOrder::PolicyBeforeObserver => {
                if call.read_builtin_policy {
                    ReadRecorder::new(self.session).read(DependencyRead::BuiltinPolicy);
                }
                self.session.ports.observe_resolved_call(call)?;
            }
        }
        Ok(())
    }
}
