//! Trusted, self-generated local cache. Restores executable sharing without parsing.
//! Custom policies, extensions, parse failures and legacy tree-only programs are rejected.
use super::*;
use crate::bytecode::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
/// Snapshot validation result carrying a diagnostic for unsupported or inconsistent compiled data.
type Result<T> = std::result::Result<T, String>;
const VERSION: u32 = 19;
/// Serde view preserving source-to-program sharing for a trusted compiled workbook plan.
#[derive(Serialize, Deserialize)]
pub struct Plan<'a> {
    version: u32,
    identity: Cow<'a, SourceIdentity>,
    programs: Vec<Program<'a>>,
    sources: Vec<Source<'a>>,
    formulas: Vec<(usize, CellAddress, usize)>,
    names: Vec<usize>,
}
/// Shared bytecode template and serialized call bindings.
#[derive(Serialize, Deserialize)]
struct Program<'a> {
    typing: Cow<'a, crate::typing::FormulaTyping>,
    template: Cow<'a, Template>,
    calls: Vec<CallDto<'a>>,
}
/// Call metadata using portable builtin spelling instead of internal slot numbers.
#[derive(Serialize, Deserialize)]
struct CallDto<'a> {
    source_offset: u32,
    resolved_name: Option<Cow<'a, str>>,
    builtin: Option<Cow<'a, str>>,
}
/// One source's program index, concrete references, and conservative dependency guards.
#[derive(Serialize, Deserialize)]
struct Source<'a> {
    source: Cow<'a, str>,
    program: usize,
    references: Cow<'a, ReferenceBindings>,
    potential_references: Cow<'a, [PotentialReference]>,
    names: Cow<'a, [String]>,
    qualified_names: Cow<'a, [PotentialQualifiedName]>,
    opaque_functions: Cow<'a, [String]>,
    dynamic_references: bool,
}
impl<'a> Plan<'a> {
    /// Borrow a supported default-policy plan, rejecting extensions and preparation failures.
    pub fn capture(plan: &'a CompiledWorkbook) -> Result<Self> {
        if plan.policy != EvaluationPolicy::default() {
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
            if formula.policy != EvaluationPolicy::default()
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
                output.programs.push(Program {
                    typing: Cow::Borrowed(&compiled.typing),
                    template: Cow::Borrowed(&compiled.template),
                    calls,
                });
                programs.insert(program_key, id);
                id
            };
            let potential = &formula.potential;
            let id = output.sources.len();
            output.sources.push(Source {
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
    pub fn into_owned(self) -> Plan<'static> {
        Plan {
            version: self.version,
            identity: Cow::Owned(self.identity.into_owned()),
            programs: self
                .programs
                .into_iter()
                .map(|p| Program {
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
                .map(|s| Source {
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
                        let mut reader = Reader::new(
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
                        policy: EvaluationPolicy::default(),
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
            policy: EvaluationPolicy::default(),
            formulas,
            names,
        })
    }
}
/// Validate instruction structure, operand bounds, and call descriptor indices.
pub(super) fn validate_template(t: &Template, calls: usize) -> Result<()> {
    // Loading quotas belong to SnapshotLimits. This validator checks only encoded
    // instruction/index structure; no independent workbook-size quota is hidden here.
    if t.code.is_empty() {
        return Err("snapshot empty template".into());
    }
    let mut previous = None;
    if t.aggregations.len() > t.code.len() {
        return Err("snapshot aggregation count".into());
    }
    for entry in &t.aggregations {
        if !entry.plan.valid()
            || previous.is_some_and(|p| entry.start <= p)
            || t.code
                .get(entry.start)
                .is_none_or(|i| i.opcode != Opcode::Enter)
        {
            return Err("snapshot aggregation plan".into());
        }
        if entry.start != 0
            && t.code
                .get(entry.start - 1)
                .is_none_or(|i| i.opcode != Opcode::Argument || i.a != 2)
        {
            return Err("snapshot aggregation argument role".into());
        }
        previous = Some(entry.start);
    }
    let target = |n: u32| {
        if (n as usize) < t.code.len() {
            Ok(())
        } else {
            Err("snapshot jump target".to_owned())
        }
    };
    for (ip, i) in t.code.iter().enumerate() {
        if i.flags != 0 || i.reserved != 0 {
            return Err("snapshot instruction flags".into());
        }
        match i.opcode {
            Opcode::Text | Opcode::Error | Opcode::Name | Opcode::QualifiedName => {
                let end = (i.a as usize)
                    .checked_add(i.b as usize)
                    .ok_or("snapshot operand overflow")?;
                let bytes = t
                    .operands
                    .get(i.a as usize..end)
                    .ok_or("snapshot operand bounds")?;
                if i.opcode == Opcode::QualifiedName {
                    let mut reader = Reader::new(bytes);
                    reader.string()?;
                    std::str::from_utf8(reader.0).map_err(|_| "snapshot qualified name UTF8")?;
                } else {
                    std::str::from_utf8(bytes).map_err(|_| "snapshot UTF8 operand")?;
                }
            }
            Opcode::Reference if i.a >= t.reference_slots => {
                return Err("snapshot reference slot".into());
            }
            Opcode::Invoke => {
                if i.a as usize >= calls {
                    return Err("snapshot call index/arity".into());
                }
                target(i.c)?;
                if i.c as usize <= ip {
                    return Err("snapshot backwards invocation".into());
                }
                let jump = t.code.get(ip + 1).ok_or("snapshot invoke jump")?;
                if jump.opcode != Opcode::Jump || jump.c != i.c {
                    return Err("snapshot invoke continuation".into());
                }
                let mut descriptor = ip + 2;
                for ordinal in 0..i.b {
                    let argument = t
                        .code
                        .get(descriptor)
                        .ok_or("snapshot argument descriptor")?;
                    if argument.opcode != Opcode::Argument
                        || argument.a != ordinal
                        || argument.c as usize <= descriptor
                        || argument.c > i.c
                    {
                        return Err("snapshot argument chain".into());
                    }
                    descriptor = argument.c as usize;
                }
                if descriptor != i.c as usize {
                    return Err("snapshot argument count".into());
                }
            }
            Opcode::Argument | Opcode::Jump => {
                target(i.c)?;
                if i.c as usize <= ip {
                    return Err("snapshot backwards jump".into());
                }
            }
            Opcode::Unary if i.a > 4 => return Err("snapshot unary".into()),
            Opcode::Binary if i.a > 14 => return Err("snapshot binary".into()),
            _ => {}
        }
    }
    Ok(())
}
/// Bounds-checked reader for serialized little-endian reference fields.
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self(b)
    }
    fn word(&mut self) -> Result<u32> {
        let b = self.0.get(..4).ok_or("snapshot truncated reference")?;
        // SAFETY: The local get(..4) succeeds before conversion and advancement, proving four bytes are present.
        let n = u32::from_le_bytes(b.try_into().unwrap());
        // SAFETY: The local get(..4) succeeds before conversion and advancement, proving four bytes are present.
        self.0 = &self.0[4..];
        Ok(n)
    }
    fn boolean(&mut self) -> Result<bool> {
        match self.word()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("snapshot reference boolean".into()),
        }
    }
    fn string(&mut self) -> Result<&'a str> {
        let n = self.word()? as usize;
        let b = self.0.get(..n).ok_or("snapshot truncated string")?;
        // SAFETY: The local get(..n) succeeds before advancing the same byte slice by n.
        self.0 = &self.0[n..];
        std::str::from_utf8(b).map_err(|_| "snapshot reference UTF8".into())
    }
    fn optional(&mut self) -> Result<()> {
        if self.boolean()? {
            self.string()?;
        }
        Ok(())
    }
    fn cell(&mut self) -> Result<()> {
        if self.word()? >= poirot_model::MAX_ROWS || self.word()? >= poirot_model::MAX_COLUMNS {
            return Err("snapshot reference coordinate".into());
        }
        self.boolean()?;
        self.boolean()?;
        self.optional()
    }
}
/// Validate concrete reference encodings against a template's slot requirements.
pub(super) fn validate_references(r: &ReferenceBindings, slots: u32) -> Result<()> {
    if r.slots.len() != slots as usize {
        return Err("snapshot reference count/bytes".into());
    }
    for s in &r.slots {
        let end = (s.offset as usize)
            .checked_add(s.length as usize)
            .ok_or("snapshot reference overflow")?;
        let mut reader = Reader::new(
            r.data
                .get(s.offset as usize..end)
                .ok_or("snapshot reference bounds")?,
        );
        match s.kind {
            ReferenceKind::Cell => reader.cell()?,
            ReferenceKind::Row | ReferenceKind::Column => {
                let bound = if s.kind == ReferenceKind::Row {
                    poirot_model::MAX_ROWS
                } else {
                    poirot_model::MAX_COLUMNS
                };
                if reader.word()? >= bound {
                    return Err("snapshot reference index".into());
                }
                reader.boolean()?;
                reader.optional()?;
            }
            ReferenceKind::SheetSpan => {
                reader.string()?;
                reader.string()?;
                reader.cell()?;
                if reader.boolean()? {
                    reader.cell()?;
                }
            }
        }
        if !reader.0.is_empty() {
            return Err("snapshot reference trailing bytes".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Workbook {
        let mut w = Workbook::new();
        let s = w.add_sheet("S").unwrap();
        for (col, formula) in ["A2+1", "A3+1", "A2+1"].into_iter().enumerate() {
            s.set_formula(
                CellAddress {
                    row: 0,
                    col: col as u32,
                },
                formula,
            )
            .unwrap();
        }
        w
    }

    #[test]
    fn random_array_and_trim_current_plans_roundtrip_without_reparsing() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet
            .set_formula("A1".parse().unwrap(), "RANDARRAY(2,3)")
            .unwrap();
        sheet
            .set_formula("G1".parse().unwrap(), "TRIMRANGE(A1:D5)")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("K1".parse().unwrap(), "MUNIT(2)")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(
                "P1".parse().unwrap(),
                "NETWORKDAYS.INTL(45292,45298,1,A1:A10)",
            )
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("T1".parse().unwrap(), "IMSUM(IMCONJUGATE({\"i\";\"2i\"}))")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("W1".parse().unwrap(), "BIN2HEX({1;10})")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("Y1".parse().unwrap(), "CONVERT({1;2},\"byte\",\"bit\")")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AA1".parse().unwrap(), "BESSELI({0;1},0)")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AD1".parse().unwrap(), "ERFC.PRECISE({0;1})")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(
                "AG1".parse().unwrap(),
                "XIRR({-100;110},{1000;1365},{0.05;0.2})",
            )
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AJ1".parse().unwrap(), "FVSCHEDULE(100,{0.1;0.2})")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AM1".parse().unwrap(), "COUPDAYS({45000;45001},46000,2,1)")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(
                "AN1".parse().unwrap(),
                "AMORLINC({1000;2000},45000,45090,100,0,0.2)",
            )
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AO1".parse().unwrap(), "BETA.DIST({0.25;0.5},2,2,TRUE)")
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AP1".parse().unwrap(), "BETA.INV({0.25;0.5},2,1)")
            .unwrap();
        for (cell, formula) in [
            ("AQ1", "MODE.MULT({7;2;7;2})"),
            ("AR1", "RANK.AVG({2;7},{2;2;7},{0,1})"),
            ("AT1", "T.TEST({0;2},{2;3;4},{1;2},{2,3})"),
            ("AV1", "PROB({0;1},{0.25;0.75},{0;1})"),
            ("AW1", "Z.TEST({0;1;2},{0;1},{1,2})"),
            ("AZ1", "LINEST({1;2;2},{0;1;2},TRUE,TRUE)"),
            ("BC1", "LOGEST({3;6;12},{0;1;2})"),
            ("BF1", "GROWTH({3;6;12},{0;1;2},{3;4})"),
            ("BI1", "FORECAST.ETS({4;5},{1;2;3},{1;2;3},0)"),
            ("BL1", "FORECAST.ETS.CONFINT(4,{1;2;3},{1;2;3},,0)"),
            ("BO1", "FORECAST.ETS.SEASONALITY({1;2;3},{1;2;3})"),
            ("BR1", "FORECAST.ETS.STAT({1;2;3},{1;2;3},{7,8},0)"),
            ("C1", "BETADIST(3,1,1,2,6)"),
            ("C2", "BETAINV(0.25,1,1,2,6)"),
            ("C3", "BINOMDIST(1,4,0.5,FALSE)"),
            ("C4", "CHIDIST(2*LN(2),2)"),
            ("C5", "CHIINV(0.5,2)"),
            ("C6", "CHITEST({2,2;2,2},{2,2;2,2})"),
            ("C7", "CONFIDENCE(0.05,1,1)"),
            ("C8", "CRITBINOM(4,0.5,5/16)"),
            ("C9", "EXPONDIST(2,0.5,TRUE)"),
            ("C10", "FDIST(3,2,2)"),
            ("C11", "FINV(0.25,2,2)"),
            ("C12", "FTEST({0;1;2},{2;3;4})"),
            ("C13", "GAMMADIST(2,1,2,TRUE)"),
            ("C14", "GAMMAINV(0.5,1,2)"),
            ("C15", "HYPGEOMDIST(1,2,2,4)"),
            ("C16", "LOGINV(0.5,0,1)"),
            ("C17", "LOGNORMDIST(1,0,1)"),
            ("C18", "NEGBINOMDIST(1,2,0.5)"),
            ("C19", "QUARTILE({0;4},1)"),
            ("C20", "TINV(0.5,1)"),
            ("C21", "TTEST({0;1;2},{0;1;2},2,2)"),
            ("C22", "WEIBULL(2,1,2,TRUE)"),
            ("C23", "ZTEST({0;1;2},1)"),
        ] {
            book.sheet_mut(0)
                .unwrap()
                .set_formula(cell.parse().unwrap(), formula)
                .unwrap();
        }
        let plan = FormulaCompiler::new().compile_workbook(&book, &EvaluationPolicy::default());
        let dto = Plan::capture(&plan).unwrap().into_owned();
        let restored = dto.restore().unwrap();
        assert!(restored.matches(&book, &EvaluationPolicy::default()));
        for (key, original) in &plan.formulas {
            let PreparedFormula::Ready(original) = original.as_ref() else {
                panic!()
            };
            let PreparedFormula::Ready(actual) = restored.formulas.get(key).unwrap().as_ref()
            else {
                panic!()
            };
            assert_eq!(actual.bytecode.typing, original.bytecode.typing);
        }
        let mut old = Plan::capture(&plan).unwrap();
        old.version = VERSION - 1;
        assert_eq!(
            old.restore().unwrap_err(),
            "unsupported compiled snapshot version"
        );
    }
    #[test]
    fn contextual_aggregation_metadata_roundtrips_and_rejects_malformed_plans() {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula(
                "A1".parse().unwrap(),
                "GROUPBY({1;1},{2;8},HSTACK(SUM,AVERAGE),0,0)",
            )
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(
                "AG1".parse().unwrap(),
                "XIRR({-100;110},{1000;1365},{0.05;0.2})",
            )
            .unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_formula("AJ1".parse().unwrap(), "FVSCHEDULE(100,{0.1;0.2})")
            .unwrap();
        let plan = FormulaCompiler::new().compile_workbook(&book, &EvaluationPolicy::default());
        let dto = Plan::capture(&plan).unwrap().into_owned();
        let restored = dto.restore().unwrap();
        assert!(restored.matches(&book, &EvaluationPolicy::default()));
        for mutation in 0..5 {
            let mut dto = Plan::capture(&plan).unwrap().into_owned();
            let entries = &mut dto.programs[0].template.to_mut().aggregations;
            match mutation {
                0 => entries[0].plan.columns = 3,
                1 => entries[0].start = usize::MAX,
                2 => entries[0].plan.rows = 2,
                3 => entries[0].plan.members[0].name.clear(),
                _ => entries.push(entries[0].clone()),
            }
            assert!(dto.restore().is_err());
        }
    }
    #[test]
    fn snapshot_restores_source_and_program_sharing_without_lazy_trees() {
        let w = fixture();
        let plan = FormulaCompiler::new().compile_workbook(&w, &EvaluationPolicy::default());
        let dto = Plan::capture(&plan).unwrap();
        assert_eq!(dto.counts(), (1, 2, 3));
        assert!(matches!(dto.identity, Cow::Borrowed(_)));
        assert!(matches!(dto.sources[0].source, Cow::Borrowed(_)));
        assert!(matches!(dto.sources[0].references, Cow::Borrowed(_)));
        assert!(matches!(
            dto.sources[0].potential_references,
            Cow::Borrowed(_)
        ));
        let original = plan
            .formulas
            .get(&(0, CellAddress { row: 0, col: 0 }))
            .unwrap();
        let PreparedFormula::Ready(original) = original.as_ref() else {
            panic!()
        };
        let Cow::Borrowed(template) = &dto.programs[0].template else {
            panic!("template copied")
        };
        assert!(std::ptr::eq(*template, &original.bytecode.template));
        let Cow::Borrowed(references) = &dto.sources[0].references else {
            panic!()
        };
        assert!(std::ptr::eq(*references, &original.bytecode.references));
        let restored = dto.restore().unwrap();
        assert!(restored.matches(&w, &EvaluationPolicy::default()));
        let mut ready = Vec::new();
        for col in 0..3 {
            let p = restored
                .formulas
                .get(&(0, CellAddress { row: 0, col }))
                .unwrap();
            let PreparedFormula::Ready(c) = p.as_ref() else {
                panic!()
            };
            ready.push(c);
        }
        assert!(Arc::ptr_eq(ready[0], ready[2]));
        assert!(Arc::ptr_eq(
            &ready[0].bytecode.executable,
            &ready[1].bytecode.executable
        ));
    }
    #[test]
    fn snapshot_rejects_identity_and_bytecode_corruption() {
        let w = fixture();
        let plan = FormulaCompiler::new().compile_workbook(&w, &EvaluationPolicy::default());
        let mut old = Plan::capture(&plan).unwrap();
        old.version = VERSION - 1;
        assert_eq!(
            old.restore().unwrap_err(),
            "unsupported compiled snapshot version"
        );
        let mut dto = Plan::capture(&plan).unwrap();
        dto.sources[0].program = usize::MAX;
        assert!(dto.restore().is_err());
        let mut dto = Plan::capture(&plan).unwrap();
        dto.sources[0].references.to_mut().slots[0].length = u32::MAX;
        assert!(dto.restore().is_err());
        let mut dto = Plan::capture(&plan).unwrap();
        dto.identity.to_mut().formulas[0].2 = "wrong".into();
        assert!(dto.restore().is_err());
        let mut dto = Plan::capture(&plan).unwrap();
        dto.programs[0].template.to_mut().code.push(Instruction {
            opcode: Opcode::Jump,
            flags: 0,
            reserved: 0,
            a: 0,
            b: 0,
            c: u32::MAX,
        });
        assert!(dto.restore().is_err());
    }
    #[test]
    fn snapshot_rejects_inconsistent_scalar_geometry() {
        let workbook = fixture();
        let plan = FormulaCompiler::new().compile_workbook(&workbook, &EvaluationPolicy::default());
        let mut dto = Plan::capture(&plan).unwrap();
        let mut entries = dto.programs[0].typing.expressions().to_vec();
        entries.first_mut().unwrap().properties.shape = poirot_model::ShapeBounds::new(2, 1);
        // FormulaTyping encodes one sequence field. Forge that field through the
        // serialization boundary instead of adding an unchecked mutation API.
        *dto.programs[0].typing.to_mut() =
            postcard::from_bytes(&postcard::to_allocvec(&entries).unwrap()).unwrap();
        assert_eq!(dto.restore().unwrap_err(), "snapshot typing boundaries");
    }
    #[test]
    fn snapshot_rejects_unsupported_preparation_and_policy() {
        let mut w = Workbook::new();
        w.add_sheet("S")
            .unwrap()
            .set_formula(CellAddress { row: 0, col: 0 }, "1+")
            .unwrap();
        assert!(
            Plan::capture(
                &FormulaCompiler::new().compile_workbook(&w, &EvaluationPolicy::default())
            )
            .is_err()
        );
        let w = fixture();
        let mut plan = FormulaCompiler::new().compile_workbook(&w, &EvaluationPolicy::default());
        plan.policy.registry.builtin_contract_version += 1;
        assert!(Plan::capture(&plan).is_err());
    }
}
