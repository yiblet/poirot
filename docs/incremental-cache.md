# Overlay result reuse and dependencies

The overlay owns persistent compiled formulas and results. An OverlayEvaluation owns
only run-local stacks, memoization, budgets and consumed-read staging. Source
workbook saved values are never imported as trusted calculation results.

Each cached formula records its consumed point reads, sparse range membership,
context fields, formatting/text-policy use, volatility and opacity. Reverse point
edges and a rectangle index make source writes invalidate the changed cell and
transitive consumers. Invalidation is iterative and handles cycles. Rectangle
dependencies include absent coordinates, so inserting or removing a previously
missing range member invalidates the relevant aggregates. Intersections and
signed/padded references track the physical region actually consumed.

Consecutive actual reads and adjacent membership guards are compressed into rectangles.
Numeric/boolean lookup prefixes are local to an evaluation; every consumer records
its consumed prefix, including prefix hits and exhausted misses. Reusing an index
therefore keeps invalidation correct without rescanning earlier candidates.

Only selected branches establish dependencies. Successful recomputation replaces
old edges, so an input used exclusively by an obsolete branch stops invalidating
that formula. On a cache hit, the existing trace is retained. Compiled bytecode
is independent of result invalidation and remains reusable after ordinary input
writes.

Starting an evaluation invalidates volatile and opaque cells and their consumers.
Time, seed, UTC offset, date order and formatting changes invalidate their actual
consumers. Unrelated cached results remain available. RAND is reexecuted each
instance but remains reproducible under an unchanged explicit seed. Extension
registrations explicitly declare deterministic, versioned, volatile, or opaque
cacheability. Deterministic/versioned providers retain tracked dependencies and
can reuse results; volatile/opaque providers refresh each new evaluation.
Versioned external inputs participate in exact registry/snapshot identity.

Dynamic spills are complete owned footprints. Producers depend on footprint
membership and on possible earlier producer membership inspected during discovery.
This catches blockers and new producers beginning outside a consumed range but
spilling into it. Children depend on their producer. Invalidating a producer
removes its children and their downstream consumers. Valid cached footprints are
restored together, with ownership and full admission checks; individual child
scalars never establish a spill. Warm reuse preserves original input dependencies.
Shrinking or blocked producers cannot leave stale children.

Each output batch stages new results and publishes them only after successful
completion and cache admission. Earlier successful calls in the same evaluation
remain valid after a later call fails. Result provenance is retained in overlay
snapshots; reverse/range indexes are rebuilt on restoration. Manual saved-cache
writes are authoring data, not evidence that a formula was evaluated.

Evaluation writeback distinguishes formula saved values from derived spill
children. `ClearSpillCache` and `SetSpillCachedValue` preserve source ownership,
so applying changes does not turn computed children into input blockers. A raw
workbook snapshot retains this source/cache distinction too.

The lower-level versioned `incremental` transaction machinery remains available
for engine provenance integrations. The primary API controls writes directly
through `Overlay` and therefore avoids capturing a second complete mutable-source
snapshot for every evaluation.

Direct `Evaluation` uses the same source/compiler/spill engine without the retained
read graph or indexes. Its memo has ordinary values and runtime spill ownership;
every effective source edit clears derived results. This is an intentional cache
strategy alongside Overlay, not a second formula execution implementation.

Overlay also retains bounded search indexes for unobserved numeric/boolean exact
MATCH, HLOOKUP, and VLOOKUP calls over physical reference axes. These indexes
survive ordinary evaluation boundaries and changes to the needle or return-value
cells. They store the first matching position and only the prefix actually
scanned. Text and approximate searches continue using the ordinary search path.

Index storage uses a dedicated BTreeMap partitioned by sheet and physical column
or row, then by range endpoints. Overlay-owned reverse memberships act as each
consumed cell's in-index flag; a cell can belong to several overlapping indexes.
The normal transitive invalidation walk removes every index containing a changed
candidate, including changed formula candidates and volatile/context-sensitive
candidates. Insertions into previously absent gaps query only the changed cell's
column and row partitions. Unrelated partitions and unconsumed suffixes survive.
The shared immutable workbook needs no mutable flags or per-absent-cell entries.

Successful output batches publish only newly scanned cell memberships with their
dependency traces; unchanged indexes retain their existing flags. Removing or
discarding an index removes its memberships while preserving overlapping indexes.
Aborted batches discard their search indexes. Candidate memberships and
numeric keys both count toward bounded index storage, and each new evaluation
revalidates retained storage against its materialization limit. Observed execution
keeps its ordinary read callbacks. Search indexes are derived runtime state and
are rebuilt after snapshot restoration or an explicit overlay reset. RunUsage
reports lookup_prefix_hits and lookup_prefix_scans separately from formula memo
hits, so actual search reuse can be measured even when a lookup formula executes
again.
