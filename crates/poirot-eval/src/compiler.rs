//! Formula compilation and program interning, separate from evaluated-result caching.
#[cfg(test)]
use crate::bound;
use crate::{EvaluationTarget, registry};
use poirot_formula::{FormulaParsePolicy, ParseError, ParsedFormula, parse_formula_with_policy};
use poirot_model::{CellAddress, Workbook};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Actual extension contracts include bound IDs, signatures, capabilities,
/// volatility and the ordered external-name namespace. Builtins are immutable.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistryIdentity {
    /// Version of the immutable builtin binding contract.
    pub builtin_contract_version: u64,
    /// Complete extension contract identity, when extensions are installed.
    pub extensions: Option<crate::extensions::ExtensionIdentity>,
}
impl Default for RegistryIdentity {
    fn default() -> Self {
        Self {
            builtin_contract_version: 1,
            extensions: None,
        }
    }
}
impl RegistryIdentity {
    /// Capture builtins and the supplied registry's exact extension contracts.
    pub fn with_extensions(registry: &crate::extensions::ExtensionRegistry) -> Self {
        Self {
            extensions: Some(registry.identity()),
            ..Self::default()
        }
    }
}
/// Preparation-time parser limits and function-registry identity.
/// A workbook plan must be recompiled when this policy changes. Execution-time
/// clock, text, formatting, and builtin choices belong to [`crate::RunConfiguration`].
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct EvaluationPolicy {
    /// Parsing grammar and preparation limits.
    pub parse: FormulaParsePolicy,
    /// Function namespace identity required during binding.
    pub registry: RegistryIdentity,
}
impl EvaluationPolicy {
    /// Select parser limits and capture the registry required for extension binding.
    pub fn with_extensions(
        parse: FormulaParsePolicy,
        registry: &crate::extensions::ExtensionRegistry,
    ) -> Self {
        Self {
            parse,
            registry: RegistryIdentity::with_extensions(registry),
        }
    }
}
/// Cumulative preparation counters for one [`FormulaCompiler`]. Reusing prepared syntax
/// reduces parse/bind attempts; these counters do not count formula evaluations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompilerUsage {
    /// Formula sources sent to the parser.
    pub parse_attempts: u64,
    /// Parsed sources sent to handler binding.
    pub bind_attempts: u64,
    /// Exact source-and-policy cache hits.
    pub reused_formulas: u64,
}
/// Conservative guards only. These are never reported as consumed runtime reads.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PotentialReference {
    /// Syntactically present cell reference, possibly in an unselected branch.
    Cell(poirot_formula::CellReference),
    /// Ordered sheet endpoints and cell anchors retained until workbook resolution.
    SheetSpan(poirot_formula::SheetSpanReference),
    /// Syntactically present whole-column reference.
    Column {
        /// Zero-based column index.
        column: u32,
        /// Absolute translation flag.
        absolute: bool,
        /// Optional sheet qualifier.
        sheet: Option<String>,
    },
    /// Syntactically present whole-row reference.
    Row {
        /// Zero-based row index.
        row: u32,
        /// Absolute translation flag.
        absolute: bool,
        /// Optional sheet qualifier.
        sheet: Option<String>,
    },
}
/// Sheet-qualified name guard found during preparation.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PotentialQualifiedName {
    /// Sheet qualifier as written in the formula.
    pub sheet: String,
    /// Defined-name spelling.
    pub name: String,
}
/// Conservative references and names visible in formula syntax before execution.
/// Unselected branches may appear here; use consumed-read events for actual dependencies.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PotentialDependencies {
    /// References found in syntax, independent of branch selection.
    pub references: Vec<PotentialReference>,
    /// Unqualified names found in syntax.
    pub names: Vec<String>,
    /// Qualified names found in syntax.
    pub qualified_names: Vec<PotentialQualifiedName>,
    /// Unresolved call spellings whose behavior cannot be statically described.
    pub opaque_functions: Vec<String>,
    /// Whether a handler can choose references at runtime.
    pub dynamic_references: bool,
    /// Extension bindings captured in resolved bytecode calls.
    pub extensions: Vec<crate::extensions::ExtensionBinding>,
}
/// Static bounds for spill discovery, computed while syntax is temporary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SpillMetadata {
    /// Statically known row and column counts, when available.
    pub shape: Option<(u64, u64)>,
    /// Whether runtime discovery may extend beyond a fixed shape.
    pub unbounded: bool,
}
/// Prepared source, reusable bytecode, and possible dependencies shared by plans.
/// Syntax inspection returns an independently owned parse view. Contains no evaluated result;
/// name/reference resolution still needs a workbook run.
#[derive(Clone, Debug)]
pub struct CompiledFormula {
    source: Arc<str>,
    /// Static spill discovery bounds captured while syntax is available.
    pub(crate) spill: SpillMetadata,
    bytecode: crate::bytecode::Compiled,
    potential: PotentialDependencies,
    policy: EvaluationPolicy,
}
impl CompiledFormula {
    /// Parse an independently owned inspection view on demand. Execution retains no tree.
    /// This diagnostic work is not included in [`CompilerUsage`].
    pub fn parsed(&self) -> Result<ParsedFormula, ParseError> {
        parse_formula_with_policy(&self.source, self.policy.parse)
    }
    #[cfg(test)]
    fn executable(&self) -> crate::bound::Expression {
        crate::bound::bind_with_executable(
            self.parsed().unwrap().expression(),
            &self.bytecode.executable,
        )
    }
    /// The immutable executable used for every successful preparation.
    pub fn bytecode(&self) -> &crate::bytecode::Compiled {
        &self.bytecode
    }
    /// Inspect conservative syntax guards; these are not consumed-read traces.
    pub fn potential_dependencies(&self) -> &PotentialDependencies {
        &self.potential
    }
}
/// A preparation failure is deferred until its cell or name is actually evaluated.
#[derive(Clone, Debug)]
pub enum PreparedFormula {
    /// Successfully compiled formula shared across matching sources and policies.
    Ready(Arc<CompiledFormula>),
    /// Deferred parse failure, raised only if execution reaches this source.
    ParseFailure {
        /// Exact submitted formula text.
        source: Arc<str>,
        /// Preparation policy under which parsing failed.
        policy: EvaluationPolicy,
        /// Parser diagnostic.
        error: ParseError,
    },
    /// Bytecode preparation failed. This operational failure is deferred until reached.
    CompilationFailure {
        /// Exact submitted formula text.
        source: Arc<str>,
        /// Policy used while lowering.
        policy: EvaluationPolicy,
        /// Structural or resource diagnostic.
        error: crate::bytecode::CompileError,
    },
    /// Preparation lacked the registry specified by its policy.
    RegistryMismatch {
        /// Exact submitted formula text.
        source: Arc<str>,
        /// Policy identifying the required registry.
        policy: EvaluationPolicy,
    },
}
impl PreparedFormula {
    /// Return the exact source text for either a program or a deferred failure.
    pub fn source(&self) -> &str {
        match self {
            Self::Ready(formula) => &formula.source,
            Self::ParseFailure { source, .. }
            | Self::CompilationFailure { source, .. }
            | Self::RegistryMismatch { source, .. } => source,
        }
    }
    /// Return the policy captured during preparation.
    pub fn policy(&self) -> &EvaluationPolicy {
        match self {
            Self::Ready(formula) => &formula.policy,
            Self::ParseFailure { policy, .. }
            | Self::CompilationFailure { policy, .. }
            | Self::RegistryMismatch { policy, .. } => policy,
        }
    }
}
/// Exact formula, name, sheet, and visibility metadata guarding workbook plan reuse.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
struct SourceIdentity {
    sheets: Vec<String>,
    visibility: Vec<(usize, u32, poirot_model::RowVisibility)>,
    formulas: Vec<(usize, CellAddress, String)>,
    names: Vec<(String, String, Option<usize>)>,
}
impl SourceIdentity {
    fn capture(workbook: &Workbook) -> Self {
        let mut formulas = Vec::new();
        for (sheet, worksheet) in workbook.sheets().iter().enumerate() {
            for (address, cell) in worksheet.cells() {
                if let Some(source) = &cell.formula {
                    formulas.push((sheet, *address, source.clone()));
                }
            }
        }
        formulas.sort_by_key(|(sheet, at, _)| (*sheet, at.row, at.col));
        Self {
            sheets: workbook
                .sheets()
                .iter()
                .map(|s| s.name().to_owned())
                .collect(),
            formulas,
            visibility: workbook
                .sheets()
                .iter()
                .enumerate()
                .flat_map(|(sheet, worksheet)| {
                    worksheet
                        .row_visibility_entries()
                        .map(move |(row, flags)| (sheet, row, flags))
                })
                .collect(),
            names: workbook
                .named_ranges
                .iter()
                .map(|n| (n.name.clone(), n.formula.clone(), n.sheet_scope))
                .collect(),
        }
    }
}
/// In-process lookup hint. Exact metadata equality is always required for validity;
/// this digest is not a persistent identity or collision-proof authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceFingerprint(
    /// Process-local digest; exact source metadata must still be compared.
    pub u64,
);
fn fingerprint(identity: &SourceIdentity) -> SourceFingerprint {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hash);
    SourceFingerprint(hash.finish())
}
/// Immutable compiled formula index and owned source topology; contains no cell results.
/// Exact equality, rather than a hash or generation,
/// protects reuse even when model fields are changed directly. Row visibility is
/// source metadata; ordinary values/styles are excluded.
#[derive(Clone, Debug)]
pub struct CompiledWorkbook {
    identity: SourceIdentity,
    fingerprint: SourceFingerprint,
    policy: EvaluationPolicy,
    formulas: HashMap<(usize, CellAddress), Arc<PreparedFormula>>,
    names: Vec<Arc<PreparedFormula>>,
}
impl CompiledWorkbook {
    /// Check exact source topology and preparation policy before reusing this plan.
    pub fn matches(&self, workbook: &Workbook, policy: &EvaluationPolicy) -> bool {
        let identity = SourceIdentity::capture(workbook);
        &self.policy == policy
            && self.fingerprint == fingerprint(&identity)
            && self.identity == identity
    }
    /// Borrow the policy captured when this plan was compiled.
    pub fn policy(&self) -> &EvaluationPolicy {
        &self.policy
    }
    /// Return the process-local source lookup hint.
    pub fn source_fingerprint(&self) -> SourceFingerprint {
        self.fingerprint
    }
    /// Find the prepared formula assigned to a stored formula cell.
    pub fn formula(&self, target: EvaluationTarget) -> Option<&Arc<PreparedFormula>> {
        self.formulas.get(&(target.sheet, target.address))
    }
    /// Find a prepared defined-name body by workbook name index.
    pub fn defined_name(&self, index: usize) -> Option<&Arc<PreparedFormula>> {
        self.names.get(index)
    }
}
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
    pool: Vec<(EvaluationPolicy, HashMap<String, Arc<PreparedFormula>>)>,
    usage: CompilerUsage,
    templates: Vec<TemplatePool>,
    template_hits: u64,
}
/// Candidate buckets for lexical rebinding and exact executable interning.
#[derive(Default)]
// Hashes select candidate buckets only. Sharing still requires normalized-source
// equality for rebinding, or full executable equality for structural interning.
struct TemplatePool {
    classified: HashMap<u64, Vec<Arc<CompiledFormula>>>,
    structural: HashMap<u64, Vec<Arc<crate::bytecode::Executable>>>,
}
fn executable_hash(program: &crate::bytecode::Executable) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    program.template.operands.hash(&mut hash);
    program.template.reference_slots.hash(&mut hash);
    for instruction in &program.template.code {
        (
            instruction.opcode as u8,
            instruction.flags,
            instruction.reserved,
            instruction.a,
            instruction.b,
            instruction.c,
        )
            .hash(&mut hash);
    }
    hash.finish()
}
fn classified_cell_count(source: &str, policy: &EvaluationPolicy) -> Option<usize> {
    let mut count = 0;
    for token in poirot_formula::classified::Tokens::new(source) {
        if let poirot_formula::classified::TokenKind::Cell(cell) = token.ok()?.kind {
            if cell.row >= policy.parse.max_rows || cell.column >= policy.parse.max_columns {
                return None;
            }
            count += 1;
        }
    }
    Some(count)
}
// Keep formulas that rebound() must reject out of its candidate buckets. Otherwise
// a copied range-shaped formula adds another failed candidate on every compile.
fn reusable_template(formula: &CompiledFormula) -> bool {
    classified_cell_count(&formula.source, &formula.policy)
        .is_some_and(|count| rebound(formula, &formula.source, count).is_some())
}
fn rebound(exemplar: &CompiledFormula, source: &str, cell_count: usize) -> Option<CompiledFormula> {
    let program = &exemplar.bytecode;
    // Rebinding point cells preserves shape. Range endpoints can change a
    // producer's dimensions, so those bounds must be compiled again.
    if exemplar.spill.shape.is_some()
        && poirot_formula::classified::Tokens::new(source)
            .any(|token| token.is_ok_and(|token| token.raw == ":"))
    {
        return None;
    }
    if program.references.slots.len() != cell_count
        || program
            .references
            .slots
            .iter()
            .any(|slot| slot.kind != crate::bytecode::ReferenceKind::Cell)
    {
        return None;
    }
    if exemplar.potential.references.len() != cell_count
        || exemplar
            .potential
            .references
            .iter()
            .any(|reference| !matches!(reference, PotentialReference::Cell(_)))
    {
        return None;
    }
    let mut references = program.references.clone();
    let mut potential = exemplar.potential.clone();
    let mut index = 0;
    for token in poirot_formula::classified::Tokens::new(source) {
        let poirot_formula::classified::TokenKind::Cell(cell) = token.ok()?.kind else {
            continue;
        };
        let slot = references.slots.get(index)?;
        for (word, value) in [
            cell.row,
            cell.column,
            cell.row_absolute.into(),
            cell.column_absolute.into(),
        ]
        .into_iter()
        .enumerate()
        {
            let offset = (slot.offset as usize).checked_add(word * 4)?;
            // SAFETY: The checked destination range has length four, matching the u32 byte array.
            references
                .data
                .get_mut(offset..offset.checked_add(4)?)?
                .copy_from_slice(&value.to_le_bytes());
        }
        let PotentialReference::Cell(reference) = potential
            .references
            .get_mut(cell_count.checked_sub(1)?.checked_sub(index)?)?
        else {
            return None;
        };
        reference.row = cell.row;
        reference.column = cell.column;
        reference.row_absolute = cell.row_absolute;
        reference.column_absolute = cell.column_absolute;
        index += 1;
    }
    Some(CompiledFormula {
        source: Arc::from(source),
        spill: exemplar.spill,
        bytecode: crate::bytecode::Compiled {
            executable: Arc::clone(&program.executable),
            references,
        },
        potential,
        policy: exemplar.policy.clone(),
    })
}
impl FormulaCompiler {
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
        policy: &EvaluationPolicy,
    ) -> Arc<PreparedFormula> {
        self.compile_formula_with_registry(source, policy, None)
    }
    /// Prepare a source against the supplied registry's exact namespace.
    pub fn compile_formula_with_extensions(
        &mut self,
        source: &str,
        policy: &EvaluationPolicy,
        registry: &crate::extensions::ExtensionRegistry,
    ) -> Arc<PreparedFormula> {
        self.compile_formula_with_registry(source, policy, Some(registry))
    }
    fn compile_formula_with_registry(
        &mut self,
        source: &str,
        policy: &EvaluationPolicy,
        registry: Option<&crate::extensions::ExtensionRegistry>,
    ) -> Arc<PreparedFormula> {
        if policy.registry.extensions
            != registry.map(crate::extensions::ExtensionRegistry::identity)
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
                error: crate::bytecode::CompileError::InvalidReference,
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
        let normalized = poirot_formula::classified::normalized_hash(source).ok();
        if let Some(bucket) =
            // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
            normalized.and_then(|hash| self.templates[pool_index].classified.get(&hash))
        {
            for exemplar in bucket {
                if poirot_formula::classified::normalized_equal(source, &exemplar.source)
                    == Ok(true)
                    && let Some(cell_count) = classified_cell_count(source, policy)
                    && let Some(formula) = rebound(exemplar, source, cell_count)
                {
                    self.template_hits += 1;
                    let prepared = Arc::new(PreparedFormula::Ready(Arc::new(formula)));
                    // SAFETY: The entry check requires templates.len() == pool.len(); position returns a valid pool index, or this function pushes one element into both vectors before indexing.
                    self.pool[pool_index]
                        .1
                        .insert(source.to_owned(), Arc::clone(&prepared));
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
                let mut bytecode =
                    match crate::bytecode::compile_with_registry(parsed.expression(), registry) {
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
                                .insert(source.to_owned(), Arc::clone(&prepared));
                            return prepared;
                        }
                    };
                let Some(potential) = potential(parsed.expression(), &bytecode) else {
                    return Arc::new(PreparedFormula::CompilationFailure {
                        source: Arc::from(source),
                        policy: policy.clone(),
                        error: crate::bytecode::CompileError::InvalidReference,
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
                    spill: SpillMetadata {
                        shape: crate::spill_publication::spill_shape(parsed.expression()),
                        unbounded: crate::spill_publication::unbounded_producer(
                            parsed.expression(),
                        ),
                    },
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
            .insert(source.to_owned(), Arc::clone(&prepared));
        prepared
    }
    /// Eagerly prepare workbook formulas and defined names under one policy.
    pub fn compile_workbook(
        &mut self,
        workbook: &Workbook,
        policy: &EvaluationPolicy,
    ) -> CompiledWorkbook {
        self.compile_workbook_with_registry(workbook, policy, None)
    }
    /// Eagerly prepare workbook sources using an explicit extension registry.
    pub fn compile_workbook_with_extensions(
        &mut self,
        workbook: &Workbook,
        policy: &EvaluationPolicy,
        registry: &crate::extensions::ExtensionRegistry,
    ) -> CompiledWorkbook {
        self.compile_workbook_with_registry(workbook, policy, Some(registry))
    }
    fn compile_workbook_with_registry(
        &mut self,
        workbook: &Workbook,
        policy: &EvaluationPolicy,
        registry: Option<&crate::extensions::ExtensionRegistry>,
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
fn potential(
    expression: &poirot_formula::Expr,
    executable: &crate::bytecode::Compiled,
) -> Option<PotentialDependencies> {
    use poirot_formula::Expr;
    let mut result = PotentialDependencies::default();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match expression {
            Expr::Cell(reference) => result
                .references
                .push(PotentialReference::Cell(reference.clone())),
            Expr::Column {
                column,
                absolute,
                sheet,
            } => result.references.push(PotentialReference::Column {
                column: *column,
                absolute: *absolute,
                sheet: sheet.clone(),
            }),
            Expr::Row {
                row,
                absolute,
                sheet,
            } => result.references.push(PotentialReference::Row {
                row: *row,
                absolute: *absolute,
                sheet: sheet.clone(),
            }),
            Expr::SheetSpan(span) => result
                .references
                .push(PotentialReference::SheetSpan(span.clone())),
            Expr::Name(name) => result.names.push(name.clone()),
            Expr::QualifiedName { sheet, name } => {
                result.qualified_names.push(PotentialQualifiedName {
                    sheet: sheet.clone(),
                    name: name.clone(),
                });
            }
            Expr::Function { name, args } => {
                match registry::resolve_function(name) {
                    None => {}
                    Some(descriptor)
                        if descriptor
                            .dependencies
                            .contains(registry::DependencyPolicy::REFERENCE_SELECTION) =>
                    {
                        result.dynamic_references = true
                    }
                    Some(_) => {}
                }
                pending.extend(args);
            }
            Expr::Unary { operand, .. } => pending.push(operand),
            Expr::Binary { left, right, .. } => {
                pending.push(left);
                pending.push(right);
            }
            Expr::Array(rows) => pending.extend(rows.iter().flatten()),
            _ => {}
        }
    }
    // Resolved call identities distinguish declared providers from opaque calls.
    for (index, call) in executable.calls.iter().enumerate() {
        match &call.target {
            crate::bytecode::CallTarget::Extension(binding) => {
                result.extensions.push(binding.clone())
            }
            crate::bytecode::CallTarget::Unresolved => {
                result
                    .opaque_functions
                    .push(executable.source_spelling(index).ok()?.to_owned());
            }
            _ => {}
        }
    }
    Some(result)
}

#[cfg(test)]
mod pool_tests {
    use super::*;

    #[test]
    fn bytecode_preparation_does_not_retain_eager_expression_trees() {
        let mut compiler = FormulaCompiler::new();
        let prepared = compiler.compile_formula("IFERROR(A1+B2,3)", &EvaluationPolicy::default());
        let PreparedFormula::Ready(formula) = prepared.as_ref() else {
            panic!("valid formula was not prepared")
        };
        assert!(!formula.bytecode().template.code.is_empty());
        assert_eq!(prepared.source(), "IFERROR(A1+B2,3)");
        // Reading exact source identity must not accidentally rehydrate syntax.
        assert_eq!(formula.parsed().unwrap().source(), prepared.source());
        assert!(matches!(
            formula.executable(),
            bound::Expression::Logical(_)
        ));
    }

    #[test]
    fn prepared_evaluation_uses_bytecode_without_materializing_trees() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        let at: CellAddress = "A1".parse().unwrap();
        sheet
            .set_formula(at, "IF(FALSE,1/0,2+3)", poirot_model::CellValue::Blank)
            .unwrap();
        let mut compiler = FormulaCompiler::new();
        let plan = compiler.compile_workbook(&workbook, &EvaluationPolicy::default());
        let prepared = plan
            .formula(EvaluationTarget {
                sheet: 0,
                address: at,
            })
            .unwrap();
        let PreparedFormula::Ready(_formula) = prepared.as_ref() else {
            panic!("valid formula")
        };
        let mut session =
            crate::Session::with_context(&workbook, crate::EvaluationInputs::default());
        session.plan = Some(&plan);
        assert_eq!(
            session.cell(0, at).unwrap(),
            poirot_model::CellValue::Number(5.)
        );
    }

    #[test]
    fn indexed_reuse_preserves_exact_source_and_survives_growth() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        let first = compiler.compile_formula("1+2", &policy);
        for number in 3..259 {
            compiler.compile_formula(&format!("1+{number}"), &policy);
        }
        assert!(Arc::ptr_eq(
            &first,
            &compiler.compile_formula("1+2", &policy)
        ));
        let spaced = compiler.compile_formula("1 + 2", &policy);
        assert!(!Arc::ptr_eq(&first, &spaced));
        assert_eq!(spaced.source(), "1 + 2");
        assert_eq!(compiler.usage().parse_attempts, 258);
        assert_eq!(compiler.usage().reused_formulas, 1);
    }

    #[test]
    fn policies_separate_successful_plans_and_cached_parse_failures() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        let mut limited = policy.clone();
        limited.parse.max_expressions = 1;
        let ready = compiler.compile_formula("1+2", &policy);
        let failed = compiler.compile_formula("1+2", &limited);
        assert!(matches!(ready.as_ref(), PreparedFormula::Ready(_)));
        assert!(matches!(
            failed.as_ref(),
            PreparedFormula::ParseFailure { .. }
        ));
        assert!(Arc::ptr_eq(
            &ready,
            &compiler.compile_formula("1+2", &policy)
        ));
        assert!(Arc::ptr_eq(
            &failed,
            &compiler.compile_formula("1+2", &limited)
        ));
        assert_eq!(
            compiler.usage(),
            &CompilerUsage {
                parse_attempts: 2,
                bind_attempts: 1,
                reused_formulas: 2,
            }
        );
    }

    #[test]
    fn registry_mismatch_cannot_poison_or_reuse_a_valid_pool() {
        let mut compiler = FormulaCompiler::new();
        let registry = crate::extensions::ExtensionRegistry::new();
        let policy = EvaluationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
        let mismatch = compiler.compile_formula("1+2", &policy);
        assert!(matches!(
            mismatch.as_ref(),
            PreparedFormula::RegistryMismatch { .. }
        ));
        let ready = compiler.compile_formula_with_extensions("1+2", &policy, &registry);
        assert!(matches!(ready.as_ref(), PreparedFormula::Ready(_)));
        assert!(matches!(
            compiler.compile_formula("1+2", &policy).as_ref(),
            PreparedFormula::RegistryMismatch { .. }
        ));
        assert!(Arc::ptr_eq(
            &ready,
            &compiler.compile_formula_with_extensions("1+2", &policy, &registry)
        ));
        assert_eq!(
            compiler.usage(),
            &CompilerUsage {
                parse_attempts: 1,
                bind_attempts: 1,
                reused_formulas: 1,
            }
        );
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;
    fn ready(formula: &PreparedFormula) -> &CompiledFormula {
        match formula {
            PreparedFormula::Ready(formula) => formula,
            _ => panic!("expected ready"),
        }
    }
    #[test]
    fn classified_hit_shares_complete_program_without_parsing_or_binding() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        let first = compiler.compile_formula("VLOOKUP(A1,'O''Brien'!$B$2:C3,2,FALSE)", &policy);
        let second = compiler.compile_formula("VLOOKUP(Z9,'O''Brien'!D4:$E$5,2,FALSE)", &policy);
        let (a, b) = (ready(&first), ready(&second));
        assert_eq!(compiler.usage().parse_attempts, 1);
        assert_eq!(compiler.usage().bind_attempts, 1);
        assert_eq!(compiler.template_hits(), 1);
        assert_eq!(compiler.unique_programs(), 1);
        assert!(Arc::ptr_eq(
            &a.bytecode().executable,
            &b.bytecode().executable
        ));
        assert_ne!(a.bytecode().references, b.bytecode().references);
        let parsed = parse_formula_with_policy(second.source(), policy.parse).unwrap();
        let expected_program = crate::bytecode::compile(parsed.expression()).unwrap();
        assert_eq!(
            b.potential,
            potential(parsed.expression(), &expected_program).unwrap()
        );
        assert_eq!(
            b.bytecode().references,
            crate::bytecode::compile(parsed.expression())
                .unwrap()
                .references
        );
        assert_eq!(b.parsed().unwrap().source(), second.source());
        assert_ne!(b.parsed().unwrap().source(), a.parsed().unwrap().source());
    }
    #[test]
    fn structural_interning_covers_unsupported_and_differently_spelled_sources() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        for source in ["SUM('One'!A:A)", "SUM('Two'!B:B)", "SUM ( 'Three'!$C:$C )"] {
            compiler.compile_formula(source, &policy);
        }
        assert_eq!(compiler.usage().parse_attempts, 3);
        assert_eq!(compiler.unique_programs(), 1);
        assert_eq!(compiler.template_hits(), 0);
    }
    #[test]
    fn normalized_hits_preserve_literals_call_roles_and_policy() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        for source in [
            "SUM(A1,1)",
            "SUM(B2,2)",
            "SUM(C3,Name)",
            "SUM(D4,Other)",
            "SUM(E5,1)+1",
            "sum(F6,1)",
            "LOG10(A1)",
            "A1(A2)",
            "SUM(A1,\"B2\")",
        ] {
            compiler.compile_formula(source, &policy);
        }
        assert_eq!(compiler.template_hits(), 0);
        let mut limited = policy.clone();
        limited.parse.max_rows = 2;
        compiler.compile_formula("SUM(A1,1)", &limited);
        assert!(matches!(
            compiler.compile_formula("SUM(A3,1)", &limited).as_ref(),
            PreparedFormula::ParseFailure { .. }
        ));
        assert_eq!(compiler.template_hits(), 0);
    }
    #[test]
    fn forced_normalized_bucket_collision_requires_complete_equality() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        let first = compiler.compile_formula("SUM(A1,1)", &policy);
        let hash = poirot_formula::classified::normalized_hash("SUM(B2,2)").unwrap();
        let PreparedFormula::Ready(first) = first.as_ref() else {
            panic!()
        };
        compiler.templates[0]
            .classified
            .insert(hash, vec![Arc::clone(first)]);
        compiler.compile_formula("SUM(B2,2)", &policy);
        assert_eq!(compiler.usage().parse_attempts, 2);
        assert_eq!(compiler.template_hits(), 0);
    }
    #[test]
    fn extension_hit_reconstructs_exact_stored_namespace_without_provider_capture() {
        use crate::extensions::*;
        let mut registry = ExtensionRegistry::new();
        registry
            .register(
                ExtensionNamespace::UserDefined,
                "CUSTOM",
                "v1",
                ExtensionContract {
                    arity: crate::registry::Arity {
                        minimum: 1,
                        maximum: 1,
                    },
                    evaluation: ExtensionEvaluationStrategy::Deferred,
                    capabilities: ExtensionCapabilities {
                        references: true,
                        arrays: false,
                        context: false,
                    },
                    cacheability: ExtensionCacheability::Deterministic,
                },
                |_: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                    panic!("preparation must not execute providers")
                },
            )
            .unwrap();
        registry
            .set_external_namespace(Some(
                crate::external_namespace::ExternalFunctionNamespace::from_stored_names(
                    ["CuStOm", "sUm", "mYsTeRy"],
                    4,
                )
                .unwrap(),
            ))
            .unwrap();
        let policy = EvaluationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
        let mut compiler = FormulaCompiler::new();
        compiler.compile_formula_with_extensions("SUM(CUSTOM(A1),MYSTERY(B2))", &policy, &registry);
        let second = compiler.compile_formula_with_extensions(
            "SUM(CUSTOM(C3),MYSTERY(D4))",
            &policy,
            &registry,
        );
        let formula = ready(&second);
        assert_eq!(compiler.usage().parse_attempts, 1);
        let expected =
            bound::bind_with_registry(formula.parsed().unwrap().expression(), Some(&registry));
        drop(registry);
        drop(compiler);
        assert_eq!(formula.executable(), expected);
    }
    #[test]
    fn forced_structural_bucket_collision_requires_complete_equality() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        let first = compiler.compile_formula("1+2", &policy);
        let other =
            crate::bytecode::compile(&poirot_formula::parse_formula("1+3").unwrap()).unwrap();
        let hash = executable_hash(&other.executable);
        compiler.templates[0]
            .structural
            .insert(hash, vec![Arc::clone(&ready(&first).bytecode().executable)]);
        let second = compiler.compile_formula("1+3", &policy);
        assert!(!Arc::ptr_eq(
            &ready(&first).bytecode().executable,
            &ready(&second).bytecode().executable
        ));
        assert_eq!(compiler.templates[0].structural[&hash].len(), 2);
    }
    #[test]
    fn registry_scopes_templates_and_lazy_call_binding() {
        let mut compiler = FormulaCompiler::new();
        let registry = crate::extensions::ExtensionRegistry::new();
        let policy = EvaluationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
        let first =
            compiler.compile_formula_with_extensions("IFERROR(A1,UNKNOWN(B2))", &policy, &registry);
        let second =
            compiler.compile_formula_with_extensions("IFERROR(C3,UNKNOWN(D4))", &policy, &registry);
        let b = ready(&second);
        assert_eq!(compiler.template_hits(), 1);
        assert_eq!(
            b.executable(),
            bound::bind_with_registry(b.parsed().unwrap().expression(), Some(&registry))
        );
        let third =
            compiler.compile_formula("IFERROR(E5,UNKNOWN(F6))", &EvaluationPolicy::default());
        assert!(!Arc::ptr_eq(
            &ready(&first).bytecode().executable,
            &ready(&third).bytecode().executable
        ));
    }

    #[test]
    fn operator_shape_metadata_preserves_point_templates_and_recompiles_range_bounds() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        let first = compiler.compile_formula("A1+IF(TRUE,B2,3)", &policy);
        let second = compiler.compile_formula("C3+IF(TRUE,D4,3)", &policy);
        assert_eq!(compiler.template_hits(), 1);
        assert_eq!(ready(&first).spill.shape, Some((1, 1)));
        assert_eq!(ready(&second).spill.shape, Some((1, 1)));
        let short = compiler.compile_formula("A1:A2+1", &policy);
        let long = compiler.compile_formula("B1:B4+1", &policy);
        assert_eq!(compiler.template_hits(), 1);
        assert_eq!(ready(&short).spill.shape, Some((2, 1)));
        assert_eq!(ready(&long).spill.shape, Some((4, 1)));
    }
}

#[cfg(test)]
mod template_admission_tests {
    use super::*;

    #[test]
    fn rejected_range_shapes_do_not_accumulate_template_candidates_after_restore() {
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::default();
        for row in 1..=32 {
            compiler.compile_formula(&format!("TRANSPOSE(A{row}:B{row})"), &policy);
        }
        assert!(compiler.templates[0].classified.is_empty());
        assert_eq!(compiler.usage().parse_attempts, 32);
        let snapshot = cache_snapshot::Cache::capture(&compiler, &policy)
            .unwrap()
            .into_owned();
        let mut restored = snapshot.restore(&policy).unwrap();
        assert!(restored.templates[0].classified.is_empty());
        restored.compile_formula("TRANSPOSE(A33:B33)", &policy);
        assert_eq!(restored.usage().parse_attempts, 1);
        assert!(restored.templates[0].classified.is_empty());
    }
}

/// Full compiled-plan DTO for the trusted local disk cache.
pub mod snapshot;

/// Serializable compiled-source cache used by overlay snapshots.
pub(crate) mod cache_snapshot;
