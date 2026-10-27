//! Shared bounded conditional selection, payload demand, and result reduction.
use super::argument_role::ArgumentRole;
use super::broadcast::Broadcast;
use super::lift::Lift;
use super::whole_collection::WholeCollection;
use crate::eval::host::{CallHost, OperandKind, OperandReader, ScalarOperand, SubtotalReader};
use crate::eval::limits::CapabilityId;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, Value, finite};
use crate::model::CellError::{Div0, Num};
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellValue};
use poirot_functions::criteria::AggregateKind;

/// Whether a conditional source consists of cells or headed database records.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) enum Layout {
    Cells,
    Records,
}

/// Describes conditional source traversal before any payload is selected.
/// It can exclude a database header and later restrict iteration to stored positions when predicates prove blank cells irrelevant.
#[derive(Clone)]
pub(in crate::eval::adapters) struct Rows {
    rows: usize,
    columns: usize,
    first: usize,
    stored: Option<Vec<(usize, usize)>>,
}

impl Rows {
    /// Bind an array or reference, excluding a leading header when requested.
    pub(in crate::eval::adapters) fn bind(
        host: &mut CallHost<'_, '_>,
        argument: usize,
        layout: Layout,
    ) -> FunctionResult<Self> {
        if !matches!(
            host.kind(argument)?,
            OperandKind::Reference | OperandKind::Array
        ) {
            return Err(VALUE_ERROR.into());
        }
        let (rows, columns) = OperandReader::new(host).shape(argument)?;
        if columns == 0 || rows == 0 {
            return Err(VALUE_ERROR.into());
        }
        rows.checked_mul(columns).ok_or(Num)?;
        Ok(Self {
            rows,
            columns,
            first: usize::from(matches!(layout, Layout::Records)),
            stored: None,
        })
    }

    /// Require aligned criteria geometry before visiting any member.
    pub(in crate::eval::adapters) fn align(
        &self,
        host: &mut CallHost<'_, '_>,
        argument: usize,
    ) -> FunctionResult<()> {
        let other = Self::bind(host, argument, Layout::Cells)?;
        if self.shape() != other.shape() {
            return Err(VALUE_ERROR.into());
        }
        Ok(())
    }

    /// Return the admitted rectangle.
    pub(in crate::eval::adapters) fn shape(&self) -> (usize, usize) {
        (self.rows, self.columns)
    }

    /// Exclude implicit blanks only after the first predicate proves them irrelevant.
    pub(in crate::eval::adapters) fn sparse(
        &mut self,
        host: &mut CallHost<'_, '_>,
        argument: usize,
    ) -> FunctionResult<()> {
        self.stored = OperandReader::new(host).stored_positions(argument)?;
        Ok(())
    }
}

/// Selected payload role, read only after a row has passed its predicates.
pub(in crate::eval::adapters) enum Field {
    /// Counting requires no payload read.
    Count,
    /// Read an aligned value relative to a range's first cell.
    Relative(usize),
    /// Read a fixed database column, or an omitted count field.
    Column(Option<usize>),
}

/// How spreadsheet predicate failures affect selection.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) enum PredicateErrors {
    /// Preserve the predicate's spreadsheet error.
    Preserve,
    /// Database criteria expose invalid conditions as VALUE.
    Value,
}

/// Reduce selected payloads, optionally finishing early without additional reads.
pub(in crate::eval::adapters) trait Reducer {
    /// Consume one accepted payload; return a result to stop traversal early.
    fn push(
        &mut self,
        host: &mut CallHost<'_, '_>,
        value: CellValue,
    ) -> FunctionResult<Option<CellValue>>;

    /// Assemble the result after all accepted payloads have been consumed.
    fn finish(self, host: &mut CallHost<'_, '_>) -> FunctionResult<CellValue>;
}

/// Traverse one bounded row set with lazy predicates and selected payload demand.
pub(in crate::eval::adapters) fn select<R: Reducer>(
    host: &mut CallHost<'_, '_>,
    source: Rows,
    field: Field,
    errors: PredicateErrors,
    mut predicate: impl FnMut(&mut CallHost<'_, '_>, usize, usize) -> FunctionResult<bool>,
    mut reducer: R,
) -> FunctionResult<CellValue> {
    let dense = if source.stored.is_some() {
        0
    } else {
        source
            .rows
            .checked_mul(if matches!(field, Field::Column(_)) {
                1
            } else {
                source.columns
            })
            .ok_or(Num)?
    };
    let width = if matches!(field, Field::Column(_)) {
        1
    } else {
        source.columns
    };
    if width == 0 {
        return Err(VALUE_ERROR.into());
    }
    let first = source.first.checked_mul(width).ok_or(Num)?;
    let positions = source
        .stored
        .into_iter()
        .flatten()
        .chain((first..dense).map(|i| {
            // SAFETY: the width == 0 guard above rejects zero divisors.
            (i / width, i % width)
        }));
    for (row, column) in positions {
        let matched = match predicate(host, row, column) {
            Err(FunctionFailure::Spreadsheet(_)) if matches!(errors, PredicateErrors::Value) => {
                return Err(VALUE_ERROR.into());
            }
            result => result?,
        };
        if !matched {
            continue;
        }
        let value = match field {
            Field::Count | Field::Column(None) => Number(0.),
            Field::Relative(argument) => {
                OperandReader::new(host)
                    .read_relative_cell(argument, row, column)?
                    .value
            }
            Field::Column(Some(column)) => {
                OperandReader::new(host).read_cell(0, row, column)?.value
            }
        };
        if let Some(result) = reducer.push(host, value)? {
            return Ok(result);
        }
    }
    reducer.finish(host)
}

/// Incremental numeric reduction over payloads accepted by conditional row selection.
/// SUM, COUNT, AVERAGE, MIN, and MAX share this fixed-size state instead of collecting selected values.
pub(in crate::eval::adapters) struct Numbers {
    operation: AggregateKind,
    sum: f64,
    count: usize,
    extremum: Option<f64>,
}

impl Numbers {
    /// Start a selected-value numerical reduction.
    pub(in crate::eval::adapters) fn new(operation: AggregateKind) -> Self {
        Self {
            operation,
            sum: 0.,
            count: 0,
            extremum: None,
        }
    }
}

impl Reducer for Numbers {
    fn push(
        &mut self,
        _: &mut CallHost<'_, '_>,
        value: CellValue,
    ) -> FunctionResult<Option<CellValue>> {
        if matches!(self.operation, AggregateKind::Count) {
            self.count = self.count.checked_add(1).ok_or(Num)?;
            return Ok(None);
        }
        if let Error(error) = value {
            return Ok(Some(Error(error)));
        }
        if let Number(number) = value {
            self.sum += number;
            self.count = self.count.checked_add(1).ok_or(Num)?;
        }
        let number = if let Number(number) = value {
            number
        } else {
            0.
        };
        self.extremum = Some(match self.extremum {
            None => number,
            Some(old) if matches!(self.operation, AggregateKind::Min) => old.min(number),
            Some(old) => old.max(number),
        });
        Ok(None)
    }

    fn finish(self, _: &mut CallHost<'_, '_>) -> FunctionResult<CellValue> {
        Ok(match self.operation {
            AggregateKind::Count => Number(self.count as f64),
            AggregateKind::Sum => finite(self.sum),
            AggregateKind::Average | AggregateKind::AverageIf if self.count == 0 => Error(Div0),
            AggregateKind::Average | AggregateKind::AverageIf => {
                finite(self.sum / self.count as f64)
            }
            AggregateKind::Min | AggregateKind::Max => finite(self.extremum.unwrap_or(0.)),
        })
    }
}

/// Admit wildcard scratch and meter matching states without losing domain errors.
pub(in crate::eval::adapters) fn matching<E: From<FunctionFailure>>(
    host: &mut CallHost<'_, '_>,
    wildcard: Option<(&str, &str)>,
    test: impl FnOnce(&mut dyn FnMut() -> Result<(), E>) -> Result<bool, E>,
) -> Result<bool, E> {
    if let Some((pattern, input)) = wildcard {
        host.admit_wildcard(pattern, input).map_err(E::from)?;
    }
    let mut visited = 0;
    test(&mut || host.wildcard_step(&mut visited).map_err(E::from))
}

/// Fold source cells while applying workbook visibility before reading their values.
pub(in crate::eval::adapters) fn visible_fold<T>(
    host: &mut CallHost<'_, '_>,
    start: usize,
    hidden: bool,
    mut state: T,
    mut step: impl FnMut(&mut CallHost<'_, '_>, &mut T, ScalarOperand) -> FunctionResult<()>,
) -> FunctionResult<T> {
    for argument in start..host.len() {
        let len = OperandReader::new(host).collection_len(argument)?;
        host.check_values(len)?;
        host.charge_work(len as u64)?;
        for offset in 0..len {
            if let Some(value) =
                SubtotalReader::new(host).subtotal_cell(argument, offset, hidden)?
            {
                step(host, &mut state, value)?;
            }
        }
    }
    Ok(state)
}

/// Evaluate disjunctions of conjunctions in order, skipping unneeded terms.
pub(in crate::eval::adapters) fn any_all(
    host: &mut CallHost<'_, '_>,
    clauses: std::ops::Range<usize>,
    terms: std::ops::Range<usize>,
    mut test: impl FnMut(&mut CallHost<'_, '_>, usize, usize) -> FunctionResult<bool>,
) -> FunctionResult<bool> {
    for clause in clauses {
        let mut accepted = true;
        for term in terms.clone() {
            if !test(host, clause, term)? {
                accepted = false;
                break;
            }
        }
        if accepted {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Source ordering and payload position for collection/control pairs.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) enum PairLayout {
    /// One collection/control pair without a selected payload.
    Single,
    /// One pair followed by an optional payload, defaulting to its collection.
    OptionalPayload,
    /// Repeated collection/control pairs without a selected payload.
    Repeated,
    /// A payload followed by repeated collection/control pairs.
    LeadingPayload,
}

/// Pairs condition ranges with their control arguments for conditional aggregation.
/// It checks range alignment once and lets only broadcast controls determine the output rectangle.
pub(in crate::eval::adapters) struct PairedCollections<R: ArgumentRole = Broadcast> {
    pairs: Vec<(WholeCollection, R)>,
    payload: Option<WholeCollection>,
    source: Rows,
    lift: Lift,
}

impl<R: ArgumentRole> PairedCollections<R> {
    /// Admit source-ordered roles and aligned collection shapes without reading members.
    pub(in crate::eval::adapters) fn admit(
        host: &mut CallHost<'_, '_>,
        layout: PairLayout,
    ) -> FunctionResult<Self> {
        let len = host.len();
        let single = matches!(layout, PairLayout::Single | PairLayout::OptionalPayload);
        let start = usize::from(matches!(layout, PairLayout::LeadingPayload));
        let available = len.checked_sub(start).ok_or(VALUE_ERROR)?;
        if available < 2
            || single
                && len
                    > if matches!(layout, PairLayout::Single) {
                        2
                    } else {
                        3
                    }
            || !single && !available.is_multiple_of(2)
        {
            return Err(VALUE_ERROR.into());
        }
        host.check_values(len)?;
        let payload = match layout {
            PairLayout::Single | PairLayout::Repeated => None,
            PairLayout::LeadingPayload => Some(WholeCollection::bind(0, 0, true)),
            PairLayout::OptionalPayload => {
                Some(WholeCollection::bind(if len == 3 { 2 } else { 0 }, 0, true))
            }
        };
        if start == 1 {
            Rows::bind(host, 0, Layout::Cells)?;
        }
        let count = if single {
            1
        } else {
            available.checked_div(2).ok_or(VALUE_ERROR)?
        };
        let mut pairs = Vec::new();
        pairs
            .try_reserve_exact(count)
            .map_err(|_| host.allocation_failure())?;
        let mut controls = Vec::new();
        controls
            .try_reserve_exact(count)
            .map_err(|_| host.allocation_failure())?;
        for pair in 0..count {
            let index = pair
                .checked_mul(2)
                .and_then(|offset| start.checked_add(offset))
                .ok_or(VALUE_ERROR)?;
            let control = index
                .checked_add(1)
                .filter(|&index| index < len)
                .ok_or(VALUE_ERROR)?;
            Rows::bind(host, index, Layout::Cells)?;
            OperandReader::new(host).describe(control)?;
            // SAFETY: count pairs and control indices were reserved before this count-iteration loop.
            pairs.push((
                WholeCollection::bind(index, 0, true),
                R::bind(control, pair, true),
            ));
            // SAFETY: count indices were reserved and this loop pushes exactly count indices.
            controls.push(control);
        }
        if single && len == 3 {
            OperandReader::new(host).describe(2)?;
        }
        let source = Rows::bind(host, start, Layout::Cells)?;
        for (range, _) in &pairs {
            source.align(host, range.argument())?;
        }
        if matches!(layout, PairLayout::LeadingPayload) {
            source.align(host, 0)?;
        }
        if let Some(payload) = &payload {
            Rows::bind(host, payload.argument(), Layout::Cells)?;
        }
        Ok(Self {
            pairs,
            payload,
            source,
            lift: Lift::new_selected(host, &controls)?,
        })
    }

    /// Borrow the bound ordered collection/control associations.
    pub(in crate::eval::adapters) fn pairs(&self) -> &[(WholeCollection, R)] {
        &self.pairs
    }

    /// Select the declared lazy payload role, or count without reading a payload.
    pub(in crate::eval::adapters) fn field(&self) -> Field {
        self.payload
            .map_or(Field::Count, |role| Field::Relative(role.argument()))
    }

    /// Clone only source geometry, optionally excluding proven-irrelevant implicit blanks.
    pub(in crate::eval::adapters) fn source(
        &self,
        host: &mut CallHost<'_, '_>,
        skip_blanks: bool,
    ) -> FunctionResult<Rows> {
        let mut source = self.source.clone();
        if skip_blanks {
            source.sparse(host, self.pairs.first().ok_or(VALUE_ERROR)?.0.argument())?;
        }
        Ok(source)
    }

    /// Map typed controls into domain predicates and assemble each bounded output member.
    pub(in crate::eval::adapters) fn evaluate<P>(
        &self,
        host: &mut CallHost<'_, '_>,
        address: CellAddress,
        mut compile: impl FnMut(&mut CallHost<'_, '_>, CellValue) -> FunctionResult<P>,
        mut apply: impl FnMut(&mut CallHost<'_, '_>, &Self, &[P]) -> FunctionResult<CellValue>,
    ) -> FunctionResult<Value> {
        self.lift.evaluate(host, address, |host, row, column| {
            let mut controls = Vec::new();
            controls
                .try_reserve_exact(self.pairs.len())
                .map_err(|_| host.allocation_failure())?;
            for member in 0..self.pairs.len() {
                let operand = self.lift.member(host, member, row, column)?;
                if operand.missing {
                    return Err(host.unsupported(
                        CapabilityId::FunctionAdapter,
                        "omitted conditional control",
                    ));
                }
                let control = compile(host, operand.value)?;
                // SAFETY: the pair count was reserved before this pair-count loop.
                controls.push(control);
            }
            apply(host, self, &controls)
        })
    }
}
