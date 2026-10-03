# Sparse queries

`query::WorkbookRead` is the read port, `WorkbookView` is its borrowed adapter, and `QueryPlan` is the ordered range-membership plan. No revision counters, persistent indexes or formula coercion are introduced. Formula values remain borrowed `Cell` records; evaluation must resolve formulas before aggregate/error/coercion decisions.

`WorkbookView::stored` seeks to each occupied row and iterates only the requested column interval. Empty rows are jumped over, columns outside the interval are not traversed, and rows outside the interval are not traversed. The row-discovery seek reads one key per occupied row, including rows which have no selected columns. `row_seeks`, `column_seeks` and `cells_visited` report these distinct operations; they are logical operations rather than counts of internal B-tree comparisons. Whole-column work is proportional to occupied rows plus returned cells, not Excel's row limit. Narrow single-row work is independent of unrelated cells in that row.

`lookup` performs a direct key lookup. `blank_counts` separates absent positions, stored literal Blank cells, and formula cells. It deliberately does not interpret formula caches, empty text or source-specific COUNTBLANK behavior. Membership descriptors cover absent cells as well as present cells, allowing dependency tracing to invalidate when an absent cell is inserted. Consumers must retain every area in source order and not deduplicate overlapping ranges.

`QueryPlan::dense_size(DenseBudget)` checks the complete ordered plan before dense allocation, including duplicate membership. The caller supplies an explicit cell budget; there is no implicit unbounded default. The evaluator retains its existing work/allocation limits.
