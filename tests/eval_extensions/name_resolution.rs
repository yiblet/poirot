use super::support::{contract, identity, run, value, workbook};
use poirot::eval::{
    compile::bound::{Expression, FormulaPlan},
    functions::extensions::*,
    functions::registry::{Arity, resolve_function},
};
use poirot::model::{CellError, CellValue};

#[test]
fn source_specific_aliases_preserve_typed_handlers_and_prefix_case() {
    assert_eq!(
        resolve_function(&poirot_parsing::formula::FunctionName::from("_xlfn.ifna"))
            .unwrap()
            .handler,
        resolve_function(&poirot_parsing::formula::FunctionName::from("IFNA"))
            .unwrap()
            .handler
    );
    assert!(resolve_function(&"_XLFN.IFNA".into()).is_none());
    let syntax = poirot_parsing::formula::parse_formula("_xlfn.IFNA(1,2)").unwrap();
    assert!(matches!(
        FormulaPlan::bind(&syntax).unwrap().expression(),
        Expression::Logical(_)
    ));
    assert_eq!(
        value(run(
            &workbook(),
            &ExtensionRegistry::new(),
            "_xlfn.IFNA(#N/A,17)"
        )),
        CellValue::Number(17.)
    );
}

#[test]
fn atp_registration_guards_reserve_builtin_names() {
    let mut registry = ExtensionRegistry::new();
    let declared = contract(Arity {
        minimum: 1,
        maximum: 1,
    });
    assert_eq!(
        registry.register(
            ExtensionNamespace::AnalysisToolPak,
            "accrint",
            "v1",
            declared,
            identity
        ),
        Err(ExtensionRegistrationError::Builtin)
    );
    assert_eq!(
        registry.register(
            ExtensionNamespace::AnalysisToolPak,
            "SUM",
            "v1",
            declared,
            identity
        ),
        Err(ExtensionRegistrationError::Builtin)
    );
    assert_eq!(
        registry.register(
            ExtensionNamespace::UserDefined,
            "unknown",
            "",
            declared,
            identity
        ),
        Err(ExtensionRegistrationError::MissingSignature)
    );
    registry
        .register(
            ExtensionNamespace::AnalysisToolPak,
            "RTD",
            "custom-rtd-v1",
            declared,
            identity,
        )
        .unwrap();
    assert!(registry.resolve("_xlfn.rtd").is_some());
    assert!(registry.resolve("_XLFN.RTD").is_none());
    assert_eq!(
        value(run(&workbook(), &registry, "_xlfn.RTD(9)")),
        CellValue::Number(9.)
    );
}

#[test]
fn frozen_external_names_reuse_first_spelling_without_widening_the_finder() {
    use poirot::eval::functions::external_namespace::{
        ExternalFunctionNamespace, ExternalNameLookup,
    };
    let workbook = workbook();
    let mut registry = ExtensionRegistry::new();
    // Independent fresh names keep the direct finder boundary: native POI throws
    // NotImplementedException here; unknown functions return the spreadsheet #NAME? error.
    for spelling in ["_XLFN.IFNA", "_Xlfn.IfNa"] {
        assert_eq!(
            value(run(&workbook, &registry, &format!("{spelling}(#N/A,2)"))),
            CellValue::Error(CellError::Name)
        );
    }
    let namespace =
        ExternalFunctionNamespace::from_stored_names(["_xlfn.IFNA", "_XLFN.ifna"], 2).unwrap();
    assert_eq!(
        namespace.lookup("_Xlfn.IfNa"),
        ExternalNameLookup::Stored("_xlfn.IFNA")
    );
    registry.set_external_namespace(Some(namespace)).unwrap();
    for spelling in ["_xlfn.IFNA", "_xlfn.ifna", "_XLFN.IFNA", "_Xlfn.IfNa"] {
        assert_eq!(
            value(run(&workbook, &registry, &format!("{spelling}(#N/A,2)"))),
            CellValue::Number(2.)
        );
    }
    // Reverse insertion is a distinct namespace identity and preserves the opaque
    // first spelling; the adapter must not silently repair it into an ATP alias.
    let _original = registry.identity();
    registry
        .set_external_namespace(Some(
            ExternalFunctionNamespace::from_stored_names(["_Xlfn.IfNa", "_xlfn.IFNA"], 2).unwrap(),
        ))
        .unwrap();

    for spelling in ["_xlfn.IFNA", "_xlfn.ifna", "_XLFN.IFNA", "_Xlfn.IfNa"] {
        assert_eq!(
            value(run(&workbook, &registry, &format!("{spelling}(#N/A,2)"))),
            CellValue::Error(CellError::Name)
        );
    }
}

#[test]
fn explicit_external_namespace_is_bounded_and_preserves_raw_plan_syntax() {
    use poirot::eval::functions::external_namespace::{
        ExternalFunctionNamespace, ExternalNameLookup, ExternalNamespaceError,
    };
    assert_eq!(
        ExternalFunctionNamespace::from_stored_names(["_xlfn.IFNA", "_xlfn.IFNA"], 1),
        Err(ExternalNamespaceError::ResourceLimit)
    );
    assert_eq!(
        ExternalFunctionNamespace::from_stored_names(["café"], 1),
        Err(ExternalNamespaceError::UnsupportedName)
    );
    let namespace = ExternalFunctionNamespace::from_stored_names(["_xlfn.IFNA"], 1).unwrap();
    assert_eq!(namespace.lookup("UNKNOWN"), ExternalNameLookup::Absent);
    assert_eq!(
        namespace.lookup("café"),
        ExternalNameLookup::UnsupportedName
    );
    let mut registry = ExtensionRegistry::new();
    registry.set_external_namespace(Some(namespace)).unwrap();
    let syntax = poirot_parsing::formula::parse_formula("_Xlfn.IfNa(#N/A,2)").unwrap();
    let plan = FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    assert_eq!(plan.syntax(), &syntax);
    assert!(matches!(plan.expression(), Expression::Logical(_)));
}
