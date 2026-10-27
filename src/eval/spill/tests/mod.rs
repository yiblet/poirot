//! Unit tests for spill discovery, settlement, admission and publication.

/// Settlement access, publication and acknowledgement through the session ports.
mod access_api;
/// Candidate rectangles against an independent coordinate oracle.
mod candidate;
/// Candidate discovery under parser and resource refusals.
mod candidate_failure;
/// Discovery proofs, settlement slots and their restoration.
mod discovery;
/// Down/right discovery frontiers and overlap priority.
mod discovery_frontier;
/// Discovery work and lifecycle through editable evaluations.
mod discovery_lifecycle;
/// Held producer results before ownership settles.
mod held_results;
/// Demand-local preparation and top-left settlement.
mod local_settlement;
/// Spill preparation, ownership settlement and blockers.
mod preparation;
/// Scalar discovery proofs and reached-source binding.
mod scalar_classification;
/// Engine retries, deferred notifications and rollback during settlement.
mod settlement;
