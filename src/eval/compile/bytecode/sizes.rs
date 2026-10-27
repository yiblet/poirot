use super::high::High;
use super::lowering::reference;
use super::sink::Sink;
use poirot_parsing::formula::Expr;

#[derive(Default)]
/// Output buffer capacities counted before compilation allocates storage.
pub(super) struct Sizes {
    pub(super) code: usize,
    pub(super) text: usize,
    pub(super) refs: usize,
    pub(super) reference_bytes: usize,
    pub(super) calls: usize,
    pub(super) expressions: usize,
    pub(super) labels: usize,
}

impl<'ast> Sink<'ast> for Sizes {
    fn emit(&mut self, op: High<'ast>) {
        if matches!(op, High::Label(_)) {
            self.labels += 1;
            return;
        }
        self.code += 1;
        match op {
            High::Enter(_, _) => self.expressions += 1,
            High::Truncated => self.text += 5,
            High::Invoke(s, _, _) => {
                self.text += 4 + s.len();
                self.calls += 1;
            }
            High::Atom(e) => {
                if reference(e, &mut self.reference_bytes).is_some() {
                    self.refs += 1;
                }
                match e {
                    Expr::String(s) | Expr::Error(s) | Expr::Name(s) => self.text += s.len(),
                    Expr::QualifiedName { sheet, name } => {
                        self.text += 4 + sheet.len() + name.len()
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
