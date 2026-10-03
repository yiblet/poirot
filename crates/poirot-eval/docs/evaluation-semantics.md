# Evaluation identity and reuse

Given the same effective source, computation policies, captured evaluation
context, and external-input versions, successful execution must produce the
same values, spreadsheet errors, and spill layout. Request order, batching,
retained-cache warmth, and valid snapshot restoration must not change them.

Formula execution counts, cache provenance, observer execution traces, provider
invocation counts, and actual work can differ with reuse. Providers must not
depend on invocation counts or order. Opaque providers using unversioned ambient
state cannot promise deterministic value equivalence.

## Randomness and evaluation boundaries

`EvaluationContext::as_of_time` carries a captured `std::time::SystemTime` through
execution, extension contexts and cache identity. Creating a default context reads
the clock once; evaluation does not read it again. Context comparison preserves
nanoseconds, even when Excel's floating-point date result cannot distinguish them.
Date/coercion leaves alone convert the instant into numeric seconds and apply
localization. Dates outside their supported calendar range still return errors.
Workspace snapshot format v4 stores exact signed epoch durations, so pre-epoch
timestamps also round-trip without floating-point conversion.

The existing versioned generator derives independent streams from the explicit
seed and the source cell's sheet, row, and column. Each cell consumes its own
logical draws. Unrelated output requests and cache hits do not advance another
cell's stream. A cell is memoized within an evaluation, including volatile cells.

Starting a new overlay evaluation invalidates volatile roots and their consumers.
This refreshes execution, not the seed: the same explicit seed reproduces draws.
Callers change `randomness_seed` to obtain a different random realization. Source
edits do not themselves invent a new random identity. Ad hoc formula probes at
the same coordinate are executions rather than distinct stored-cell identities.

## Dynamic spill conflicts

Supported producers have statically bounded footprints. Competing source
producers are considered in ascending address order (row, then column) within
each sheet, independently of requested output order. A successfully admitted
earlier footprint reserves its cells. An earlier producer which is blocked or
whose actual footprint does not intersect a later one does not reserve the
unused bound. Source formulas and nonblank literals remain blockers.

Spill hydration restores complete validated footprints, including child ownership
and admission checks. Removing the winning producer invalidates retained losing
producers and affected consumers. Circular competing-producer dependencies and
producers without supported bounds can fail operationally; this rule does not
claim support for arbitrary dynamic geometry.

## Extension cacheability

`ExtensionFunction: Send + Sync` establishes thread safety, not purity.
Registrations explicitly select `ExtensionCacheability`:

- `Deterministic`: tracked arguments/host reads plus implementation identity
  determine the result.
- `Versioned { external_inputs_revision }`: additionally uses an immutable
  external-data snapshot identified by that revision. Updating the revision
  requires replacing the registration and rebinding prepared formulas.
- `Volatile`: refresh when reached in each new evaluation.
- `Opaque`: dependencies cannot be described; refresh conservatively each new
  evaluation. Invocation counts and side effects are outside value equivalence.

Implementation signatures identify code and captured configuration. Versioned
external data is part of the exact registration contract and snapshot identity;
restoration rejects an incompatible registry rather than trusting stale results.
Context-capable providers conservatively depend on every exposed context field.

## Budgets and limits

Work and execution depth measure work actually performed. Reuse can allow a
warmed evaluation to finish when a cold evaluation exceeds those budgets.
Successful values must still agree. Text-buffer and collection-size admission
limits apply to restored results too, including entire spill footprints.
Retention limits describe storage retained by the chosen execution/cache path.

## Coverage and remaining integration

`tests/evaluation_semantics.rs` compares the existing `EvaluationRun` and overlay
paths, individual and batched requests, cold/warm results, and postcard snapshot
restoration. It also exercises provider input/context invalidation and incompatible
external revisions, spill winner removal, and result admission after cache hits.

`tests/dual_evaluation.rs` exercises the editable direct workspace against overlay
execution, repeated reads/edits, sparse membership, dynamic references, intersections,
spill conflicts and blockers, snapshot/bytecode restoration, exact baseline identity,
thread transfer, and failed-batch writeback. Both paths use the same engine.
Direct source edits clear local results but preserve cumulative work budgets;
`begin(context)` is the explicit reset of evaluation identity, budgets and failure.
Direct snapshots deliberately omit results without validity evidence.
