//! Owned stack compiler and runtime metadata for lazy bytecode execution.
//!
//! Preparation validates and counts the source, then emits instructions and type facts
//! in one shared traversal. Each compiled body owns an instruction vector and a dense
//! label vector. Branches and deferred arguments occupy that same instruction buffer;
//! they do not own boxed child programs. Labels resolve to instruction positions.
//!
//! Operators consume values from the shared VM stack. They retain generic semantics:
//! `Add` handles both scalar operands and singleton-axis array broadcasting at runtime.
//! Operand types and shapes can change without rewriting or recompiling the instructions.
//! Type inference remains available for preparation and spill bounds; no runtime
//! specialization cache is required by this instruction format.
//!
//! Every call, including unknown and extension calls, owns deferred argument blocks.
//! Selection preserves function-specific arity, laziness, omission and error ordering.
//! Calls dispatch through function-family executors, which request only the argument
//! blocks they need. `Conditional` identifies `IF` calls and uses the same execution
//! path as `Invoke`; neither instruction eagerly executes every argument block.
//! References stay values until an operator or family adapter requests a read.
//!
//! Only final buffers and the shared executable owner allocate during compilation;
//! the streamed preparation events and temporary label bindings allocate nothing.
//! Source retention and template interning belong to the caller.

pub mod aggregation;

use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};
use std::sync::Arc;

const LIMIT: usize = 100_000;
pub(in crate::eval) const DEPTH: usize = 128;
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

/// Index into a compiled body's label table, distinct from an instruction offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Label(pub u32);

/// Position in a compiled body's instruction vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstructionIndex(pub u32);
impl InstructionIndex {
    /// Convert to a slice index for bounds-checked instruction access.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

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
    /// `a`: call binding index; `b`: arity; `c`: continuation label.
    /// Arguments start at ip+2; following `Argument.c` walks the descriptors.
    Invoke,
    /// Inline argument descriptor: `a`: argument ordinal; `c`: next descriptor/end label.
    Argument,
    /// Return the argument's reference/value to the requesting family adapter.
    YieldArgument,
    /// Continue at label `c`, resolved through the enclosing template's label table.
    Jump,
    /// Add scalar or broadcast array operands using their runtime types.
    Add,
    /// Subtract scalar or broadcast array operands using their runtime types.
    Subtract,
    /// Multiply scalar or broadcast array operands using their runtime types.
    Multiply,
    /// Divide scalar or broadcast array operands using their runtime types.
    Divide,
    /// Exponentiate scalar or broadcast array operands using their runtime types.
    Power,
    /// Concatenate scalar or broadcast array operands using their runtime types.
    Concat,
    /// Compare scalar or broadcast array operands for equality.
    Equal,
    /// Compare scalar or broadcast array operands for inequality.
    NotEqual,
    /// Compare scalar or broadcast array operands with less-than ordering.
    Less,
    /// Compare scalar or broadcast array operands with less-or-equal ordering.
    LessEqual,
    /// Compare scalar or broadcast array operands with greater-than ordering.
    Greater,
    /// Compare scalar or broadcast array operands with greater-or-equal ordering.
    GreaterEqual,
    /// Conditional over deferred branch labels, with the same operand layout as `Invoke`.
    Conditional,
}

impl Opcode {
    /// Whether the instruction owns deferred argument blocks and a continuation label.
    pub(in crate::eval) fn is_call(self) -> bool {
        matches!(self, Self::Invoke | Self::Conditional)
    }

    /// Scalar operation lifted by this stable, shape-polymorphic instruction.
    pub(in crate::eval) fn binary_operation(self) -> Option<BinaryOp> {
        Some(match self {
            Self::Add => BinaryOp::Add,
            Self::Subtract => BinaryOp::Subtract,
            Self::Multiply => BinaryOp::Multiply,
            Self::Divide => BinaryOp::Divide,
            Self::Power => BinaryOp::Power,
            Self::Concat => BinaryOp::Concat,
            Self::Equal => BinaryOp::Equal,
            Self::NotEqual => BinaryOp::NotEqual,
            Self::Less => BinaryOp::Less,
            Self::LessEqual => BinaryOp::LessEqual,
            Self::Greater => BinaryOp::Greater,
            Self::GreaterEqual => BinaryOp::GreaterEqual,
            _ => return None,
        })
    }
    /// Select a primitive from syntax independently of inferred or observed types.
    fn for_binary(operation: BinaryOp) -> Self {
        match operation {
            BinaryOp::Add => Self::Add,
            BinaryOp::Subtract => Self::Subtract,
            BinaryOp::Multiply => Self::Multiply,
            BinaryOp::Divide => Self::Divide,
            BinaryOp::Power => Self::Power,
            BinaryOp::Concat => Self::Concat,
            BinaryOp::Equal => Self::Equal,
            BinaryOp::NotEqual => Self::NotEqual,
            BinaryOp::Less => Self::Less,
            BinaryOp::LessEqual => Self::LessEqual,
            BinaryOp::Greater => Self::Greater,
            BinaryOp::GreaterEqual => Self::GreaterEqual,
            BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection => Self::Binary,
        }
    }
}

/// Fixed-width stack instruction. Control-flow targets are label-table indices.
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
    /// Third operand word, including label IDs for control-flow instructions.
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
    /// Dense mapping from label IDs to instruction positions in this body.
    pub labels: Vec<InstructionIndex>,
    /// Contextual aggregation projections keyed by expression entry.
    pub aggregations: Vec<aggregation::Entry>,
    /// Executable instructions in instruction-pointer order.
    pub code: Vec<Instruction>,
    /// Packed literal and name bytes addressed by instructions.
    #[serde(with = "serde_bytes")]
    pub operands: Vec<u8>,
    /// Number of per-formula reference slots required by this template.
    pub reference_slots: u32,
}
impl Template {
    /// Resolve a body-local label without changing instructions or retaining pointers into vectors.
    pub fn resolve_label(&self, label: Label) -> Option<InstructionIndex> {
        self.labels.get(label.0 as usize).copied()
    }
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
/// Locates one formula reference's encoded coordinates in [`ReferenceBindings::data`].
/// Data uses little-endian u32 fields and strings encoded as u32 byte length + UTF-8.
/// Cell: row,column,row_absolute,column_absolute,optional sheet; column/row: index,
/// absolute,optional sheet; span: first/last sheet,cell start,end-present,optional end.
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
    pub typing: crate::eval::typing::FormulaTyping,
    /// Shared instruction and literal layout.
    pub template: Template,
    /// Bound call identities indexed by invocation instructions.
    pub calls: Vec<Call>,
    /// Exact registry identity required by extension bindings.
    pub extension_identity: Option<crate::eval::extensions::ExtensionIdentity>,
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
/// Resolved function identity and spelling metadata for one bytecode call.
/// The VM uses it to select a handler while argument expressions remain deferred
/// in instruction blocks.
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
    Builtin(crate::eval::registry::FunctionId),
    /// Explicit extension registry binding.
    Extension(crate::eval::extensions::ExtensionBinding),
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
    registry: Option<&crate::eval::extensions::ExtensionRegistry>,
) -> Result<Compiled, CompileError> {
    compile_inner(expression, registry, false)
}
/// Compile an independently supplied argument for the contextual aggregation role.
/// Ordinary compile entry points deliberately do not project root worksheet names.
pub fn compile_aggregation_argument(expression: &Expr) -> Result<Compiled, CompileError> {
    let mut compiled = compile_inner(expression, None, true)?;
    if let Some(plan) = aggregation::lower(expression)? {
        compiled
            .template
            .aggregations
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        // SAFETY: Index zero is always a valid insertion position; one additional slot was reserved locally.
        compiled
            .template
            .aggregations
            .insert(0, aggregation::Entry { start: 0, plan });
    }
    Ok(compiled)
}
fn compile_inner(
    expression: &Expr,
    registry: Option<&crate::eval::extensions::ExtensionRegistry>,
    strict: bool,
) -> Result<Compiled, CompileError> {
    let mut sizes = Sizes::default();
    let mut nodes = 0;
    validate(expression, 0, &mut nodes, strict)?;
    stream(expression, 0, &mut sizes);
    if [
        sizes.code,
        sizes.labels,
        sizes.text,
        sizes.refs,
        sizes.reference_bytes,
    ]
    .iter()
    .any(|n| *n > u32::MAX as usize)
    {
        return Err(CompileError::ResourceLimit);
    }
    let writer = Writer {
        output: Compiled {
            executable: Arc::new(Executable {
                typing: crate::eval::typing::FormulaTyping::default(),
                template: Template {
                    labels: Vec::with_capacity(sizes.labels),
                    aggregations: Vec::new(),
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
        pending_labels: [None; DEPTH * 2],
        aggregation_roles: [false; DEPTH],
        registry,
    };
    let mut preparation = Preparation {
        writer,
        inference: crate::eval::typing::InferenceMachine::new(sizes.expressions)?,
        error: None,
    };
    stream(expression, 0, &mut preparation);
    let Preparation {
        mut writer,
        inference,
        error,
    } = preparation;
    if let Some(error) = error.or(writer.error) {
        return Err(error);
    }
    if writer.pending_labels.iter().any(Option::is_some)
        || writer.output.template.labels.len() != sizes.labels
        || writer
            .output
            .template
            .labels
            .iter()
            .any(|target| target.0 == NONE)
        || writer.output.template.code.len() != sizes.code
        || writer.output.template.operands.len() != sizes.text
        || writer.output.references.data.len() != sizes.reference_bytes
    {
        return Err(CompileError::InvalidReference);
    }
    writer.output.typing = inference.finish()?;
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
        c.row >= crate::model::MAX_ROWS || c.column >= crate::model::MAX_COLUMNS
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
        Expr::Column { column, .. } if *column >= crate::model::MAX_COLUMNS => {
            return Err(CompileError::InvalidReference);
        }
        Expr::Row { row, .. } if *row >= crate::model::MAX_ROWS => {
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
// Unresolved source labels are scoped by syntax depth. A fixed table maps them
// to unique body-local IDs in the final dense label vector.
/// Temporary high-level instruction stream lowered into fixed-width executable bytecode.
enum High<'a> {
    Enter(usize, &'a Expr),
    Leave,
    Atom(&'a Expr),
    Truncated,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Array(usize, usize),
    ArrayAllocate(usize, usize),
    Scalarize,
    Invoke(&'a str, usize, usize),
    Argument(usize, usize, usize, &'a Expr),
    YieldArgument,
    Goto(usize),
    Label(usize),
}
/// Bytecode lowering destination shared by sizing and emission passes.
trait Sink<'ast> {
    fn emit(&mut self, op: High<'ast>);
}
fn stream<'ast>(e: &'ast Expr, depth: usize, sink: &mut impl Sink<'ast>) {
    sink.emit(High::Enter(depth, e));
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
                sink.emit(High::Argument(i, next, depth, a));
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
    expressions: usize,
    labels: usize,
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
/// Coordinates independent inference and bytecode machines over the same expression events.
struct Preparation<'registry, 'ast> {
    writer: Writer<'registry>,
    inference: crate::eval::typing::InferenceMachine<'ast>,
    error: Option<CompileError>,
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
/// Lowers streamed operations into final buffers and resolves label fixups.
struct Writer<'registry> {
    error: Option<CompileError>,
    registry: Option<&'registry crate::eval::extensions::ExtensionRegistry>,
    output: Compiled,
    pending_labels: [Option<Label>; DEPTH * 2],
    aggregation_roles: [bool; DEPTH],
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
impl<'ast> Sink<'ast> for Writer<'_> {
    fn emit(&mut self, op: High<'ast>) {
        if self.error.is_some() {
            return;
        }
        if let High::Label(label) = op {
            let destination = self.output.template.code.len() as u32;
            let Some(pending) = self.pending_labels.get_mut(label) else {
                self.error = Some(CompileError::ResourceLimit);
                return;
            };
            // A zero-argument call still has a referenced continuation label.
            if let Some(label) = pending.take() {
                let Some(slot) = self.output.template.labels.get_mut(label.0 as usize) else {
                    self.error = Some(CompileError::InvalidReference);
                    return;
                };
                *slot = InstructionIndex(destination);
            } else {
                self.error = Some(CompileError::InvalidReference);
            }
            return;
        }
        let mut target = None;
        let mut instruction = match op {
            High::Enter(d, _) => Instruction::new(Opcode::Enter, d as u32, 0),
            High::Leave => Instruction::new(Opcode::Leave, 0, 0),
            High::Atom(e) => self.atom(e),
            High::Truncated => {
                let (a, b) = self.text("#NUM!");
                Instruction::new(Opcode::Error, a, b)
            }
            High::Unary(op) => Instruction::new(Opcode::Unary, unary(op), 0),
            High::Binary(op) => Instruction::new(Opcode::for_binary(op), binary(op), 0),
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
                    Some(crate::eval::external_namespace::ExternalNameLookup::Stored(stored)) => {
                        Some(stored)
                    }
                    Some(crate::eval::external_namespace::ExternalNameLookup::UnsupportedName) => {
                        None
                    }
                    _ => Some(name),
                };
                let call_target = match lookup {
                    Some(lookup) => match crate::eval::registry::resolve_function(
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
                if let Some(role) = self.aggregation_roles.get_mut(label / 2) {
                    *role = !matches!(call_target, CallTarget::Extension(_))
                        && lookup.and_then(|name| {
                            crate::eval::registry::resolve_excel_function(&name.into())
                        }) == Some(crate::eval::registry::FunctionId::Matrix(
                            crate::eval::registry::MatrixFunction::Groupby,
                        ));
                }
                let opcode = if call_target
                    == CallTarget::Builtin(crate::eval::registry::FunctionId::Logical(
                        crate::eval::registry::LogicalFunction::If,
                    )) {
                    Opcode::Conditional
                } else {
                    Opcode::Invoke
                };
                self.output.calls.push(Call {
                    source_offset: a,
                    resolved_name: lookup.filter(|s| *s != name).map(str::to_owned),
                    target: call_target,
                });
                target = Some(label);
                Instruction::new(opcode, index, count as u32)
            }
            High::Argument(index, label, depth, expression) => {
                if index == 2 && self.aggregation_roles.get(depth) == Some(&true) {
                    match aggregation::lower_with_registry(expression, self.registry) {
                        Ok(Some(plan)) => {
                            let start = self.output.template.code.len().saturating_add(1);
                            if self.output.template.aggregations.try_reserve(1).is_err() {
                                self.error = Some(CompileError::ResourceLimit);
                                return;
                            }
                            // SAFETY: The successful reservation admits one entry and nothing has been inserted since.
                            self.output
                                .template
                                .aggregations
                                .push(aggregation::Entry { start, plan });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.error = Some(error);
                            return;
                        }
                    }
                }
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
            let Some(pending) = self.pending_labels.get_mut(label) else {
                self.error = Some(CompileError::ResourceLimit);
                return;
            };
            let label = match *pending {
                Some(label) => label,
                None => {
                    let label = Label(self.output.template.labels.len() as u32);
                    if self.output.template.labels.len() == self.output.template.labels.capacity() {
                        self.error = Some(CompileError::ResourceLimit);
                        return;
                    }
                    // SAFETY: The capacity check above proves one spare slot, and no vector
                    // mutation occurs before this push.
                    self.output.template.labels.push(InstructionIndex(NONE));
                    *pending = Some(label);
                    label
                }
            };
            instruction.c = label.0;
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
        UnaryOp::SpillRange => 4,
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
    fn labels_are_dense_body_local_ids_independent_of_instruction_positions() {
        let program = compiled("IF(A1,SUM(1,2),IF(FALSE,1/0,3))");
        assert!(!program.template.labels.is_empty());
        assert_eq!(
            program.template.labels.len(),
            program.template.labels.capacity()
        );
        assert!(program.template.resolve_label(Label(u32::MAX)).is_none());
        for (ip, instruction) in program.template.code.iter().enumerate() {
            if instruction.opcode.is_call()
                || matches!(instruction.opcode, Opcode::Jump | Opcode::Argument)
            {
                let position = program
                    .template
                    .resolve_label(Label(instruction.c))
                    .unwrap()
                    .index();
                assert!(position > ip);
                assert!(position < program.template.code.len());
            }
        }
        assert!(
            program
                .template
                .labels
                .iter()
                .enumerate()
                .any(|(id, position)| id != position.index())
        );
    }
    #[test]
    fn bytecode_lazy_calls_have_forward_blocks_and_continuations() {
        for name in ["IF", "IFERROR", "SUM", "UNKNOWN"] {
            let result = compiled(&format!("{name}(A1,IFERROR(B1,9),1/0)"));
            let code = &result.template.code;
            let invoke = code.iter().position(|i| i.opcode.is_call()).unwrap();
            assert_eq!(code[invoke].b, 3);
            assert_eq!(code[invoke + 1].opcode, Opcode::Jump);
            let done = result
                .template
                .resolve_label(Label(code[invoke].c))
                .unwrap()
                .index();
            assert_eq!(code[invoke + 1].c, code[invoke].c);
            assert_eq!(code[done].opcode, Opcode::Leave);
            let mut arg = invoke + 2;
            for index in 0..3 {
                assert_eq!(code[arg].opcode, Opcode::Argument);
                assert_eq!(code[arg].a, index);
                assert_eq!(code[arg + 1].opcode, Opcode::Enter);
                let next = result
                    .template
                    .resolve_label(Label(code[arg].c))
                    .unwrap()
                    .index();
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
                .rfind(|i| i.opcode.binary_operation().is_some() || i.opcode == Opcode::Binary)
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
                row: crate::model::MAX_ROWS,
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
