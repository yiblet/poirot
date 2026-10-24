//! Isolated allocator instrumentation: parsing is explicitly outside the measured region.
fn measure_allocations<T>(operation: impl FnOnce() -> T) -> (T, u64) {
    let mut result = None;
    let allocations = allocation_counter::measure(|| result = Some(operation()));
    (result.unwrap(), allocations.count_total)
}

#[test]
fn bytecode_only_final_buffers_and_shared_owner_allocate() {
    // Parent registries are built once. Measure compilation after registration,
    // so this gate continues to count only per-compile buffers and ownership.
    // Final buffers retain expression typing and, for calls, the dense label table.
    // Both share with the executable; branches do not allocate separate programs.
    use poirot_eval::adapters::FunctionAdapter;
    assert!(
        poirot_eval::adapters::builtins()
            .resolve(&"SUM".into())
            .is_some()
    );
    for (source, expected) in [("1+2", 3), ("SUM(A1,IFERROR(B2,\"fallback\"))", 8)] {
        let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
        let (result, allocations) = measure_allocations(|| poirot_eval::bytecode::compile(&syntax));
        assert!(result.is_ok());
        assert_eq!(allocations, expected, "{source}");
        let result = result.unwrap();
        drop(syntax);
        assert!(!result.template.code.is_empty());
    }
    let bad = poirot_parsing::formula::Expr::Array(vec![]);
    let (result, allocations) = measure_allocations(|| poirot_eval::bytecode::compile(&bad));
    assert!(result.is_err());
    assert_eq!(allocations, 0);
}

#[test]
fn classified_hit_allocates_only_final_instance_storage() {
    use poirot_eval::compiler::{EvaluationPolicy, FormulaCompiler, PreparedFormula};
    let mut compiler = FormulaCompiler::new();
    let policy = EvaluationPolicy::default();
    let first = compiler.compile_formula("A1+B2", &policy);
    let (second, allocations) = measure_allocations(|| compiler.compile_formula("C3+D4", &policy));
    // Reference slots/data, potential references, source Arc, two prepared Arcs,
    // and the source-index String. No syntax, bytecode or transient token buffers.
    assert_eq!(allocations, 7);
    assert_eq!(compiler.usage().parse_attempts, 1);
    assert_eq!(compiler.usage().bind_attempts, 1);
    assert_eq!(compiler.template_hits(), 1);
    let (PreparedFormula::Ready(first), PreparedFormula::Ready(second)) =
        (first.as_ref(), second.as_ref())
    else {
        panic!("valid formulas")
    };
    assert!(std::sync::Arc::ptr_eq(
        &first.bytecode().executable,
        &second.bytecode().executable
    ));
}
