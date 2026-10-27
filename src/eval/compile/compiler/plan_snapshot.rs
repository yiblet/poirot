//! Trusted, self-generated local cache. Restores executable sharing without parsing.
//! Custom policies, extensions, parse failures and legacy tree-only programs are rejected.
use super::call_dto::CallDto;
use super::compiled_formula::CompiledFormula;
use super::compiled_workbook::CompiledWorkbook;
use super::potential_dependencies::PotentialDependencies;
use super::preparation_policy::PreparationPolicy;
use super::prepared_formula::PreparedFormula;
use super::program_dto::ProgramDto;
use super::snapshot_reader::SnapshotReader;
use super::source_dto::SourceDto;
use super::source_fingerprint::fingerprint;
use super::source_identity::SourceIdentity;
use super::template_validation::{validate_references, validate_template};
use crate::eval::compile::bytecode::{Call, CallTarget, Compiled, Executable, Opcode};
use crate::eval::functions::registry;
use crate::model::CellAddress;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// Snapshot validation result carrying a diagnostic for unsupported or inconsistent compiled data.
type Result<T> = std::result::Result<T, String>;

/// Wire version of the plan snapshot layout.
pub(super) const VERSION: u32 = 21;

/// Serde view preserving source-to-program sharing for a trusted compiled workbook plan.
#[derive(Serialize, Deserialize)]
pub struct PlanSnapshot<'a> {
    pub(super) version: u32,
    pub(super) identity: Cow<'a, SourceIdentity>,
    pub(super) programs: Vec<ProgramDto<'a>>,
    pub(super) sources: Vec<SourceDto<'a>>,
    formulas: Vec<(usize, CellAddress, usize)>,
    names: Vec<usize>,
}

impl<'a> PlanSnapshot<'a> {
    /// Borrow a supported default-policy plan, rejecting extensions and preparation failures.
    pub fn capture(plan: &'a CompiledWorkbook) -> Result<Self> {
        if plan.policy != PreparationPolicy::default() {
            return Err("snapshot requires default policy/no extensions".into());
        }
        let mut output = Self {
            version: VERSION,
            identity: Cow::Borrowed(&plan.identity),
            programs: Vec::new(),
            sources: Vec::new(),
            formulas: Vec::new(),
            names: Vec::new(),
        };
        let mut sources = HashMap::new();
        let mut programs = HashMap::new();
        let mut add = |prepared: &'a Arc<PreparedFormula>| -> Result<usize> {
            let key = Arc::as_ptr(prepared) as usize;
            if let Some(id) = sources.get(&key) {
                return Ok(*id);
            }
            let PreparedFormula::Ready(formula) = prepared.as_ref() else {
                return Err("snapshot does not support preparation failures".into());
            };
            if formula.policy != PreparationPolicy::default()
                || !formula.potential.extensions.is_empty()
            {
                return Err("snapshot requires default policy/no extensions".into());
            }
            let compiled = &formula.bytecode;
            if compiled.extension_identity.is_some() {
                return Err("snapshot does not support extensions".into());
            }
            let program_key = Arc::as_ptr(&compiled.executable) as usize;
            let program = if let Some(id) = programs.get(&program_key) {
                *id
            } else {
                let calls = compiled
                    .calls
                    .iter()
                    .map(|c| {
                        Ok(CallDto {
                            source_offset: c.source_offset,
                            resolved_name: c.resolved_name.as_deref().map(Cow::Borrowed),
                            builtin: match c.target {
                                CallTarget::Builtin(id) => Some(Cow::Borrowed(
                                    registry::FUNCTION_DESCRIPTORS
                                        .iter()
                                        .find(|d| d.handler == id)
                                        .ok_or("unknown builtin ID")?
                                        .name,
                                )),
                                CallTarget::Unresolved => None,
                                CallTarget::Extension(_) => {
                                    return Err("snapshot does not support extensions".into());
                                }
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let id = output.programs.len();
                output.programs.push(ProgramDto {
                    typing: Cow::Borrowed(&compiled.typing),
                    template: Cow::Borrowed(&compiled.template),
                    calls,
                });
                programs.insert(program_key, id);
                id
            };
            let potential = &formula.potential;
            let id = output.sources.len();
            output.sources.push(SourceDto {
                source: Cow::Borrowed(&formula.source),
                program,
                references: Cow::Borrowed(&compiled.references),
                potential_references: Cow::Borrowed(&potential.references),
                names: Cow::Borrowed(&potential.names),
                qualified_names: Cow::Borrowed(&potential.qualified_names),
                opaque_functions: Cow::Borrowed(&potential.opaque_functions),
                dynamic_references: potential.dynamic_references,
            });
            sources.insert(key, id);
            Ok(id)
        };
        let mut formulas: Vec<_> = plan.formulas.iter().collect();
        formulas.sort_by_key(|((sheet, at), _)| (*sheet, *at));
        for ((sheet, at), formula) in formulas {
            output.formulas.push((*sheet, *at, add(formula)?));
        }
        for formula in &plan.names {
            output.names.push(add(formula)?);
        }
        Ok(output)
    }

    /// Copy borrowed payloads only for an independently retained snapshot.
    pub fn into_owned(self) -> PlanSnapshot<'static> {
        PlanSnapshot {
            version: self.version,
            identity: Cow::Owned(self.identity.into_owned()),
            programs: self
                .programs
                .into_iter()
                .map(|p| ProgramDto {
                    typing: Cow::Owned(p.typing.into_owned()),
                    template: Cow::Owned(p.template.into_owned()),
                    calls: p
                        .calls
                        .into_iter()
                        .map(|c| CallDto {
                            source_offset: c.source_offset,
                            resolved_name: c.resolved_name.map(|v| Cow::Owned(v.into_owned())),
                            builtin: c.builtin.map(|v| Cow::Owned(v.into_owned())),
                        })
                        .collect(),
                })
                .collect(),
            sources: self
                .sources
                .into_iter()
                .map(|s| SourceDto {
                    source: Cow::Owned(s.source.into_owned()),
                    program: s.program,
                    references: Cow::Owned(s.references.into_owned()),
                    potential_references: Cow::Owned(s.potential_references.into_owned()),
                    names: Cow::Owned(s.names.into_owned()),
                    qualified_names: Cow::Owned(s.qualified_names.into_owned()),
                    opaque_functions: Cow::Owned(s.opaque_functions.into_owned()),
                    dynamic_references: s.dynamic_references,
                })
                .collect(),
            formulas: self.formulas,
            names: self.names,
        }
    }

    /// Return shared program, distinct source, and total source-assignment counts.
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.programs.len(),
            self.sources.len(),
            self.formulas.len() + self.names.len(),
        )
    }

    /// Validate serialized programs and reconstruct a default-policy workbook plan.
    pub fn restore(self) -> Result<CompiledWorkbook> {
        if self.version != VERSION {
            return Err("unsupported compiled snapshot version".into());
        }
        let programs = self
            .programs
            .into_iter()
            .map(|p| {
                validate_template(&p.template, p.calls.len())?;
                if !p.typing.matches(&p.template) {
                    return Err("snapshot typing boundaries".into());
                }
                let calls = p
                    .calls
                    .into_iter()
                    .map(|c| {
                        let mut reader = SnapshotReader::new(
                            p.template
                                .operands
                                .get(c.source_offset as usize..)
                                .ok_or("call source offset")?,
                        );
                        reader.string()?;
                        Ok(Call {
                            source_offset: c.source_offset,
                            resolved_name: c.resolved_name.map(Cow::into_owned),
                            target: match c.builtin {
                                Some(name) => CallTarget::Builtin(
                                    registry::resolve_function(
                                        &poirot_parsing::formula::FunctionName::from(&*name),
                                    )
                                    .ok_or("snapshot builtin unavailable")?
                                    .handler,
                                ),
                                None => CallTarget::Unresolved,
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                for instruction in &p.template.code {
                    if instruction.opcode == Opcode::Conditional
                        && calls.get(instruction.a as usize).is_none_or(|call| {
                            call.target
                                != CallTarget::Builtin(registry::FunctionId::Logical(
                                    registry::LogicalFunction::If,
                                ))
                        })
                    {
                        return Err("snapshot conditional binding".into());
                    }
                }
                Ok(Arc::new(Executable {
                    typing: p.typing.into_owned(),
                    template: p.template.into_owned(),
                    calls,
                    extension_identity: None,
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        let sources = self
            .sources
            .into_iter()
            .map(|s| {
                let executable = programs
                    .get(s.program)
                    .ok_or("snapshot program ID")?
                    .clone();
                validate_references(&s.references, executable.template.reference_slots)?;
                Ok(Arc::new(PreparedFormula::Ready(Arc::new(
                    CompiledFormula {
                        source: Arc::from(s.source.into_owned()),

                        bytecode: Compiled {
                            executable,
                            references: s.references.into_owned(),
                        },
                        potential: PotentialDependencies {
                            references: s.potential_references.into_owned(),
                            names: s.names.into_owned(),
                            qualified_names: s.qualified_names.into_owned(),
                            opaque_functions: s.opaque_functions.into_owned(),
                            dynamic_references: s.dynamic_references,
                            extensions: Vec::new(),
                        },
                        policy: PreparationPolicy::default(),
                    },
                ))))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut formulas = HashMap::with_capacity(self.formulas.len());
        for (sheet, at, id) in self.formulas {
            at.validate().map_err(|e| e.to_string())?;
            if formulas
                .insert(
                    (sheet, at),
                    sources.get(id).ok_or("snapshot source ID")?.clone(),
                )
                .is_some()
            {
                return Err("duplicate formula target".into());
            }
        }
        let names = self
            .names
            .into_iter()
            .map(|id| {
                sources
                    .get(id)
                    .cloned()
                    .ok_or_else(|| "snapshot name source ID".into())
            })
            .collect::<Result<Vec<_>>>()?;
        // Ensure source-ID assignment agrees with exact source topology, without a parser.
        if formulas.len() != self.identity.formulas.len()
            || names.len() != self.identity.names.len()
        {
            return Err("snapshot source topology count".into());
        }
        for (sheet, at, source) in &self.identity.formulas {
            if formulas.get(&(*sheet, *at)).map(|f| f.source()) != Some(source.as_str()) {
                return Err("snapshot formula identity".into());
            }
        }
        for (prepared, (_, source, _)) in names.iter().zip(&self.identity.names) {
            if prepared.source() != source {
                return Err("snapshot defined-name identity".into());
            }
        }
        Ok(CompiledWorkbook {
            fingerprint: fingerprint(&self.identity),
            identity: self.identity.into_owned(),
            policy: PreparationPolicy::default(),
            formulas,
            names,
        })
    }
}
