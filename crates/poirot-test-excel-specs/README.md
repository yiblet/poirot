# Excel workbook tests

One table-driven E2E test creates workbooks, sets values/formulas/names, evaluates outputs, edits inputs, and checks typed results. Cases cite Microsoft documentation and link to the [coverage inventory](test-specs/inventory.json). [Coverage](test-specs/coverage.md) lists specified and missing obligations; specifications are not a claim of passing poirot behavior.

All fixtures, evidence, generation code and maintenance inputs are package-local. poirot is the engine dependency.

Run from this crate directory:

```sh
cargo test -p poirot-test-excel-specs --test workbook_e2e
cargo clippy --all-targets --all-features --locked --offline -- -D warnings
python3 tools/maintain.py --check
```

`python3 tools/maintain.py` regenerates the reviewed matrix prefix, source index and coverage reports while preserving authored cases in the one JSON table. Python tools use only the standard library. See [the schema and authoring guide](test-specs/README.md).

To grow coverage, inventory the behavior first, research its rule, author a small case with explained evidence, then add variations for distinct semantic paths. Append authored cases directly to `cases.json`; update `generate.py`/`extend.py` only for reviewed matrix-generation rules. Never use poirot output as the expected-result authority.

The runner checks the default 256-entry registry through poirot's public API; `registered` is reserved for that projection. `partial` features have explicitly scoped policy support in the inventory and generated coverage reports. The runner selects `ExcelDocumentation` builtin contracts, the Version 1 POI UTF-16 proxy or `ExcelCompatibilityV2Documented`, and `DynamicSpill` only for fixtures that request it. It reads every footprint cell from the public runtime and preserves exact UTF-16 units.

Supported scope includes documented modern statistical aliases and bounded family overrides, V2 LEN/MID/LEFT/RIGHT/REPLACE counting, typed static spills and SUBTOTAL with explicitly declared visibility. FIND/SEARCH remain pinned unit adapters; casing remains source specific. General broadcasting/CSE, unbounded dynamic-reference producers, incremental spill execution, mutable cache materialization and imported row-visibility fidelity remain outside scope. See [builtin contracts](../poirot-eval/docs/excel-builtin-policy.md), [dynamic array limits](../poirot-eval/docs/dynamic-array-results.md) and [text policy scope](../poirot-text/CONTRACT.md).

Runtime failures remain visible. Specification generation and validation check authoring evidence and coverage links without executing evaluator tests; generated `runtime_tests: not_run` and `passing_behavior: unverified` describe that authoring step. Report actual `cargo test` outcomes separately. Neither metadata nor documented derivations claim native Excel observations or complete compatibility.
