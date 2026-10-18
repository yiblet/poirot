# Positional CHOOSE selection

Microsoft documents CHOOSE as selecting the indexed result, truncating fractional
indices and allowing array indices to return multiple values:
https://support.microsoft.com/en-us/excel/functions/choose-function.
All thirteen independently authored gaps concern loss of positional index
information before a consuming aggregate receives the selected sequence.

Three options were considered:

1. Reuse concrete call-local selection storage and selected-result materialization
   from IFS/SWITCH, with a local index scan using the existing scalar numeric
   admission policy. Selected: the demonstrated conformable positional contract
   fits existing patterns without persistent tags or a generic planner.
2. Add a dedicated CHOOSE rectangular kernel to reproduce Calc's truncation or
   padding for unequal non-singleton extents. Rejected: this adds an exception
   disproportionate to the demonstrated defect. The uniform shared #VALUE!
   shape rejection is kept and documented.
3. Patch each aggregate or introduce separate array-index admission. Rejected:
   aggregates do not own selection; duplicating scalar conversion would introduce
   policy drift.

Under ExcelDocumentation an explicit array index, or a reference under inherited
array demand, selects each position independently. Singleton axes broadcast.
Selected branch expressions execute once; unselected branches and unselected
reference members remain deferred. Scalar selection keeps its existing reference
and omitted-blank result channel; singleton index arrays retain array identity.
PinnedPoi remains scalar. Allocations, traversal, numeric parsing and final
materialization retain existing metering and operational failures. Builtin policy
reads invalidate retained plans in both profiles. Spill bounds include indices
as well as results, and unbounded producers retain existing refusal behavior.
Snapshot version and compatibility revision advance for the changed semantics.

The same centralized scalar numeric conversion handles numeric text, booleans,
blank, errors, fractions and nonfinite values. Fractions truncate, invalid bounds
return #VALUE!, worksheet errors propagate per index, and the existing finite
policy rejects nonfinite values with #NUM!. No new conversion policy is added.

Independent Calc probes corroborate positional branching, singleton broadcasting,
truncation, laziness, selected errors and scalar/reference identity. They also
falsify broader compatibility claims: Calc rejects array boolean indices while
Poirot's existing scalar policy accepts TRUE as 1; Calc truncates/pads unequal
non-singleton branch shapes whereas Poirot uniformly rejects them. These
disagreements are retained. Missing members materialize as blank using the
existing rectangular reader, without a new omission state. Worksheet publication
alone converts remaining blank to zero. Needed compound branch expressions
execute as a whole; no coordinate-lazy expression interpreter is introduced.

The diagnostic report preserves all eighteen probes, with zero-valued placeholder
expectations explicitly identified. It does not claim those mismatches represent
failed authored expectations. Fresh source-bound Calc observations corroborate all
48 production suite checks. No native Excel execution was available.
