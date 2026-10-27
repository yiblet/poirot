use super::compiled_formula::CompiledFormula;
use super::compiled_workbook::CompiledWorkbook;
use super::compiler_usage::CompilerUsage;
use super::potential_dependencies::potential;
use super::preparation_policy::PreparationPolicy;
use super::prepared_formula::PreparedFormula;
use super::source_fingerprint::fingerprint;
use super::source_identity::SourceIdentity;
use super::template_pool::TemplatePool;
use super::templates::{
    classified_cell_count, executable_hash, rebound, reusable_template, scalar_classification_key,
};
use crate::model::Workbook;
use poirot_parsing::formula::parse_formula_with_policy;
use std::{collections::HashMap, sync::Arc};

/// Prepared formulas of one policy keyed by exact source text.
type SourcePool = HashMap<Arc<str>, Arc<PreparedFormula>>;

/// Parses, binds, and interns reusable formula bytecode; stores no evaluated results.
/// The overlay owns one compiler and a separate dependency-aware result cache.
///
/// Each distinct source/policy pair is retained for this compiler's lifetime.
/// Dropping it releases its references; compiled plans and prepared formulas held
/// elsewhere keep their shared artifacts alive.
#[derive(Default)]
pub struct FormulaCompiler {
    // Policy groups are expected to be few, so only these use linear full-Eq search.
    // Each group has a source
    // HashMap with exact string equality; hash collisions do not establish identity.
    // The pool has no eviction; retained plans may outlive this compiler through Arcs.
    pub(super) pool: Vec<(PreparationPolicy, SourcePool)>,
    pub(super) usage: CompilerUsage,
    pub(super) templates: Vec<TemplatePool>,
    pub(super) template_hits: u64,
    pub(super) classification_key: Vec<u8>,
    pub(super) classification_cells: Vec<(usize, usize)>,
}

impl FormulaCompiler {
    /// Classify possible geometry without binding scalar source variants.
    pub(in crate::eval) fn spill_shape(
        &mut self,
        source: &str,
        policy: &PreparationPolicy,
        registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Option<crate::model::ShapeBounds> {
        let namespace_matches = policy.registry.extensions.as_deref()
            == registry
                .map(crate::eval::functions::extensions::ExtensionRegistry::identity)
                .as_ref();
        let pool_index = self.pool.iter().position(|(p, _)| p == policy);
        if namespace_matches
            && let Some(prepared) = self
                .pool
                .iter()
                .find(|(p, _)| p == policy)
                .and_then(|(_, sources)| sources.get(source))
        {
            if let Some(pool) = pool_index.and_then(|index| self.templates.get_mut(index)) {
                pool.scalar_probation = None;
            }
            return prepared.spill_shape();
        }
        if namespace_matches
            && let Some(pool) = pool_index.and_then(|index| self.templates.get_mut(index))
            && pool
                .last_scalar
                .as_ref()
                .is_some_and(|matcher| matcher.matches(source, policy))
        {
            pool.scalar_probation = None;
            return None;
        }
        let hint = namespace_matches
            .then(|| {
                scalar_classification_key(
                    source,
                    policy,
                    &mut self.classification_key,
                    &mut self.classification_cells,
                )
            })
            .flatten();
        let classified = hint.is_some();
        if classified
            && self
                .pool
                .iter()
                .position(|(p, _)| p == policy)
                .and_then(|index| self.templates.get(index))
                .is_some_and(|pool| pool.scalar.contains(self.classification_key.as_slice()))
        {
            if let (Some(pool), Some(hint)) = (
                pool_index.and_then(|index| self.templates.get_mut(index)),
                hint,
            ) {
                pool.observe_scalar(source, hint, &self.classification_cells);
            }
            return None;
        }
        // Ordinary compilation does not modify the classification scratch buffers;
        // the key and bounded point spans still belong to this exact source below.
        let prepared = self.compile_formula_with_registry(source, policy, registry);
        let shape = prepared.spill_shape();
        if classified
            && matches!(prepared.as_ref(), PreparedFormula::Ready(formula)
                if formula.properties().is_some_and(|p| !p.result.may_spill()))
            && let Some(index) = self.pool.iter().position(|(p, _)| p == policy)
            && let Some(pool) = self.templates.get_mut(index)
        {
            let mut key = Vec::new();
            if pool.scalar.try_reserve(1).is_ok()
                && key.try_reserve_exact(self.classification_key.len()).is_ok()
            {
                // SAFETY: The successful reservation admitted the complete key bytes.
                key.extend_from_slice(&self.classification_key);
                // SAFETY: The set reservation admitted one additional learned proof.
                pool.scalar.insert(key);
            }
            if let Some(hint) = hint {
                pool.observe_scalar(source, hint, &self.classification_cells);
            }
        } else if let Some(pool) = pool_index.and_then(|index| self.templates.get_mut(index)) {
            pool.scalar_probation = None;
        }
        shape
    }

    /// Create an empty compiled-source and executable-template cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Distinct sources prepared from lexical templates without AST parsing/binding.
    pub fn template_hits(&self) -> u64 {
        self.template_hits
    }

    /// Complete immutable programs retained across all policy namespaces.
    pub fn unique_programs(&self) -> usize {
        self.templates
            .iter()
            .map(|pool| pool.structural.values().map(Vec::len).sum::<usize>())
            .sum()
    }

    /// Return cumulative parsing, binding, and exact-reuse counters.
    pub fn usage(&self) -> &CompilerUsage {
        &self.usage
    }

    /// Prepare a source lazily reusable by evaluations, without extension providers.
    pub fn compile_formula(
        &mut self,
        source: &str,
        policy: &PreparationPolicy,
    ) -> Arc<PreparedFormula> {
        self.compile_formula_with_registry(source, policy, None)
    }

    /// Prepare a source against the supplied registry's exact namespace.
    pub fn compile_formula_with_extensions(
        &mut self,
        source: &str,
        policy: &PreparationPolicy,
        registry: &crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Arc<PreparedFormula> {
        self.compile_formula_with_registry(source, policy, Some(registry))
    }

    pub(super) fn compile_formula_with_registry(
        &mut self,
        source: &str,
        policy: &PreparationPolicy,
        registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Arc<PreparedFormula> {
        if policy.registry.extensions.as_deref()
            != registry
                .map(crate::eval::functions::extensions::ExtensionRegistry::identity)
                .as_ref()
        {
            // Never poison the valid registry's pool with an unbound identity claim.
            return Arc::new(PreparedFormula::RegistryMismatch {
                source: Arc::from(source),
                policy: policy.clone(),
            });
        }
        if self.templates.len() != self.pool.len() {
            return Arc::new(PreparedFormula::CompilationFailure {
                source: Arc::from(source),
                policy: policy.clone(),
                error: crate::eval::compile::bytecode::CompileError::InvalidReference,
            });
        }
        let pool_index = self.pool.iter().position(|(p, _)| p == policy);
        // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
        if let Some(prepared) = pool_index.and_then(|index| self.pool[index].1.get(source)) {
            self.usage.reused_formulas += 1;
            return Arc::clone(prepared);
        }
        let pool_index = pool_index.unwrap_or_else(|| {
            self.pool.push((policy.clone(), HashMap::new()));
            self.templates.push(TemplatePool::default());
            self.pool.len() - 1
        });
        let normalized = poirot_parsing::formula::classified::normalized_hash(source).ok();
        if let Some(bucket) =
            // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
            normalized.and_then(|hash| self.templates[pool_index].classified.get(&hash))
        {
            for exemplar in bucket {
                if poirot_parsing::formula::classified::normalized_equal(source, &exemplar.source)
                    == Ok(true)
                    && let Some(cell_count) = classified_cell_count(source, policy)
                    && let Some(formula) = rebound(exemplar, source, cell_count)
                {
                    self.template_hits += 1;
                    let prepared = Arc::new(PreparedFormula::Ready(Arc::new(formula)));
                    // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
                    self.pool[pool_index]
                        .1
                        .insert(Arc::from(source), Arc::clone(&prepared));
                    return prepared;
                }
            }
        }
        self.usage.parse_attempts += 1;
        let prepared = match parse_formula_with_policy(source, policy.parse) {
            Err(error) => PreparedFormula::ParseFailure {
                source: Arc::from(source),
                policy: policy.clone(),
                error,
            },
            Ok(parsed) => {
                self.usage.bind_attempts += 1;
                let mut bytecode = match crate::eval::compile::bytecode::compile_with_registry(
                    parsed.expression(),
                    registry,
                ) {
                    Ok(program) => program,
                    Err(error) => {
                        let prepared = Arc::new(PreparedFormula::CompilationFailure {
                            source: Arc::from(source),
                            policy: policy.clone(),
                            error,
                        });
                        // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
                        self.pool[pool_index]
                            .1
                            .insert(Arc::from(source), Arc::clone(&prepared));
                        return prepared;
                    }
                };
                let Some(potential) = potential(parsed.expression(), &bytecode) else {
                    return Arc::new(PreparedFormula::CompilationFailure {
                        source: Arc::from(source),
                        policy: policy.clone(),
                        error: crate::eval::compile::bytecode::CompileError::InvalidReference,
                    });
                };
                // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
                let bucket = self.templates[pool_index]
                    .structural
                    .entry(executable_hash(&bytecode.executable))
                    .or_default();
                if let Some(existing) = bucket
                    .iter()
                    .find(|existing| ***existing == *bytecode.executable)
                {
                    bytecode.executable = Arc::clone(existing);
                } else {
                    bucket.push(Arc::clone(&bytecode.executable));
                }
                PreparedFormula::Ready(Arc::new(CompiledFormula {
                    source: Arc::from(parsed.source()),
                    bytecode,
                    potential,
                    policy: policy.clone(),
                }))
            }
        };
        let prepared = Arc::new(prepared);
        if let (Some(hash), PreparedFormula::Ready(formula)) = (normalized, prepared.as_ref()) {
            // Only templates whose complete lexical bindings match executable slots
            // can bypass parsing; truncation and unsupported references fall back.
            if reusable_template(formula) {
                // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
                self.templates[pool_index]
                    .classified
                    .entry(hash)
                    .or_default()
                    .push(Arc::clone(formula));
            }
        }
        // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
        self.pool[pool_index]
            .1
            .insert(Arc::from(source), Arc::clone(&prepared));
        prepared
    }

    /// Eagerly prepare workbook formulas and defined names under one policy.
    pub fn compile_workbook(
        &mut self,
        workbook: &Workbook,
        policy: &PreparationPolicy,
    ) -> CompiledWorkbook {
        self.compile_workbook_with_registry(workbook, policy, None)
    }

    /// Eagerly prepare workbook sources using an explicit extension registry.
    pub fn compile_workbook_with_extensions(
        &mut self,
        workbook: &Workbook,
        policy: &PreparationPolicy,
        registry: &crate::eval::functions::extensions::ExtensionRegistry,
    ) -> CompiledWorkbook {
        self.compile_workbook_with_registry(workbook, policy, Some(registry))
    }

    pub(super) fn compile_workbook_with_registry(
        &mut self,
        workbook: &Workbook,
        policy: &PreparationPolicy,
        registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> CompiledWorkbook {
        let identity = SourceIdentity::capture(workbook);
        let formulas = identity
            .formulas
            .iter()
            .map(|(sheet, at, source)| {
                (
                    (*sheet, *at),
                    self.compile_formula_with_registry(source, policy, registry),
                )
            })
            .collect();
        let names = identity
            .names
            .iter()
            .map(|(_, source, _)| self.compile_formula_with_registry(source, policy, registry))
            .collect();
        let fingerprint = fingerprint(&identity);
        CompiledWorkbook {
            identity,
            fingerprint,
            policy: policy.clone(),
            formulas,
            names,
        }
    }
}
