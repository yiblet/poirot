use super::call_host::CallHost;
use super::dynamic_references::DynamicReferences;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::inputs::EffectiveWorkbook;
use crate::eval::limits::CapabilityId;
use crate::eval::ports::DependencyRead;
use crate::eval::session::ReadRecorder;
use crate::eval::values::FunctionResult;
use crate::model::{CellAddress, CellError, CellValue, HorizontalAlignment};

/// Named CELL requests accepted at the formula text boundary.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CellInformationKind {
    Address,
    Row,
    Col,
    Contents,
    Type,
    Prefix,
    Format,
    Color,
    Parentheses,
    Filename,
    Protect,
    Width,
}

/// Answers English CELL requests using explicit single-sheet reference metadata.
/// Ranges select their upper-left cell. Format and color requests recognize only
/// the format strings listed below; parentheses currently returns zero for those
/// formats, including currency formats. Other formats and requests requiring UI
/// selection, file identity, protection or display width are unsupported.
/// Contract: <https://support.microsoft.com/en-us/excel/functions/cell-function>
/// Independently implemented; no upstream code or fixtures are reused.
pub(in crate::eval) struct CellInformation<'h, 'call, 'workbook> {
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> CellInformation<'h, 'call, 'workbook> {
    /// Borrow a call whose reads and allocations are recorded.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Answer one request for the cell selected by the reference adapter.
    pub(in crate::eval) fn read(
        &mut self,
        request: &str,
        sheet: usize,
        address: CellAddress,
    ) -> FunctionResult<CellValue> {
        let request = [
            ("address", CellInformationKind::Address),
            ("row", CellInformationKind::Row),
            ("col", CellInformationKind::Col),
            ("contents", CellInformationKind::Contents),
            ("type", CellInformationKind::Type),
            ("prefix", CellInformationKind::Prefix),
            ("format", CellInformationKind::Format),
            ("color", CellInformationKind::Color),
            ("parentheses", CellInformationKind::Parentheses),
            ("filename", CellInformationKind::Filename),
            ("protect", CellInformationKind::Protect),
            ("width", CellInformationKind::Width),
        ]
        .into_iter()
        .find_map(|(name, kind)| request.eq_ignore_ascii_case(name).then_some(kind))
        .ok_or(CellError::Value)?;
        address.validate().map_err(|_| CellError::Ref)?;
        if self.host.session.sources.workbook.sheet(sheet).is_none() {
            return Err(CellError::Ref.into());
        }
        self.host.charge_work(1)?;
        // Record metadata separately so style or source edits invalidate this result
        // even when the referenced cell keeps the same calculated value.
        ReadRecorder::new(self.host.session).read(DependencyRead::CellMetadata(EvaluationTarget {
            sheet,
            address,
        }));
        if request == CellInformationKind::Row {
            return Ok(CellValue::Number(f64::from(address.row) + 1.));
        }
        if request == CellInformationKind::Col {
            return Ok(CellValue::Number(f64::from(address.col) + 1.));
        }
        if request == CellInformationKind::Address {
            self.host.check_text(16)?;
            // SAFETY: validate above bounds both zero-based coordinates below u32::MAX.
            let text = poirot_functions::lookup::address_text(
                address.row + 1,
                address.col + 1,
                poirot_functions::lookup::AddressAxes::Both,
                poirot_functions::lookup::AddressStyle::A1,
                None,
            )
            .map_err(|error| match error {
                poirot_functions::lookup::LookupError::Spreadsheet(_) => {
                    crate::eval::values::FunctionError::Spreadsheet(CellError::Value)
                }
                error @ poirot_functions::lookup::LookupError::Operational(source) => {
                    let kind = match source {
                        poirot_functions::lookup::LookupOperationalError::ResourceOverflow => {
                            crate::eval::limits::ResourceFailureKind::SizeOverflow
                        }
                        poirot_functions::lookup::LookupOperationalError::BudgetExceeded
                        | poirot_functions::lookup::LookupOperationalError::Cancelled
                        | poirot_functions::lookup::LookupOperationalError::HostUnavailable => {
                            crate::eval::limits::ResourceFailureKind::BackendGuard
                        }
                    };
                    self.host
                        .resource_failure(
                            kind,
                            crate::eval::limits::EvaluationReasonError::Lookup(
                                std::sync::Arc::new(error),
                            ),
                        )
                        .into()
                }
                error @ poirot_functions::lookup::LookupError::Unsupported(_) => {
                    self.host.unsupported(
                        CapabilityId::FunctionAdapter,
                        crate::eval::limits::EvaluationReasonError::Lookup(std::sync::Arc::new(
                            error,
                        )),
                    )
                }
            })?;
            return Ok(CellValue::Text(text));
        }
        if request == CellInformationKind::Contents
            || request == CellInformationKind::Type
            || request == CellInformationKind::Prefix
        {
            let value = DynamicReferences::new(self.host).sheet_cell(sheet, address)?;
            if request == CellInformationKind::Contents {
                return Ok(if value == CellValue::Blank {
                    CellValue::Number(0.)
                } else {
                    value
                });
            }
            if request == CellInformationKind::Type {
                return self.text(match value {
                    CellValue::Blank => "b",
                    CellValue::Text(_) | CellValue::TextUtf16(_) => "l",
                    _ => "v",
                });
            }
            if !matches!(value, CellValue::Text(_) | CellValue::TextUtf16(_)) {
                return self.text("");
            }
            let alignment = EffectiveWorkbook::new(
                self.host.session.sources.workbook,
                self.host.session.sources.inputs,
            )
            .cell(sheet, address)
            .and_then(|cell| cell.style.visual.as_ref())
            .and_then(|visual| visual.alignment.as_ref())
            .and_then(|alignment| alignment.horizontal)
            .unwrap_or(HorizontalAlignment::General);
            return self.text(match alignment {
                HorizontalAlignment::General | HorizontalAlignment::Left => "'",
                HorizontalAlignment::Right => "\"",
                HorizontalAlignment::Center => "^",
                HorizontalAlignment::Fill => "\\",
                _ => "",
            });
        }
        if request == CellInformationKind::Format
            || request == CellInformationKind::Color
            || request == CellInformationKind::Parentheses
        {
            let format = EffectiveWorkbook::new(
                self.host.session.sources.workbook,
                self.host.session.sources.inputs,
            )
            .cell(sheet, address)
            .and_then(|cell| cell.style.number_format.as_deref())
            .unwrap_or("General");
            let code = match format {
                "General" | "# ?/?" | "# ??/??" => "G",
                "0" => "F0",
                "0.00" => "F2",
                "#,##0" => ",0",
                "#,##0.00" => ",2",
                "$#,##0_);($#,##0)" => "C0",
                "$#,##0_);[Red]($#,##0)" => "C0-",
                "$#,##0.00_);($#,##0.00)" => "C2",
                "$#,##0.00_);[Red]($#,##0.00)" => "C2-",
                "0%" => "P0",
                "0.00%" => "P2",
                "0.00E+00" => "S2",
                "m/d/yy" | "m/d/yy h:mm" | "mm/dd/yy" => "D4",
                "d-mmm-yy" | "dd-mmm-yy" => "D1",
                "d-mmm" | "dd-mmm" => "D2",
                "mmm-yy" => "D3",
                "mm/dd" => "D5",
                "h:mm AM/PM" => "D7",
                "h:mm:ss AM/PM" => "D6",
                "h:mm" => "D9",
                "h:mm:ss" => "D8",
                _ => {
                    return Err(self.host.unsupported(
                        CapabilityId::FunctionAdapter,
                        crate::eval::limits::EvaluationReasonError::CellCustomNumberFormat,
                    ));
                }
            };
            if request == CellInformationKind::Format {
                return self.text(code);
            }
            return Ok(CellValue::Number(
                if request == CellInformationKind::Color && code.ends_with('-') {
                    1.
                } else {
                    0.
                },
            ));
        }
        if matches!(
            request,
            CellInformationKind::Filename
                | CellInformationKind::Protect
                | CellInformationKind::Width
        ) {
            return Err(self.host.unsupported(
                CapabilityId::FunctionAdapter,
                crate::eval::limits::EvaluationReasonError::CellUnmodeledMetadata,
            ));
        }
        Err(CellError::Value.into())
    }

    /// Allocate a small result string after admission against the text budget.
    fn text(&mut self, text: &str) -> FunctionResult<CellValue> {
        self.host.check_text(text.len())?;
        let mut result = String::new();
        result
            .try_reserve_exact(text.len())
            .map_err(|source| self.host.reservation_failure(source))?;
        // SAFETY: reserve_exact admitted the complete input length above.
        result.push_str(text);
        Ok(CellValue::Text(result))
    }
}
