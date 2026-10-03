use poirot::{EvaluationContext, Overlay, OverlayPolicy, eval, model};
use schema::{
    Case, Cell, CellContent, Currency, EvidenceKind, ImplementationStatus, Name, RegistryInventory,
    Scalar, SettingsUpdate, TextPolicy,
};

mod schema;

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

fn write_cells(workbook: &mut model::Workbook, cells: &[Cell]) {
    for cell in cells {
        let worksheet = workbook
            .sheet_mut(cell.target.sheet)
            .expect("fixture worksheet");
        match &cell.content {
            CellContent::Formula(formula) => worksheet
                .set_formula(cell.target.address, formula, model::CellValue::Blank)
                .unwrap(),
            CellContent::Literal(value) => worksheet
                .set_value(cell.target.address, value.cell_value())
                .unwrap(),
        }
    }
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
        assert!(
            tolerance.is_finite() && tolerance >= 0.0,
            "valid absolute tolerance"
        );
        matches!((actual, &expected), (model::CellValue::Number(a), model::CellValue::Number(e))
            if a.is_finite() && (a - e).abs() <= tolerance)
    } else if let (Some(a), Some(e)) = (actual.text_ref(), expected.text_ref()) {
        a.units().eq(e.units())
    } else {
        actual == &expected
    }
}

fn run_case(case: &Case) -> Result<(), String> {
    let label = format!(
        "case {:04}: {} [{}; {}]; {}; sources: {}",
        case.id,
        case.name,
        case.compatibility,
        case.category,
        case.rationale,
        case.source_urls.join(", ")
    );
    let mut model = model::Workbook::new();
    for name in &case.sheets {
        model.add_sheet(name).unwrap();
    }
    model.date_system = case.settings.date_system.into();
    write_cells(&mut model, &case.cells);
    write_names(&mut model, &case.names);
    let mut settings = Settings::new(case);
    let mut overlay = Overlay::with_workbook(std::sync::Arc::new(model.clone()))
        .with_policy(settings.policy())
        .build()
        .unwrap();
    let mut failures = Vec::new();
    for (step_index, step) in case.steps.iter().enumerate() {
        if let Some(update) = &step.settings {
            settings.update(update);
            overlay.set_policy(settings.policy()).unwrap();
        }
        write_cells(&mut model, &step.edits);
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
                .unwrap();
        } else {
            for edit in &step.edits {
                match &edit.content {
                    CellContent::Literal(value) => overlay
                        .set_value(edit.target.evaluation(), value.cell_value())
                        .unwrap(),
                    CellContent::Formula(formula) => overlay
                        .set_formula(edit.target.evaluation(), formula)
                        .unwrap(),
                }
            }
        }
        // Array assertions expand to every worksheet coordinate in the footprint.
        let targets: Vec<_> = step
            .assertions
            .iter()
            .filter(|assertion| assertion.evidence.kind != EvidenceKind::Unresolved)
            .flat_map(|assertion| {
                assertion
                    .expected_cells()
                    .into_iter()
                    .map(|(target, _)| target)
            })
            .collect();
        let context = settings.execution_context();
        let mut evaluation = overlay
            .evaluation(&context)
            .map_err(|e| format!("{label}: {e}"))?;
        let outputs = match evaluation.outputs(&targets) {
            Ok(values) => values,
            Err(error) => {
                failures.push(format!("{label}, step {step_index}: {error:?}"));
                continue;
            }
        };
        let values = outputs
            .into_iter()
            .map(|r| ((r.target.sheet, r.target.address), r.value))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (assertion_index, assertion) in step.assertions.iter().enumerate() {
            let evidence = assertion.evidence.describe();
            if assertion.evidence.kind == EvidenceKind::Unresolved {
                failures.push(format!("{label}, step {step_index}, assertion {assertion_index}: unresolved expectation: {evidence}"));
                continue;
            }
            for (at, expected) in assertion.expected_cells() {
                let actual = values.get(&(at.sheet, at.address));
                if !actual
                    .is_some_and(|actual| scalar_matches(actual, expected, assertion.tolerance))
                {
                    failures.push(format!("{label}, step {step_index}, assertion {assertion_index}, {at:?}: expected {expected:?}, got {actual:?}; evidence: {evidence}; obligations: {:?}", assertion.obligations));
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Validate evidence and execute the documented Excel case table through the public overlay API.
pub(super) fn run_table() {
    let inventory: RegistryInventory =
        serde_json::from_str(include_str!("../../test-specs/inventory.json"))
            .expect("typed coverage inventory");
    let registered: std::collections::BTreeSet<_> = inventory
        .features
        .iter()
        .filter(|feature| feature.implementation_status == ImplementationStatus::Registered)
        .map(|feature| feature.name.as_str())
        .collect();
    let actual: std::collections::BTreeSet<_> = eval::SUPPORTED_FUNCTIONS.iter().copied().collect();
    assert_eq!(
        registered, actual,
        "update the local inventory when ROI's public registry changes"
    );
    let cases: Vec<Case> = serde_json::from_str(include_str!("../../test-specs/cases.json"))
        .expect("typed Excel specification table");
    assert!(cases.len() >= 1000, "at least 1,000 specified cases");
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case.schema_version, 2, "supported specification schema");
        assert_eq!(case.id, index, "stable contiguous case IDs");
        // Failures in one fixture never hide later cases.
        match std::panic::catch_unwind(|| run_case(case)) {
            Ok(Ok(())) => {}
            Ok(Err(failure)) => failures.push(failure),
            Err(payload) => {
                let detail = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic");
                failures.push(format!("case {index:04}: {}: {detail}", case.name));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} Excel cases failed:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}
