# Deferred function host

`CallHost` is an internal adapter between a bound family call and one immutable evaluation session. It does not look up function names. Binding resolves each name to a typed family identifier; that family forwards directly to its helper's typed executor.

Each argument is evaluated once, on demand. Inspecting a reference's shape does not read its cells. Scalar selection, indexed reads, reference resizing and collection traversal are distinct operations. A reference to one cell and a one-cell area retain different identities, including through names and reference-returning calls. Unions remain lists of references; a rectangular consumer cannot silently flatten them into a fabricated rectangle.

`ScalarOperand` retains origin and missing-argument metadata. A literal missing argument differs from a blank referenced cell. Direct literal array missing slots retain metadata where the bound syntax exposes them. Existing legacy handlers keep their accepted coercion paths; helper bridges explicitly select the pinned POI numeric conversion policy.

Every host query returns either data, a genuine spreadsheet error, or an operational failure. Unsupported helper domains are `UnsupportedCapability` failures, so IFERROR cannot turn them into an apparent successful spreadsheet result. Unknown function names still follow the accepted deferred `#NAME?` behavior.

Host reservations check materialized-value or UTF-8 buffer limits before copying. Helper operation estimates consume the same run and cell work budgets as engine operations. Indexed traversal charges actual requested cells; metadata inspection does not allocate a whole dense rectangle. These units are explicit execution-policy units, not a measurement of every CPU instruction or allocator call.

The host captures the session's execution context and workbook date system.
Dependency reads feed session tracing and incremental invalidation where supported.
Dynamic, external and structured references require actual bounded workbook
queries; unavailable domains produce operational diagnostics. Missing-slot
metadata already lowered to ordinary blank values is not reconstructed.

Helper crates and bridge files are separate from this shared host. A bridge owns source-specific argument order and coercion, while the session owns cell resolution, context, budgets and final atomic cache publication.

## Reference classification provenance

Pinned `WorkbookEvaluator.java:879` converts array tokens to an area value. `LogicalFunction.java:145` classifies RefEval, AreaEval and RefListEval as references, and `Index.java:134`/`:143` returns row/column views. The ISREF bridge therefore reports true for a literal array and for a one-cell INDEX view of an array. This is source-derived POI behavior; it is not presented as observed Excel behavior. The INDEX view remains scalar-consumable by ordinary arithmetic and final cell-result conversion.

## Selected missing values and operational source exceptions

Selected helper missing arguments retain a private `Value::Missing` identity through nested calls. An actual blank reference stays blank. Pinned `OperandResolver.java:271–296` and `:363–398` throw for missing numeric/boolean coercion; the logical port transports that boundary as `UnsupportedCapability`, never a catchable spreadsheet error. `Errortype.java:57–60` instead reports `#N/A` for a non-error missing value.

Final helper missing is also operational: `WorkbookEvaluator.java:794–807` preserves MissingArgEval, and `HSSFFormulaEvaluator.java:200–214` rejects that final result class. The final-result guard therefore aborts before any cache publication. Direct legacy `Expr::Missing` lowering and internal legacy scalar consumers retain their accepted blank-compatible behavior. Array-element missing metadata that has already become a model blank remains the separately documented limitation.


## Scalar selection and array coordinates

The new host scalar port explicitly models `OperandResolver.getSingleValue`, including row, column and rectangular caller intersection (`OperandResolver.java:196–215`). Literal arrays have a proven `(0,0)` anchor (`WorkbookEvaluator.java:879–880`). INDEX preserves that anchor and offsets it when selecting its row or column view. Selected opaque logical values preserve the view identity rather than rediscovering it from syntax. A single-cell view always has a unique scalar.

Computed matrix/criteria arrays and arrays returned through the lookup helper currently have no established core coordinate anchor. They are marked unknown; a multi-cell value reaching the new scalar port produces an operational capability diagnostic. Matrix source anchors can depend on their input area (`MatrixFunction.java:127,230`), so the host does not invent an origin. Indexed collection consumers do not require scalar intersection and remain supported. A RefListEval reaching this scalar port likewise produces an explicit source-coercion capability diagnostic.

Legacy Session scalar consumers retain their prior top-left array selection. This separation does not implement POI's ArrayFunction/array-mode vectorization, nor establish complete scalar-selection parity for lookup helper ports. Those source-specific capabilities remain separate work; no generic global first-element or caller-coordinate override is claimed for them.
