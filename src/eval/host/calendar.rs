//! Evaluator calendar coercion and captured clock.
use crate::eval::calendar::{civil, serial, valid};
use crate::eval::*;

impl Session<'_> {
    /// Convert the captured clock to a workbook serial with consumed context reads.
    pub(in crate::eval) fn clock_serial(&mut self) -> Result<f64, CellError> {
        self.read(DependencyRead::ContextField(run::ContextRead::AsOf));
        self.read(DependencyRead::ContextField(run::ContextRead::UtcOffset));
        let o = self.context;
        if !(-86400..=86400).contains(&o.localization.utc_offset_seconds) {
            return Err(Num);
        }
        let sec = crate::eval::clock::unix_seconds(o.as_of_time)
            + f64::from(o.localization.utc_offset_seconds);
        let days = (sec / 86400.).floor();
        if !(-25568. ..2_932_897.).contains(&days) {
            return Err(Num);
        }
        let d = civil(days as i64);
        self.read(DependencyRead::ContextField(run::ContextRead::DateSystem));
        valid(
            serial(d, self.workbook.date_system)? + sec.rem_euclid(86400.) / 86400.,
            self.workbook.date_system,
        )
    }
    /// Coerce a calendar serial with attributed text-date context reads.
    pub(in crate::eval) fn coercev2_date_argument(&mut self, v: &CellValue) -> FunctionResult<f64> {
        let n = if let Some(text) = v.text_ref() {
            let text = checked_text_view(text, &mut self.budget)?;
            match self.coercev2_formatted_text(&text)? {
                super::numeric_policy::CoercionV2Decision::Number(number) => number?,
                super::numeric_policy::CoercionV2Decision::DateTime => {
                    self.coercev2_numeric_date_time(&text)?
                }
            }
        } else {
            self.coercev2_scalar_number(v)?
        };
        Ok(valid(n, self.workbook.date_system)?)
    }
}
