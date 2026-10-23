//! Portable compiler data. Runtime providers and lazily materialized ASTs are excluded.
use super::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Portable compiled-source cache preserving shared executable ownership.
#[derive(Serialize, Deserialize)]
pub(crate) struct Cache<'a> {
    programs: Vec<Cow<'a, crate::bytecode::Executable>>,
    formulas: Vec<Formula<'a>>,
}
/// Exact source and its compiled body or deferred parse failure.
#[derive(Serialize, Deserialize)]
struct Formula<'a> {
    source: Cow<'a, str>,
    body: Body<'a>,
}
/// Serialized compiler-cache entry retaining ready programs or preparation failures.
#[derive(Serialize, Deserialize)]
enum Body<'a> {
    Ready {
        program: usize,
        references: Cow<'a, crate::bytecode::ReferenceBindings>,
        potential: Cow<'a, PotentialDependencies>,
    },
    Failed(Cow<'a, ParseError>),
    CompilationFailed(crate::bytecode::CompileError),
}
impl<'a> Cache<'a> {
    /// Borrow sources and programs belonging to one exact preparation policy.
    pub(crate) fn capture(
        compiler: &'a FormulaCompiler,
        policy: &EvaluationPolicy,
    ) -> Result<Self, String> {
        let mut snapshot = Self {
            programs: Vec::new(),
            formulas: Vec::new(),
        };
        let Some((_, pool)) = compiler.pool.iter().find(|(p, _)| p == policy) else {
            return Ok(snapshot);
        };
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
                    }
                }
                PreparedFormula::ParseFailure { error, .. } => Body::Failed(Cow::Borrowed(error)),
                PreparedFormula::CompilationFailure { error, .. } => {
                    Body::CompilationFailed(*error)
                }
                PreparedFormula::RegistryMismatch { .. } => {
                    return Err("registry mismatch in compiler cache".into());
                }
            };
            snapshot.formulas.push(Formula {
                source: Cow::Borrowed(source),
                body,
            });
        }
        Ok(snapshot)
    }
    /// Copy borrowed payloads into independently retained serialized state.
    pub(crate) fn into_owned(self) -> Cache<'static> {
        Cache {
            programs: self
                .programs
                .into_iter()
                .map(|p| Cow::Owned(p.into_owned()))
                .collect(),
            formulas: self
                .formulas
                .into_iter()
                .map(|f| Formula {
                    source: Cow::Owned(f.source.into_owned()),

                    body: match f.body {
                        Body::Ready {
                            program,
                            references,
                            potential,
                        } => Body::Ready {
                            program,
                            references: Cow::Owned(references.into_owned()),
                            potential: Cow::Owned(potential.into_owned()),
                        },
                        Body::Failed(error) => Body::Failed(Cow::Owned(error.into_owned())),
                        Body::CompilationFailed(error) => Body::CompilationFailed(error),
                    },
                })
                .collect(),
        }
    }
    /// Validate bytecode and rebuild compilation pools without retaining syntax trees.
    pub(crate) fn restore(self, policy: &EvaluationPolicy) -> Result<FormulaCompiler, String> {
        if self.programs.len() > 1_000_000 || self.formulas.len() > 1_000_000 {
            return Err("compiler snapshot count limit".into());
        }
        let mut templates = TemplatePool::default();
        let programs = self
            .programs
            .into_iter()
            .map(|p| {
                snapshot::validate_template(&p.template, p.calls.len())?;
                if !p.typing.matches(&p.template) {
                    return Err("compiler typing boundaries".into());
                }
                if p.extension_identity != policy.registry.extensions {
                    return Err("compiled registry identity differs from overlay policy".into());
                }
                for call in &p.calls {
                    let start = call.source_offset as usize;
                    let bytes = p
                        .template
                        .operands
                        .get(start..)
                        .ok_or("call source offset")?;
                    let n = u32::from_le_bytes(
                        // SAFETY: The immediately preceding get(..4) returns an exactly four-byte slice or an error, so conversion to [u8; 4] succeeds.
                        bytes
                            .get(..4)
                            .ok_or("call source length")?
                            .try_into()
                            .unwrap(),
                    ) as usize;
                    std::str::from_utf8(
                        bytes
                            .get(4..4usize.checked_add(n).ok_or("call source overflow")?)
                            .ok_or("call source text")?,
                    )
                    .map_err(|_| "call source UTF-8")?;
                }
                let program = Arc::new(p.into_owned());
                templates
                    .structural
                    .entry(executable_hash(&program))
                    .or_default()
                    .push(program.clone());
                Ok(program)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut pool = HashMap::new();
        for formula in self.formulas {
            let source = formula.source.into_owned();
            let prepared = match formula.body {
                Body::Ready {
                    program,
                    references,
                    potential,
                } => {
                    let executable = programs
                        .get(program)
                        .ok_or("compiled program index")?
                        .clone();
                    snapshot::validate_references(
                        &references,
                        executable.template.reference_slots,
                    )?;
                    PreparedFormula::Ready(Arc::new(CompiledFormula {
                        source: Arc::from(source.as_str()),

                        bytecode: crate::bytecode::Compiled {
                            executable,
                            references: references.into_owned(),
                        },
                        potential: potential.into_owned(),
                        policy: policy.clone(),
                    }))
                }
                Body::CompilationFailed(error) => PreparedFormula::CompilationFailure {
                    source: Arc::from(source.as_str()),
                    policy: policy.clone(),
                    error,
                },
                Body::Failed(error) => PreparedFormula::ParseFailure {
                    source: Arc::from(source.as_str()),
                    policy: policy.clone(),
                    error: error.into_owned(),
                },
            };
            let prepared = Arc::new(prepared);
            if let PreparedFormula::Ready(formula) = prepared.as_ref()
                && reusable_template(formula)
                && let Ok(hash) = poirot_parsing::formula::classified::normalized_hash(&source)
            {
                templates
                    .classified
                    .entry(hash)
                    .or_default()
                    .push(formula.clone());
            }
            if pool.insert(source, prepared).is_some() {
                return Err("duplicate compiled source".into());
            }
        }
        Ok(FormulaCompiler {
            pool: vec![(policy.clone(), pool)],
            templates: vec![templates],
            ..FormulaCompiler::default()
        })
    }
}
