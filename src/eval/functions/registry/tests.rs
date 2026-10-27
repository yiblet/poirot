use super::arity::Arity;
use super::catalog::{
    EXCEL_DESCRIPTORS, FUNCTION_DESCRIPTORS, SUPPORTED_FUNCTIONS, resolve_excel_function,
    resolve_function,
};
use super::dependency_policy::DependencyPolicy;
use super::evaluation_strategy::EvaluationStrategy;
use super::function_id::FunctionId;
use super::logical_function::LogicalFunction;
use super::udf_identity::UdfIdentity;

#[test]
fn descriptor_contracts_and_family_owned_execution_are_not_stubs() {
    let mut workbook = crate::model::Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let address = crate::model::CellAddress { row: 0, col: 0 };
    assert_eq!(SUPPORTED_FUNCTIONS.len(), 262);
    let mut names = std::collections::HashSet::new();
    for descriptor in FUNCTION_DESCRIPTORS {
        assert!(names.insert(descriptor.name));
        assert_eq!(
            resolve_function(&poirot_parsing::formula::FunctionName::from(
                descriptor.name
            )),
            Some(descriptor)
        );
        let strategy = match descriptor.handler {
            FunctionId::Logical(
                LogicalFunction::If
                | LogicalFunction::IfError
                | LogicalFunction::IfNa
                | LogicalFunction::Choose
                | LogicalFunction::Ifs
                | LogicalFunction::Switch,
            ) => EvaluationStrategy::Lazy,
            FunctionId::Math(_) | FunctionId::Text(_) => EvaluationStrategy::FamilyControlled,
            _ => EvaluationStrategy::FamilyControlled,
        };
        assert_eq!(descriptor.strategy, strategy, "{}", descriptor.name);
        let arguments = vec!["1"; descriptor.arity.minimum].join(",");
        let formula = format!("{}({arguments})", descriptor.name);
        match crate::eval::entry::run_formula(
            &workbook,
            0,
            address,
            &formula,
            Default::default(),
            Default::default(),
        ) {
            crate::eval::limits::EvaluationOutcome::Complete { value, .. } => assert_ne!(
                value,
                crate::model::CellValue::Error(crate::model::CellError::Name),
                "advertised handler missing: {formula}"
            ),
            crate::eval::limits::EvaluationOutcome::Aborted {
                failure:
                    crate::eval::limits::EvaluationFailure::UnsupportedCapability { detail, .. },
                ..
            } => {
                assert!(
                    !detail.is_empty(),
                    "missing capability diagnostic: {formula}"
                );
            }
            outcome => panic!("unexpected execution failure for {formula}: {outcome:?}"),
        }
    }
    for (name, minimum, maximum) in [
        ("IF", 2, 3),
        ("IFERROR", 2, 2),
        ("CHOOSE", 2, usize::MAX),
        ("NOT", 1, 1),
        ("MATCH", 2, 3),
        ("VLOOKUP", 3, 4),
        ("HLOOKUP", 3, 4),
        ("INDEX", 2, 4),
        ("ROW", 0, 1),
        ("ROWS", 1, 1),
        ("DATE", 3, 3),
        ("YEARFRAC", 2, 3),
        ("ABS", 1, 1),
        ("ROUND", 2, 2),
        ("TEXT", 2, 2),
        ("CONCAT", 1, 255),
    ] {
        assert_eq!(
            resolve_function(&poirot_parsing::formula::FunctionName::from(name))
                .unwrap()
                .arity,
            Arity { minimum, maximum },
            "{name}"
        );
    }
    for name in ["ABS", "POWER", "LOG", "TRUNC"] {
        let descriptor =
            resolve_function(&poirot_parsing::formula::FunctionName::from(name)).unwrap();
        assert_eq!(descriptor.strategy, EvaluationStrategy::FamilyControlled);
        assert!(descriptor.dependencies.contains(DependencyPolicy::TIME));
        assert!(
            descriptor
                .dependencies
                .contains(DependencyPolicy::LOCALIZATION)
        );
        assert!(
            descriptor
                .dependencies
                .contains(DependencyPolicy::REFERENCE_SELECTION)
        );
        assert!(
            !descriptor
                .dependencies
                .contains(DependencyPolicy::DATE_SYSTEM)
        );
    }
    let text = resolve_function(&poirot_parsing::formula::FunctionName::from("TEXT")).unwrap();
    assert!(text.dependencies.contains(DependencyPolicy::DATE_SYSTEM));
    assert!(
        resolve_function(&poirot_parsing::formula::FunctionName::from("NOW"))
            .unwrap()
            .dependencies
            .contains(DependencyPolicy::TIME)
    );
    assert!(
        resolve_function(&poirot_parsing::formula::FunctionName::from("YEARFRAC"))
            .unwrap()
            .dependencies
            .contains(DependencyPolicy::LOCALIZATION)
    );
    assert!(resolve_function(&poirot_parsing::formula::FunctionName::from("_xlfn.IFNA")).is_some());
    assert!(resolve_function(&poirot_parsing::formula::FunctionName::from("_XLFN.IFNA")).is_none());
    assert!(
        resolve_function(&poirot_parsing::formula::FunctionName::from(
            "_xlfn.PERCENTILE.INC"
        ))
        .is_none()
    );
    assert_ne!(
        UdfIdentity {
            registration: 1,
            version: 1
        },
        UdfIdentity {
            registration: 1,
            version: 2
        }
    );
}

#[test]
fn policy_catalog_adds_only_excel_policy_registrations() {
    let pinned: Vec<_> = super::function_descriptor::descriptors_for_policy(
        crate::eval::config::BuiltinSemanticsPolicy::PinnedPoi,
    )
    .collect();
    assert_eq!(
        pinned,
        super::catalog::FUNCTION_DESCRIPTORS
            .iter()
            .collect::<Vec<_>>()
    );
    let excel: Vec<_> = super::function_descriptor::descriptors_for_policy(
        crate::eval::config::BuiltinSemanticsPolicy::ExcelDocumentation,
    )
    .collect();
    assert_eq!(
        excel.len(),
        pinned.len() + super::catalog::EXCEL_DESCRIPTORS.len()
    );
    for d in super::catalog::EXCEL_DESCRIPTORS {
        assert!(excel.contains(&d));
        assert!(!pinned.contains(&d));
    }
}

#[test]
fn atomic_dependency_categories_have_distinct_bits() {
    let categories = [
        DependencyPolicy::ARGUMENTS,
        DependencyPolicy::POSITION,
        DependencyPolicy::TIME,
        DependencyPolicy::LOCALIZATION,
        DependencyPolicy::DATE_SYSTEM,
        DependencyPolicy::REFERENCE_SELECTION,
        DependencyPolicy::NAMESPACE,
        DependencyPolicy::SEED,
        DependencyPolicy::CURRENCY,
        DependencyPolicy::HOST_POLICY,
    ];
    for (index, category) in categories.iter().enumerate() {
        assert_eq!(category.0.count_ones(), 1);
        for other in &categories[index + 1..] {
            assert_eq!(category.0 & other.0, 0);
        }
    }
}

#[test]
fn typed_descriptors_match_ordered_catalog_scan_for_every_alias() {
    for declared in FUNCTION_DESCRIPTORS.iter().chain(EXCEL_DESCRIPTORS) {
        let expected = FUNCTION_DESCRIPTORS
            .iter()
            .chain(EXCEL_DESCRIPTORS)
            .find(|descriptor| descriptor.handler == declared.handler)
            .unwrap();
        assert_eq!(declared.handler.descriptor(), expected, "{}", declared.name);
        assert_eq!(
            declared.handler.canonical_name(),
            expected.name,
            "{}",
            declared.name
        );
        let resolved =
            resolve_function(&poirot_parsing::formula::FunctionName::from(declared.name))
                .map(|descriptor| descriptor.handler)
                .or_else(|| {
                    resolve_excel_function(&poirot_parsing::formula::FunctionName::from(
                        declared.name,
                    ))
                });
        assert_eq!(resolved, Some(declared.handler), "{}", declared.name);
    }
    // This documented Excel alias reuses an advertised handler without a row.
    let percentile = resolve_excel_function(&poirot_parsing::formula::FunctionName::from(
        "PERCENTILE.INC",
    ))
    .unwrap();
    assert_eq!(
        percentile.descriptor(),
        resolve_function(&poirot_parsing::formula::FunctionName::from("PERCENTILE")).unwrap()
    );
    assert_eq!(percentile.canonical_name(), "PERCENTILE");
}
