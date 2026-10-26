//! Historical euro conversion from factual fixed rates and documented Excel rules.
//! Rates: Bank of Greece, irrevocable euro conversion rates, read 2026-10-22:
//! <https://www.bankofgreece.gr/en/main-tasks/markets/exchange-rates/irrevocable-euro-conversion-rates>
//! Precision: <https://support.microsoft.com/en-us/excel/functions/euroconvert-function>
//! Constants were independently transcribed; no upstream code or algorithm was reused.
use super::{FinancialError, Result, checked};

/// Currency admitted by the documented EUROCONVERT historical currency table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EuroCurrency {
    /// Euro.
    Eur,
    /// Austrian schilling.
    Ats,
    /// Belgian franc.
    Bef,
    /// German mark.
    Dem,
    /// Spanish peseta.
    Esp,
    /// Finnish markka.
    Fim,
    /// French franc.
    Frf,
    /// Greek drachma.
    Grd,
    /// Irish pound.
    Iep,
    /// Italian lira.
    Itl,
    /// Luxembourg franc.
    Luf,
    /// Dutch guilder.
    Nlg,
    /// Portuguese escudo.
    Pte,
    /// Slovenian tolar.
    Sit,
}
impl EuroCurrency {
    /// Admit an ISO code without locale-sensitive case conversion.
    pub fn parse(code: &str) -> Option<Self> {
        [
            Self::Eur,
            Self::Ats,
            Self::Bef,
            Self::Dem,
            Self::Esp,
            Self::Fim,
            Self::Frf,
            Self::Grd,
            Self::Iep,
            Self::Itl,
            Self::Luf,
            Self::Nlg,
            Self::Pte,
            Self::Sit,
        ]
        .into_iter()
        .find(|currency| code.eq_ignore_ascii_case(currency.code()))
    }
    /// Historical ISO spelling of this currency.
    pub fn code(self) -> &'static str {
        match self {
            Self::Eur => "EUR",
            Self::Ats => "ATS",
            Self::Bef => "BEF",
            Self::Dem => "DEM",
            Self::Esp => "ESP",
            Self::Fim => "FIM",
            Self::Frf => "FRF",
            Self::Grd => "GRD",
            Self::Iep => "IEP",
            Self::Itl => "ITL",
            Self::Luf => "LUF",
            Self::Nlg => "NLG",
            Self::Pte => "PTE",
            Self::Sit => "SIT",
        }
    }
    fn rate(self) -> f64 {
        match self {
            Self::Eur => 1.0,
            Self::Ats => 13.7603,
            Self::Bef | Self::Luf => 40.3399,
            Self::Dem => 1.95583,
            Self::Esp => 166.386,
            Self::Fim => 5.94573,
            Self::Frf => 6.55957,
            Self::Grd => 340.750,
            Self::Iep => 0.787564,
            Self::Itl => 1936.27,
            Self::Nlg => 2.20371,
            Self::Pte => 200.482,
            Self::Sit => 239.640,
        }
    }
    fn precision(self) -> f64 {
        match self {
            Self::Bef | Self::Luf | Self::Esp | Self::Itl => 0.0,
            _ => 2.0,
        }
    }
}
/// Convert fixed national units through EUR, with optional significant-digit triangulation.
pub fn euro_convert(
    number: f64,
    source: EuroCurrency,
    target: EuroCurrency,
    full_precision: bool,
    triangulation: Option<f64>,
) -> Result<f64> {
    if !number.is_finite()
        || triangulation.is_some_and(|p| !p.is_finite() || p < 3.0 || p != p.trunc())
    {
        return Err(FinancialError::Value);
    }
    if source == target {
        return Ok(number);
    }
    let mut euro = number / source.rate();
    if source != EuroCurrency::Eur
        && let Some(digits) = triangulation
        && euro != 0.0
    {
        let places = digits - 1.0 - euro.abs().log10().floor();
        euro = crate::math::round(euro, places).map_err(|_| FinancialError::Value)?;
    }
    let value = checked(euro * target.rate())?;
    if full_precision {
        Ok(value)
    } else {
        crate::math::round(value, target.precision()).map_err(|_| FinancialError::Value)
    }
}
