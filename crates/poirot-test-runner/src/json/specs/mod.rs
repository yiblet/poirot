//! Source-backed Excel specification tables in the combined JSON corpus.
use poirot::{EvaluationContext, Overlay, OverlayPolicy, eval, model};
use schema::{Cell, Currency, EvidenceKind, Name, Scalar, SettingsUpdate, TextPolicy};

mod schema;
pub(crate) use schema::{Case, CellContent};

/// Effective execution context and overlay policies retained between fixture steps.
struct Settings {
    context: eval::EvaluationInputs,
    currency: Option<Currency>,
    text_policy: TextPolicy,
    array_result_policy: schema::ArrayResultPolicy,
}
impl Settings {
    fn new(case: &Case) -> Self {
        let mut context = eval::EvaluationInputs::default();
        case.settings.context.apply(&mut context);
        Self {
            context,
            currency: case.settings.currency,
            text_policy: case.settings.text_policy.unwrap_or(case.text_policy),
            array_result_policy: case.settings.array_result_policy.unwrap_or_default(),
        }
    }
    fn update(&mut self, update: &SettingsUpdate) {
        update.context.apply(&mut self.context);
        if let Some(currency) = update.currency {
            self.currency = Some(currency);
        }
        if let Some(policy) = update.array_result_policy {
            self.array_result_policy = policy;
        }
        if let Some(policy) = update.text_policy {
            self.text_policy = policy;
        }
    }
    fn execution_context(&self) -> EvaluationContext {
        let formatting = match self.currency {
            None => eval::FormattingConfiguration::default(),
            Some(Currency::Usd) => {
                eval::FormattingConfiguration::with_currency(eval::CurrencyContext {
                    decimal_separator: ".".into(),
                    grouping_separator: ",".into(),
                    primary_group: 3,
                    secondary_group: 3,
                    positive_prefix: "$".into(),
                    positive_suffix: "".into(),
                    negative_prefix: "($".into(),
                    negative_suffix: ")".into(),
                })
            }
        };
        EvaluationContext {
            as_of_time: self.context.as_of_time,
            randomness_seed: self.context.randomness_seed,
            localization: self.context.localization,
            formatting,
            ..Default::default()
        }
    }
    fn policy(&self) -> OverlayPolicy {
        let source = match self.text_policy {
            TextPolicy::ExcelV1 => eval::TextSourceVersion::Poi7321209OpenJdk21Unicode15,
            TextPolicy::ExcelV2 => eval::TextSourceVersion::ExcelCompatibilityV2Documented,
        };
        OverlayPolicy {
            builtin: eval::BuiltinSemanticsPolicy::ExcelDocumentation,
            arrays: self.array_result_policy.execution(),
            text: eval::TextSemanticsPolicy { source },
            ..Default::default()
        }
    }
}

fn write_cells(workbook: &mut model::Workbook, cells: &[Cell]) -> Result<(), String> {
    for cell in cells {
        let worksheet = workbook
            .sheet_mut(cell.target.sheet)
            .ok_or("fixture worksheet is missing")?;
        match &cell.content {
            CellContent::Formula(formula) => worksheet
                .set_formula(cell.target.address, formula, model::CellValue::Blank)
                .map_err(|e| e.to_string())?,
            CellContent::Literal(value) => worksheet
                .set_value(cell.target.address, value.cell_value())
                .map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}
fn write_names(workbook: &mut model::Workbook, names: &[Name]) {
    workbook.named_ranges = names
        .iter()
        .map(|name| model::NamedRange {
            name: name.name.clone(),
            formula: name.formula.clone(),
            sheet_scope: name.sheet_scope,
        })
        .collect();
}
fn scalar_matches(actual: &model::CellValue, expected: &Scalar, tolerance: Option<f64>) -> bool {
    let expected = expected.cell_value();
    if let Some(tolerance) = tolerance {
        matches!((actual, &expected), (model::CellValue::Number(a), model::CellValue::Number(e))
            if a.is_finite() && (a - e).abs() <= tolerance)
    } else if let (Some(a), Some(e)) = (actual.text_ref(), expected.text_ref()) {
        a.units().eq(e.units())
    } else {
        actual == &expected
    }
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Case>, String> {
    let cases: Vec<Case> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut names = std::collections::BTreeSet::new();
    for (index, case) in cases.iter().enumerate() {
        if case.schema_version != 2
            || case.id != index
            || !names.insert(&case.name)
            || case.sheets.is_empty()
            || case.steps.is_empty()
        {
            return Err(format!(
                "invalid Excel specification identity: {}",
                case.name
            ));
        }
        for step in &case.steps {
            if step.assertions.is_empty() {
                return Err("specification step has no assertions".into());
            }
            for assertion in &step.assertions {
                if assertion
                    .tolerance
                    .is_some_and(|t| !t.is_finite() || t < 0.)
                {
                    return Err("invalid specification tolerance".into());
                }
                if assertion.evidence.kind != EvidenceKind::Unresolved {
                    if assertion.evidence.source_urls.is_empty()
                        || assertion.evidence.explanation.trim().is_empty()
                    {
                        return Err("specification assertion lacks evidence".into());
                    }
                    assertion.expected_cells()?;
                }
            }
        }
    }
    Ok(cases)
}

pub(crate) struct Observation {
    pub checks: Vec<serde_json::Value>,
    pub failures: Vec<serde_json::Value>,
    pub mode_errors: Vec<String>,
}

pub(crate) fn observe(case: &Case) -> Result<Observation, String> {
    let mut model = model::Workbook::new();
    for name in &case.sheets {
        model.add_sheet(name).map_err(|e| e.to_string())?;
    }
    model.date_system = case.settings.date_system.into();
    write_cells(&mut model, &case.cells)?;
    write_names(&mut model, &case.names);
    let mut settings = Settings::new(case);
    let mut overlay = Overlay::with_workbook(std::sync::Arc::new(model.clone()))
        .with_policy(settings.policy())
        .build()
        .map_err(|e| e.to_string())?;
    let mut result = Observation {
        checks: Vec::new(),
        failures: Vec::new(),
        mode_errors: Vec::new(),
    };
    for (step_index, step) in case.steps.iter().enumerate() {
        if let Some(update) = &step.settings {
            settings.update(update);
            overlay
                .set_policy(settings.policy())
                .map_err(|e| e.to_string())?;
        }
        write_cells(&mut model, &step.edits)?;
        let recompile = step.recompile
            || step.names.is_some()
            || step
                .edits
                .iter()
                .any(|edit| matches!(edit.content, CellContent::Formula(_)));
        if let Some(names) = &step.names {
            write_names(&mut model, names);
        }
        if recompile {
            overlay = Overlay::with_workbook(std::sync::Arc::new(model.clone()))
                .with_policy(settings.policy())
                .build()
                .map_err(|e| e.to_string())?;
        } else {
            for edit in &step.edits {
                match &edit.content {
                    CellContent::Literal(value) => {
                        overlay.set_value(edit.target.evaluation(), value.cell_value())
                    }
                    CellContent::Formula(formula) => {
                        overlay.set_formula(edit.target.evaluation(), formula)
                    }
                }
                .map_err(|e| e.to_string())?;
            }
        }
        let footprints = step
            .assertions
            .iter()
            .map(|assertion| {
                if assertion.evidence.kind == EvidenceKind::Unresolved {
                    Ok(Vec::new())
                } else {
                    assertion.expected_cells()
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let targets: Vec<_> = footprints
            .iter()
            .flatten()
            .map(|(target, _)| *target)
            .collect();
        let context = settings.execution_context();
        let mut direct = poirot::Evaluation::with_workbook(&model)
            .with_policy(settings.policy())
            .build(context.clone())
            .map_err(|e| e.to_string())?;
        let mut evaluation = overlay.evaluation(&context).map_err(|e| e.to_string())?;
        for (mode, outputs) in [
            ("normal", direct.outputs(&targets)),
            ("overlay", evaluation.outputs(&targets)),
        ] {
            let values = match outputs {
                Ok(outputs) => outputs
                    .into_iter()
                    .map(|r| ((r.target.sheet, r.target.address), r.value))
                    .collect::<std::collections::BTreeMap<_, _>>(),
                Err(error) => {
                    result
                        .mode_errors
                        .push(format!("{mode} step {step_index}: {error}"));
                    continue;
                }
            };
            for (assertion_index, (assertion, footprint)) in
                step.assertions.iter().zip(&footprints).enumerate()
            {
                if assertion.evidence.kind == EvidenceKind::Unresolved {
                    result.failures.push(serde_json::json!({"mode":mode,"step":step_index,"assertion":assertion_index,"unresolved_evidence":assertion.evidence.describe()}));
                }
                for (at, expected) in footprint {
                    let actual = values.get(&(at.sheet, at.address));
                    let matched = actual.is_some_and(|actual| {
                        scalar_matches(actual, expected, assertion.tolerance)
                    });
                    let check = serde_json::json!({
                        "mode":mode,"step":step_index,"assertion":assertion_index,
                        "sheet":at.sheet,"cell":at.address.to_string(),
                        "actual":actual.map(super::schema::Scalar::from_cell),
                        "expected":super::schema::Scalar::from_cell(&expected.cell_value()),
                        "tolerance":assertion.tolerance,"matched":matched,
                        "evidence":assertion.evidence.describe(),"obligations":assertion.obligations,
                    });
                    if !matched {
                        result.failures.push(check.clone());
                    }
                    result.checks.push(check);
                }
            }
        }
    }
    Ok(result)
}

/// Original strict Excel-specification test entry point, now backed by the shared JSON table.
/// This retains the registry inventory check and does not waive compatibility gaps.
pub fn check_table() -> Result<(), String> {
    let root = crate::package_root();
    let inventory: schema::RegistryInventory = serde_json::from_slice(
        &std::fs::read(root.join("../poirot-test-excel-specs/test-specs/inventory.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let registered: std::collections::BTreeSet<_> = inventory
        .features
        .iter()
        .filter(|f| f.implementation_status == schema::ImplementationStatus::Registered)
        .map(|f| f.name.as_str())
        .collect();
    if registered != eval::SUPPORTED_FUNCTIONS.iter().copied().collect() {
        return Err("Excel specification inventory differs from the public registry".into());
    }
    let bytes = std::fs::read(root.join("corpus/json/cases.json")).map_err(|e| e.to_string())?;
    let cases = parse(&bytes)?;
    if cases.len() < 1000 {
        return Err("at least 1,000 Excel specifications are required".into());
    }
    let mut failures = Vec::new();
    for case in &cases {
        let observed = observe(case)?;
        if !observed.failures.is_empty() || !observed.mode_errors.is_empty() {
            failures.push(format!(
                "{}: {} assertion failures; {:?}",
                case.name,
                observed.failures.len(),
                observed.mode_errors
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
