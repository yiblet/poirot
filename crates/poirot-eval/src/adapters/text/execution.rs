//! Family-owned evaluation; generic argument coercion remains in the execution core.
use crate::*;
#[path = "execution/operator_units.rs"]
mod operator_units;
impl Session<'_> {
    /// Dispatch text builtins under the selected text and compatibility policies.
    pub(crate) fn text_function(
        &mut self,
        s: usize,
        at: CellAddress,
        name: TextFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        // CODE's documented numeric contract is independent of text indexing policy.
        if name == TextFunction::Code
            && self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
        {
            return adapters::text::execute_units(self, s, at, name, args, false);
        }
        if matches!(
            name,
            TextFunction::Len
                | TextFunction::Left
                | TextFunction::Right
                | TextFunction::Mid
                | TextFunction::Replace
                | TextFunction::Concat
                | TextFunction::Concatenate
                | TextFunction::Find
                | TextFunction::Search
                | TextFunction::Lower
                | TextFunction::Upper
                | TextFunction::Rept
                | TextFunction::Code
                | TextFunction::Proper
        ) {
            self.read(DependencyRead::TextPolicy);
            {
                if self.text_policy.source == TextSourceVersion::ExcelCompatibilityV2Unverified {
                    return Err(EvaluationFailure::UnsupportedCapability {
                        capability: CapabilityId::FunctionAdapter,
                        source: None,
                        detail: "unverified Excel text policy".into(),
                    });
                }
                let full_case = matches!(
                    self.text_policy.source,
                    TextSourceVersion::Poi7321209OpenJdk21Unicode15
                        | TextSourceVersion::ExcelCompatibilityV2Documented
                );
                return adapters::text::execute_units(self, s, at, name, args, full_case);
            }
        }

        if let Some(function) = name.helper() {
            return adapters::text::execute(self, s, at, function, args);
        }
        let n = args.len();
        if name == TextFunction::Text {
            self.read(DependencyRead::TextPolicy);
            let source = poirot_format::FormatSourcePolicy::Poi7321209DataFormatter;
            if n != 2 {
                return Ok(err(VALUE_ERROR));
            }
            let value = self.argument(s, at, args, 0)?;
            let pattern = self.argument(s, at, args, 1)?;
            if let Error(e) = value {
                return Ok(err(e));
            }
            let pattern = match semantic(coercev2_formula_text(&pattern, &mut self.budget))? {
                Ok(t) => t,
                Err(e) => return Ok(err(e)),
            };
            let (value, format_value) = self.coercev2_text_format_value(value)?;
            if !format_value {
                return Ok(value.into());
            }
            self.budget.text(pattern.len())?;
            self.budget.allocation(pattern.chars().count())?;
            let format = match poirot_format::compile_format_with_source(
                &pattern,
                poirot_format::FormatCompilePolicy::default(),
                source,
            ) {
                Ok(format) => format,
                Err(_) => return Ok(err(VALUE_ERROR)),
            };
            let context = poirot_format::FormatRenderContext {
                date_system: self.workbook.date_system,
                width: None,
                ..poirot_format::FormatRenderContext::default()
            };
            if let Some(text) = value.text_ref() {
                self.budget
                    .charge(WorkKind::TextByte, text.storage_bytes() as u64)?;
            }
            let reservation = match format.render_byte_bound(&value, context) {
                Ok(bytes) => bytes,
                Err(error) => return function_value(Err(core_projection::format_failure(error))),
            };
            // Conservative maximum buffer reservation, checked before formatter growth.
            // It is deliberately not an eventual-length or total-allocation claim.
            self.budget.text(reservation)?;
            return Ok(
                match poirot_format::render_value(&value, &format, context) {
                    Ok(display) => Text(display.text).into(),
                    Err(error) => {
                        return function_value(Err(core_projection::format_failure(error)));
                    }
                },
            );
        }
        match self.eager_arguments(s, at, args)? {
            Err(error) => Ok(err(error)),
            Ok(values) => {
                let result = self.text_scalar(name, &values);
                function_value(result)
            }
        }
    }

    fn text_scalar(&mut self, name: TextFunction, a: &[CellValue]) -> FunctionResult<CellValue> {
        let n = a.len();
        if !FunctionId::Text(name).descriptor().arity.accepts(n) {
            return Err(VALUE_ERROR.into());
        }

        if matches!(name, TextFunction::Char | TextFunction::UniChar) {
            let code = self.coercev2_text_argument_number(a, 0)?;
            if code < 1.
                || code
                    > if name == TextFunction::Char {
                        255.
                    } else {
                        1114111.
                    }
            {
                return Err(VALUE_ERROR.into());
            }
            let character = char::from_u32(code as u32).ok_or(VALUE_ERROR)?;
            self.budget.text(character.len_utf8())?;
            return Ok(Text(character.to_string()));
        }
        if name == TextFunction::Value {
            let value = a.first().ok_or(VALUE_ERROR)?;
            let number =
                poirot_coercionv2::compatibility::coercev2_value_number(value.into(), |text| {
                    let text = checked_text_view(text, &mut self.budget)?;
                    match self.coercev2_formatted_text(&text)? {
                        poirot_coercionv2::compatibility::NumericTextDecision::Number(result) => {
                            result.map_err(FunctionFailure::from)
                        }
                        poirot_coercionv2::compatibility::NumericTextDecision::DateTime => {
                            self.coercev2_numeric_date_time(&text)
                        }
                    }
                })?;
            return Ok(Number(number));
        }

        let t = self.coercev2_text_argument(a, 0)?;
        self.budget.text(t.len())?;
        self.budget.allocation(t.chars().count())?;

        Ok(match name {
            TextFunction::NumberValue
            | TextFunction::Proper
            | TextFunction::Arabic
            | TextFunction::Roman
            | TextFunction::TextJoin => return Err(VALUE_ERROR.into()),
            TextFunction::Trim => Text(
                t.split(' ')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            TextFunction::Clean => Text(t.chars().filter(|c| (*c as u32) >= 32).collect()),
            TextFunction::Unicode => Number(t.chars().next().ok_or(VALUE_ERROR)? as u32 as f64),
            TextFunction::Exact => Boolean(t == self.coercev2_text_argument(a, 1)?),
            TextFunction::Substitute => {
                let old = self.coercev2_text_argument(a, 1)?;
                let new = self.coercev2_text_argument(a, 2)?;
                if old.is_empty() {
                    Text(t)
                } else {
                    let nth = if n == 3 {
                        None
                    } else {
                        let nth = self.coercev2_text_argument_number(a, 3)?;
                        if nth < 1. {
                            return Err(VALUE_ERROR.into());
                        }
                        Some(nth as usize)
                    };
                    self.budget.text(t.len())?;
                    let matches = t.match_indices(&old).count();
                    let replaced = nth.map_or(matches, |nth| usize::from(nth <= matches));
                    let bytes = t
                        .len()
                        .saturating_sub(replaced.saturating_mul(old.len()))
                        .saturating_add(replaced.saturating_mul(new.len()));
                    self.budget.text(bytes)?;
                    if nth.is_none() {
                        Text(t.replace(&old, &new))
                    } else {
                        let mut result = String::with_capacity(bytes);
                        let mut occurrence = 0;
                        for part in t.split_inclusive(&old) {
                            if let Some(prefix) = part.strip_suffix(&old) {
                                occurrence += 1;
                                result.push_str(prefix);
                                result.push_str(if Some(occurrence) == nth { &new } else { &old });
                            } else {
                                result.push_str(part);
                            }
                        }
                        Text(result)
                    }
                }
            }
            TextFunction::Text
            | TextFunction::Char
            | TextFunction::UniChar
            | TextFunction::Value
            | TextFunction::Concat
            | TextFunction::Concatenate
            | TextFunction::Len
            | TextFunction::Left
            | TextFunction::Right
            | TextFunction::Mid
            | TextFunction::Replace
            | TextFunction::Find
            | TextFunction::Search
            | TextFunction::Lower
            | TextFunction::Upper
            | TextFunction::Rept
            | TextFunction::Code => return Err(VALUE_ERROR.into()),
        })
    }
}

fn text_unsupported(reason: poirot_text::Unsupported) -> FunctionFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        source: None,
        detail: format!("text semantics: {reason:?}"),
    }
    .into()
}

impl Session<'_> {
    fn coercev2_text_argument(
        &mut self,
        arguments: &[CellValue],
        index: usize,
    ) -> FunctionResult<String> {
        coercev2_formula_text(arguments.get(index).ok_or(VALUE_ERROR)?, &mut self.budget)
    }
    fn coercev2_text_argument_number(
        &mut self,
        arguments: &[CellValue],
        index: usize,
    ) -> FunctionResult<f64> {
        use poirot_coercionv2::compatibility::{
            NumericTextDecision, TextIntegerPolicy, coercev2_text_integer_number,
        };
        let value = arguments.get(index).ok_or(VALUE_ERROR)?;
        self.read(DependencyRead::TextPolicy);
        let policy = if matches!(
            self.text_policy.source,
            TextSourceVersion::Poi7321209 | TextSourceVersion::Poi7321209OpenJdk21Unicode15
        ) {
            TextIntegerPolicy::PinnedPoi
        } else {
            TextIntegerPolicy::Checked
        };
        let number = if let Some(text) = value.text_ref()
            && matches!(policy, TextIntegerPolicy::PinnedPoi)
        {
            let projected = checked_text_view(text, &mut self.budget)?;
            match self.coercev2_formatted_text(&projected)? {
                NumericTextDecision::Number(result) => result?,
                NumericTextDecision::DateTime => self.coercev2_numeric_date_time(&projected)?,
            }
        } else {
            self.coercev2_scalar_number(value)?
        };
        coercev2_text_integer_number(number, policy).map_err(Into::into)
    }

    // Reserve before coercion clones or number rendering allocate.
    fn coercev2_text_render(&mut self, value: &CellValue) -> FunctionResult<String> {
        let bound = match value {
            Text(value) => value.len(),
            Number(_) => poirot_numeric::number_text::ALLOCATION_BOUND,
            Boolean(_) => 5,
            _ => 0,
        };
        self.budget.text(bound)?;
        if let Number(value) = value {
            self.read(DependencyRead::TextPolicy);
            return Ok(poirot_coercionv2::compatibility::coercev2_render_number(
                *value,
            ));
        }
        coercev2_formula_text(value, &mut self.budget)
    }
    /// Execute concatenation with selected text-unit semantics and metered storage.
    pub(crate) fn text_concat_operator(
        &mut self,
        left: CellValue,
        right: CellValue,
    ) -> FunctionResult<CellValue> {
        if let Error(error) = left {
            return Ok(Error(error));
        }
        if let Error(error) = right {
            return Ok(Error(error));
        }
        self.read(DependencyRead::TextPolicy);
        let policy = self
            .text_policy
            .operation(poirot_text::Operation::ConcatOperator)
            .map_err(text_unsupported)?;
        if policy.units == poirot_text::Units::Utf16CodeUnits {
            return self.text_unit_operator(&left, &right);
        }
        let left = self.coercev2_text_render(&left)?;
        let right = self.coercev2_text_render(&right)?;
        let bytes =
            poirot_text::checked_total(left.len(), right.len()).map_err(text_unsupported)?;
        self.budget.charge(WorkKind::TextByte, bytes as u64)?;
        let measured = poirot_text::measure_concat([left.as_str(), right.as_str()], policy)
            .map_err(text_unsupported)?;
        if !measured.within_limit {
            return Err(VALUE_ERROR.into());
        }
        self.budget.text(measured.bytes)?;
        let mut output = String::with_capacity(measured.bytes);
        output.push_str(&left);
        output.push_str(&right);
        Ok(Text(output))
    }
}

impl Session<'_> {
    /// TEXT requests its own text admission policy; contextual parsing reuses the
    /// same metered locale/calendar grammar as other numeric argument consumers.
    fn coercev2_text_format_value(&mut self, value: CellValue) -> EvalResult<(CellValue, bool)> {
        use poirot_coercionv2::compatibility::{NumericTextDecision, TextFormatPolicy};
        self.read(DependencyRead::BuiltinPolicy);
        let policy = if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
            TextFormatPolicy::Contextual
        } else {
            TextFormatPolicy::PinnedPoi
        };
        poirot_coercionv2::compatibility::coercev2_text_format_value(value, policy, |view| {
            self.budget
                .charge(WorkKind::TextByte, view.len_units() as u64)?;
            if view.utf8_len().is_err() {
                return Ok(None);
            }
            let projected = match checked_text_view(view, &mut self.budget) {
                Ok(text) => text,
                Err(FunctionFailure::Operational(error)) => return Err(error),
                Err(FunctionFailure::Spreadsheet(_)) => return Ok(None),
            };
            if policy == TextFormatPolicy::PinnedPoi {
                return Ok(
                    poirot_coercionv2::compatibility::coercev2_arithmetic_text(&projected).ok(),
                );
            }
            let number = match self.coercev2_formatted_text(&projected) {
                Ok(NumericTextDecision::Number(result)) => result.map_err(FunctionFailure::from),
                Ok(NumericTextDecision::DateTime) => self.coercev2_numeric_date_time(&projected),
                Err(error) => Err(error),
            };
            match number {
                Ok(number) => Ok(Some(number)),
                Err(FunctionFailure::Spreadsheet(_)) => Ok(None),
                Err(FunctionFailure::Operational(error)) => Err(error),
            }
        })
    }
}

#[cfg(test)]
#[path = "execution/semantics_tests.rs"]
mod semantics_tests;
