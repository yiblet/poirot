use super::expression_properties::ExpressionProperties;
use super::external_behavior::ExternalBehavior;
use super::formula_effects::FormulaEffects;
use crate::eval::{
    compile::bytecode::{CallTarget, CompileError},
    functions::registry::DependencyPolicy,
};
use crate::model::{FormulaType, ScalarKinds, ShapeBounds};
use poirot_parsing::formula::{BinaryOp, Expr, UnaryOp};

/// Apply the existing modal rules to operands already completed by the shared driver.
pub(super) fn transfer(
    expr: &Expr,
    depth: usize,
    operands: &[ExpressionProperties],
    target: Option<&CallTarget>,
) -> Result<ExpressionProperties, CompileError> {
    let mut children = operands.iter().copied();
    let mut effects = FormulaEffects::default();
    let mut shape = ShapeBounds::SCALAR;
    let error = FormulaType::scalar(ScalarKinds::ERROR);
    let result = if depth >= crate::eval::compile::bytecode::DEPTH {
        // The bytecode compiler replaces this subtree with a deferred #NUM! atom.
        error
    } else {
        match expr {
            Expr::Number(_) => FormulaType::scalar(ScalarKinds::NUMBER),
            Expr::String(_) => FormulaType::scalar(ScalarKinds::TEXT),
            Expr::Bool(_) => FormulaType::scalar(ScalarKinds::BOOLEAN),
            Expr::Error(_) => error,
            Expr::Missing => FormulaType {
                missing: true,
                ..FormulaType::BOTTOM
            },
            Expr::Cell(_) => {
                effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                FormulaType::single_reference(ScalarKinds::ANY).join(error)
            }
            Expr::Column { .. } | Expr::Row { .. } | Expr::SheetSpan(_) => {
                shape = reference_shape(expr);
                effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                FormulaType::multi_reference(ScalarKinds::ANY).join(error)
            }
            Expr::Name(_) | Expr::QualifiedName { .. } => {
                shape = ShapeBounds::UNKNOWN;
                effects.possible_reads = DependencyPolicy::REFERENCE_READS;
                // Names are resolved against workbook scope at execution. Their bodies
                // may use any context or provider; no bound source is assumed here.
                effects.possible_reads = effects
                    .possible_reads
                    .union(DependencyPolicy::TIME_READS)
                    .union(DependencyPolicy::SEED)
                    .union(DependencyPolicy::CURRENCY);
                effects.external = ExternalBehavior::Opaque;
                FormulaType::TOP
            }
            Expr::Unary { op, .. } => {
                let p = children.next().ok_or(CompileError::InvalidReference)?;
                effects = p.effects;
                if *op == UnaryOp::SpillRange {
                    shape = ShapeBounds::UNKNOWN;
                    effects.possible_reads = effects
                        .possible_reads
                        .union(DependencyPolicy::REFERENCE_READS);
                    FormulaType::multi_reference(ScalarKinds::ANY).join(error)
                } else if *op == UnaryOp::ImplicitIntersection {
                    FormulaType::scalar(ScalarKinds::ANY)
                } else {
                    effects.possible_reads = effects
                        .possible_reads
                        .union(DependencyPolicy::NUMERIC_COERCION)
                        .union(DependencyPolicy::DATE_SYSTEM);
                    shape = p.shape;
                    numeric_lift(p.result, ScalarKinds::NUMBER.join(ScalarKinds::ERROR))
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
                    FormulaType::single_reference(ScalarKinds::ANY)
                        .join(FormulaType::multi_reference(ScalarKinds::ANY))
                        .join(error)
                } else {
                    effects.possible_reads = effects
                        .possible_reads
                        .union(DependencyPolicy::NUMERIC_COERCION)
                        .union(DependencyPolicy::DATE_SYSTEM);
                    let kinds = match op {
                        BinaryOp::Concat => ScalarKinds::TEXT.join(ScalarKinds::ERROR),
                        BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual => ScalarKinds::BOOLEAN.join(ScalarKinds::ERROR),
                        _ => ScalarKinds::NUMBER.join(ScalarKinds::ERROR),
                    };
                    shape = a.shape.join(b.shape);
                    numeric_lift(a.result.join(b.result), kinds)
                }
            }
            Expr::Array(rows) => {
                shape = ShapeBounds::new(
                    u32::try_from(rows.len()).unwrap_or(0),
                    u32::try_from(rows.first().map_or(0, Vec::len)).unwrap_or(0),
                );
                let mut elements = ScalarKinds::NONE;
                for _ in rows.iter().flatten() {
                    let p = children.next().ok_or(CompileError::InvalidReference)?;
                    effects = effects.join(p.effects);
                    elements = elements.join(p.result.elements());
                }
                FormulaType::array(elements.join(ScalarKinds::ERROR)).join(error)
            }
            Expr::Function { name, args } => {
                let target = target.ok_or(CompileError::InvalidReference)?;
                let mut types = poirot_functions::scalar::ParamVec::<FormulaType>::new();
                let mut shapes = poirot_functions::scalar::ParamVec::<ShapeBounds>::new();
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
                        if let crate::eval::functions::registry::FunctionId::Reference(kind) = id
                            && kind.helper().is_some_and(|helper| helper.policy().volatile)
                        {
                            effects.external = effects.external.max(ExternalBehavior::Volatile);
                        }
                        shape = descriptor.result.shape(&shapes, &constants);
                        descriptor.result.apply(&types)
                    }
                    CallTarget::Extension(binding) => {
                        shape = ShapeBounds::UNKNOWN;
                        use crate::eval::functions::extensions::ExtensionCacheability;
                        let contract = binding.registration.contract;
                        effects.external = effects.external.max(match contract.cacheability {
                            ExtensionCacheability::Deterministic
                            | ExtensionCacheability::Versioned { .. } => ExternalBehavior::Tracked,
                            ExtensionCacheability::Volatile => ExternalBehavior::Volatile,
                            ExtensionCacheability::Opaque => ExternalBehavior::Opaque,
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
                        let arguments = types
                            .iter()
                            .copied()
                            .fold(FormulaType::BOTTOM, FormulaType::join);
                        let output = if contract.capabilities.arrays {
                            FormulaType::array(ScalarKinds::ANY)
                                .join(FormulaType::scalar(ScalarKinds::ANY))
                        } else {
                            FormulaType::scalar(ScalarKinds::ANY)
                        };
                        output.join(arguments)
                    }
                    CallTarget::Unresolved => {
                        effects.external = ExternalBehavior::Opaque;
                        // The selected runtime profile can resolve an otherwise
                        // unbound call. Its possible result must be admitted before
                        // producer classification, including reference-valued aliases.
                        if let Some(handler) =
                            crate::eval::functions::registry::resolve_excel_function(name)
                        {
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
        shape = ShapeBounds::SCALAR;
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
pub(super) fn numeric_lift(input: FormulaType, kinds: ScalarKinds) -> FormulaType {
    let scalar = FormulaType::scalar(kinds);
    if input.may_spill() {
        scalar.join(FormulaType::array(kinds))
    } else {
        scalar
    }
}

/// Bound physical rectangular reference syntax without resolving names or computed endpoints.
pub(super) fn reference_shape(expr: &Expr) -> ShapeBounds {
    match expr {
        Expr::Cell(_) => ShapeBounds::SCALAR,
        Expr::Column { .. } => ShapeBounds::new(crate::model::MAX_ROWS, 1),
        Expr::Row { .. } => ShapeBounds::new(1, crate::model::MAX_COLUMNS),
        Expr::SheetSpan(span) => {
            let end = span.end.as_ref().unwrap_or(&span.start);
            ShapeBounds::new(
                span.start.row.abs_diff(end.row).saturating_add(1),
                span.start.column.abs_diff(end.column).saturating_add(1),
            )
        }
        Expr::Binary {
            op: BinaryOp::Range,
            left,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Cell(a), Expr::Cell(b)) => ShapeBounds::new(
                a.row.abs_diff(b.row).saturating_add(1),
                a.column.abs_diff(b.column).saturating_add(1),
            ),
            (Expr::Column { column: a, .. }, Expr::Column { column: b, .. }) => {
                ShapeBounds::new(crate::model::MAX_ROWS, a.abs_diff(*b).saturating_add(1))
            }
            (Expr::Row { row: a, .. }, Expr::Row { row: b, .. }) => {
                ShapeBounds::new(a.abs_diff(*b).saturating_add(1), crate::model::MAX_COLUMNS)
            }
            _ => ShapeBounds::UNKNOWN,
        },
        _ => ShapeBounds::UNKNOWN,
    }
}
