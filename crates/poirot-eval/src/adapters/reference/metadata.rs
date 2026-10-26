//! Reference metadata kernels with shared binding, conversion and publication.
use crate::adapters::elementwise::reference::Axis;
use crate::adapters::elementwise::{
    self, Broadcast, Deferred, Kernel, MemberContext, NumericParameter, NumericSignature, Optional,
    Signature, SourceIdentity, WholeCollection,
};
use crate::host::CallHost;
use crate::*;

/// Format one ADDRESS member after shared numeric conversion and array admission.
struct Address;
impl Kernel for Address {
    /// Broadcast coordinates and optional spelling controls.
    type Signature = (
        Broadcast,
        Broadcast,
        Optional<Broadcast>,
        Optional<Broadcast>,
        Optional<Broadcast>,
    );
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        roles: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let numbers = NumericSignature::new(&[
            NumericParameter::required(),
            NumericParameter::required(),
            NumericParameter::constant(1.),
            NumericParameter::boolean_default(true),
        ])
        .numbers_roles(
            context,
            [Some(roles.0), Some(roles.1), roles.2.0, roles.3.0],
        )?;
        let row = numbers.get(0)?.trunc();
        let column = numbers.get(1)?.trunc();
        if !(1. ..=f64::from(MAX_ROWS)).contains(&row)
            || !(1. ..=f64::from(MAX_COLUMNS)).contains(&column)
        {
            return Err(VALUE_ERROR.into());
        }
        let sheet = roles
            .4
            .0
            .as_ref()
            .map(|role| role.read(context))
            .transpose()?;
        let sheet = match sheet {
            Some(operand) if !operand.missing => {
                Some(context.host.coercev2_scalar_text(&operand.value)?)
            }
            _ => None,
        };
        let capacity = sheet
            .as_ref()
            .map_or(Some(32), |sheet| {
                sheet.len().checked_mul(4).and_then(|n| n.checked_add(32))
            })
            .ok_or_else(|| context.host.size_overflow())?;
        context.host.check_text(capacity)?;
        use poirot_functions_lookup::{AddressAxes, AddressStyle};
        let axes = match numbers.get(2)? as u8 {
            1 => AddressAxes::Both,
            2 => AddressAxes::Row,
            3 => AddressAxes::Column,
            4 => AddressAxes::Relative,
            _ => return Err(VALUE_ERROR.into()),
        };
        let style = if numbers.get(3)? != 0. {
            AddressStyle::A1
        } else {
            AddressStyle::R1C1
        };
        let result = poirot_functions_lookup::address_text(
            row as u32,
            column as u32,
            axes,
            style,
            sheet.as_deref(),
        )
        .map_err(|error| match error {
            poirot_functions_lookup::Failure::Spreadsheet(_) => {
                FunctionFailure::Spreadsheet(VALUE_ERROR)
            }
            poirot_functions_lookup::Failure::Operational(_) => {
                context.host.allocation_failure().into()
            }
            poirot_functions_lookup::Failure::Unsupported(_) => context.host.unsupported(
                CapabilityId::FunctionAdapter,
                "ADDRESS sheet-name control character",
            ),
        })?;
        Ok(Text(result))
    }
}

/// Execute metadata without eagerly dereferencing retained source operands.
pub(super) fn execute(
    host: &mut CallHost<'_, '_>,
    function: ReferenceFunction,
) -> FunctionResult<Value> {
    let (source,) = <(SourceIdentity,)>::bind(host.len());
    let axis = match function {
        ReferenceFunction::Rows | ReferenceFunction::Row => Axis::Row,
        _ => Axis::Column,
    };
    match function {
        ReferenceFunction::Address => elementwise::execute(host, host.target().address, Address),
        ReferenceFunction::Areas => source.areas(host).map(Into::into),
        ReferenceFunction::Sheet if host.len() == 0 => {
            Ok(Number(host.target().sheet as f64 + 1.).into())
        }
        ReferenceFunction::Sheet => source.sheet(host).map(Into::into),
        ReferenceFunction::Rows | ReferenceFunction::Columns => {
            source.dimension(host, axis).map(Into::into)
        }
        ReferenceFunction::Row | ReferenceFunction::Column if host.len() == 0 => {
            let address = host.target().address;
            Ok(Number(
                f64::from(match axis {
                    Axis::Row => address.row,
                    Axis::Column => address.col,
                }) + 1.,
            )
            .into())
        }
        ReferenceFunction::Row | ReferenceFunction::Column => source.coordinates(host, axis),
        ReferenceFunction::Hyperlink => {
            let (location, label) = <(
                Deferred<WholeCollection>,
                Optional<Deferred<WholeCollection>>,
            )>::bind(host.len());
            let selected = label.0.as_ref().unwrap_or(&location);
            Ok(selected.prepare(host)?.into_value())
        }
        _ => Err(VALUE_ERROR.into()),
    }
}
