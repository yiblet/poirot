/// Column traversal across edits, tombstones and generated cells.
mod column_ranges;
/// Full-width interval traversal in both directions.
mod full_width;
/// Paged column traversal and edit payload reuse.
mod page_column;
/// Whole-row traversal order and admission.
mod whole_rows;
