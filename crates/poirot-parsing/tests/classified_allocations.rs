//! Isolated allocation measurement for the classification fast path.
#[test]
fn classified_zero_allocations_on_success_miss_and_unsupported() {
    use poirot_parsing::formula::classified::{
        TokenKind, Tokens, cell_prefix, normalized_equal, normalized_hash,
    };
    for source in [
        "SUM(A1,LOG10($B$2))+C3:D4",
        "A1 (A2)",
        "A1+\"quoted A2\"",
        "Sheet1!A1",
        "VLOOKUP(A1,'O''Brien'!$B$2:C3,2,FALSE)",
        "A:B",
        "XFE1",
        "A1[bad]",
        "A1+\"unterminated",
    ] {
        let allocations = allocation_counter::measure(|| {
            let prefix = cell_prefix(source);
            let hash = normalized_hash(source);
            let equal = normalized_equal(source, "SUM(Z9,LOG10(C3))+D4:E5");
            let mut cells = 0;
            for token in Tokens::new(source) {
                if matches!(token, Ok(t) if matches!(t.kind, TokenKind::Cell(_))) {
                    cells += 1;
                }
            }
            let _ = std::hint::black_box((prefix, hash, equal, cells));
        });
        assert_eq!(allocations.count_total, 0, "{source}");
    }
}
