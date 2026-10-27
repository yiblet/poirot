//! Per-cell side effects of function calls that must stay stable within a run.

use crate::eval::config;
use crate::eval::coordinates::CellMap;
use crate::eval::functions;

/// Effects recorded by function calls: each cell's random stream, its numeric
/// date coercion effects and the extension call trace.
#[derive(Default)]
pub(in crate::eval) struct CallEffects {
    /// Random stream and usage slot assigned to each drawing cell.
    pub(in crate::eval) random_streams: CellMap<(config::random::Stream, usize)>,
    /// Date-system effects accumulated by numeric date coercion in each cell.
    pub(in crate::eval) numeric_date_effects:
        CellMap<poirot_functions::coercion::date_time::Effects>,
    /// Extension calls recorded for the active evaluation.
    pub(in crate::eval) extension_trace: functions::extensions::ExtensionTrace,
}
