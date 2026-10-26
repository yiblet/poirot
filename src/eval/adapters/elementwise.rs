//! Typed scalar, collection, and broadcast roles with independent argument demand.
//! Eager signatures admit geometry; selectors bind deferred roles without admission.
use crate::eval::host::{CallHost, NumericPolicy, OperandDescription, ScalarOperand};
use crate::eval::*;

pub(super) mod collections;
pub(super) mod controls;
pub(super) mod matrices;
pub(super) mod recovery;
pub(super) mod reference;
pub(super) mod rows;
pub(super) mod selection;
mod signatures;
pub(super) mod source;
pub(super) mod text;
pub(super) use signatures::{
    BroadcastCollections, NumericArguments, NumericErrorAdmission, NumericParameter,
    NumericSignature, PreparedSources, Variadic,
};
pub(super) use source::SourceIdentity;

/// Carries broadcast shape planning into member execution.
/// It records which source arguments form the output rectangle, maps each output coordinate to their scalar members, and assembles the results.
pub(super) struct Lift {
    shapes: poirot_functions::scalar::ParamVec<OperandDescription>,
    start: usize,
    indices: Option<poirot_functions::scalar::ParamVec<usize>>,
    rows: usize,
    columns: usize,
}
impl Lift {
    /// Describe a bounded prefix of scalar arguments before reading its members.
    pub(super) fn new(host: &mut CallHost<'_, '_>, count: usize) -> FunctionResult<Self> {
        Self::new_at(host, 0, count)
    }
    /// Describe a selected contiguous group, excluding collection operands.
    /// Member indices are relative to `start`; zero count admits a scalar default.
    /// Every host access checks the original `start + index` argument position.
    pub(super) fn new_at(
        host: &mut CallHost<'_, '_>,
        start: usize,
        count: usize,
    ) -> FunctionResult<Self> {
        let end = start.checked_add(count).ok_or(VALUE_ERROR)?;
        Self::from_arguments(host, start..end, start, None)
    }
    /// Describe explicitly selected scalar slots, excluding fixed collection operands.
    pub(super) fn new_selected(
        host: &mut CallHost<'_, '_>,
        selected: &[usize],
    ) -> FunctionResult<Self> {
        host.check_values(selected.len())?;
        let mut indices = poirot_functions::scalar::ParamVec::new();
        indices
            .try_reserve(selected.len())
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: selected.len slots were reserved for exactly selected.len indices.
        indices.extend(selected.iter().copied());
        Self::from_arguments(host, selected.iter().copied(), 0, Some(indices))
    }
    /// Build shared geometry, retaining contiguous slots without an index allocation.
    fn from_arguments(
        host: &mut CallHost<'_, '_>,
        mut selected: impl ExactSizeIterator<Item = usize>,
        start: usize,
        indices: Option<poirot_functions::scalar::ParamVec<usize>>,
    ) -> FunctionResult<Self> {
        let count = selected.len();
        host.check_values(count)?;
        let mut shapes = poirot_functions::scalar::ParamVec::new();
        shapes
            .try_reserve(count)
            .map_err(|_| host.allocation_failure())?;
        let (mut rows, mut columns) = (1, 1);
        for _ in 0..count {
            let index = selected.next().ok_or(VALUE_ERROR)?;
            let shape = host.describe(index)?;
            if shape.sheets != 1 || shape.rows == 0 || shape.columns == 0 {
                return Err(VALUE_ERROR.into());
            }
            rows = rows.max(shape.rows);
            columns = columns.max(shape.columns);
            // SAFETY: count slots were reserved locally and this loop pushes count shapes.
            shapes.push(shape);
        }
        if rows > MAX_ROWS as usize || columns > MAX_COLUMNS as usize {
            return Err(Num.into());
        }
        host.check_values(
            rows.checked_mul(columns)
                .ok_or_else(|| host.size_overflow())?,
        )?;
        host.check_values(rows)?;
        Ok(Self {
            shapes,
            start,
            indices,
            rows,
            columns,
        })
    }
    /// Return the admitted output dimensions for demand-sensitive adapters.
    pub(super) fn dimensions(&self) -> (usize, usize) {
        (self.rows, self.columns)
    }
    /// Read one argument's corresponding member, replicating singleton axes.
    pub(super) fn member(
        &self,
        host: &mut CallHost<'_, '_>,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        let shape = self.shapes.get(index).ok_or(VALUE_ERROR)?;
        if shape.rows != 1 && row >= shape.rows || shape.columns != 1 && column >= shape.columns {
            return Err(NA.into());
        }
        host.read_cell(
            match &self.indices {
                Some(indices) => *indices.get(index).ok_or(VALUE_ERROR)?,
                None => self.start.checked_add(index).ok_or(VALUE_ERROR)?,
            },
            if shape.rows == 1 { 0 } else { row },
            if shape.columns == 1 { 0 } else { column },
        )
    }
    /// Numerically coerce one member, distinguishing omission from an actual blank.
    pub(super) fn number(
        &self,
        host: &mut CallHost<'_, '_>,
        index: usize,
        row: usize,
        column: usize,
        default: Option<f64>,
    ) -> FunctionResult<f64> {
        if index >= self.shapes.len() {
            return default.ok_or(VALUE_ERROR.into());
        }
        let operand = self.member(host, index, row, column)?;
        if operand.missing {
            return default.ok_or(VALUE_ERROR.into());
        }
        host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)
    }
    /// Evaluate each output member, preserving operational failures outside cells.
    pub(super) fn evaluate(
        &self,
        host: &mut CallHost<'_, '_>,
        address: CellAddress,
        mut evaluate: impl FnMut(&mut CallHost<'_, '_>, usize, usize) -> FunctionResult<CellValue>,
    ) -> FunctionResult<Value> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.rows)
            .map_err(|_| host.allocation_failure())?;
        for row in 0..self.rows {
            let mut values = Vec::new();
            values
                .try_reserve_exact(self.columns)
                .map_err(|_| host.allocation_failure())?;
            for column in 0..self.columns {
                host.charge_work(1)?;
                let value = match evaluate(host, row, column) {
                    Ok(value) => value,
                    Err(FunctionFailure::Spreadsheet(error)) => Error(error),
                    Err(error) => return Err(error),
                };
                // SAFETY: Exactly columns pushes follow the local columns reservation.
                values.push(value);
            }
            // SAFETY: Exactly rows pushes follow the local rows reservation.
            result.push(values);
        }
        if self.rows == 1 && self.columns == 1 {
            return result
                .into_iter()
                .next()
                .and_then(|row| row.into_iter().next())
                .map(Value::Scalar)
                .ok_or(VALUE_ERROR.into());
        }
        Ok(Value::Array(ArrayView::anchored(result, Some(address))))
    }
}

/// A source shape whose binding never reads a member or coerces a value.
pub(super) trait ArgumentRole: Sized {
    /// Whether this role contributes geometry to rectangular lifting.
    const BROADCAST: bool;
    /// Whether geometry may be inspected before a selector demands this argument.
    const EAGER: bool = true;
    /// Return the original source ordinal without resolving it.
    fn argument(&self) -> usize;
    /// Bind the source slot and its position among broadcast operands.
    fn bind(argument: usize, member: usize, present: bool) -> Self;
}

/// Argument role whose shape contributes to the output rectangle and whose value is read at each output coordinate.
/// Singleton rows or columns are replicated along the corresponding output axis.
#[derive(Clone, Copy)]
pub(super) struct Broadcast {
    argument: usize,
    member: usize,
}
impl ArgumentRole for Broadcast {
    const BROADCAST: bool = true;
    fn argument(&self) -> usize {
        self.argument
    }
    fn bind(argument: usize, member: usize, _present: bool) -> Self {
        Self { argument, member }
    }
}
impl Broadcast {
    /// Read a member without discarding its origin or explicit omission marker.
    #[inline]
    pub(super) fn read(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
    ) -> FunctionResult<ScalarOperand> {
        context
            .lift
            .member(context.host, self.member, context.row, context.column)
    }
    /// Coerce a numeric member, applying a default only to explicit omission.
    #[inline]
    pub(super) fn number(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        default: Option<f64>,
    ) -> FunctionResult<f64> {
        context.lift.number(
            context.host,
            self.member,
            context.row,
            context.column,
            default,
        )
    }
}

/// Argument role for one scalar control selected with the host's scalar-intersection rules.
/// Its source shape does not enlarge the output rectangle.
pub(super) struct FixedScalar {
    argument: usize,
}
impl ArgumentRole for FixedScalar {
    const BROADCAST: bool = false;
    fn argument(&self) -> usize {
        self.argument
    }
    fn bind(argument: usize, _member: usize, _present: bool) -> Self {
        Self { argument }
    }
}
impl FixedScalar {
    /// Bind a fixed slot outside a rectangular signature.
    pub(super) fn at(argument: usize) -> Self {
        Self { argument }
    }
    /// Resolve using the host's existing scalar intersection and omission rules.
    pub(super) fn read(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<ScalarOperand> {
        use poirot_functions::scalar::Input;
        host.input(self.argument, crate::eval::host::MissingPolicy::Blank)
            .force()
    }
}

/// Argument role for an entire source collection, such as a statistical sample.
/// It provides ordered member access while leaving output dimensions to other roles.
#[derive(Clone, Copy)]
pub(super) struct WholeCollection {
    argument: usize,
}
impl ArgumentRole for WholeCollection {
    const BROADCAST: bool = false;
    fn argument(&self) -> usize {
        self.argument
    }
    fn bind(argument: usize, _member: usize, _present: bool) -> Self {
        Self { argument }
    }
}
impl WholeCollection {
    /// Describe the bound collection without resolving a member.
    pub(super) fn describe(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<OperandDescription> {
        host.describe(self.argument)
    }

    /// Read an aligned cell through the bound collection's source identity.
    pub(super) fn read_at(
        &self,
        host: &mut CallHost<'_, '_>,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        host.read_cell(self.argument, row, column)
    }
    /// Return the bounded collection length using the shared host admission.
    pub(super) fn len(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<usize> {
        host.collection_len(self.argument)
    }
    /// Read an ordered member while retaining its original source classification.
    pub(super) fn read(
        &self,
        host: &mut CallHost<'_, '_>,
        offset: usize,
    ) -> FunctionResult<ScalarOperand> {
        host.collection_cell(self.argument, offset)
    }
}

/// Demand timing independent of a role's scalar, collection, or broadcast shape.
pub(super) trait Demand {
    /// Whether signature admission may describe this operand's geometry.
    const EAGER: bool;
}
/// Demand marker that permits signature preparation to inspect an argument before output traversal.
pub(super) struct Eager;
impl Demand for Eager {
    const EAGER: bool = true;
}
/// Demand marker that postpones an argument's geometry and value until a selector chooses it.
pub(super) struct Lazy;
impl Demand for Lazy {
    const EAGER: bool = false;
}
/// Changes when an argument role may be inspected without changing its scalar, collection, or broadcast shape.
/// Use `Optional<WithDemand<R, D>>` for a trailing optional role; wrapping `Optional` in demand or nesting demand wrappers is unsupported.
pub(super) struct WithDemand<R, D = Eager> {
    role: R,
    demand: std::marker::PhantomData<D>,
}
impl<R: ArgumentRole, D: Demand> ArgumentRole for WithDemand<R, D> {
    const BROADCAST: bool = R::BROADCAST;
    const EAGER: bool = D::EAGER;
    fn argument(&self) -> usize {
        self.role.argument()
    }
    fn bind(argument: usize, member: usize, present: bool) -> Self {
        Self {
            role: R::bind(argument, member, present),
            demand: std::marker::PhantomData,
        }
    }
}
impl<R> WithDemand<R, Eager> {
    /// Access the eager shape's ordinary scalar, collection, or member operations.
    pub(super) fn role(&self) -> &R {
        &self.role
    }
}
impl<R: ArgumentRole> WithDemand<R, Lazy> {
    /// Transfer a demanded value once, retaining its original reference/array channel.
    /// The executor owns it through all later shape inspection and member reads.
    pub(super) fn prepare(&self, host: &mut CallHost<'_, '_>) -> EvalResult<Prepared<R>> {
        Ok(Prepared::new(host.selected_argument(self.argument())?.1))
    }
}
/// Owns a value after a deferred argument has been selected and evaluated.
/// It preserves the scalar, array, or reference channel for subsequent member reads or whole-value consumption; computed arrays are still evaluated as a whole.
pub(super) struct Prepared<R> {
    value: Value,
    role: std::marker::PhantomData<R>,
}
impl<R> Prepared<R> {
    /// Retain an owned selected value or a function's omitted-result default.
    pub(super) fn new(value: Value) -> Self {
        Self {
            value,
            role: std::marker::PhantomData,
        }
    }
    /// Return the original result channel without scalarizing references or arrays.
    pub(super) fn into_value(self) -> Value {
        self.value
    }
}
impl Prepared<Broadcast> {
    /// Inspect selected geometry without reading any reference members.
    pub(super) fn shape(
        &self,
        host: &CallHost<'_, '_>,
    ) -> Option<crate::eval::array_operator::Shape> {
        host.selected_shape(&self.value)
    }
    /// Read only a selected output coordinate through the checked broadcast reader.
    pub(super) fn read(
        &self,
        reader: &mut crate::eval::host::SelectedReader<'_, '_>,
        shape: crate::eval::array_operator::Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        reader.member(&self.value, shape, row, column)
    }
}
/// A shape role whose source is prepared only after selection.
pub(super) type Deferred<R> = WithDemand<R, Lazy>;

/// A role whose trailing source argument may be absent; explicit omissions and blanks remain present.
/// Keep this wrapper outermost, including around `WithDemand<R, D>`, so absence
/// can be checked before preparing a deferred source.
pub(super) struct Optional<R>(pub(super) Option<R>, usize);
impl<R: ArgumentRole> ArgumentRole for Optional<R> {
    const BROADCAST: bool = R::BROADCAST;
    const EAGER: bool = R::EAGER;
    fn argument(&self) -> usize {
        self.1
    }
    fn bind(argument: usize, member: usize, present: bool) -> Self {
        Self(present.then(|| R::bind(argument, member, true)), argument)
    }
}

/// A tuple of typed roles with shared eager broadcast admission.
pub(super) trait Signature: Sized {
    /// Bind source ordinals without inspecting geometry or forcing arguments.
    fn bind(present: usize) -> Self;
    /// Admit eager geometry and bind roles without reading scalar or collection members.
    fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)>;
}
/// A signature whose eager source expressions can be prepared in source order.
pub(super) trait PreparedSignature: Signature {
    /// Prepare eager operands while retaining lazy member reads and original roles.
    fn prepare(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<()>;
}
macro_rules! signatures {
    ($($role:ident:$index:tt),+) => {
        impl<$($role: ArgumentRole),+> PreparedSignature for ($($role,)+) {
            fn prepare(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<()> {
                $(if $index < host.len() && $role::EAGER { host.describe(self.$index.argument())?; })+
                Ok(())
            }
        }
        impl<$($role: ArgumentRole),+> Signature for ($($role,)+) {
            fn bind(present: usize) -> Self {
                let mut member = 0;
                let bound = ($({
                    let role = $role::bind($index, member, $index < present);
                    member += usize::from($role::BROADCAST && $role::EAGER && $index < present);
                    role
                },)+);
                let _ = member;
                bound
            }
            fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)> {
                let roles = [$($role::BROADCAST && $role::EAGER),+];
                let mut selected = [0usize; [$($index),+].len()];
                let mut count = 0;
                for (index, broadcast) in roles.into_iter().enumerate().take(host.len()) {
                    if broadcast {
                        // SAFETY: selected and roles have equal lengths; count cannot exceed the visited prefix.
                        selected[count] = index;
                        count += 1;
                    }
                }
                let indices = selected.get(..count).ok_or(VALUE_ERROR)?;
                let lift = if indices.iter().copied().eq(0..count) {
                    Lift::new(host, count)?
                } else {
                    Lift::new_selected(host, indices)?
                };
                let bound = Self::bind(host.len());
                Ok((lift, bound))
            }
        }
    };
}
signatures!(A:0);
signatures!(A:0, B:1);
signatures!(A:0, B:1, C:2);
signatures!(A:0, B:1, C:2, D:3);
signatures!(A:0, B:1, C:2, D:3, E:4);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

/// Connects a kernel's current output coordinate to the shared broadcast plan and evaluator services.
/// Argument roles use it to read corresponding source members without owning traversal state.
pub(super) struct MemberContext<'member, 'call, 'workbook> {
    /// Shared operational services; the family retains control over coercion order.
    pub(super) host: &'member mut CallHost<'call, 'workbook>,
    lift: &'member Lift,
    row: usize,
    column: usize,
}

/// A context-aware scalar kernel with explicit typed argument roles.
pub(super) trait Kernel {
    /// Deferred argument roles supplying the kernel's scalar and collection inputs.
    type Signature: Signature;
    /// Prepare shared collections after geometry admission and before output traversal.
    fn prepare(
        &mut self,
        _host: &mut CallHost<'_, '_>,
        _arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        Ok(())
    }
    /// Calculate a member, reading and coercing roles in the family's required order.
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue>;
}

/// Execute a typed kernel while keeping operational failures outside spreadsheet cells.
pub(super) fn execute<K: Kernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    mut kernel: K,
) -> FunctionResult<Value> {
    let (lift, arguments) = K::Signature::admit(host)?;
    kernel.prepare(host, &arguments)?;
    lift.evaluate(host, address, |host, row, column| {
        kernel.apply(
            &mut MemberContext {
                host,
                lift: &lift,
                row,
                column,
            },
            &arguments,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_optional_deferred_shapes_do_not_admit_or_prepare_sources() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let arguments = vec![bound::bind(
            &poirot_parsing::formula::parse_formula("{1;2}").unwrap(),
        )];
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        /// Trailing optional roles retain absence outside their demand policy.
        type Roles = (
            Broadcast,
            Optional<Deferred<Broadcast>>,
            Optional<Deferred<FixedScalar>>,
            Optional<Deferred<WholeCollection>>,
        );
        let (lift, (_, broadcast, scalar, collection)) = Roles::admit(&mut host).unwrap();
        assert_eq!(lift.dimensions(), (2, 1));
        assert_eq!(broadcast.argument(), 1);
        assert_eq!(scalar.argument(), 2);
        assert_eq!(collection.argument(), 3);
        assert!(broadcast.0.is_none());
        assert!(scalar.0.is_none());
        assert!(collection.0.is_none());
    }

    #[test]
    fn optional_deferred_shapes_keep_omission_present_and_preserve_value_channels() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let blank_reference = bound::bind(&poirot_parsing::formula::parse_formula("A1").unwrap());
        // Each shape keeps omission present; selectors inspect omission before preparation.
        for (case, arguments) in [
            vec![bound::Expression::Missing; 3],
            vec![blank_reference; 3],
            vec![bound::Expression::Number(7.0); 3],
        ]
        .into_iter()
        .enumerate()
        {
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            let mut host = CallHost::new_inheriting(
                &mut session,
                0,
                CellAddress { row: 0, col: 0 },
                &arguments,
            );
            /// Optional roles distinguish absent slots from explicitly supplied sources.
            type Roles = (
                Optional<Deferred<Broadcast>>,
                Optional<Deferred<FixedScalar>>,
                Optional<Deferred<WholeCollection>>,
            );
            let (lift, (broadcast, scalar, collection)) = Roles::admit(&mut host).unwrap();
            assert_eq!(lift.dimensions(), (1, 1));
            assert_eq!(host.selected_is_missing(broadcast.argument()), case == 0);
            assert_eq!(host.selected_is_missing(scalar.argument()), case == 0);
            assert_eq!(host.selected_is_missing(collection.argument()), case == 0);
            let values = [
                broadcast
                    .0
                    .unwrap()
                    .prepare(&mut host)
                    .unwrap()
                    .into_value(),
                scalar.0.unwrap().prepare(&mut host).unwrap().into_value(),
                collection
                    .0
                    .unwrap()
                    .prepare(&mut host)
                    .unwrap()
                    .into_value(),
            ];
            for value in values {
                assert!(match case {
                    // The host's ordinary expression path projects omission to blank.
                    0 => matches!(value, Value::Scalar(Blank)),
                    1 => matches!(value, Value::Refs(_)),
                    2 => matches!(value, Value::Scalar(Number(7.0))),
                    _ => unreachable!(),
                });
            }
        }
    }

    #[test]
    fn eager_fixed_and_collection_shapes_do_not_contribute_geometry_or_shift_broadcast_ordinals() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let arguments: Vec<_> = ["{1;2}", "{3;4;5}", "{6,7,8}", "{10,20}"]
            .into_iter()
            .map(|formula| bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()))
            .collect();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        /// Only the first and fourth eager shapes contribute broadcast members.
        type Roles = (
            Broadcast,
            WithDemand<FixedScalar>,
            WithDemand<WholeCollection>,
            Broadcast,
        );
        let (lift, (first, scalar, collection, last)) = Roles::admit(&mut host).unwrap();
        assert_eq!(lift.dimensions(), (2, 2));
        assert_eq!(scalar.role().argument(), 1);
        assert_eq!(collection.role().argument(), 2);
        let mut context = MemberContext {
            host: &mut host,
            lift: &lift,
            row: 1,
            column: 1,
        };
        assert_eq!(first.read(&mut context).unwrap().value, Number(2.0));
        assert_eq!(last.read(&mut context).unwrap().value, Number(20.0));
    }

    #[test]
    fn deferred_and_nonbroadcast_roles_do_not_shift_eager_member_ordinals() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let arguments: Vec<_> = ["{1;2}", "RAND()", "{3;4;5}", "{10,20}"]
            .into_iter()
            .map(|formula| bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()))
            .collect();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        {
            let mut host = CallHost::new_inheriting(
                &mut session,
                0,
                CellAddress { row: 0, col: 0 },
                &arguments,
            );
            /// Deferred broadcast and fixed roles leave eager member ordinals contiguous.
            type Roles = (Broadcast, Deferred<Broadcast>, FixedScalar, Broadcast);
            let (lift, (first, deferred, fixed, last)) = Roles::admit(&mut host).unwrap();
            assert_eq!(lift.dimensions(), (2, 2));
            assert_eq!(deferred.argument(), 1);
            assert_eq!(fixed.argument(), 2);
            let mut context = MemberContext {
                host: &mut host,
                lift: &lift,
                row: 1,
                column: 1,
            };
            assert_eq!(first.read(&mut context).unwrap().value, Number(2.0));
            assert_eq!(last.read(&mut context).unwrap().value, Number(20.0));
        }
        assert!(session.budget.usage.random_accesses.is_empty());
    }

    #[test]
    fn demand_is_orthogonal_to_shape_and_deferred_broadcast_skips_admission() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let arguments: Vec<_> = ["{1;2}", "RAND()", "RAND()", "RAND()"]
            .into_iter()
            .map(|formula| bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()))
            .collect();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        {
            let mut host = CallHost::new_inheriting(
                &mut session,
                0,
                CellAddress { row: 0, col: 0 },
                &arguments,
            );
            /// One eager broadcast and three independently deferred argument shapes.
            type Roles = (
                WithDemand<Broadcast>,
                Deferred<Broadcast>,
                Deferred<FixedScalar>,
                Deferred<WholeCollection>,
            );
            let (lift, (eager, broadcast, scalar, collection)) = Roles::admit(&mut host).unwrap();
            assert_eq!(lift.dimensions(), (2, 1));
            assert_eq!(eager.argument(), 0);
            assert_eq!(broadcast.argument(), 1);
            assert_eq!(scalar.argument(), 2);
            assert_eq!(collection.argument(), 3);
            // No deferred shape was admitted: none of the three RAND expressions ran.
            let _ = (broadcast, scalar, collection);
        }
        assert!(session.budget.usage.random_accesses.is_empty());
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        let broadcast = Deferred::<Broadcast>::bind(1, 0, true)
            .prepare(&mut host)
            .unwrap();
        assert_eq!(broadcast.shape(&host).unwrap().rows, 1);
        let scalar = Deferred::<FixedScalar>::bind(2, 0, true)
            .prepare(&mut host)
            .unwrap();
        let collection = Deferred::<WholeCollection>::bind(3, 0, true)
            .prepare(&mut host)
            .unwrap();
        for value in [
            broadcast.into_value(),
            scalar.into_value(),
            collection.into_value(),
        ] {
            assert!(matches!(value, Value::Scalar(Number(n)) if (0.0..1.0).contains(&n)));
        }
        drop(host);
        assert_eq!(session.budget.usage.random_accesses[0].draws, 3);
    }
}
