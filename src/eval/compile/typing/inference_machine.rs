use super::expression_contract::ExpressionContract;
use super::expression_properties::ExpressionProperties;
use super::formula_typing::FormulaTyping;
use super::inference_frame::InferenceFrame;
use super::transfer::transfer;
use super::typed_expression::TypedExpression;
use crate::eval::compile::bytecode::{Call, CallTarget, CompileError};
use poirot_parsing::formula::Expr;

/// Abstract interpreter advanced by the same enter/bind/leave events as bytecode emission.
pub(in crate::eval) struct InferenceMachine<'a> {
    pub(super) expressions: Vec<TypedExpression>,
    pub(super) frames: poirot_functions::scalar::ParamVec<InferenceFrame<'a>>,
    pub(super) operands: poirot_functions::scalar::ParamVec<ExpressionProperties>,
    locals: poirot_functions::scalar::ParamVec<ExpressionProperties>,
}

impl<'a> InferenceMachine<'a> {
    /// Reserve the preflight-counted expression metadata without a second syntax walk.
    pub(in crate::eval) fn new(count: usize) -> Result<Self, CompileError> {
        let mut expressions = Vec::new();
        expressions
            .try_reserve_exact(count)
            .map_err(|_| CompileError::ResourceLimit)?;
        Ok(Self {
            expressions,
            frames: poirot_functions::scalar::ParamVec::new(),
            operands: poirot_functions::scalar::ParamVec::new(),
            locals: poirot_functions::scalar::ParamVec::new(),
        })
    }

    /// Suspend a parent while the common driver visits its operands.
    pub(in crate::eval) fn enter(
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
            contract: if depth >= crate::eval::compile::bytecode::DEPTH {
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
            local: None,
        });
        Ok(())
    }

    /// Reuse the compiler's authoritative function binding, including external aliases.
    pub(in crate::eval) fn bind(
        &mut self,
        target: &CallTarget,
        binding: usize,
    ) -> Result<(), CompileError> {
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

    /// Associate a local slot with the most recently completed initializer's type.
    pub(in crate::eval) fn bind_local(&mut self, slot: usize) -> Result<(), CompileError> {
        if slot != self.locals.len() {
            return Err(CompileError::InvalidReference);
        }
        let value = self
            .operands
            .last()
            .copied()
            .ok_or(CompileError::InvalidReference)?;
        self.locals
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        // SAFETY: The reservation admits exactly one property entry.
        self.locals.push(ExpressionProperties {
            effects: Default::default(),
            ..value
        });
        Ok(())
    }

    /// Annotate a local read with the type already established by its initializer.
    pub(in crate::eval) fn read_local(&mut self, slot: usize) -> Result<(), CompileError> {
        let local = self
            .locals
            .get(slot)
            .copied()
            .ok_or(CompileError::InvalidReference)?;
        let frame = self
            .frames
            .last_mut()
            .ok_or(CompileError::InvalidReference)?;
        frame.local = Some(local);
        Ok(())
    }

    /// Leave a lexical scope while retaining the result's inferred properties.
    pub(in crate::eval) fn clear_locals(&mut self, first: usize) -> Result<(), CompileError> {
        if first >= self.locals.len() {
            return Err(CompileError::InvalidReference);
        }
        self.locals.truncate(first);
        Ok(())
    }

    /// Infer the completed expression from its immediate operands and resume its parent.
    pub(in crate::eval) fn leave(&mut self, bindings: &[Call]) -> Result<(), CompileError> {
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
        let properties = if let Some(local) = frame.local {
            if !operands.is_empty() {
                return Err(CompileError::InvalidReference);
            }
            local
        } else {
            transfer(frame.expression, frame.depth, operands, target)?
        };
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
    pub(in crate::eval) fn finish(self) -> Result<FormulaTyping, CompileError> {
        if !self.frames.is_empty() || self.operands.len() != 1 || !self.locals.is_empty() {
            return Err(CompileError::InvalidReference);
        }
        Ok(FormulaTyping {
            expressions: self.expressions,
        })
    }
}
