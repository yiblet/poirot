# poirot-functions

Workbook-independent spreadsheet algorithms, argument coercion, exact scalar
values and Excel display formatting. Use this crate when you supply operands and
execution policy yourself. For workbook loading, formula evaluation or incremental
scenarios, start with [poirot](https://docs.rs/poirot).

```rust
fn main() -> Result<(), poirot_functions::format::FormatCompileError> {
    use poirot_functions::{math, scalar::CellValue};
    use poirot_functions::format::{
        compile_format, render_value, FormatCompilePolicy, FormatRenderContext,
    };

    let rounded = match math::round(12.345, 2.) {
        Ok(number) => CellValue::Number(number),
        Err(error) => CellValue::Error(error),
    };
    let plan = compile_format("0.00", FormatCompilePolicy::default())?;
    let display = render_value(
        &rounded, &plan, FormatRenderContext::default(),
    )?;
    assert_eq!(display.text, "12.35");
    Ok(())
}
```

| Task | Modules |
| --- | --- |
| Represent values and deferred arguments | `scalar` |
| Select argument conversions and retain operand origin | `coercion` |
| Compare spreadsheet numbers or operate on exact text units | `numeric`, `text_units` |
| Compile number formats and render scalar values | `format` |
| Match criteria and aggregate database rows | `criteria`, `database` |
| Run a function algorithm on supplied operands | `dates`, `engineering`, `financial`, `logical`, `lookup`, `math`, `matrix`, `statistics`, `text` |

These algorithms do not resolve cell references or own a workbook. Typed numeric
kernels accept already-admitted operands; family coercion APIs distinguish direct,
reference and array inputs. Hosts supply context such as calendar, locale, lazy
argument forcing and resource admission where required. Spreadsheet errors use
`scalar::CellError`; APIs with allocation, geometry or host failures expose
separate module-owned `thiserror` types, such as `lookup::LookupError` and
`statistics::StatisticalKernelError`. Match their variants and typed payloads;
their source chains retain dependency and host causes. Consult each function's contract for admitted inputs,
error precedence and compatibility policy.

## Regex backend

Rust regex is the default backend and requires no C dependency. Enable
`features = ["pcre2"]` to select PCRE2 on native targets.
`text::regex::Program::BACKEND` identifies the
selected semantics. Lookarounds, backreferences and PCRE2 directives are
unsupported with Rust regex, and empty-match iteration differs. Caller admission
still applies; `ExecutionLimits::heap_kib` bounds compiled-program size, while
PCRE2's backtracking step and depth guards are unused.
