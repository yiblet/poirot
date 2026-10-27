//! Declarative numeric coercion and allocation-free homogeneous role binding.
use super::argument_role::ArgumentRole;
use super::broadcast::Broadcast;
use super::lift::Lift;
use super::member_context::MemberContext;
use super::tuple_signature::{PreparedSignature, Signature};
use super::whole_collection::WholeCollection;
use crate::eval::host::{ArgumentCoercion, CallHost, ScalarOperand};
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR};
use crate::model::CellError::Num;
use crate::model::CellValue;
use crate::model::CellValue::{Blank, Error, Number, Text};
use std::marker::PhantomData;

/// Signature wrapper that prepares eager source expressions in order before planning broadcast output.
/// Evaluation follows source positions even when only some roles contribute output dimensions.
pub(in crate::eval::adapters) struct PreparedSources<S>(pub(in crate::eval::adapters) S);

impl<S: PreparedSignature> Signature for PreparedSources<S> {
    fn bind(present: usize) -> Self {
        Self(S::bind(present))
    }

    fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)> {
        let roles = Self::bind(host.len());
        roles.0.prepare(host)?;
        let (lift, _) = S::admit(host)?;
        Ok((lift, roles))
    }
}

/// Homogeneous argument roles bound by source position for variadic functions.
/// Roles are created on demand from the count and starting slot, without reading arguments or allocating a role list.
pub(in crate::eval::adapters) struct Variadic<R> {
    count: usize,
    start: usize,
    role: PhantomData<R>,
}

impl<R: ArgumentRole> Variadic<R> {
    /// Return the number of supplied source arguments, including explicit omissions.
    pub(in crate::eval::adapters) fn len(&self) -> usize {
        self.count
    }

    /// Bind one present source role while preserving its original ordinal.
    pub(in crate::eval::adapters) fn get(&self, index: usize) -> Option<R> {
        (index < self.count).then(|| R::bind(self.start.saturating_add(index), index, true))
    }

    /// Bind present roles in source order without evaluating their operands.
    pub(in crate::eval::adapters) fn iter(&self) -> impl ExactSizeIterator<Item = R> {
        (0..self.count).map(|index| R::bind(self.start.saturating_add(index), index, true))
    }
}

impl<R: ArgumentRole> Signature for Variadic<R> {
    fn bind(present: usize) -> Self {
        Self {
            count: present,
            start: 0,
            role: PhantomData,
        }
    }

    fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)> {
        let count = host.len();
        let lifted = if R::BROADCAST && R::EAGER { count } else { 0 };
        Ok((Lift::new(host, lifted)?, Self::bind(count)))
    }
}

/// NPV signature with one broadcast rate followed by whole cash-flow collections.
/// Only the rate contributes output dimensions; each output consumes the collections in source order.
pub(in crate::eval::adapters) struct BroadcastCollections {
    /// Scalar control contributing the output geometry.
    pub(in crate::eval::adapters) scalar: Broadcast,
    /// Remaining arguments consumed as complete ordered collections.
    pub(in crate::eval::adapters) collections: Variadic<WholeCollection>,
}

impl Signature for BroadcastCollections {
    fn bind(present: usize) -> Self {
        Self {
            scalar: Broadcast::bind(0, 0, present > 0),
            collections: Variadic {
                count: present.saturating_sub(1),
                start: 1,
                role: PhantomData,
            },
        }
    }

    fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)> {
        Ok((
            Lift::new(host, usize::from(host.len() > 0))?,
            Self::bind(host.len()),
        ))
    }
}

impl Signature for () {
    fn bind(_present: usize) -> Self {}

    fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)> {
        Ok((Lift::new(host, 0)?, ()))
    }
}

/// Numeric replacement for an absent argument or an explicit omitted argument.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) enum NumericDefault {
    Required,
    Constant(f64),
    Prior(usize),
}

/// Scalar conversion applied before a pure numeric kernel receives its arguments.
#[derive(Clone, Copy)]
enum NumericCoercion {
    Number,
    Boolean,
    NonTextNumber,
    CalendarDate,
    CalendarSerial,
    DecimalNumber,
}

/// Conversion of a present omitted numeric slot, independently of absent defaults.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NumericOmission {
    /// Retain the parameter's declared absent/default behavior.
    Default,
    /// Convert the omitted slot as an actual blank value.
    Blank,
}

/// Conversion and omission policy for one numeric kernel parameter.
/// It can also restrict integer selectors to a declared set of choices before the family algorithm runs.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) struct NumericParameter {
    default: NumericDefault,
    coercion: NumericCoercion,
    choices: Option<&'static [i32]>,
    omission: NumericOmission,
}

impl NumericParameter {
    /// Declare ordinary numeric coercion with a chosen omission rule.
    pub(in crate::eval::adapters) const fn new(default: NumericDefault) -> Self {
        Self {
            default,
            coercion: NumericCoercion::Number,
            choices: None,
            omission: NumericOmission::Default,
        }
    }

    /// Coerce a present omitted slot as blank while retaining the absent default.
    pub(in crate::eval::adapters) const fn missing_as_blank(mut self) -> Self {
        self.omission = NumericOmission::Blank;
        self
    }

    /// Reject a truncated numeric selector outside the declared finite choices.
    pub(in crate::eval::adapters) const fn integer_choices(
        mut self,
        choices: &'static [i32],
    ) -> Self {
        self.choices = Some(choices);
        self
    }

    /// Reject absent or explicitly omitted operands.
    pub(in crate::eval::adapters) const fn required() -> Self {
        Self::new(NumericDefault::Required)
    }

    /// Replace absence or explicit omission with a constant, leaving blanks intact.
    pub(in crate::eval::adapters) const fn constant(value: f64) -> Self {
        Self::new(NumericDefault::Constant(value))
    }

    /// Default to an already coerced earlier parameter from the same output member.
    pub(in crate::eval::adapters) const fn prior(index: usize) -> Self {
        Self::new(NumericDefault::Prior(index))
    }

    /// Reject text values while retaining ordinary numeric and blank coercion.
    pub(in crate::eval::adapters) const fn nontext() -> Self {
        Self {
            default: NumericDefault::Required,
            coercion: NumericCoercion::NonTextNumber,
            choices: None,
            omission: NumericOmission::Default,
        }
    }

    /// Require finite decimal text or an existing number, rejecting logical and blank inputs.
    pub(in crate::eval::adapters) const fn decimal() -> Self {
        Self {
            default: NumericDefault::Required,
            coercion: NumericCoercion::DecimalNumber,
            choices: None,
            omission: NumericOmission::Default,
        }
    }

    /// Convert a required calendar date to its workbook serial value.
    pub(in crate::eval::adapters) const fn calendar_date() -> Self {
        Self {
            default: NumericDefault::Required,
            coercion: NumericCoercion::CalendarDate,
            choices: None,
            omission: NumericOmission::Default,
        }
    }

    /// Convert a calendar serial, treating omission as zero and preserving date-domain errors.
    pub(in crate::eval::adapters) const fn date_serial() -> Self {
        Self {
            default: NumericDefault::Constant(0.),
            coercion: NumericCoercion::CalendarSerial,
            choices: None,
            omission: NumericOmission::Default,
        }
    }

    /// Coerce a logical selector, substituting a default only for omission.
    pub(in crate::eval::adapters) const fn boolean_default(value: bool) -> Self {
        Self {
            default: NumericDefault::Constant(if value { 1.0 } else { 0.0 }),
            coercion: NumericCoercion::Boolean,
            choices: None,
            omission: NumericOmission::Default,
        }
    }

    /// Coerce a required logical selector to numeric zero or one.
    pub(in crate::eval::adapters) const fn boolean() -> Self {
        Self {
            default: NumericDefault::Required,
            coercion: NumericCoercion::Boolean,
            choices: None,
            omission: NumericOmission::Default,
        }
    }
}

/// At most twelve coerced parameters retained on the stack for one output member.
pub(in crate::eval::adapters) struct NumericArguments {
    values: [f64; 12],
    len: usize,
}

impl NumericArguments {
    /// Borrow the initialized numeric argument prefix.
    pub(in crate::eval::adapters) fn as_slice(&self) -> &[f64] {
        self.values.get(..self.len).unwrap_or(&[])
    }

    /// Read one initialized parameter through a checked ordinal.
    pub(in crate::eval::adapters) fn get(&self, index: usize) -> FunctionResult<f64> {
        self.as_slice()
            .get(index)
            .copied()
            .ok_or(VALUE_ERROR.into())
    }
}

/// Ordering of spreadsheet error admission relative to numeric conversion.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::eval::adapters) enum NumericErrorAdmission {
    /// Read and convert each operand before visiting its successor.
    Ordinal,
    /// Admit source errors across the eager signature before converting values.
    BeforeCoercion,
}

/// Applies a family's numeric parameter rules to one output member.
/// It centralizes defaults, coercion, and source-error precedence while leaving source roles and calculations to the caller.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) struct NumericSignature<'a> {
    parameters: &'a [NumericParameter],
    errors: NumericErrorAdmission,
}

impl<'a> NumericSignature<'a> {
    /// Retain a declarative, source-ordered parameter table.
    pub(in crate::eval::adapters) const fn new(parameters: &'a [NumericParameter]) -> Self {
        Self {
            parameters,
            errors: NumericErrorAdmission::Ordinal,
        }
    }

    /// Select when eager source errors take precedence over conversion errors.
    pub(in crate::eval::adapters) const fn with_error_admission(
        mut self,
        errors: NumericErrorAdmission,
    ) -> Self {
        self.errors = errors;
        self
    }

    /// Coerce supplied members and apply omitted defaults in source order.
    pub(in crate::eval::adapters) fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Variadic<Broadcast>,
    ) -> FunctionResult<NumericArguments> {
        self.numbers_roles(context, arguments.iter().map(Some))
    }

    /// Coerce selected tuple roles without admitting intervening collection geometry.
    /// A missing role uses its table default; supplied roles retain their source ordinals.
    pub(in crate::eval::adapters) fn numbers_roles<I>(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        roles: I,
    ) -> FunctionResult<NumericArguments>
    where
        I: IntoIterator<Item = Option<Broadcast>>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut roles = roles.into_iter();
        if roles.len() > self.parameters.len() {
            return Err(VALUE_ERROR.into());
        }
        let lift = context.lift;
        let row = context.row;
        let column = context.column;
        self.read(context.host, |host, _| {
            roles
                .next()
                .flatten()
                .map(|role| {
                    role.read(&mut MemberContext {
                        host,
                        lift,
                        row,
                        column,
                    })
                })
                .transpose()
        })
    }

    /// Coerce fixed scalar slots in source order without admitting their collection geometry.
    pub(in crate::eval::adapters) fn scalars(
        &self,
        host: &mut CallHost<'_, '_>,
        start: usize,
    ) -> FunctionResult<NumericArguments> {
        self.read(host, |host, index| {
            let index = start.checked_add(index).ok_or(VALUE_ERROR)?;
            if index >= host.len() {
                return Ok(None);
            }
            super::fixed_scalar::FixedScalar::at(index)
                .read(host)
                .map(Some)
        })
    }

    /// Apply one coercion/default protocol to fixed and broadcast operand readers.
    fn read(
        &self,
        host: &mut CallHost<'_, '_>,
        mut read: impl FnMut(
            &mut CallHost<'_, '_>,
            usize,
        ) -> FunctionResult<Option<crate::eval::host::ScalarOperand>>,
    ) -> FunctionResult<NumericArguments> {
        if self.parameters.len() > 12 {
            return Err(VALUE_ERROR.into());
        }
        if self.errors == NumericErrorAdmission::Ordinal {
            return self.coerce(host, read);
        }
        let mut prepared: [Option<crate::eval::host::ScalarOperand>; 12] =
            std::array::from_fn(|_| None);
        for (index, _) in self.parameters.iter().enumerate() {
            let operand = read(host, index)?;
            if let Some(operand) = &operand
                && let Error(error) = operand.value
            {
                return Err(error.into());
            }
            *prepared.get_mut(index).ok_or(VALUE_ERROR)? = operand;
        }
        self.coerce(host, |_, index| {
            Ok(prepared.get_mut(index).ok_or(VALUE_ERROR)?.take())
        })
    }

    /// Convert admitted operands and defaults without re-reading eager sources.
    fn coerce(
        &self,
        host: &mut CallHost<'_, '_>,
        mut read: impl FnMut(
            &mut CallHost<'_, '_>,
            usize,
        ) -> FunctionResult<Option<crate::eval::host::ScalarOperand>>,
    ) -> FunctionResult<NumericArguments> {
        if self.parameters.len() > 12 {
            return Err(VALUE_ERROR.into());
        }
        let mut result = NumericArguments {
            values: [0.0; 12],
            len: 0,
        };
        for (index, parameter) in self.parameters.iter().enumerate() {
            let operand = read(host, index)?.map(|mut operand| {
                if operand.missing && parameter.omission == NumericOmission::Blank {
                    operand.value = Blank;
                    operand.missing = false;
                }
                operand
            });
            let value = match operand {
                Some(operand) if !operand.missing => match parameter.coercion {
                    NumericCoercion::NonTextNumber if matches!(operand.value, Text(_)) => {
                        return Err(VALUE_ERROR.into());
                    }
                    NumericCoercion::Number | NumericCoercion::NonTextNumber => {
                        ArgumentCoercion::new(host).coerce_scalar_number(&operand.value)?
                    }
                    NumericCoercion::DecimalNumber => match operand.value {
                        Number(value) if value.is_finite() => value,
                        Error(error) => return Err(error.into()),
                        value if value.text_ref().is_some() => {
                            let text = ArgumentCoercion::new(host)
                                .project_text(value.text_ref().ok_or(VALUE_ERROR)?)?;
                            poirot_functions::coercion::compatibility::coerce_financial_decimal_text(&text)
                                .ok_or(VALUE_ERROR)?
                        }
                        _ => return Err(VALUE_ERROR.into()),
                    },
                    NumericCoercion::CalendarSerial => {
                        ArgumentCoercion::new(host).coerce_calendar_date(&operand.value)?
                    }
                    NumericCoercion::CalendarDate => {
                        if let Error(error) = operand.value {
                            return Err(error.into());
                        }
                        ArgumentCoercion::new(host)
                            .coerce_calendar_date(&operand.value)
                            .map_err(|error| match error {
                                FunctionError::Spreadsheet(_) => {
                                    FunctionError::Spreadsheet(VALUE_ERROR)
                                }
                                other => other,
                            })?
                    }
                    NumericCoercion::Boolean => f64::from(
                        crate::eval::values::core_projection::coerce_operand_boolean(
                            &operand.value,
                        )?,
                    ),
                },
                _ => match parameter.default {
                    NumericDefault::Required => return Err(VALUE_ERROR.into()),
                    NumericDefault::Constant(value) => value,
                    NumericDefault::Prior(prior) => result.get(prior)?,
                },
            };
            if let Some(choices) = parameter.choices
                && (!value.is_finite()
                    || !choices
                        .iter()
                        .any(|choice| f64::from(*choice) == value.trunc()))
            {
                return Err(Num.into());
            }
            *result.values.get_mut(index).ok_or(VALUE_ERROR)? = value;
            // SAFETY: parameters.len() was checked to be at most twelve above.
            result.len += 1;
        }
        Ok(result)
    }
}

impl Variadic<Broadcast> {
    /// Apply a shared numeric signature to this output member's broadcast roles.
    pub(in crate::eval::adapters) fn numbers(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        parameters: &[NumericParameter],
    ) -> FunctionResult<NumericArguments> {
        NumericSignature::new(parameters).numbers(context, self)
    }

    /// Materialize source-ordered scalar members with shared error propagation.
    pub(in crate::eval::adapters) fn scalar_values(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
    ) -> FunctionResult<Vec<CellValue>> {
        context.host.check_values(self.len())?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(self.len())
            .map_err(|reservation_source| context.host.reservation_failure(reservation_source))?;
        for role in self.iter() {
            let operand = role.read(context)?;
            if let Error(error) = operand.value {
                return Err(error.into());
            }
            values.try_reserve(1).map_err(|reservation_source| {
                context.host.reservation_failure(reservation_source)
            })?;
            // SAFETY: one additional slot was reserved immediately above.
            values.push(operand.value);
        }
        Ok(values)
    }

    /// Coerce a numeric option without truncating selector values before validation.
    pub(in crate::eval::adapters) fn number(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
        default: Option<f64>,
    ) -> FunctionResult<f64> {
        match self.get(index) {
            Some(role) => role.number(context, default),
            None => default.ok_or(VALUE_ERROR.into()),
        }
    }

    /// Read one present typed role in the current output coordinate.
    pub(in crate::eval::adapters) fn operand(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.get(index).ok_or(VALUE_ERROR)?.read(context)
    }

    /// Render a typed role without losing unpaired UTF-16 units.
    pub(in crate::eval::adapters) fn text_units(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
    ) -> FunctionResult<Vec<u16>> {
        let operand = self.operand(context, index)?;
        super::text::units(context.host, &operand)
    }

    /// Coerce an omitted integer slot as zero; only an absent slot uses the default.
    pub(in crate::eval::adapters) fn text_integer(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
        default: Option<i32>,
    ) -> FunctionResult<i32> {
        let parameter = default
            .map_or_else(NumericParameter::required, |value| {
                NumericParameter::constant(f64::from(value))
            })
            .missing_as_blank();
        let numbers =
            NumericSignature::new(&[parameter]).numbers_roles(context, [self.get(index)])?;
        super::text::integer(numbers.get(0)?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::whole_collection::WholeCollection;
    use super::super::with_demand::Deferred;
    use super::*;
    use crate::eval::compile;
    use crate::eval::config::EvaluationInputs;
    use crate::eval::session::Session;
    use crate::model::CellError::Div0;
    use crate::model::{CellAddress, Workbook};

    fn numbers(sources: &[&str], parameters: &[NumericParameter]) -> FunctionResult<Vec<f64>> {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let arguments: Vec<_> = sources
            .iter()
            .map(|source| {
                if source.is_empty() {
                    compile::bound::Expression::Missing
                } else {
                    compile::bound::bind(&poirot_parsing::formula::parse_formula(source).unwrap())
                }
            })
            .collect();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        let (lift, arguments) = Variadic::<Broadcast>::admit(&mut host)?;
        arguments
            .numbers(
                &mut MemberContext {
                    host: &mut host,
                    lift: &lift,
                    row: 0,
                    column: 0,
                },
                parameters,
            )
            .map(|values| values.as_slice().to_vec())
    }

    #[test]
    fn scalar_prefix_collection_tail_preserves_original_ordinals() {
        let roles = BroadcastCollections::bind(4);
        assert_eq!(roles.scalar.argument(), 0);
        assert_eq!(
            roles
                .collections
                .iter()
                .map(|role| role.argument())
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(roles.collections.get(3).is_none());
    }

    #[test]
    fn numeric_decimal_and_selector_constraints_reject_before_later_errors() {
        assert_eq!(
            numbers(&["\"1.25\""], &[NumericParameter::decimal()]).unwrap(),
            [1.25]
        );
        assert!(matches!(
            numbers(&["TRUE"], &[NumericParameter::decimal()]),
            Err(FunctionError::Spreadsheet(VALUE_ERROR))
        ));
        assert!(matches!(
            numbers(
                &["3", "1/0"],
                &[
                    NumericParameter::required().integer_choices(&[1, 2, 4]),
                    NumericParameter::required()
                ]
            ),
            Err(FunctionError::Spreadsheet(Num))
        ));
    }

    #[test]
    fn numeric_defaults_distinguish_missing_blank_and_absent() {
        let parameters = [
            NumericParameter::constant(2.0),
            NumericParameter::constant(9.0),
            NumericParameter::required(),
            NumericParameter::prior(2),
        ];
        assert_eq!(
            numbers(&["", "A1", "7"], &parameters).unwrap(),
            [2.0, 0.0, 7.0, 7.0]
        );
        assert!(numbers(&[""], &[NumericParameter::required()]).is_err());
        assert_eq!(
            numbers(&["\"TRUE\""], &[NumericParameter::boolean()]).unwrap(),
            [1.0]
        );
    }

    #[test]
    fn numeric_signatures_check_capacity_defaults_and_source_errors() {
        assert!(numbers(&[], &[NumericParameter::constant(0.0); 13]).is_err());
        assert!(numbers(&[], &[NumericParameter::prior(0)]).is_err());
        assert!(numbers(&["1", "2"], &[NumericParameter::required()]).is_err());
        assert!(matches!(
            numbers(&["1/0", "#N/A"], &[NumericParameter::required(); 2]),
            Err(FunctionError::Spreadsheet(Div0))
        ));
    }

    #[test]
    fn variadic_binding_and_nonbroadcast_admission_do_not_force_sources() {
        let mut workbook = Workbook::new();
        workbook.add_sheet("S").unwrap();
        let sources = vec![compile::bound::bind(
            &poirot_parsing::formula::parse_formula("RAND()").unwrap(),
        )];
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        {
            let mut host =
                CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &sources);
            let (lift, collection) = Variadic::<WholeCollection>::admit(&mut host).unwrap();
            assert_eq!(lift.dimensions(), (1, 1));
            assert_eq!(collection.len(), 1);
            assert_eq!(
                collection
                    .iter()
                    .map(|role| role.argument())
                    .collect::<Vec<_>>(),
                [0]
            );
            assert!(collection.get(1).is_none());
            let (_, lazy) = Variadic::<Deferred<Broadcast>>::admit(&mut host).unwrap();
            assert_eq!(lazy.get(0).unwrap().argument(), 0);
            assert_eq!(<()>::admit(&mut host).unwrap().0.dimensions(), (1, 1));
        }
        assert!(
            session
                .state
                .runtime
                .budget
                .usage
                .random_accesses
                .is_empty()
        );
    }
}
