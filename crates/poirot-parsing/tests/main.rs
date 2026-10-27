//! Integration tests, compiled as one binary to avoid linking each file separately.
//! Add a new test file here as a module; Cargo does not discover it automatically.
mod classified_allocations;
mod grammar_boundaries;
mod implicit_intersection;
mod java_number;
mod numeric_sheet_qualifiers;
mod qualified_names;
mod reference_ranges;
mod sheet_spans;
mod spill_references;
