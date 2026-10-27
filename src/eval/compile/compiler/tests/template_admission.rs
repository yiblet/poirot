use super::super::cache_snapshot;
use super::super::formula_compiler::FormulaCompiler;
use super::super::preparation_policy::PreparationPolicy;
use super::super::prepared_formula::PreparedFormula;

#[test]
fn rejected_range_shapes_do_not_accumulate_template_candidates_after_restore() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    for row in 1..=32 {
        compiler.compile_formula(&format!("TRANSPOSE(A{row}:B{row})"), &policy);
    }
    assert!(compiler.templates[0].classified.is_empty());
    assert_eq!(compiler.usage().parse_attempts, 32);
    let snapshot = cache_snapshot::CacheSnapshot::capture(&compiler, &policy)
        .unwrap()
        .into_owned();
    let mut restored = snapshot.restore(&policy).unwrap();
    assert!(restored.templates[0].classified.is_empty());
    restored.compile_formula("TRANSPOSE(A33:B33)", &policy);
    assert_eq!(restored.usage().parse_attempts, 1);
    assert!(restored.templates[0].classified.is_empty());
}

/// Bucket keys and exemplar sources of the first policy's rebinding index.
fn template_index(compiler: &FormulaCompiler) -> Vec<(u64, Vec<String>)> {
    let mut index: Vec<_> = compiler.templates[0]
        .classified
        .iter()
        .map(|(hash, bucket)| {
            let mut sources: Vec<_> = bucket.iter().map(|f| f.source.to_string()).collect();
            sources.sort();
            (*hash, sources)
        })
        .collect();
    index.sort();
    index
}

/// Round-trip a compiler cache through its wire encoding.
fn restore_encoded(compiler: &FormulaCompiler, policy: &PreparationPolicy) -> FormulaCompiler {
    let snapshot = cache_snapshot::CacheSnapshot::capture(compiler, policy).unwrap();
    let bytes = postcard::to_allocvec(&snapshot).unwrap();
    postcard::from_bytes::<cache_snapshot::CacheSnapshot<'static>>(&bytes)
        .unwrap()
        .restore(policy)
        .unwrap()
}

#[test]
fn restored_template_index_matches_the_captured_compiler_without_reparsing() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for source in ["A1+1", "B2+1", "C3*2", "SUM(A1:B2)", "1+", "$A$1-B$2"] {
        compiler.compile_formula(source, &policy);
    }
    assert_eq!(compiler.template_hits(), 1);
    let expected = template_index(&compiler);
    // Only freshly compiled formulas become exemplars; the rebound "B2+1" does not.
    let sources: Vec<_> = expected.iter().flat_map(|(_, s)| s.clone()).collect();
    assert!(sources.contains(&"A1+1".to_owned()));
    assert!(!sources.contains(&"B2+1".to_owned()));
    let mut restored = restore_encoded(&compiler, &policy);
    assert_eq!(template_index(&restored), expected);
    // Restored exemplars rebind new sources exactly as a fresh compiler prepares them.
    for source in ["D4+1", "E5*2", "C$1-$D7"] {
        let reused = restored.compile_formula(source, &policy);
        let fresh = FormulaCompiler::new().compile_formula(source, &policy);
        let (PreparedFormula::Ready(reused), PreparedFormula::Ready(fresh)) =
            (reused.as_ref(), fresh.as_ref())
        else {
            panic!("{source} did not compile");
        };
        assert_eq!(reused.bytecode(), fresh.bytecode(), "{source}");
        assert_eq!(
            reused.potential_dependencies(),
            fresh.potential_dependencies()
        );
    }
    assert_eq!(restored.usage().parse_attempts, 0);
    assert_eq!(restored.template_hits(), 3);
}

#[test]
fn wrong_template_keys_only_miss_rebinding() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    compiler.compile_formula("A1+1", &policy);
    compiler.compile_formula("SUM(A1)*3", &policy);
    let mut restored = restore_encoded(&compiler, &policy);
    // Simulate untrusted keys: file each exemplar under the other's key.
    let classified = &mut restored.templates[0].classified;
    let plus = poirot_parsing::formula::classified::normalized_hash("A1+1").unwrap();
    let sum = poirot_parsing::formula::classified::normalized_hash("SUM(A1)*3").unwrap();
    let plus_bucket = classified.remove(&plus).unwrap();
    let sum_bucket = classified.remove(&sum).unwrap();
    classified.insert(plus, sum_bucket);
    classified.insert(sum, plus_bucket);
    for source in ["B2+1", "SUM(B2)*3"] {
        let prepared = restored.compile_formula(source, &policy);
        let fresh = FormulaCompiler::new().compile_formula(source, &policy);
        let (PreparedFormula::Ready(prepared), PreparedFormula::Ready(fresh)) =
            (prepared.as_ref(), fresh.as_ref())
        else {
            panic!("{source} did not compile");
        };
        assert_eq!(prepared.bytecode(), fresh.bytecode(), "{source}");
    }
    assert_eq!(restored.template_hits(), 0);
    assert_eq!(restored.usage().parse_attempts, 2);
}

#[test]
fn snapshot_sources_restore_exactly_and_reject_invalid_text() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for source in ["\"é\"&A1", "1+", "SUM(B1:B3)"] {
        compiler.compile_formula(source, &policy);
    }
    let restored = restore_encoded(&compiler, &policy);
    let sources = |compiler: &FormulaCompiler| {
        let mut sources: Vec<_> = compiler.pool[0]
            .1
            .iter()
            .map(|(key, prepared)| {
                assert_eq!(&**key, prepared.source());
                key.to_string()
            })
            .collect();
        sources.sort();
        sources
    };
    assert_eq!(sources(&restored), sources(&compiler));
    let snapshot = cache_snapshot::CacheSnapshot::capture(&compiler, &policy).unwrap();
    let mut bytes = postcard::to_allocvec(&snapshot).unwrap();
    // Formula sources follow the programs, whose operands may also hold the literal.
    let at = bytes.windows(2).rposition(|w| w == "é".as_bytes()).unwrap();
    bytes[at] = 0xFF;
    assert!(postcard::from_bytes::<cache_snapshot::CacheSnapshot<'static>>(&bytes).is_err());
}
