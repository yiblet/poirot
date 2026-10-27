use super::compile_error::CompileError;
use super::high::High;
use super::sink::Sink;
use super::writer::Writer;

/// Coordinates independent inference and bytecode machines over the same expression events.
pub(super) struct Preparation<'registry, 'ast> {
    pub(super) writer: Writer<'registry>,
    pub(super) inference: crate::eval::compile::typing::InferenceMachine<'ast>,
    pub(super) error: Option<CompileError>,
}

impl<'ast> Sink<'ast> for Preparation<'_, 'ast> {
    fn emit(&mut self, op: High<'ast>) {
        if self.error.is_some() || self.writer.error.is_some() {
            return;
        }
        let invocation = matches!(&op, High::Invoke(..));
        let inferred = match &op {
            High::Enter(depth, expression) => u32::try_from(self.writer.output.template.code.len())
                .map_err(|_| CompileError::ResourceLimit)
                .and_then(|instruction| self.inference.enter(expression, *depth, instruction)),
            High::Leave => self.inference.leave(&self.writer.output.calls),
            _ => Ok(()),
        };
        if let Err(error) = inferred {
            self.error = Some(error);
            return;
        }
        self.writer.emit(op);
        if self.writer.error.is_some() {
            return;
        }
        if invocation {
            let Some((index, binding)) = self.writer.output.calls.iter().enumerate().next_back()
            else {
                self.error = Some(CompileError::InvalidReference);
                return;
            };
            if let Err(error) = self.inference.bind(&binding.target, index) {
                self.error = Some(error);
            }
        }
    }
}
