#![doc = include_str!("../README.md")]
use CellError::*;
use CellValue::{Blank, Boolean, Error, Number, Text};
use poirot_formula::{BinaryOp, UnaryOp};
use poirot_model::{CellAddress, CellError, CellValue, MAX_COLUMNS, MAX_ROWS, Workbook};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;
#[cfg(test)]
use std::time::{Duration, UNIX_EPOCH};
mod clock;
mod coordinate_hash;
use coordinate_hash::{CellMap, CellSet};

/// Conversion and guarded publication of evaluated values into workbook caches.
pub mod materialization;
/// Sparse input overrides and effective workbook reads.
pub mod run_overlay;
/// Formula cache publication policies and receipts.
pub use materialization::{
    CellSetterSourceBinding, FormulaCacheMaterializationIdentity,
    FormulaCacheMaterializationPolicy, FormulaCachePublicationReceipt, FormulaCacheWriteReceipt,
    MaterializedScalarSummary, run_recalculation_with_receipts,
};
/// Shared immutable workbook ownership for retained evaluation plans.
pub use run::shared_workbook;
/// Numeric result classification and compatibility policy.
pub mod numeric_outcome;
/// Numeric policy and its pinned source identity.
pub use numeric_outcome::{NumericOutcomePolicy, NumericSourceBinding};
/// Identified exceptions to pinned source behavior.
pub use poirot_model::KnownSourceException;
/// Execution policies and formatting identities.
pub mod configuration;
/// Run configuration, policy selections, and cache identity types.
pub use configuration::{
    ArrayResultPolicy, BuiltinSemanticsPolicy, ComputationFingerprint, ConfigurationFingerprint,
    CurrencyContext, FormattingConfiguration, RunConfiguration,
};
/// Explicit numeric input tokens for locale-parametric argument parsing.
pub use poirot_coercionv2::NumberLocaleV2;
/// Formula compilation and immutable execution preparation.
pub mod compiler;
mod consumed_calls;
/// Optional execution events for tracing and profiling.
pub mod execution_observation;
/// Retention accounting for observed execution state.
pub mod execution_retention;
/// Sparse workbook edits, compiled formula reuse, and incremental evaluations.
pub mod overlay;
/// Complete per-evaluation clock, formatting, work, and retention settings.
pub use overlay::EvaluationContext;
/// Bounded execution sessions, read ports, and completion reports.
pub mod run;
/// Formula compilation, prepared programs, and registry identities.
pub use compiler::{
    CompiledFormula, CompiledWorkbook, CompilerUsage, EvaluationPolicy, FormulaCompiler,
    PreparedFormula, RegistryIdentity, SourceFingerprint,
};
/// Text semantics and the pinned implementation identity.
pub use poirot_text::{SourceVersion as TextSourceVersion, TextSemanticsPolicy};
/// Run entry points, observation ports, and result metadata.
pub use run::{
    BuiltinInvocationObserver, CachedCellReader, CellCompletion, CellObserver, CellProvenance,
    ConsumedBuiltinInvocation, DependencyRead, DependencyReadSink, EvaluationProbe, EvaluationRun,
    ReadEvent, RunReport, RunUsage, WorkbookView, same_result_identity,
};
/// Typed builtin adapters over the shared evaluation host.
pub mod adapters;
mod atp;
/// Bound expression representation used while compiling formulas.
pub mod bound;
/// Serializable executable formula instructions and operands.
pub mod bytecode;
mod bytecode_vm;
mod call_admission;
/// Explicit registration and execution contracts for extension functions.
pub mod extensions;
/// External workbook and name resolution identities.
pub mod external_namespace;
/// Reproducible cell-local random streams.
pub mod random;
/// Builtin handler identities, spellings, arity, and dependency policies.
pub mod registry;
#[cfg(test)]
use bound::Expression as Expr;
use registry::*;
/// Registered builtin metadata and advertised function names.
pub use registry::{FUNCTION_DESCRIPTORS, SUPPORTED_FUNCTIONS};

/// Work limits, usage counters, and operational evaluation failures.
pub mod budget;
mod context_dependencies;
/// Public evaluation limits, outcomes, failures, and usage counters.
pub use budget::{
    BudgetFailure, BudgetKind, CapabilityId, EvaluationBudgets, EvaluationFailure,
    EvaluationOutcome, EvaluationTarget, EvaluationUsage, WorkKind,
};
use budget::{BudgetMeter, EvalResult};
mod function_failure;
mod signed_reference;
use function_failure::{FunctionFailure, FunctionResult, function_value, identifier_failure};
/// Cell-versus-area provenance retained for reference coercion and intersection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AreaKind {
    Cell,
    Area,
}
/// Inclusive rectangle on one sheet, retaining cell-versus-area provenance.
#[derive(Clone, Copy)]
struct Area {
    kind: AreaKind,
    sheet: usize,
    r0: u32,
    r1: u32,
    c0: u32,
    c1: u32,
}
impl Area {
    fn contains(self, a: CellAddress) -> bool {
        (self.r0..=self.r1).contains(&a.row) && (self.c0..=self.c1).contains(&a.col)
    }
    fn size(self) -> u64 {
        u64::from(self.r1 - self.r0 + 1) * u64::from(self.c1 - self.c0 + 1)
    }
}
/// Materialized rectangular values with optional source coordinates.
#[derive(Clone)]
struct ArrayView {
    values: Vec<Vec<CellValue>>,
    /// None means that the producing adapter has not established source coordinates.
    anchor: Option<CellAddress>,
}
impl ArrayView {
    fn unknown(values: Vec<Vec<CellValue>>) -> Self {
        Self {
            values,
            anchor: None,
        }
    }
    fn anchored(values: Vec<Vec<CellValue>>, anchor: Option<CellAddress>) -> Self {
        Self { values, anchor }
    }
}
impl std::ops::Deref for ArrayView {
    /// Row-major backing values borrowed by array-aware consumers.
    type Target = Vec<Vec<CellValue>>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl IntoIterator for ArrayView {
    /// One owned row of array values.
    type Item = Vec<CellValue>;
    /// Consuming iterator over owned rows.
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}
/// Internal scalar/array/reference channels shared by tree, bytecode, and builtin adapters.
#[derive(Clone)]
enum Value {
    SignedRefs(Vec<signed_reference::SignedArea>),
    /// Selected helper MissingArgEval, distinct from an actual blank cell.
    Missing,
    Scalar(CellValue),
    /// Materialized selection retaining its original argument-admission provenance.
    SelectedScalar(CellValue, host::OperandOrigin),
    Refs(Vec<Area>),
    /// One true ThreeDEval/RefEval, distinct from a RefListEval union.
    SheetSpan(Vec<Area>),
    Array(ArrayView),
}
impl From<CellValue> for Value {
    fn from(v: CellValue) -> Self {
        Self::Scalar(v)
    }
}
fn err(e: CellError) -> Value {
    Error(e).into()
}
/// Explicit legacy projection for consumers that intentionally collapse omission.
/// Host operand APIs must retain the separate missing flag alongside this value.
// TODO(coercion-non-argument): [MEDIUM] Collapsing omission to blank can change predicates and defaults unless the missing flag is retained.
fn coerce_legacy_missing_blank() -> CellValue {
    Blank
}
// TODO(coercion-non-argument): [HIGH] Numeric result publication turns nonfinite results into NUM; argument admission must select its own policy.
fn finite(n: f64) -> CellValue {
    if n.is_finite() { Number(n) } else { Error(Num) }
}
const VALUE_ERROR: CellError = CellError::Value;
mod core_projection;
mod source_arithmetic;
use core_projection::{
    checked_text_view, coercev2_arithmetic_number, coercev2_formula_text, coercev2_logical_boolean,
    compare, remap_semantic, semantic,
};
/// Numeric slash-date parsing policy. Formula grammar remains canonical OOXML syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DateOrder {
    /// Interpret slash dates as month, day, then year.
    MonthDayYear,
    /// Interpret slash dates as day, month, then year.
    DayMonthYear,
}
/// Explicit localization policy. Month names are English; arbitrary locale/calendar
/// modes are not accepted. Fixed offsets do not model daylight-saving transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Localization {
    /// Fixed offset applied when converting the evaluation clock to local time.
    pub utc_offset_seconds: i32,
    /// Ordering of numeric components in slash-date text.
    pub date_order: DateOrder,
}
/// Clock, seed, and localization captured for lower-level execution APIs.
/// Workspace evaluation uses the complete [`EvaluationContext`] instead.
/// RAND and RANDBETWEEN use reproducible seed-and-cell-specific streams.
/// UTC is the default, rather than the host's local timezone.
#[derive(Clone, Copy, Debug)]
pub struct EvaluationInputs {
    /// Fixed instant used by date functions and extension contexts during this pass.
    pub as_of_time: SystemTime,
    /// Seed for reproducible cell-specific random streams.
    pub randomness_seed: u64,
    /// Date parsing and fixed-offset clock policy.
    pub localization: Localization,
}
/// Complete context identity. Incremental evaluators should select the fields actually
/// used by each cell rather than invalidating deterministic cells on any change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContextFingerprint {
    /// Exact captured instant, retaining nanoseconds and pre-epoch timestamps.
    #[serde(with = "crate::clock::timestamp")]
    pub as_of_time: SystemTime,
    /// Captured random seed.
    pub randomness_seed: u64,
    /// Captured localization policy.
    pub localization: Localization,
}
impl EvaluationInputs {
    /// Capture a stable, bit-exact identity for these execution inputs.
    pub fn fingerprint(self) -> ContextFingerprint {
        ContextFingerprint {
            as_of_time: self.as_of_time,
            randomness_seed: self.randomness_seed,
            localization: self.localization,
        }
    }
}
impl Default for EvaluationInputs {
    fn default() -> Self {
        Self {
            as_of_time: SystemTime::now(),
            randomness_seed: 0,
            localization: Localization {
                utc_offset_seconds: 0,
                date_order: DateOrder::MonthDayYear,
            },
        }
    }
}
fn execute_configuration<T>(
    workbook: &Workbook,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
    operation: impl FnOnce(&mut Session<'_>) -> EvalResult<T>,
) -> EvaluationOutcome<T> {
    if let Err(failure) = configuration.validate() {
        return EvaluationOutcome::Aborted {
            failure,
            usage: EvaluationUsage::default(),
        };
    }
    // Validate budgets before preparation to preserve the compatibility error order.
    let meter = match BudgetMeter::new(budgets) {
        Ok(meter) => meter,
        Err(failure) => {
            return EvaluationOutcome::Aborted {
                failure,
                usage: EvaluationUsage::default(),
            };
        }
    };
    let plan = FormulaCompiler::new().compile_workbook(workbook, &EvaluationPolicy::default());
    let mut session = Session::new(workbook, configuration.context(), meter);
    session.currency = configuration.formatting().currency();
    session.number_locale = configuration.number_locale();
    session.text_policy = configuration.text_policy();
    session.numeric_policy = configuration.numeric_policy();
    session.builtin_policy = configuration.builtin_policy();
    session.array_result_policy = configuration.array_result_policy();
    session.plan = Some(&plan);
    let mut run = EvaluationRun { session };
    let result = operation(&mut run.session);
    run.finish(result).outcome
}
/// Run one cell. Completed formula blanks publish as numeric zero; empty cells stay blank.
/// Semantic spreadsheet errors are successful `Complete` values.
pub fn run_cell(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_cell_with_configuration(workbook, sheet, address, &context.into(), budgets)
}
/// Evaluate one cell with explicit policies, preserving operational failures.
pub fn run_cell_with_configuration(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    execute_configuration(workbook, configuration, budgets, |session| {
        session.cell(sheet, address)
    })
}
/// Evaluate supplied expression text without publishing a worksheet formula result.
/// Blank expression results remain blank. Parsing/binding retain their own
/// preparation limits. Runtime work is metered separately.
pub fn run_formula(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(workbook, sheet, address, formula, &context.into(), budgets)
}
/// Compile and execute supplied formula text with explicit policies and work limits.
pub fn run_formula_with_configuration(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    execute_configuration(workbook, configuration, budgets, |session| {
        let prepared =
            FormulaCompiler::new().compile_formula(formula, &EvaluationPolicy::default());
        session.prepared_at(EvaluationTarget { sheet, address }, &prepared)
    })
}
/// Execute supplied bound syntax without reparsing; name bodies use prepared workbook plans.
pub fn run_plan(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    plan: &bound::FormulaPlan,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_plan_with_configuration(workbook, sheet, address, plan, &context.into(), budgets)
}
/// Execute supplied bound syntax with explicit policies and work limits.
pub fn run_plan_with_configuration(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    plan: &bound::FormulaPlan,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    execute_configuration(workbook, configuration, budgets, |session| {
        session.bound_at(EvaluationTarget { sheet, address }, plan)
    })
}
/// Read-only successful bulk results. This is not a public cache-application plan.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEvaluation {
    /// Cell whose result was requested.
    pub target: EvaluationTarget,
    /// Computed value, including semantic spreadsheet errors.
    pub value: CellValue,
    /// Optional source-cell presentation, independent of the computed value.
    /// Explicit TEXT() results remain text; this metadata never coerces values.
    pub formatting: Option<poirot_model::CellStyle>,
}
impl Session<'_> {
    fn output(&self, target: EvaluationTarget, value: CellValue) -> CellEvaluation {
        let formatting = run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs)
            .cell(target.sheet, target.address)
            .map(|cell| &cell.style)
            .filter(|style| style.number_format.is_some())
            .cloned();
        CellEvaluation {
            target,
            value,
            formatting,
        }
    }
}
/// Evaluate stored workbook formulas without applying their results to the model.
pub fn run_workbook(
    workbook: &Workbook,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<Vec<CellEvaluation>> {
    run_workbook_with_configuration(workbook, &context.into(), budgets)
}
/// Evaluate workbook formulas under explicit policies without publishing saved caches.
pub fn run_workbook_with_configuration(
    workbook: &Workbook,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<Vec<CellEvaluation>> {
    execute_configuration(workbook, configuration, budgets, |session| {
        materialization::formula_updates(workbook, session)
    })
}
/// Commit all completed caches under one exclusive borrow; an abort publishes none.
pub fn run_recalculation(
    workbook: &mut Workbook,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<usize> {
    run_recalculation_with_configuration(workbook, &context.into(), budgets)
}
/// Evaluate and atomically publish formula caches under explicit policies.
pub fn run_recalculation_with_configuration(
    workbook: &mut Workbook,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<usize> {
    if configuration.array_result_policy() == ArrayResultPolicy::DynamicSpill {
        return EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::FunctionAdapter,
                source: None,
                detail: "dynamic array ownership cannot be materialized into model formula caches; use the read-only runtime".into(),
            },
            usage: EvaluationUsage::default(),
        };
    }
    match run_workbook_with_configuration(workbook, configuration, budgets) {
        EvaluationOutcome::Aborted { failure, usage } => {
            EvaluationOutcome::Aborted { failure, usage }
        }
        EvaluationOutcome::Complete {
            value: updates,
            usage,
        } => {
            let count = updates.len();
            if updates.iter().any(|update| {
                workbook
                    .sheet(update.target.sheet)
                    .and_then(|sheet| sheet.get(update.target.address))
                    .is_none()
            }) {
                return crate::EvaluationOutcome::Aborted {
                    failure: crate::EvaluationFailure::StalePlan,
                    usage,
                };
            }
            for update in updates {
                let Some(cell) = workbook
                    .sheet_mut(update.target.sheet)
                    .and_then(|sheet| sheet.get_mut(update.target.address))
                else {
                    return crate::EvaluationOutcome::Aborted {
                        failure: crate::EvaluationFailure::StalePlan,
                        usage,
                    };
                };
                // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
                cell.value = configuration
                    .materialization_policy()
                    .coerce_stored_value(update.value);
            }
            EvaluationOutcome::Complete {
                value: count,
                usage,
            }
        }
    }
}
/// Mutable execution state shared by nested formulas within one bounded run.
struct Session<'a> {
    bytecode_stacks: Vec<Vec<Value>>,
    lookup_prefixes: adapters::search::execution::LookupPrefixes,
    random_streams: CellMap<(random::Stream, usize)>,
    extensions: Option<&'a extensions::ExtensionRegistry>,
    extension_trace: extensions::ExtensionTrace,
    workbook: &'a Workbook,
    inputs: Option<&'a run_overlay::RunInputs>,
    context: EvaluationInputs,
    currency: Option<&'a CurrencyContext>,
    number_locale: &'a poirot_coercionv2::NumberLocaleV2,
    text_policy: poirot_text::TextSemanticsPolicy,
    numeric_policy: NumericOutcomePolicy,
    builtin_policy: BuiltinSemanticsPolicy,
    array_result_policy: ArrayResultPolicy,
    expression_demand: expression_execution::ExpressionDemand,
    spill_owners: CellMap<CellAddress>,
    pending_spills: CellMap<ArrayView>,
    spill_read_ranges: CellMap<Vec<Area>>,
    blank_reads: CellMap<CellSet>,
    cache: run::CellMemo,
    memo_limits: Option<run::MemoRetentionLimits>,
    memo_usage: run::MemoRetentionUsage,
    visiting: CellSet,
    names: HashSet<(usize, usize)>,
    numeric_date_effects: CellMap<poirot_coercionv2::date_time::Effects>,
    budget: BudgetMeter,
    plan: Option<&'a CompiledWorkbook>,
    lazy_compiler: Option<&'a std::cell::RefCell<FormulaCompiler>>,
    preparation_policy: Option<&'a EvaluationPolicy>,
    overlay_cache: Option<&'a std::cell::RefCell<overlay::dependencies::Cache>>,
    overlay_reads: Option<overlay::dependencies::Reads>,
    dependency_sink: Option<&'a mut dyn DependencyReadSink>,
    cell_reader: Option<&'a mut dyn CachedCellReader>,
    cell_observer: Option<&'a mut dyn CellObserver>,
    builtin_invocation_observer: Option<&'a mut dyn BuiltinInvocationObserver>,
    execution_observer: Option<&'a mut dyn execution_observation::ExecutionObserver>,
    observation_state: execution_observation::ObservationState,
    consumer: Option<EvaluationTarget>,
    run_usage: RunUsage,
}
mod criteria_adapter;
mod functions;
mod host;
/// Lookup traversal axis, matching mode, and selection rules.
pub use adapters::search::execution::{SearchAxis, SearchMode, SearchSelection};
use adapters::statistics::execution::AggregateFunction;

mod array_operator;
mod executable;
mod expression_execution;
use executable::{ArgumentsRef, ExecutableRef};
mod namespace_resolution;
mod session_execution;
mod value_projection;
mod wildcard_engine;
use wildcard_engine::{wildcard, wildcard_prefix};

#[cfg(test)]
mod identifier_consumer_tests;
#[cfg(test)]
mod numeric_admission_tests;
#[cfg(test)]
mod numeric_density_tests;
#[cfg(test)]
mod numeric_handler_tests;
#[cfg(test)]
mod numeric_outcome_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod overlay_tests;

#[cfg(test)]
mod signed_offset_tests;

#[cfg(test)]
mod short_offset_tests;

mod spill_publication;
