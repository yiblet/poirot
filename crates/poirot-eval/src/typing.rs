//! Possible-result abstract interpretation at the existing bytecode expression boundaries.
#![doc = include_str!("../FORMULA_TYPES.md")]
use crate::{
    bytecode::{Call, CallTarget, CompileError, Compiled, Opcode, Template},
    registry::DependencyPolicy,
};
use poirot_model::{FormulaType as T, ScalarKinds as K};
use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};

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
        typed.next().is_none() && !self.expressions.is_empty()
    }
}
/// Bounded traversal matching the compiler's syntax-preorder expression and call emission.
struct Inference<'a> {
    entries: std::iter::Enumerate<std::slice::Iter<'a, crate::bytecode::Instruction>>,
    calls: std::slice::Iter<'a, Call>,
    expressions: Vec<TypedExpression>,
}
/// Infer properties once during existing bytecode preparation, with fallible metadata allocation.
pub(crate) fn infer(expr: &Expr, compiled: &Compiled) -> Result<FormulaTyping, CompileError> {
    let count = compiled
        .template
        .code
        .iter()
        .filter(|i| i.opcode == Opcode::Enter)
        .count();
    let mut expressions = Vec::new();
    expressions
        .try_reserve_exact(count)
        .map_err(|_| CompileError::ResourceLimit)?;
    let mut inference = Inference {
        entries: compiled.template.code.iter().enumerate(),
        calls: compiled.calls.iter(),
        expressions,
    };
    inference.expression(expr, 0)?;
    if inference
        .entries
        .any(|(_, instruction)| instruction.opcode == Opcode::Enter)
        || inference.calls.next().is_some()
    {
        return Err(CompileError::InvalidReference);
    }
    Ok(FormulaTyping {
        expressions: inference.expressions,
    })
}
impl Inference<'_> {
    /// Find the next existing expression boundary without allocating a position buffer.
    fn next_entry(&mut self) -> Result<u32, CompileError> {
        for (i, instruction) in self.entries.by_ref() {
            if instruction.opcode == Opcode::Enter {
                return u32::try_from(i).map_err(|_| CompileError::ResourceLimit);
            }
        }
        Err(CompileError::InvalidReference)
    }
    /// Interpret one bounded syntax node and retain its possible properties at the emitted boundary.
    fn expression(
        &mut self,
        expr: &Expr,
        depth: usize,
    ) -> Result<ExpressionProperties, CompileError> {
        let instruction = self.next_entry()?;
        let slot = self.expressions.len();
        self.expressions.push(TypedExpression {
            instruction,
            properties: ExpressionProperties::default(),
        });
        let mut effects = FormulaEffects::default();
        let error = T::scalar(K::ERROR);
        let result = if depth >= crate::bytecode::DEPTH {
            // The bytecode compiler replaces this subtree with a deferred #NUM! atom.
            error
        } else {
            let child_depth = depth.checked_add(1).ok_or(CompileError::ResourceLimit)?;
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
                    effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                    T::multi_reference(K::ANY).join(error)
                }
                Expr::Name(_) | Expr::QualifiedName { .. } => {
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
                Expr::Unary { op, operand } => {
                    let p = self.expression(operand, child_depth)?;
                    effects = p.effects;
                    if *op == UnaryOp::ImplicitIntersection {
                        T::scalar(K::ANY)
                    } else {
                        effects.possible_reads = effects
                            .possible_reads
                            .union(DependencyPolicy::NUMERIC_COERCION)
                            .union(DependencyPolicy::DATE_SYSTEM);
                        numeric_lift(p.result, K::NUMBER.join(K::ERROR))
                    }
                }
                Expr::Binary { op, left, right } => {
                    let a = self.expression(left, child_depth)?;
                    let b = self.expression(right, child_depth)?;
                    effects = a.effects.join(b.effects);
                    if matches!(
                        op,
                        BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection
                    ) {
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
                        numeric_lift(a.result.join(b.result), kinds)
                    }
                }
                Expr::Array(rows) => {
                    let mut elements = K::NONE;
                    for child in rows.iter().flatten() {
                        let p = self.expression(child, child_depth)?;
                        effects = effects.join(p.effects);
                        elements = elements.join(p.result.elements());
                    }
                    T::array(elements.join(K::ERROR)).join(error)
                }
                Expr::Function { args, .. } => {
                    let target = &self
                        .calls
                        .next()
                        .ok_or(CompileError::InvalidReference)?
                        .target;
                    let mut types = poirot_functions_core::ParamVec::<T>::new();
                    types
                        .try_reserve(args.len())
                        .map_err(|_| CompileError::ResourceLimit)?;
                    for arg in args {
                        let p = self.expression(arg, child_depth)?;
                        effects = effects.join(p.effects);
                        types.push(p.result);
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
                            descriptor.result.apply(&types)
                        }
                        CallTarget::Extension(binding) => {
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
                            error
                        }
                    }
                }
            }
        };
        effects.possible_reads = effects.possible_reads.union(DependencyPolicy::HOST_POLICY);
        let properties = ExpressionProperties { result, effects };
        self.expressions
            .get_mut(slot)
            .ok_or(CompileError::InvalidReference)?
            .properties = properties;
        Ok(properties)
    }
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
