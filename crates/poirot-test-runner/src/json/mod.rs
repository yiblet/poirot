//! Authored JSON workbook scenarios, checked through normal and overlay evaluation.
mod cli;
pub mod schema;
pub mod specs;
pub use cli::{Cli, OutputFormat};

use poirot::{
    CellEvaluation, CellStyle, CellValue, EvaluationContext, EvaluationTarget, Workbook,
    WorkbookChange,
};
// The authored-style interpreter exercises component APIs unavailable on a value-only scenario.
use poirot::eval::overlay::{Evaluation, Overlay, OverlayPolicy};
use schema::{Action, Content, Scenario, Suite};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    KnownGap,
    UnexpectedPass,
    Failed,
    HarnessError,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct CheckResult {
    pub action: usize,
    pub cell: String,
    pub expected: schema::Scalar,
    pub actual: schema::Scalar,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct ModeResult {
    pub mode: &'static str,
    /// Builtin contract used consistently by both execution modes.
    pub builtin_policy: poirot::eval::BuiltinSemanticsPolicy,
    pub checks: Vec<CheckResult>,
    pub changes: Vec<WorkbookChange>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub status: Status,
    pub normal: ModeResult,
    pub overlay: ModeResult,
    pub outputs_equal: bool,
    pub changes_equal: bool,
}

impl ScenarioResult {
    pub fn fails(&self, strict: bool) -> bool {
        matches!(
            self.status,
            Status::Failed | Status::HarnessError | Status::UnexpectedPass
        ) || (strict && self.status == Status::KnownGap)
    }
}

// The same action interpreter drives both public APIs.
enum Session<'a> {
    Normal(Box<Evaluation<'a, &'a poirot::model::Workbook>>),
    Overlay(Box<Overlay<'a, &'a poirot::model::Workbook>>),
}
impl<'a> Session<'a> {
    fn new(
        book: &'a Workbook,
        normal: bool,
        builtin: poirot::eval::BuiltinSemanticsPolicy,
        arrays: poirot::eval::ArrayResultPolicy,
    ) -> Result<Self, String> {
        let policy = OverlayPolicy {
            builtin,
            arrays,
            ..Default::default()
        };
        if normal {
            Evaluation::with_workbook(book.model())
                .with_policy(policy)
                .build(EvaluationContext::default())
                .map(|e| Self::Normal(Box::new(e)))
                .map_err(|e| format!("{e:?}"))
        } else {
            Overlay::with_workbook(book.model())
                .with_policy(policy)
                .build()
                .map(|e| Self::Overlay(Box::new(e)))
                .map_err(|e| format!("{e:?}"))
        }
    }
    fn target(&self, cell: &str) -> Result<EvaluationTarget, String> {
        match self {
            Self::Normal(e) => e.target(cell),
            Self::Overlay(e) => e.target(cell),
        }
        .map_err(|e| e.to_string())
    }
    fn set(&mut self, cell: &str, content: &Content) -> Result<(), String> {
        let target = self.target(cell)?;
        match content {
            Content::Formula(f) => match self {
                Self::Normal(e) => e.set_formula(target, &f.formula),
                Self::Overlay(e) => e.set_formula(target, &f.formula),
            },
            Content::Value(v) => {
                let value = v.cell_value()?;
                match self {
                    Self::Normal(e) => e.set_value(target, value),
                    Self::Overlay(e) => e.set_value(target, value),
                }
            }
        }
        .map_err(|e| e.to_string())
    }
    fn format(&mut self, cell: &str, number_format: &Option<String>) -> Result<(), String> {
        let target = self.target(cell)?;
        let style = CellStyle {
            number_format: number_format.clone(),
        };
        match self {
            Self::Normal(e) => e.set_style(target, style),
            Self::Overlay(e) => e.set_style(target, style),
        }
        .map_err(|e| e.to_string())
    }
    fn output(&mut self, cell: &str) -> Result<CellEvaluation, String> {
        let target = self.target(cell)?;
        let outputs = match self {
            Self::Normal(e) => e.outputs(&[target]).map_err(|e| format!("{e:?}"))?,
            Self::Overlay(e) => e
                .evaluation(&EvaluationContext::default())
                .map_err(|e| format!("{e:?}"))?
                .outputs(&[target])
                .map_err(|e| format!("{e:?}"))?,
        };
        outputs
            .into_iter()
            .next()
            .ok_or_else(|| "output API returned no result".into())
    }
    fn changes(&mut self) -> Vec<WorkbookChange> {
        match self {
            Self::Normal(e) => e.changes(),
            Self::Overlay(e) => e.changes(),
        }
    }
}

/// Decode a versioned suite. Unknown fields and malformed typed values are errors.
pub fn parse_suite(json: &str) -> Result<Suite, String> {
    let suite: Suite = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if suite.schema_version != 1 {
        return Err("unsupported schema_version; expected 1".into());
    }
    if suite
        .group
        .as_ref()
        .is_some_and(|group| group.trim().is_empty())
    {
        return Err("suite group must be nonempty when provided".into());
    }
    if suite.workbook.sheets.is_empty() || suite.scenarios.is_empty() {
        return Err("a suite needs worksheets and scenarios".into());
    }
    let mut names = BTreeSet::new();
    for scenario in &suite.scenarios {
        if scenario.name.trim().is_empty() || !names.insert(&scenario.name) {
            return Err("scenario names must be nonempty and unique within a suite".into());
        }
        if scenario.evidence.is_empty() || scenario.evidence.iter().any(|s| s.trim().is_empty()) {
            return Err(format!(
                "{}: provide evidence for expectations",
                scenario.name
            ));
        }
        if !scenario
            .actions
            .iter()
            .any(|a| matches!(a, Action::Check { .. }))
        {
            return Err(format!(
                "{}: scenario needs at least one check",
                scenario.name
            ));
        }
        for action in &scenario.actions {
            match action {
                Action::Set {
                    content: Content::Value(v),
                    ..
                } => {
                    v.cell_value()?;
                }
                Action::Check {
                    expected,
                    tolerance,
                    known_gap,
                    ..
                } => {
                    expected.cell_value()?;
                    if !tolerance.is_finite() || *tolerance < 0.0 {
                        return Err("tolerance must be finite and nonnegative".into());
                    }
                    if *tolerance != 0.0 && !matches!(expected, schema::Scalar::Number(_)) {
                        return Err("tolerance is only valid for numeric expectations".into());
                    }
                    if let Some(gap) = known_gap {
                        if gap.reason.trim().is_empty() {
                            return Err("known gaps need a reason".into());
                        }
                        if equal(
                            &gap.actual.cell_value()?,
                            &expected.cell_value()?,
                            *tolerance,
                        ) {
                            return Err("known gap actual must differ from expected".into());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(suite)
}

fn workbook(suite: &Suite) -> Result<Workbook, String> {
    let mut book = Workbook::new();
    for sheet in &suite.workbook.sheets {
        book.add_sheet(sheet).map_err(|e| e.to_string())?;
    }
    let changes = {
        let mut session = Session::new(
            &book,
            false,
            suite.builtin_policy,
            suite.array_result_policy,
        )?;
        let mut cells = BTreeSet::new();
        for cell in &suite.workbook.cells {
            let target = session.target(&cell.cell)?;
            if !cells.insert((target.sheet, target.address)) {
                return Err(format!("duplicate initial cell: {}", cell.cell));
            }
            session.set(&cell.cell, &cell.content)?;
            if cell.number_format.is_some() {
                session.format(&cell.cell, &cell.number_format)?;
            }
        }
        session.changes()
    };
    book.apply(changes).map_err(|e| e.to_string())?;
    Ok(book)
}

fn equal(actual: &CellValue, expected: &CellValue, tolerance: f64) -> bool {
    match (actual, expected) {
        (CellValue::Number(a), CellValue::Number(b)) => a == b || (a - b).abs() <= tolerance,
        _ => actual == expected,
    }
}

fn run_mode(
    book: &Workbook,
    scenario: &Scenario,
    normal: bool,
    builtin_policy: poirot::eval::BuiltinSemanticsPolicy,
    arrays: poirot::eval::ArrayResultPolicy,
) -> ModeResult {
    let mut result = ModeResult {
        mode: if normal { "normal" } else { "overlay" },
        builtin_policy,
        checks: Vec::new(),
        changes: Vec::new(),
        error: None,
    };
    let mut execute = || -> Result<(), String> {
        let mut session = Session::new(book, normal, builtin_policy, arrays)?;
        for (index, action) in scenario.actions.iter().enumerate() {
            let action_result = (|| -> Result<(), String> {
                match action {
                    Action::Set { cell, content } => session.set(cell, content)?,
                    Action::Format {
                        cell,
                        number_format,
                    } => session.format(cell, number_format)?,
                    Action::Check {
                        cell,
                        expected,
                        tolerance,
                        display,
                        number_format,
                        known_gap,
                    } => {
                        let output = session.output(cell)?;
                        let value_matches =
                            equal(&output.value, &expected.cell_value()?, *tolerance);
                        let actual_format = output
                            .formatting
                            .as_ref()
                            .and_then(|s| s.number_format.as_deref());
                        let format_matches = number_format
                            .as_ref()
                            .is_none_or(|s| Some(s.as_str()) == actual_format);
                        let rendered = if display.is_some() {
                            let plan = poirot::format::compile_format(
                                actual_format.unwrap_or("General"),
                                Default::default(),
                            )
                            .map_err(|e| format!("{e:?}"))?;
                            Some(
                                poirot::format::render_value(
                                    &output.value,
                                    &plan,
                                    Default::default(),
                                )
                                .map_err(|e| format!("{e:?}"))?
                                .text,
                            )
                        } else {
                            None
                        };
                        let display_matches = display
                            .as_ref()
                            .is_none_or(|s| Some(s) == rendered.as_ref());
                        let status = if !format_matches || !display_matches {
                            Status::Failed
                        } else if value_matches {
                            if known_gap.is_some() {
                                Status::UnexpectedPass
                            } else {
                                Status::Passed
                            }
                        } else if let Some(gap) = known_gap {
                            if equal(&output.value, &gap.actual.cell_value()?, 0.0) {
                                Status::KnownGap
                            } else {
                                Status::Failed
                            }
                        } else {
                            Status::Failed
                        };
                        result.checks.push(CheckResult {
                            action: index,
                            cell: cell.clone(),
                            expected: expected.clone(),
                            actual: schema::Scalar::from_cell(&output.value),
                            status,
                            detail: format!(
                                "format={actual_format:?}, display={rendered:?}{}",
                                known_gap
                                    .as_ref()
                                    .map(|g| format!(", known gap: {}", g.reason))
                                    .unwrap_or_default()
                            ),
                        });
                    }
                }
                Ok(())
            })();
            action_result.map_err(|e| format!("action {index}: {e}"))?;
        }
        result.changes = session.changes();
        Ok(())
    };
    if let Err(error) = execute() {
        result.error = Some(error);
    }
    result
}

/// Run each scenario in isolation in both modes. Compare observations and the raw,
/// ordered change lists after the last action; no extra evaluation is performed.
pub fn run_suite(suite: &Suite, filter: Option<&str>) -> Result<Vec<ScenarioResult>, String> {
    let names = suite
        .scenarios
        .iter()
        .filter(|scenario| filter.is_none_or(|filter| scenario.name.contains(filter)))
        .map(|scenario| scenario.name.as_str())
        .collect();
    run_selected(suite, &names)
}

/// Execute exact selected scenario names, retaining one workbook and both modes.
pub fn run_selected(suite: &Suite, names: &BTreeSet<&str>) -> Result<Vec<ScenarioResult>, String> {
    let book = workbook(suite)?;
    let mut results = Vec::new();
    for scenario in &suite.scenarios {
        if !names.contains(scenario.name.as_str()) {
            continue;
        }
        let normal = run_mode(
            &book,
            scenario,
            true,
            suite.builtin_policy,
            suite.array_result_policy,
        );
        let overlay = run_mode(
            &book,
            scenario,
            false,
            suite.builtin_policy,
            suite.array_result_policy,
        );
        results.push(compare_modes(scenario.name.clone(), normal, overlay));
    }
    Ok(results)
}

fn compare_modes(name: String, normal: ModeResult, overlay: ModeResult) -> ScenarioResult {
    // Compare typed observations, including their display/format details.
    let outputs_equal = normal.checks == overlay.checks;
    let changes_equal = normal.changes == overlay.changes;
    let checks = normal.checks.iter().chain(&overlay.checks);
    let status = if normal.error.is_some() || overlay.error.is_some() {
        Status::HarnessError
    } else if !outputs_equal || !changes_equal || checks.clone().any(|c| c.status == Status::Failed)
    {
        Status::Failed
    } else if checks.clone().any(|c| c.status == Status::UnexpectedPass) {
        Status::UnexpectedPass
    } else if checks.clone().any(|c| c.status == Status::KnownGap) {
        Status::KnownGap
    } else {
        Status::Passed
    };
    ScenarioResult {
        name,
        status,
        normal,
        overlay,
        outputs_equal,
        changes_equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(name: &'static str, changes: Vec<WorkbookChange>) -> ModeResult {
        ModeResult {
            mode: name,
            builtin_policy: Default::default(),
            changes,
            checks: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn raw_change_list_differences_fail_even_with_identical_observations() {
        let change = WorkbookChange::SetValue {
            sheet: 0,
            address: "A1".parse().unwrap(),
            value: 2.0.into(),
        };
        let result = compare_modes(
            "different changes".into(),
            mode("normal", vec![]),
            mode("overlay", vec![change]),
        );
        assert!(result.outputs_equal);
        assert!(!result.changes_equal);
        assert_eq!(result.status, Status::Failed);
    }

    #[test]
    fn change_order_is_significant() {
        let a = WorkbookChange::SetValue {
            sheet: 0,
            address: "A1".parse().unwrap(),
            value: 2.0.into(),
        };
        let b = WorkbookChange::SetValue {
            sheet: 0,
            address: "B1".parse().unwrap(),
            value: 3.0.into(),
        };
        let result = compare_modes(
            "ordered changes".into(),
            mode("normal", vec![a.clone(), b.clone()]),
            mode("overlay", vec![b, a]),
        );
        assert_eq!(result.status, Status::Failed);
    }
}
