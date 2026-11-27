/// Matrix algebra and array aggregation handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MatrixFunction {
    /// Select rows or columns by Boolean conditions.
    Filter,
    /// Sort records by an internal key.
    Sort,
    /// Sort records by corresponding external keys.
    Sortby,
    /// Retain distinct or exactly-once records.
    Unique,
    /// Modern Excel sequential rectangular array generation.
    Sequence,
    /// Concatenate arrays horizontally.
    Hstack,
    /// Concatenate arrays vertically.
    Vstack,
    /// Retain signed row and column counts.
    Take,
    /// Remove signed row and column counts.
    Drop,
    /// Select ordered column indices.
    Choosecols,
    /// Select ordered row indices.
    Chooserows,
    /// Pad an array to larger dimensions.
    Expand,
    /// Flatten into a column.
    Tocol,
    /// Flatten into a row.
    Torow,
    /// Wrap a vector by columns.
    Wrapcols,
    /// Wrap a vector by rows.
    Wraprows,
    /// MDETERM handler.
    Mdeterm,
    /// MINVERSE handler.
    Minverse,
    /// MMULT handler.
    Mmult,
    /// TRANSPOSE handler.
    Transpose,
    /// FREQUENCY handler.
    Frequency,
    /// SUMPRODUCT handler.
    Sumproduct,
    /// TREND handler.
    Trend,
    /// Group rows with typed built-in reductions and optional hierarchy totals.
    Groupby,
    /// Group records on two axes with typed built-in reductions.
    Pivotby,
    /// Ratio of two independently summed sets.
    Percentof,
    /// Seeded rectangular random numeric generation.
    Randarray,
    /// Remove blank outside borders while preserving the input channel.
    Trimrange,
    /// Numeric identity matrix.
    Munit,
    /// Exponential regression predictions.
    Growth,
    /// Linear least-squares coefficients and optional diagnostics.
    Linest,
    /// Exponential least-squares coefficients and optional diagnostics.
    Logest,
    /// Apply a callable to corresponding array members.
    Map,
    /// Apply a callable once per row.
    Byrow,
    /// Apply a callable once per column.
    Bycol,
    /// Produce a sized array by calling a function at each position.
    Makearray,
}

impl MatrixFunction {
    /// Whether this operation uses the higher-order array adapter.
    pub(in crate::eval) fn uses_array_callback_adapter(self) -> bool {
        matches!(
            self,
            Self::Map | Self::Byrow | Self::Bycol | Self::Makearray
        )
    }
}
