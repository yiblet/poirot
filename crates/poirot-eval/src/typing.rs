//! Possible-result abstract interpretation at the existing bytecode expression boundaries.
//!
//! Inference tracks unions of scalar kinds, arrays and references, together with
//! conservative row/column bounds and possible effects. Compilation advances this
//! state alongside bytecode emission. Unknown names and function contracts remain
//! conservative; inferred bounds never establish runtime spill ownership.
//! Actual consumed dependencies determine whether evaluated values can be reused.
use crate::{
    bytecode::{Call, CallTarget, CompileError, Opcode, Template},
    registry::DependencyPolicy,
};
use poirot_model::{FormulaType as T, ScalarKinds as K, ShapeBounds as S};
use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};

mod contracts;
pub use contracts::{ArgumentEvaluation, ConditionalArguments, ExpressionContract};

/// Possible external behavior, ordered Tracked <= Volatile <= Opaque.
/// Opaque subsumes volatility: both require refresh, and opaque behavior additionally
/// lacks a complete tracked-input contract. This order does not authorize reordering calls.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ExternalBehavior {
    /// All external inputs have tracked identities.
    #[default]
    Tracked,
    /// Refresh on every evaluation despite a declared read contract.
    Volatile,
    /// Unknown external inputs or effects prevent reuse.
    Opaque,
}
/// Modal characteristics conservatively describing possible expression reads and effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FormulaEffects {
    /// Existing conservative host capability categories, not actual consumed reads.
    pub possible_reads: DependencyPolicy,
    /// External reuse contract, independent of captured clock or seed reads.
    pub external: ExternalBehavior,
}
impl FormulaEffects {
    /// Whether every possible dependency/effect is admitted by the other characteristics.
    pub fn is_subtype_of(self, other: Self) -> bool {
        other.possible_reads.contains(self.possible_reads) && self.external <= other.external
    }
    /// Least upper bound of possible reads and external behavior.
    pub fn join(self, other: Self) -> Self {
        Self {
            possible_reads: self.possible_reads.union(other.possible_reads),
            external: self.external.max(other.external),
        }
    }
}
/// Core possible-result type paired with composable evaluation characteristics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpressionProperties {
    /// Possible runtime result forms and scalar element kinds.
    pub result: T,
    /// Independent upper bounds on successful materialized dimensions.
    pub shape: S,
    /// Conservative possible effects; actual traces govern retained-result validity.
    pub effects: FormulaEffects,
}
/// One inferred expression at its existing bytecode admission instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TypedExpression {
    /// Instruction offset of this expression's Enter opcode.
    pub instruction: u32,
    /// Possible results and characteristics at this boundary.
    pub properties: ExpressionProperties,
    /// Transfer and argument-demand contract for the expression operator.
    pub contract: ExpressionContract,
}
/// Prepared root and intermediate properties shared with the executable template.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FormulaTyping {
    expressions: Vec<TypedExpression>,
}
impl FormulaTyping {
    /// Root properties of this prepared formula/cell, when it contains an expression.
    pub fn root(&self) -> Option<ExpressionProperties> {
        self.expressions.first().map(|e| e.properties)
    }
    /// Properties of all executable expression boundaries, in instruction order.
    pub fn expressions(&self) -> &[TypedExpression] {
        &self.expressions
    }
    /// Check that serialized properties cover exactly the executable expression boundaries.
    pub(crate) fn matches(&self, template: &Template) -> bool {
        let mut typed = self.expressions.iter();
        for (i, instruction) in template.code.iter().enumerate() {
            if instruction.opcode == Opcode::Enter
                && typed
                    .next()
                    .is_none_or(|t| usize::try_from(t.instruction).ok() != Some(i))
            {
                return false;
            }
        }
        typed.next().is_none()
            && !self.expressions.is_empty()
            && self.expressions.iter().all(|entry| {
                entry.properties.result.may_spill() || entry.properties.shape == S::SCALAR
            })
    }
}
/// One suspended expression in the shared depth-first preparation traversal.
struct InferenceFrame<'a> {
    expression: &'a Expr,
    depth: usize,
    slot: usize,
    base: usize,
    binding: Option<usize>,
}
/// Abstract interpreter advanced by the same enter/bind/leave events as bytecode emission.
pub(crate) struct InferenceMachine<'a> {
    expressions: Vec<TypedExpression>,
    frames: poirot_functions_core::ParamVec<InferenceFrame<'a>>,
    operands: poirot_functions_core::ParamVec<ExpressionProperties>,
}
impl<'a> InferenceMachine<'a> {
    /// Reserve the preflight-counted expression metadata without a second syntax walk.
    pub(crate) fn new(count: usize) -> Result<Self, CompileError> {
        let mut expressions = Vec::new();
        expressions
            .try_reserve_exact(count)
            .map_err(|_| CompileError::ResourceLimit)?;
        Ok(Self {
            expressions,
            frames: poirot_functions_core::ParamVec::new(),
            operands: poirot_functions_core::ParamVec::new(),
        })
    }
    /// Suspend a parent while the common driver visits its operands.
    pub(crate) fn enter(
        &mut self,
        expression: &'a Expr,
        depth: usize,
        instruction: u32,
    ) -> Result<(), CompileError> {
        self.frames
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        self.expressions
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        let slot = self.expressions.len();
        // SAFETY: The expression reservation admits one additional metadata entry.
        self.expressions.push(TypedExpression {
            instruction,
            properties: ExpressionProperties::default(),
            contract: if depth >= crate::bytecode::DEPTH {
                ExpressionContract::Atom
            } else {
                ExpressionContract::of(expression)
            },
        });
        // SAFETY: The frame reservation admits one additional suspended expression.
        self.frames.push(InferenceFrame {
            expression,
            depth,
            slot,
            base: self.operands.len(),
            binding: None,
        });
        Ok(())
    }
    /// Reuse the compiler's authoritative function binding, including external aliases.
    pub(crate) fn bind(&mut self, target: &CallTarget, binding: usize) -> Result<(), CompileError> {
        let frame = self
            .frames
            .last_mut()
            .ok_or(CompileError::InvalidReference)?;
        let Expr::Function { name, .. } = frame.expression else {
            return Err(CompileError::InvalidReference);
        };
        let entry = self
            .expressions
            .get_mut(frame.slot)
            .ok_or(CompileError::InvalidReference)?;
        entry.contract = ExpressionContract::function(name, target);
        frame.binding = Some(binding);
        Ok(())
    }
    /// Infer the completed expression from its immediate operands and resume its parent.
    pub(crate) fn leave(&mut self, bindings: &[Call]) -> Result<(), CompileError> {
        let frame = self.frames.last().ok_or(CompileError::InvalidReference)?;
        let operands = self
            .operands
            .get(frame.base..)
            .ok_or(CompileError::InvalidReference)?;
        let target = frame
            .binding
            .map(|index| {
                bindings
                    .get(index)
                    .map(|binding| &binding.target)
                    .ok_or(CompileError::InvalidReference)
            })
            .transpose()?;
        let properties = transfer(frame.expression, frame.depth, operands, target)?;
        self.expressions
            .get_mut(frame.slot)
            .ok_or(CompileError::InvalidReference)?
            .properties = properties;
        self.operands.truncate(frame.base);
        self.operands
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        // SAFETY: The reservation admits the single completed operand.
        self.operands.push(properties);
        // Remove the suspended frame only after every fallible completion step.
        self.frames.pop().ok_or(CompileError::InvalidReference)?;
        Ok(())
    }
    /// Publish only a complete root with no suspended expressions.
    pub(crate) fn finish(self) -> Result<FormulaTyping, CompileError> {
        if !self.frames.is_empty() || self.operands.len() != 1 {
            return Err(CompileError::InvalidReference);
        }
        Ok(FormulaTyping {
            expressions: self.expressions,
        })
    }
}
/// Apply the existing modal rules to operands already completed by the shared driver.
fn transfer(
    expr: &Expr,
    depth: usize,
    operands: &[ExpressionProperties],
    target: Option<&CallTarget>,
) -> Result<ExpressionProperties, CompileError> {
    let mut children = operands.iter().copied();
    let mut effects = FormulaEffects::default();
    let mut shape = S::SCALAR;
    let error = T::scalar(K::ERROR);
    let result = if depth >= crate::bytecode::DEPTH {
        // The bytecode compiler replaces this subtree with a deferred #NUM! atom.
        error
    } else {
        match expr {
            Expr::Number(_) => T::scalar(K::NUMBER),
            Expr::String(_) => T::scalar(K::TEXT),
            Expr::Bool(_) => T::scalar(K::BOOLEAN),
            Expr::Error(_) => error,
            Expr::Missing => T {
                missing: true,
                ..T::BOTTOM
            },
            Expr::Cell(_) => {
                effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                T::single_reference(K::ANY).join(error)
            }
            Expr::Column { .. } | Expr::Row { .. } | Expr::SheetSpan(_) => {
                shape = reference_shape(expr);
                effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                T::multi_reference(K::ANY).join(error)
            }
            Expr::Name(_) | Expr::QualifiedName { .. } => {
                shape = S::UNKNOWN;
                effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                // Names are resolved against workbook scope at execution. Their bodies
                // may use any context or provider; no bound source is assumed here.
                effects.possible_reads = effects
                    .possible_reads
                    .union(DependencyPolicy::TIME_READS)
                    .union(DependencyPolicy::SEED)
                    .union(DependencyPolicy::CURRENCY);
                effects.external = ExternalBehavior::Opaque;
                T::TOP
            }
            Expr::Unary { op, .. } => {
                let p = children.next().ok_or(CompileError::InvalidReference)?;
                effects = p.effects;
                if *op == UnaryOp::SpillRange {
                    shape = S::UNKNOWN;
                    effects.possible_reads = effects
                        .possible_reads
                        .union(DependencyPolicy::REFERENCE_READS);
                    T::multi_reference(K::ANY).join(error)
                } else if *op == UnaryOp::ImplicitIntersection {
                    T::scalar(K::ANY)
                } else {
                    effects.possible_reads = effects
                        .possible_reads
                        .union(DependencyPolicy::NUMERIC_COERCION)
                        .union(DependencyPolicy::DATE_SYSTEM);
                    shape = p.shape;
                    numeric_lift(p.result, K::NUMBER.join(K::ERROR))
                }
            }
            Expr::Binary { op, .. } => {
                let a = children.next().ok_or(CompileError::InvalidReference)?;
                let b = children.next().ok_or(CompileError::InvalidReference)?;
                effects = a.effects.join(b.effects);
                if matches!(
                    op,
                    BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection
                ) {
                    shape = reference_shape(expr);
                    T::single_reference(K::ANY)
                        .join(T::multi_reference(K::ANY))
                        .join(error)
                } else {
                    effects.possible_reads = effects
                        .possible_reads
                        .union(DependencyPolicy::NUMERIC_COERCION)
                        .union(DependencyPolicy::DATE_SYSTEM);
                    let kinds = match op {
                        BinaryOp::Concat => K::TEXT.join(K::ERROR),
                        BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual => K::BOOLEAN.join(K::ERROR),
                        _ => K::NUMBER.join(K::ERROR),
                    };
                    shape = a.shape.join(b.shape);
                    numeric_lift(a.result.join(b.result), kinds)
                }
            }
            Expr::Array(rows) => {
                shape = S::new(
                    u32::try_from(rows.len()).unwrap_or(0),
                    u32::try_from(rows.first().map_or(0, Vec::len)).unwrap_or(0),
                );
                let mut elements = K::NONE;
                for _ in rows.iter().flatten() {
                    let p = children.next().ok_or(CompileError::InvalidReference)?;
                    effects = effects.join(p.effects);
                    elements = elements.join(p.result.elements());
                }
                T::array(elements.join(K::ERROR)).join(error)
            }
            Expr::Function { name, args } => {
                let target = target.ok_or(CompileError::InvalidReference)?;
                let mut types = poirot_functions_core::ParamVec::<T>::new();
                let mut shapes = poirot_functions_core::ParamVec::<S>::new();
                // Dimension proofs inspect only the first two literal arguments;
                // no extra collection is needed for constants in ordinary calls.
                let constants = [args.first(), args.get(1)].map(|argument| match argument {
                    Some(Expr::Number(number)) => Some(*number),
                    None | Some(Expr::Missing) => Some(1.0),
                    _ => None,
                });
                shapes
                    .try_reserve(args.len())
                    .map_err(|_| CompileError::ResourceLimit)?;
                types
                    .try_reserve(args.len())
                    .map_err(|_| CompileError::ResourceLimit)?;
                for _ in args {
                    let p = children.next().ok_or(CompileError::InvalidReference)?;
                    effects = effects.join(p.effects);
                    types.push(p.result);
                    // SAFETY: Each collection reserved args.len() slots above and
                    // receives exactly one item per argument in this loop.
                    shapes.push(p.shape);
                }
                match target {
                    CallTarget::Builtin(id) => {
                        let descriptor = id.descriptor();
                        effects.possible_reads =
                            effects.possible_reads.union(descriptor.dependencies);
                        if descriptor
                            .dependencies
                            .contains(DependencyPolicy::NUMERIC_COERCION)
                        {
                            effects.possible_reads =
                                effects.possible_reads.union(DependencyPolicy::DATE_SYSTEM);
                        }
                        if let crate::registry::FunctionId::Reference(kind) = id
                            && kind.helper().is_some_and(|helper| helper.policy().volatile)
                        {
                            effects.external = effects.external.max(ExternalBehavior::Volatile);
                        }
                        shape = descriptor.result.shape(&shapes, &constants);
                        descriptor.result.apply(&types)
                    }
                    CallTarget::Extension(binding) => {
                        shape = S::UNKNOWN;
                        use crate::extensions::ExtensionCacheability as C;
                        let contract = binding.registration.contract;
                        effects.external = effects.external.max(match contract.cacheability {
                            C::Deterministic | C::Versioned { .. } => ExternalBehavior::Tracked,
                            C::Volatile => ExternalBehavior::Volatile,
                            C::Opaque => ExternalBehavior::Opaque,
                        });
                        if contract.capabilities.context {
                            effects.possible_reads = effects
                                .possible_reads
                                .union(DependencyPolicy::TIME_READS)
                                .union(DependencyPolicy::SEED);
                        }
                        if contract.capabilities.references {
                            effects.possible_reads = effects
                                .possible_reads
                                .union(DependencyPolicy::REFERENCE_READS);
                        }
                        // Argument output preserves a requested argument's form. Host
                        // capability validation narrows provider-created values only.
                        let arguments = types.iter().copied().fold(T::BOTTOM, T::join);
                        let output = if contract.capabilities.arrays {
                            T::array(K::ANY).join(T::scalar(K::ANY))
                        } else {
                            T::scalar(K::ANY)
                        };
                        output.join(arguments)
                    }
                    CallTarget::Unresolved => {
                        effects.external = ExternalBehavior::Opaque;
                        // The selected runtime profile can resolve an otherwise
                        // unbound call. Its possible result must be admitted before
                        // producer classification, including reference-valued aliases.
                        if let Some(handler) = crate::registry::resolve_excel_function(name) {
                            let descriptor = handler.descriptor();
                            effects.possible_reads =
                                effects.possible_reads.union(descriptor.dependencies);
                            if descriptor
                                .dependencies
                                .contains(DependencyPolicy::NUMERIC_COERCION)
                            {
                                effects.possible_reads =
                                    effects.possible_reads.union(DependencyPolicy::DATE_SYSTEM);
                            }
                            shape = descriptor.result.shape(&shapes, &constants);
                            descriptor.result.apply(&types).join(error)
                        } else {
                            error
                        }
                    }
                }
            }
        }
    };
    effects.possible_reads = effects.possible_reads.union(DependencyPolicy::HOST_POLICY);
    // A scalar-only result cannot acquire geometry from its arguments.
    if !result.may_spill() {
        shape = S::SCALAR;
    }
    let properties = ExpressionProperties {
        result,
        shape,
        effects,
    };
    if children.next().is_some() {
        return Err(CompileError::InvalidReference);
    }
    Ok(properties)
}
/// Scalar numeric/comparison coercion with possible array lifting from array or multi-reference inputs.
fn numeric_lift(input: T, kinds: K) -> T {
    let scalar = T::scalar(kinds);
    if input.may_spill() {
        scalar.join(T::array(kinds))
    } else {
        scalar
    }
}

#[cfg(test)]
mod tests;

/// Bound physical rectangular reference syntax without resolving names or computed endpoints.
fn reference_shape(expr: &Expr) -> S {
    match expr {
        Expr::Cell(_) => S::SCALAR,
        Expr::Column { .. } => S::new(poirot_model::MAX_ROWS, 1),
        Expr::Row { .. } => S::new(1, poirot_model::MAX_COLUMNS),
        Expr::SheetSpan(span) => {
            let end = span.end.as_ref().unwrap_or(&span.start);
            S::new(
                span.start.row.abs_diff(end.row).saturating_add(1),
                span.start.column.abs_diff(end.column).saturating_add(1),
            )
        }
        Expr::Binary {
            op: BinaryOp::Range,
            left,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Cell(a), Expr::Cell(b)) => S::new(
                a.row.abs_diff(b.row).saturating_add(1),
                a.column.abs_diff(b.column).saturating_add(1),
            ),
            (Expr::Column { column: a, .. }, Expr::Column { column: b, .. }) => {
                S::new(poirot_model::MAX_ROWS, a.abs_diff(*b).saturating_add(1))
            }
            (Expr::Row { row: a, .. }, Expr::Row { row: b, .. }) => {
                S::new(a.abs_diff(*b).saturating_add(1), poirot_model::MAX_COLUMNS)
            }
            _ => S::UNKNOWN,
        },
        _ => S::UNKNOWN,
    }
}
