//! Owned stack compiler and runtime metadata for lazy bytecode execution.
//!
//! Compilation has three stages: validate/count, stream borrowed high-level operations
//! (including N-ary arrays and lazy calls with labels), then lower to stack instructions.
//! No AST is retained. The intermediate stream and label fixups allocate nothing;
//! only final buffers and the shared executable owner allocate; call bindings may own registry
//! metadata and resolved spellings. Source retention and
//! template interning belong to the caller. The VM executes the final instructions through the shared family adapters.
//!
//! Every function, including unknown/extension functions, owns lazy argument blocks.
//! Family adapters select blocks and preserve their own arity/error order.
//! `Invoke` must never eagerly execute the blocks following it. References stay values
//! on the stack until an operator or family adapter requests dereferencing/coercion.

use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};
use std::sync::Arc;

const LIMIT: usize = 100_000;
pub(crate) const DEPTH: usize = 128;
const NONE: u32 = u32::MAX;

/// Structural or resource failure while producing an executable program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompileError {
    /// Compilation exceeded a structural count or depth limit.
    ResourceLimit,
    /// An array was empty or had inconsistent row widths.
    InvalidArray,
    /// A reference exceeded workbook coordinate limits.
    InvalidReference,
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bytecode compilation failed: {self:?}")
    }
}
impl std::error::Error for CompileError {}

/// Stack operations and lazy argument descriptors understood by the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Opcode {
    /// Admission in AST preorder. `a` is the original expression depth.
    Enter,
    /// Leave the current expression admission frame.
    Leave,
    /// Push f64 bits from low `a` and high `b` words.
    Number,
    /// Push UTF-8 text at operand byte offset `a`, length `b`.
    Text,
    /// Push boolean `a != 0`.
    Boolean,
    /// Push error spelling at operand byte offset `a`, length `b`.
    Error,
    /// Push an omitted-argument marker.
    Missing,
    /// Push a reference value from per-instance slot `a`, without dereferencing.
    Reference,
    /// Resolve name spelling at operand byte offset `a`, length `b`.
    Name,
    /// `a,b` identify operand bytes: length-prefixed sheet, then remaining name.
    QualifiedName,
    /// Consume one value. `a`: plus=0, minus=1, percent=2, implicit intersection=3.
    Unary,
    /// Consume left/right values, preserving reference operators. `a` follows
    /// Add,Subtract,Multiply,Divide,Power,Concat,Equal,NotEqual,Less,LessEqual,
    /// Greater,GreaterEqual,Range,Union,Intersection in order from zero.
    Binary,
    /// Consume `a` rows * `b` columns of values in row-major order.
    Array,
    /// Admit `a` array elements before evaluation; `b,c` are rows and columns.
    ArrayAllocate,
    /// Scalarize the preceding array element before evaluating its successor.
    Scalarize,
    /// `a`: call binding index; `b`: arity; `c`: continuation.
    /// Arguments start at ip+2; following `Argument.c` walks the descriptors.
    Invoke,
    /// Inline argument descriptor: `a`: argument ordinal; `c`: next descriptor/end.
    Argument,
    /// Return the argument's reference/value to the requesting family adapter.
    YieldArgument,
    /// Continue at absolute instruction index `c`.
    Jump,
}

/// Fixed-width stack instruction. Branch targets are absolute instruction indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Instruction {
    /// Operation selecting the interpretation of operand words.
    pub opcode: Opcode,
    /// Operation flags, currently emitted as zero.
    pub flags: u8,
    /// Reserved storage, currently emitted as zero.
    pub reserved: u16,
    /// First operand word; interpretation depends on the opcode.
    pub a: u32,
    /// Second operand word; interpretation depends on the opcode.
    pub b: u32,
    /// Third operand word, including absolute continuation targets.
    pub c: u32,
}
impl Instruction {
    fn new(opcode: Opcode, a: u32, b: u32) -> Self {
        Self {
            opcode,
            flags: 0,
            reserved: 0,
            a,
            b,
            c: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// Shared instructions and literal operand bytes. Reference instructions name
/// slots; their concrete coordinates live in each formula's [`ReferenceBindings`].
pub struct Template {
    /// Executable instructions in instruction-pointer order.
    pub code: Vec<Instruction>,
    /// Packed literal and name bytes addressed by instructions.
    #[serde(with = "serde_bytes")]
    pub operands: Vec<u8>,
    /// Number of per-formula reference slots required by this template.
    pub reference_slots: u32,
}
/// Encoding discriminator for a formula's concrete reference binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceKind {
    /// Single-cell reference.
    Cell,
    /// Reference spanning ordered sheet endpoints.
    SheetSpan,
    /// Whole-column reference.
    Column,
    /// Whole-row reference.
    Row,
}
/// Data uses little-endian u32 fields and strings encoded as u32 byte length + UTF-8.
/// Cell: row,column,row_absolute,column_absolute,optional sheet. Column/row: index,
/// absolute,optional sheet. Span: first/last sheet,cell start,end-present,optional end.
/// Optional strings have a presence u32 before their length/string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceBinding {
    /// Reference encoding used by this slot.
    pub kind: ReferenceKind,
    /// Byte offset into the reference data buffer.
    pub offset: u32,
    /// Encoded reference length in bytes.
    pub length: u32,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// Occurrence-ordered reference slots and their encoded per-formula coordinates
/// and qualifiers. They can differ while the executable template is shared.
pub struct ReferenceBindings {
    /// Reference descriptors in source occurrence order.
    pub slots: Vec<ReferenceBinding>,
    /// Packed reference coordinates and sheet qualifiers.
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
/// A template with resolved builtin/extension call identities. Programs can be
/// shared only when this complete binding identity agrees, not just their opcodes.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Executable {
    /// Possible root and intermediate properties inferred during preparation.
    pub typing: crate::typing::FormulaTyping,
    /// Shared instruction and literal layout.
    pub template: Template,
    /// Bound call identities indexed by invocation instructions.
    pub calls: Vec<Call>,
    /// Exact registry identity required by extension bindings.
    pub extension_identity: Option<crate::extensions::ExtensionIdentity>,
}
/// An executable shared by `Arc` plus reference coordinates for this formula.
/// Cloning shares the executable and copies instance bindings; mutable access
/// through `DerefMut` uses copy-on-write so other owners keep their program.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    /// Shared immutable instructions and call bindings.
    pub executable: Arc<Executable>,
    /// Concrete reference coordinates for this formula instance.
    pub references: ReferenceBindings,
}
impl std::ops::Deref for Compiled {
    /// Shared executable instructions and call bindings, excluding concrete references.
    type Target = Executable;
    fn deref(&self) -> &Executable {
        &self.executable
    }
}
impl std::ops::DerefMut for Compiled {
    fn deref_mut(&mut self) -> &mut Executable {
        Arc::make_mut(&mut self.executable)
    }
}
/// Final call binding; argument expressions live exclusively in instruction blocks.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Call {
    /// Operand-buffer byte offset of the source function spelling.
    pub source_offset: u32,
    /// Exact lookup spelling when it differs from the source spelling.
    pub resolved_name: Option<String>,
    /// Builtin, extension, or deferred unresolved call target.
    pub target: CallTarget,
}
/// Handler identity selected during compilation, without retaining a provider object.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CallTarget {
    /// Registered builtin handler.
    Builtin(crate::registry::FunctionId),
    /// Explicit extension registry binding.
    Extension(crate::extensions::ExtensionBinding),
    /// Unknown function, rejected only when execution reaches the call.
    Unresolved,
}

/// Compile caller-owned syntax. The result is fully independent of the syntax lifetime.
/// Structural errors are reported before allocating any output buffers.
pub fn compile(expression: &Expr) -> Result<Compiled, CompileError> {
    compile_inner(expression, None, true)
}
/// Bind calls once against exactly the same namespace as the tree binder.
/// Production binding truncates excessive expression depth to a deferred #NUM! atom.
pub fn compile_with_registry(
    expression: &Expr,
    registry: Option<&crate::extensions::ExtensionRegistry>,
) -> Result<Compiled, CompileError> {
    compile_inner(expression, registry, false)
}
fn compile_inner(
    expression: &Expr,
    registry: Option<&crate::extensions::ExtensionRegistry>,
    strict: bool,
) -> Result<Compiled, CompileError> {
    let mut sizes = Sizes::default();
    let mut nodes = 0;
    validate(expression, 0, &mut nodes, strict)?;
    stream(expression, 0, &mut sizes);
    if [sizes.code, sizes.text, sizes.refs, sizes.reference_bytes]
        .iter()
        .any(|n| *n > u32::MAX as usize)
    {
        return Err(CompileError::ResourceLimit);
    }
    let mut writer = Writer {
        output: Compiled {
            executable: Arc::new(Executable {
                typing: crate::typing::FormulaTyping::default(),
                template: Template {
                    code: Vec::with_capacity(sizes.code),
                    operands: Vec::with_capacity(sizes.text),
                    reference_slots: 0,
                },
                calls: Vec::with_capacity(sizes.calls),
                extension_identity: registry.map(|r| r.identity()),
            }),
            references: ReferenceBindings {
                slots: Vec::with_capacity(sizes.refs),
                data: Vec::with_capacity(sizes.reference_bytes),
            },
        },
        error: None,
        heads: [NONE; DEPTH * 2],
        registry,
    };
    stream(expression, 0, &mut writer);
    if let Some(error) = writer.error {
        return Err(error);
    }
    if writer.heads.iter().any(|h| *h != NONE)
        || writer.output.template.code.len() != sizes.code
        || writer.output.template.operands.len() != sizes.text
        || writer.output.references.data.len() != sizes.reference_bytes
    {
        return Err(CompileError::InvalidReference);
    }
    writer.output.typing = crate::typing::infer(expression, &writer.output)?;
    Ok(writer.output)
}

fn validate(e: &Expr, depth: usize, nodes: &mut usize, strict: bool) -> Result<(), CompileError> {
    if depth >= DEPTH && !strict {
        return Ok(());
    }
    *nodes += 1;
    if *nodes > LIMIT || depth >= DEPTH {
        return Err(CompileError::ResourceLimit);
    }
    let invalid_cell = |c: &poirot_parsing::formula::CellReference| {
        c.row >= poirot_model::MAX_ROWS || c.column >= poirot_model::MAX_COLUMNS
    };
    match e {
        Expr::Unary { operand, .. } => validate(operand, depth + 1, nodes, strict)?,
        Expr::Binary { left, right, .. } => {
            validate(left, depth + 1, nodes, strict)?;
            validate(right, depth + 1, nodes, strict)?;
        }
        Expr::Function { args, .. } => {
            for a in args {
                validate(a, depth + 1, nodes, strict)?;
            }
        }
        Expr::Array(rows) => {
            let width = rows.first().map_or(0, Vec::len);
            if width == 0 || rows.iter().any(|r| r.len() != width) {
                return Err(CompileError::InvalidArray);
            }
            for a in rows.iter().flatten() {
                validate(a, depth + 1, nodes, strict)?;
            }
        }
        Expr::Cell(c) if invalid_cell(c) => return Err(CompileError::InvalidReference),
        Expr::Column { column, .. } if *column >= poirot_model::MAX_COLUMNS => {
            return Err(CompileError::InvalidReference);
        }
        Expr::Row { row, .. } if *row >= poirot_model::MAX_ROWS => {
            return Err(CompileError::InvalidReference);
        }
        Expr::SheetSpan(s)
            if s.first_sheet.is_empty()
                || s.last_sheet.is_empty()
                || std::iter::once(&s.start)
                    .chain(s.end.iter())
                    .any(|c| invalid_cell(c) || c.sheet.is_some()) =>
        {
            return Err(CompileError::InvalidReference);
        }
        _ => {}
    }
    Ok(())
}

// Borrowed high-level IR. N-ary operations remain intact until the stack sink.
// Labels are scoped by syntax depth, so an on-stack fixed table is sufficient.
/// Temporary high-level instruction stream lowered into fixed-width executable bytecode.
enum High<'a> {
    Enter(usize),
    Leave,
    Atom(&'a Expr),
    Truncated,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Array(usize, usize),
    ArrayAllocate(usize, usize),
    Scalarize,
    Invoke(&'a str, usize, usize),
    Argument(usize, usize),
    YieldArgument,
    Goto(usize),
    Label(usize),
}
/// Bytecode lowering destination shared by sizing and emission passes.
trait Sink {
    fn emit(&mut self, op: High<'_>);
}
fn stream(e: &Expr, depth: usize, sink: &mut impl Sink) {
    sink.emit(High::Enter(depth));
    if depth >= DEPTH {
        sink.emit(High::Truncated);
        sink.emit(High::Leave);
        return;
    }
    match e {
        Expr::Unary { op, operand } => {
            stream(operand, depth + 1, sink);
            sink.emit(High::Unary(*op));
        }
        Expr::Binary { op, left, right } => {
            stream(left, depth + 1, sink);
            stream(right, depth + 1, sink);
            sink.emit(High::Binary(*op));
        }
        Expr::Array(rows) => {
            sink.emit(High::ArrayAllocate(
                rows.len(),
                rows.first().map_or(0, Vec::len),
            ));
            for a in rows.iter().flatten() {
                stream(a, depth + 1, sink);
                sink.emit(High::Scalarize);
            }
            sink.emit(High::Array(rows.len(), rows.first().map_or(0, Vec::len)));
        }
        Expr::Function { name, args } => {
            let done = depth * 2;
            let next = done + 1;
            sink.emit(High::Invoke(name, args.len(), done));
            sink.emit(High::Goto(done));
            for (i, a) in args.iter().enumerate() {
                sink.emit(High::Argument(i, next));
                stream(a, depth + 1, sink);
                sink.emit(High::YieldArgument);
                sink.emit(High::Label(next));
            }
            sink.emit(High::Label(done));
        }
        _ => sink.emit(High::Atom(e)),
    }
    sink.emit(High::Leave);
}

// Byte sinks are shared by counting and final emission, so exact capacities cannot drift.
/// Binary operand writer used to encode the executable's constant/reference storage.
trait Bytes {
    fn bytes(&mut self, data: &[u8]);
    fn word(&mut self, n: u32) {
        self.bytes(&n.to_le_bytes());
    }
    fn string(&mut self, s: &str) {
        self.word(s.len() as u32);
        self.bytes(s.as_bytes());
    }
    fn optional(&mut self, s: Option<&str>) {
        self.word(u32::from(s.is_some()));
        if let Some(s) = s {
            self.string(s);
        }
    }
    fn cell(&mut self, c: &poirot_parsing::formula::CellReference) {
        self.word(c.row);
        self.word(c.column);
        self.word(c.row_absolute.into());
        self.word(c.column_absolute.into());
        self.optional(c.sheet.as_deref());
    }
}
impl Bytes for Vec<u8> {
    fn bytes(&mut self, data: &[u8]) {
        self.extend_from_slice(data);
    }
}
impl Bytes for usize {
    fn bytes(&mut self, data: &[u8]) {
        *self += data.len();
    }
}
fn reference(e: &Expr, out: &mut impl Bytes) -> Option<ReferenceKind> {
    Some(match e {
        Expr::Cell(c) => {
            out.cell(c);
            ReferenceKind::Cell
        }
        Expr::Column {
            column,
            absolute,
            sheet,
        } => {
            out.word(*column);
            out.word((*absolute).into());
            out.optional(sheet.as_deref());
            ReferenceKind::Column
        }
        Expr::Row {
            row,
            absolute,
            sheet,
        } => {
            out.word(*row);
            out.word((*absolute).into());
            out.optional(sheet.as_deref());
            ReferenceKind::Row
        }
        Expr::SheetSpan(s) => {
            out.string(&s.first_sheet);
            out.string(&s.last_sheet);
            out.cell(&s.start);
            out.word(s.end.is_some().into());
            if let Some(c) = &s.end {
                out.cell(c);
            }
            ReferenceKind::SheetSpan
        }
        _ => return None,
    })
}
#[derive(Default)]
/// Output buffer capacities counted before compilation allocates storage.
struct Sizes {
    code: usize,
    text: usize,
    refs: usize,
    reference_bytes: usize,
    calls: usize,
}
impl Sink for Sizes {
    fn emit(&mut self, op: High<'_>) {
        if matches!(op, High::Label(_)) {
            return;
        }
        self.code += 1;
        match op {
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
/// Lowers streamed operations into final buffers and resolves label fixups.
struct Writer<'a> {
    error: Option<CompileError>,
    registry: Option<&'a crate::extensions::ExtensionRegistry>,
    output: Compiled,
    heads: [u32; DEPTH * 2],
}
impl Writer<'_> {
    fn text(&mut self, s: &str) -> (u32, u32) {
        let start = self.output.template.operands.len() as u32;
        self.output
            .template
            .operands
            .extend_from_slice(s.as_bytes());
        (start, s.len() as u32)
    }
    fn atom(&mut self, e: &Expr) -> Instruction {
        let offset = self.output.references.data.len() as u32;
        if let Some(kind) = reference(e, &mut self.output.references.data) {
            let slot = self.output.template.reference_slots;
            self.output.template.reference_slots += 1;
            self.output.references.slots.push(ReferenceBinding {
                kind,
                offset,
                length: self.output.references.data.len() as u32 - offset,
            });
            return Instruction::new(Opcode::Reference, slot, 0);
        }
        match e {
            Expr::Number(n) => {
                let bits = n.to_bits();
                Instruction::new(Opcode::Number, bits as u32, (bits >> 32) as u32)
            }
            Expr::Bool(b) => Instruction::new(Opcode::Boolean, (*b).into(), 0),
            Expr::Missing => Instruction::new(Opcode::Missing, 0, 0),
            Expr::String(s) | Expr::Error(s) | Expr::Name(s) => {
                let opcode = match e {
                    Expr::String(_) => Opcode::Text,
                    Expr::Error(_) => Opcode::Error,
                    _ => Opcode::Name,
                };
                let (a, b) = self.text(s);
                Instruction::new(opcode, a, b)
            }
            Expr::QualifiedName { sheet, name } => {
                let a = self.output.template.operands.len() as u32;
                self.output.template.operands.string(sheet);
                self.output
                    .template
                    .operands
                    .extend_from_slice(name.as_bytes());
                Instruction::new(
                    Opcode::QualifiedName,
                    a,
                    (4 + sheet.len() + name.len()) as u32,
                )
            }
            _ => {
                self.error = Some(CompileError::InvalidReference);
                Instruction::new(Opcode::Missing, 0, 0)
            }
        }
    }
}
impl Sink for Writer<'_> {
    fn emit(&mut self, op: High<'_>) {
        if self.error.is_some() {
            return;
        }
        if let High::Label(label) = op {
            let destination = self.output.template.code.len() as u32;
            let Some(head) = self.heads.get_mut(label) else {
                self.error = Some(CompileError::ResourceLimit);
                return;
            };
            let mut cursor = std::mem::replace(head, NONE);
            while cursor != NONE {
                let Some(instruction) = self.output.template.code.get_mut(cursor as usize) else {
                    self.error = Some(CompileError::InvalidReference);
                    return;
                };
                cursor = std::mem::replace(&mut instruction.c, destination);
            }
            return;
        }
        let mut target = None;
        let mut instruction = match op {
            High::Enter(d) => Instruction::new(Opcode::Enter, d as u32, 0),
            High::Leave => Instruction::new(Opcode::Leave, 0, 0),
            High::Atom(e) => self.atom(e),
            High::Truncated => {
                let (a, b) = self.text("#NUM!");
                Instruction::new(Opcode::Error, a, b)
            }
            High::Unary(op) => Instruction::new(Opcode::Unary, unary(op), 0),
            High::Binary(op) => Instruction::new(Opcode::Binary, binary(op), 0),
            High::ArrayAllocate(rows, columns) => {
                let mut instruction =
                    Instruction::new(Opcode::ArrayAllocate, (rows * columns) as u32, rows as u32);
                instruction.c = columns as u32;
                instruction
            }
            High::Scalarize => Instruction::new(Opcode::Scalarize, 0, 0),
            High::Array(r, c) => Instruction::new(Opcode::Array, r as u32, c as u32),
            High::Invoke(name, count, label) => {
                let a = self.output.template.operands.len() as u32;
                self.output.template.operands.string(name);
                let index = self.output.calls.len() as u32;
                let lookup = match self.registry.map(|r| r.external_name_lookup(name)) {
                    Some(crate::external_namespace::ExternalNameLookup::Stored(stored)) => {
                        Some(stored)
                    }
                    Some(crate::external_namespace::ExternalNameLookup::UnsupportedName) => None,
                    _ => Some(name),
                };
                let call_target = match lookup {
                    Some(lookup) => match crate::registry::resolve_function(
                        &poirot_parsing::formula::FunctionName::from(lookup),
                    ) {
                        Some(d) => CallTarget::Builtin(d.handler),
                        None => self
                            .registry
                            .and_then(|r| r.resolve(lookup))
                            .map_or(CallTarget::Unresolved, CallTarget::Extension),
                    },
                    None => CallTarget::Unresolved,
                };
                self.output.calls.push(Call {
                    source_offset: a,
                    resolved_name: lookup.filter(|s| *s != name).map(str::to_owned),
                    target: call_target,
                });
                target = Some(label);
                Instruction::new(Opcode::Invoke, index, count as u32)
            }
            High::Argument(index, label) => {
                target = Some(label);
                Instruction::new(Opcode::Argument, index as u32, 0)
            }
            High::YieldArgument => Instruction::new(Opcode::YieldArgument, 0, 0),
            High::Goto(label) => {
                target = Some(label);
                Instruction::new(Opcode::Jump, 0, 0)
            }
            // SAFETY: The High::Label branch above returns before this match.
            High::Label(_) => unreachable!(),
        };
        if let Some(label) = target {
            let Some(head) = self.heads.get_mut(label) else {
                self.error = Some(CompileError::ResourceLimit);
                return;
            };
            instruction.c = *head;
            *head = self.output.template.code.len() as u32;
        }
        self.output.template.code.push(instruction);
    }
}
fn unary(op: UnaryOp) -> u32 {
    match op {
        UnaryOp::Plus => 0,
        UnaryOp::Minus => 1,
        UnaryOp::Percent => 2,
        UnaryOp::ImplicitIntersection => 3,
    }
}
fn binary(op: BinaryOp) -> u32 {
    match op {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Power => 4,
        BinaryOp::Concat => 5,
        BinaryOp::Equal => 6,
        BinaryOp::NotEqual => 7,
        BinaryOp::Less => 8,
        BinaryOp::LessEqual => 9,
        BinaryOp::Greater => 10,
        BinaryOp::GreaterEqual => 11,
        BinaryOp::Range => 12,
        BinaryOp::Union => 13,
        BinaryOp::Intersection => 14,
    }
}

/// Convenience entry point retaining neither the parsed source nor its AST.
pub fn lower_parsed(
    parsed: &poirot_parsing::formula::ParsedFormula,
) -> Result<Compiled, CompileError> {
    compile(parsed.expression())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compiled(source: &str) -> Compiled {
        let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
        compile(&syntax).unwrap()
    }
    #[test]
    fn bytecode_lazy_calls_have_forward_blocks_and_continuations() {
        for name in ["IF", "IFERROR", "SUM", "UNKNOWN"] {
            let result = compiled(&format!("{name}(A1,IFERROR(B1,9),1/0)"));
            let code = &result.template.code;
            let invoke = code
                .iter()
                .position(|i| i.opcode == Opcode::Invoke)
                .unwrap();
            assert_eq!(code[invoke].b, 3);
            assert_eq!(code[invoke + 1].opcode, Opcode::Jump);
            let done = code[invoke].c as usize;
            assert_eq!(code[invoke + 1].c as usize, done);
            assert_eq!(code[done].opcode, Opcode::Leave);
            let mut arg = invoke + 2;
            for index in 0..3 {
                assert_eq!(code[arg].opcode, Opcode::Argument);
                assert_eq!(code[arg].a, index);
                assert_eq!(code[arg + 1].opcode, Opcode::Enter);
                let next = code[arg].c as usize;
                assert!(next > arg);
                assert_eq!(code[next - 1].opcode, Opcode::YieldArgument);
                arg = next;
            }
            assert_eq!(arg, done);
        }
    }
    #[test]
    fn bytecode_templates_ignore_only_concrete_references() {
        let first = compiled("SUM(A1,$B$2)+C3:D4");
        let second = compiled("SUM(Z9,$C$8)+E5:F6");
        assert_eq!(first.template, second.template);
        assert_ne!(first.references, second.references);
        assert_eq!(first.template.reference_slots, 4);
        let slots: Vec<_> = first
            .template
            .code
            .iter()
            .filter(|i| i.opcode == Opcode::Reference)
            .map(|i| i.a)
            .collect();
        assert_eq!(slots, [0, 1, 2, 3]);
        assert_eq!(
            first
                .template
                .code
                .iter()
                .rfind(|i| i.opcode == Opcode::Binary)
                .unwrap()
                .a,
            binary(BinaryOp::Add)
        );
    }
    #[test]
    fn bytecode_owned_result_and_preorder_admission() {
        let result = compiled("-A1+{1,2;3,4}");
        let code = &result.template.code;
        let depths: Vec<_> = code
            .iter()
            .filter(|i| i.opcode == Opcode::Enter)
            .map(|i| i.a)
            .collect();
        assert_eq!(depths, [0, 1, 2, 1, 2, 2, 2, 2]);
        let array = code.iter().find(|i| i.opcode == Opcode::Array).unwrap();
        assert_eq!((array.a, array.b), (2, 2));
        assert_eq!(std::mem::size_of::<Instruction>(), 16);
        assert_eq!(code.len(), result.template.code.capacity());
        assert_eq!(
            result.references.data.len(),
            result.references.data.capacity()
        );
        assert_eq!(
            result.references.slots.len(),
            result.references.slots.capacity()
        );
    }
    #[test]
    fn bytecode_all_syntax_atoms_and_operators() {
        let cell = poirot_parsing::formula::CellReference {
            row: 1,
            column: 2,
            row_absolute: true,
            column_absolute: false,
            sheet: Some("sheet".into()),
        };
        let atoms = [
            Expr::Number(3.0),
            Expr::String("hello".into()),
            Expr::Bool(true),
            Expr::Error("#N/A".into()),
            Expr::Missing,
            Expr::Name("named".into()),
            Expr::QualifiedName {
                sheet: "sheet".into(),
                name: "named".into(),
            },
            Expr::Cell(cell.clone()),
            Expr::Column {
                column: 3,
                absolute: true,
                sheet: None,
            },
            Expr::Row {
                row: 4,
                absolute: false,
                sheet: Some("sheet".into()),
            },
            Expr::SheetSpan(poirot_parsing::formula::SheetSpanReference {
                first_sheet: "one".into(),
                last_sheet: "three".into(),
                start: poirot_parsing::formula::CellReference {
                    sheet: None,
                    ..cell
                },
                end: None,
            }),
        ];
        for atom in atoms {
            assert_eq!(compile(&atom).unwrap().template.code.len(), 3);
        }
        for op in [
            BinaryOp::Add,
            BinaryOp::Subtract,
            BinaryOp::Multiply,
            BinaryOp::Divide,
            BinaryOp::Power,
            BinaryOp::Concat,
            BinaryOp::Equal,
            BinaryOp::NotEqual,
            BinaryOp::Less,
            BinaryOp::LessEqual,
            BinaryOp::Greater,
            BinaryOp::GreaterEqual,
            BinaryOp::Range,
            BinaryOp::Union,
            BinaryOp::Intersection,
        ] {
            let e = Expr::Binary {
                op,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Number(2.0)),
            };
            assert_eq!(compile(&e).unwrap().template.code[7].a, binary(op));
        }
    }
    #[test]
    fn bytecode_structural_errors_before_lowering() {
        assert_eq!(
            compile(&Expr::Array(vec![])),
            Err(CompileError::InvalidArray)
        );
        assert_eq!(
            compile(&Expr::Row {
                row: poirot_model::MAX_ROWS,
                absolute: false,
                sheet: None
            }),
            Err(CompileError::InvalidReference)
        );
        let mut e = Expr::Missing;
        for _ in 0..128 {
            e = Expr::Unary {
                op: UnaryOp::Plus,
                operand: Box::new(e),
            };
        }
        assert_eq!(compile(&e), Err(CompileError::ResourceLimit));
    }
}
