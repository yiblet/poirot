# XLS export planning

`XlsExportPlan::new(&workbook, policy)` checks workbook identities, name scopes,
BIFF representation, cached value types, SST entries, custom number formats and
formula token streams before serialization. It performs no filesystem or container
IO and does not modify or recalculate the workbook. The plan borrows the workbook
immutably for its entire lifetime, so safe Rust cannot execute it after mutating
that workbook.

`encode_workbook_stream()` emits BIFF records. `wrap_xls_container()` independently
wraps an already validated BIFF stream in CFB. `to_buffer()` composes these steps;
`write_file()` completes both before writing the destination. The public
`write_xls_to_buffer` and `write_xls` wrappers use the default policy.

`predicted_stream_bytes()` counts the entire BIFF Workbook stream, including SST
and cached STRING continuations. It excludes CFB container overhead.
`BiffFormulaPlan::predicted_record_bytes()` counts one cell FORMULA record and
excludes any associated cached STRING records.

Policy limits are operational export budgets. BIFF representation limits always
apply. `strict_names: false` permits imported names outside the authoring syntax,
while still rejecting empty or oversized names, control characters, duplicate
identities and invalid scopes. This switch does not expand formula support.

SST and format entries are checked before insertion. Known record sizes and cached
strings count toward the stream budget before formula parsing. Formula lowering
checks its token budget incrementally after each emitted subtree; allocation can
exceed that budget by one bounded token before rejection. The formula parser's
own input/AST allocation policy is separate from token lowering.

Errors distinguish unsupported features, representation failures, resource limits
and IO failures. Cell and defined-name failures retain their location through
`XlsError::Context`; IO errors retain their original `std::io::Error` as a source.

Independent tests inspect raw reference and function tokens, typed cached values,
Unicode SST/string continuation flags and NAME records. A deliberately wrong
cached value cannot establish formula token correctness. Calamine has known
Unicode formula/name, relative-area and long cached STRING continuation gaps;
those reader limitations do not justify changing valid writer bytes. Microsoft
Excel compatibility remains unverified without direct evidence.
