//! Portable compiler data. Runtime providers and lazily materialized ASTs are excluded.
use super::cached_source::CachedSource;
use super::compiled_formula::CompiledFormula;
use super::formula_compiler::FormulaCompiler;
use super::plan_snapshot_error::PlanSnapshotError;
use super::potential_dependencies::PotentialDependencies;
use super::preparation_policy::PreparationPolicy;
use super::prepared_formula::PreparedFormula;
use super::template_pool::TemplatePool;
use super::template_validation;
use super::templates::executable_hash;
use poirot_parsing::formula::ParseError;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// Portable compiled-source cache preserving shared executable ownership.
#[derive(Serialize, Deserialize)]
pub(in crate::eval) struct CacheSnapshot<'a> {
    programs: Vec<Cow<'a, crate::eval::compile::bytecode::Executable>>,
    formulas: Vec<Formula<'a>>,
}

/// Exact source and its compiled body or deferred parse failure.
#[derive(Serialize, Deserialize)]
struct Formula<'a> {
    source: CachedSource<'a>,
    body: Body<'a>,
}

/// Serialized compiler-cache entry retaining ready programs or preparation failures.
#[derive(Serialize, Deserialize)]
enum Body<'a> {
    Ready {
        program: usize,
        references: Cow<'a, crate::eval::compile::bytecode::ReferenceBindings>,
        potential: Cow<'a, PotentialDependencies>,
        /// Normalized-source hash under which the compiler indexed this formula as a
        /// rebinding exemplar. Restoration reuses it instead of re-tokenizing the
        /// source; `normalized_hash` is a fixed algorithm, so keys stay valid across
        /// toolchains. It is only a lookup hint: reuse still requires normalized-source
        /// equality and a successful rebinding, so a wrong key can only miss.
        template: Option<u64>,
    },
    Failed(Cow<'a, ParseError>),
    CompilationFailed(crate::eval::compile::bytecode::CompileError),
}

impl<'a> CacheSnapshot<'a> {
    /// Borrow sources and programs belonging to one exact preparation policy.
    pub(in crate::eval) fn capture(
        compiler: &'a FormulaCompiler,
        policy: &PreparationPolicy,
    ) -> Result<Self, PlanSnapshotError> {
        let mut snapshot = Self {
            programs: Vec::new(),
            formulas: Vec::new(),
        };
        let Some(index) = compiler.pool.iter().position(|(p, _)| p == policy) else {
            return Ok(snapshot);
        };
        let pool = compiler
            .pool
            .get(index)
            .map(|(_, pool)| pool)
            .ok_or(PlanSnapshotError::CompilerCachePolicyIndex)?;
        let exemplars: HashMap<*const CompiledFormula, u64> = compiler
            .templates
            .get(index)
            .into_iter()
            .flat_map(|templates| &templates.classified)
            .flat_map(|(hash, bucket)| bucket.iter().map(|f| (Arc::as_ptr(f), *hash)))
            .collect();
        let mut programs = HashMap::new();
        let mut formulas: Vec<_> = pool.iter().collect();
        formulas.sort_by_key(|(source, _)| *source);
        for (source, prepared) in formulas {
            let body = match prepared.as_ref() {
                PreparedFormula::Ready(formula) => {
                    let compiled = &formula.bytecode;
                    let next = snapshot.programs.len();
                    let id = *programs
                        .entry(Arc::as_ptr(&compiled.executable))
                        .or_insert_with(|| {
                            snapshot.programs.push(Cow::Borrowed(&compiled.executable));
                            next
                        });
                    Body::Ready {
                        program: id,
                        references: Cow::Borrowed(&compiled.references),
                        potential: Cow::Borrowed(&formula.potential),
                        template: exemplars.get(&Arc::as_ptr(formula)).copied(),
                    }
                }
                PreparedFormula::ParseFailure { error, .. } => Body::Failed(Cow::Borrowed(error)),
                PreparedFormula::CompilationFailure { error, .. } => {
                    Body::CompilationFailed(*error)
                }
                PreparedFormula::RegistryMismatch { .. } => {
                    return Err(PlanSnapshotError::RegistryMismatchInCompilerCache);
                }
            };
            snapshot.formulas.push(Formula {
                source: CachedSource::Borrowed(source),
                body,
            });
        }
        Ok(snapshot)
    }

    /// Copy borrowed payloads into independently retained serialized state.
    pub(in crate::eval) fn into_owned(self) -> CacheSnapshot<'static> {
        CacheSnapshot {
            programs: self
                .programs
                .into_iter()
                .map(|p| Cow::Owned(p.into_owned()))
                .collect(),
            formulas: self
                .formulas
                .into_iter()
                .map(|f| Formula {
                    source: CachedSource::Shared(f.source.into_shared()),
                    body: match f.body {
                        Body::Ready {
                            program,
                            references,
                            potential,
                            template,
                        } => Body::Ready {
                            program,
                            references: Cow::Owned(references.into_owned()),
                            potential: Cow::Owned(potential.into_owned()),
                            template,
                        },
                        Body::Failed(error) => Body::Failed(Cow::Owned(error.into_owned())),
                        Body::CompilationFailed(error) => Body::CompilationFailed(error),
                    },
                })
                .collect(),
        }
    }

    /// Validate bytecode and rebuild compilation pools without retaining syntax trees.
    pub(in crate::eval) fn restore(
        self,
        policy: &PreparationPolicy,
    ) -> Result<FormulaCompiler, PlanSnapshotError> {
        if self.programs.len() > 1_000_000 || self.formulas.len() > 1_000_000 {
            return Err(PlanSnapshotError::CompilerSnapshotCountLimit);
        }
        let mut templates = TemplatePool::default();
        let programs = self
            .programs
            .into_iter()
            .map(|p| {
                template_validation::validate_template(&p.template, p.calls.len())?;
                if !p.typing.matches(&p.template) {
                    return Err(PlanSnapshotError::CompilerTypingBoundaries);
                }
                if p.extension_identity.as_ref() != policy.registry.extensions.as_deref() {
                    return Err(
                        PlanSnapshotError::CompiledRegistryIdentityDiffersFromOverlayPolicy,
                    );
                }
                for call in &p.calls {
                    let start = call.source_offset as usize;
                    let bytes = p
                        .template
                        .operands
                        .get(start..)
                        .ok_or(PlanSnapshotError::CallSourceOffset)?;
                    let n = u32::from_le_bytes(
                        // SAFETY: The immediately preceding get(..4) returns an exactly four-byte slice or an error, so conversion to [u8; 4] succeeds.
                        bytes
                            .get(..4)
                            .ok_or(PlanSnapshotError::CallSourceLength)?
                            .try_into()
                            .unwrap(),
                    ) as usize;
                    std::str::from_utf8(
                        bytes
                            .get(
                                4..4usize
                                    .checked_add(n)
                                    .ok_or(PlanSnapshotError::CallSourceOverflow)?,
                            )
                            .ok_or(PlanSnapshotError::CallSourceText)?,
                    )
                    .map_err(|_| PlanSnapshotError::CallSourceUTF8)?;
                }
                template_validation::validate_aggregation_roles(&p.template, &p.calls)?;
                let program = Arc::new(p.into_owned());
                templates
                    .structural
                    .entry(executable_hash(&program))
                    .or_default()
                    .push(program.clone());
                Ok(program)
            })
            .collect::<Result<Vec<_>, PlanSnapshotError>>()?;
        let mut pool = HashMap::with_capacity(self.formulas.len());
        for formula in self.formulas {
            let source = formula.source.into_shared();
            let prepared = match formula.body {
                Body::Ready {
                    program,
                    references,
                    potential,
                    template,
                } => {
                    let executable = programs
                        .get(program)
                        .ok_or(PlanSnapshotError::CompiledProgramIndex)?
                        .clone();
                    template_validation::validate_references(
                        &references,
                        executable.template.reference_slots,
                    )?;
                    let formula = Arc::new(CompiledFormula {
                        source: Arc::clone(&source),
                        bytecode: crate::eval::compile::bytecode::Compiled {
                            executable,
                            references: references.into_owned(),
                        },
                        potential: potential.into_owned(),
                        policy: policy.clone(),
                    });
                    if let Some(hash) = template {
                        templates
                            .classified
                            .entry(hash)
                            .or_default()
                            .push(Arc::clone(&formula));
                    }
                    PreparedFormula::Ready(formula)
                }
                Body::CompilationFailed(error) => PreparedFormula::CompilationFailure {
                    source: Arc::clone(&source),
                    policy: policy.clone(),
                    error,
                },
                Body::Failed(error) => PreparedFormula::ParseFailure {
                    source: Arc::clone(&source),
                    policy: policy.clone(),
                    error: error.into_owned(),
                },
            };
            if pool.insert(source, Arc::new(prepared)).is_some() {
                return Err(PlanSnapshotError::DuplicateCompiledSource);
            }
        }
        Ok(FormulaCompiler {
            pool: vec![(policy.clone(), pool)],
            templates: vec![templates],
            ..FormulaCompiler::default()
        })
    }
}
