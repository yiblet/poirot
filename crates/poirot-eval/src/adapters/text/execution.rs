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
            if self.text_policy.source != TextSourceVersion::PoirotF6c572d
                && !(matches!(name, TextFunction::Code | TextFunction::Replace)
                    && self.text_policy.source == TextSourceVersion::ExcelCompatibilityV2Unverified)
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
        if name == TextFunction::Concat {
            return self.text_concat_members(s, at, args);
        }
        if let Some(function) = name.helper() {
            return adapters::text::execute(self, s, at, function, args);
        }
        let n = args.len();
        if name == TextFunction::Text {
            self.read(DependencyRead::TextPolicy);
            let source = match self.text_policy.source {
                TextSourceVersion::Poi7321209 | TextSourceVersion::Poi7321209OpenJdk21Unicode15 => {
                    poirot_format::FormatSourcePolicy::Poi7321209DataFormatter
                }
                _ => poirot_format::FormatSourcePolicy::LegacyExcelStyle,
            };
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
                poirot_format::FormatCompilePolicy::legacy_compatible(),
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
    /// CONCAT consumes each member lazily; expression demand remains scalar, as
    /// with other legacy collection consumers. Array-valued expressions retain
    /// their shape, while reference expressions retain implicit intersection.
    fn text_concat_members(
        &mut self,
        sheet: usize,
        address: CellAddress,
        arguments: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        let policy = self.text_policy;
        let mut host = crate::host::CallHost::new(self, sheet, address, arguments);
        let result = (|| -> FunctionResult<CellValue> {
            let policy = policy
                .operation(poirot_text::Operation::Concatenate)
                .map_err(text_unsupported)?;
            let mut output = String::new();
            let mut units = 0usize;
            for index in 0..arguments.len() {
                let count = host.collection_len(index)?;
                for offset in 0..count {
                    host.charge_work(1)?;
                    let scalar = host.collection_cell(index, offset)?;
                    // The legacy formula-text consumer admits omission as blank;
                    // the host retains both states until this conversion boundary.
                    let part = host.coercev2_concat_member_text(&scalar)?;
                    host.charge_work(part.len() as u64)?;
                    units =
                        poirot_text::checked_total(units, poirot_text::length(&part, policy.units))
                            .map_err(text_unsupported)?;
                    if policy.limit.is_some_and(|limit| units > limit) {
                        return Err(VALUE_ERROR.into());
                    }
                    let bytes = poirot_text::checked_total(output.len(), part.len())
                        .map_err(text_unsupported)?;
                    host.check_text_capacity(bytes)?;
                    output.push_str(&part);
                }
            }
            if !FunctionId::Text(TextFunction::Concat)
                .descriptor()
                .arity
                .accepts(arguments.len())
            {
                return Err(VALUE_ERROR.into());
            }
            Ok(Text(output))
        })();
        function_value(result)
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
        if name == TextFunction::Concatenate {
            let policy = self
                .text_policy
                .operation(poirot_text::Operation::Concatenate)
                .map_err(text_unsupported)?;
            self.budget.allocation(a.len())?;
            let mut parts = Vec::with_capacity(a.len());
            for value in a {
                parts.push(self.coercev2_text_render(value)?);
            }
            let bytes = parts.iter().try_fold(0usize, |total, part| {
                poirot_text::checked_total(total, part.len()).map_err(text_unsupported)
            })?;
            self.budget.charge(WorkKind::TextByte, bytes as u64)?;
            let measured = poirot_text::measure_concat(parts.iter().map(String::as_str), policy)
                .map_err(text_unsupported)?;
            if !measured.within_limit {
                return Err(VALUE_ERROR.into());
            }
            self.budget.text(measured.bytes)?;
            let mut output = String::with_capacity(measured.bytes);
            for part in parts {
                output.push_str(&part);
            }
            return Ok(Text(output));
        }
        if matches!(
            name,
            TextFunction::Len | TextFunction::Left | TextFunction::Right | TextFunction::Mid
        ) {
            let operation = if name == TextFunction::Len {
                poirot_text::Operation::Len
            } else {
                poirot_text::Operation::Index
            };
            let units = self
                .text_policy
                .operation(operation)
                .map_err(text_unsupported)?
                .units;
            let value = self.coercev2_text_render(a.first().ok_or(VALUE_ERROR)?)?;
            self.budget
                .charge(WorkKind::TextByte, value.len().saturating_mul(3) as u64)?;
            if name == TextFunction::Len {
                return Ok(Number(poirot_text::length(&value, units) as f64));
            }
            let poi = self.text_policy.source == TextSourceVersion::Poi7321209;
            let coercev2_text_integer = |number: f64| -> FunctionResult<usize> {
                let policy = if poi {
                    poirot_coercionv2::compatibility::TextIntegerPolicy::PinnedPoi
                } else {
                    poirot_coercionv2::compatibility::TextIntegerPolicy::Checked
                };
                poirot_coercionv2::compatibility::coercev2_text_integer_number(number, policy)
                    .map(|number| number as usize)
                    .map_err(Into::into)
            };
            let selected = match name {
                TextFunction::Left | TextFunction::Right => {
                    let count = if n == 2 {
                        self.coercev2_text_argument_number(a, 1)?
                    } else {
                        1.
                    };
                    let count_int = coercev2_text_integer(count)?;
                    if count < 0. {
                        return Err(VALUE_ERROR.into());
                    }
                    if name == TextFunction::Left {
                        poirot_text::left(&value, count_int, units)
                    } else {
                        poirot_text::right(&value, count_int, units)
                    }
                }
                TextFunction::Mid => {
                    let start = self.coercev2_text_argument_number(a, 1)?;
                    let count = self.coercev2_text_argument_number(a, 2)?;
                    let start_int = coercev2_text_integer(start)?;
                    let count_int = coercev2_text_integer(count)?;
                    if start < 1. || count < 0. {
                        return Err(VALUE_ERROR.into());
                    }
                    if poi {
                        poirot_text::poi_mid(&value, start_int as i32, count_int as i32)
                    } else {
                        poirot_text::slice(&value, start_int.saturating_sub(1), count_int, units)
                    }
                }
                // SAFETY: This enclosing branch selects Len/Left/Right/Mid, and Len returns before this match, whose explicit arms cover Left/Right/Mid.
                _ => unreachable!(),
            }
            .map_err(text_unsupported)?;
            self.budget.text(selected.len())?;
            return Ok(Text(selected.to_owned()));
        }
        let t = self.coercev2_text_argument(a, 0)?;
        self.budget.text(t.len())?;
        self.budget.allocation(t.chars().count())?;
        if matches!(
            name,
            TextFunction::Lower | TextFunction::Upper | TextFunction::Search
        ) {
            self.read(DependencyRead::TextPolicy);
            if self.text_policy.source != TextSourceVersion::PoirotF6c572d {
                return Err(text_unsupported(
                    if self.text_policy.source == TextSourceVersion::ExcelCompatibilityV2Unverified
                    {
                        poirot_text::Unsupported::UnverifiedExcelVersion
                    } else {
                        poirot_text::Unsupported::JavaUnicodeCaseMapping
                    },
                ));
            }
        }
        let chars: Vec<_> = t.chars().collect();
        let len = chars.len();
        Ok(match name {
            TextFunction::NumberValue
            | TextFunction::Proper
            | TextFunction::Arabic
            | TextFunction::Roman
            | TextFunction::TextJoin => return Err(VALUE_ERROR.into()),
            TextFunction::Len => Number(len as f64),
            TextFunction::Lower => {
                self.budget.text(
                    t.chars()
                        .flat_map(char::to_lowercase)
                        .map(char::len_utf8)
                        .sum(),
                )?;
                Text(t.to_lowercase())
            }
            TextFunction::Upper => {
                self.budget.text(
                    t.chars()
                        .flat_map(char::to_uppercase)
                        .map(char::len_utf8)
                        .sum(),
                )?;
                Text(t.to_uppercase())
            }
            TextFunction::Trim => Text(
                t.split(' ')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            TextFunction::Clean => Text(t.chars().filter(|c| (*c as u32) >= 32).collect()),
            TextFunction::Code | TextFunction::Unicode => {
                let c = chars.first().ok_or(VALUE_ERROR)?;
                if name == TextFunction::Code && *c as u32 > 255 {
                    return Err(VALUE_ERROR.into());
                }
                let code = *c as u32;
                if name == TextFunction::Code {
                    self.read(DependencyRead::TextPolicy);
                    if self.text_policy.source == TextSourceVersion::Poi7321209 {
                        // CODE currently admits 0..=255; preflight exact decimal bytes.
                        let bytes = if code >= 100 {
                            3
                        } else if code >= 10 {
                            2
                        } else {
                            1
                        };
                        self.budget.text(bytes)?;
                        return Ok(Text(code.to_string()));
                    }
                }
                Number(code as f64)
            }
            TextFunction::Exact => Boolean(t == self.coercev2_text_argument(a, 1)?),
            TextFunction::Left | TextFunction::Right => {
                let count = if n == 2 {
                    self.coercev2_text_argument_number(a, 1)?
                } else {
                    1.
                };
                if count < 0. {
                    return Err(VALUE_ERROR.into());
                }
                let count = (count as usize).min(len);
                Text(if name == TextFunction::Left {
                    // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                    chars[..count].iter().collect()
                } else {
                    // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                    chars[len - count..].iter().collect()
                })
            }
            TextFunction::Mid => {
                let start = self.coercev2_text_argument_number(a, 1)?;
                let count = self.coercev2_text_argument_number(a, 2)?;
                if start < 1. || count < 0. {
                    return Err(VALUE_ERROR.into());
                }
                Text(
                    chars
                        .iter()
                        .skip(start as usize - 1)
                        .take(count as usize)
                        .collect(),
                )
            }
            TextFunction::Rept => {
                let count = self.coercev2_text_argument_number(a, 1)?;
                if count < 0. || len as f64 * count.trunc() > 32767. {
                    return Err(VALUE_ERROR.into());
                }
                let repeats = (count as usize).min(32767);
                let bytes = t
                    .len()
                    .checked_mul(repeats)
                    .filter(|n| *n <= isize::MAX as usize)
                    .ok_or(VALUE_ERROR)?;
                self.budget.text(bytes)?;
                // SAFETY: The checked product fits the maximum string capacity and t is unchanged since that check.
                Text(t.repeat(repeats))
            }
            TextFunction::Replace => {
                let start = self.coercev2_text_argument_number(a, 1)?;
                let count = self.coercev2_text_argument_number(a, 2)?;
                if start < 1. || count < 0. {
                    return Err(VALUE_ERROR.into());
                }
                let start = (start as usize - 1).min(len);
                let new = self.coercev2_text_argument(a, 3)?;
                let tail = (start.saturating_add(count as usize)).min(len);
                // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                let bytes = chars[..start]
                    .iter()
                    // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                    .chain(&chars[tail..])
                    .map(|c| c.len_utf8())
                    .sum::<usize>()
                    .saturating_add(new.len());
                self.budget.text(bytes)?;
                let mut result = String::with_capacity(bytes);
                // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                result.extend(&chars[..start]);
                result.push_str(&new);
                // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                result.extend(&chars[tail..]);
                Text(result)
            }
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
            TextFunction::Find | TextFunction::Search => {
                let hay = self.coercev2_text_argument(a, 1)?;
                let start = if n == 3 {
                    self.coercev2_text_argument_number(a, 2)?
                } else {
                    1.
                };
                self.budget.text(hay.len())?;
                self.budget.allocation(hay.chars().count())?;
                let h: Vec<_> = hay.chars().collect();
                self.read(DependencyRead::BuiltinPolicy);
                let offset = if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
                    poirot_coercionv2::compatibility::coercev2_text_search_start(start, h.len())?
                } else {
                    if start < 1. || start > h.len() as f64 + 1. {
                        return Err(VALUE_ERROR.into());
                    }
                    (start as usize).checked_sub(1).ok_or(VALUE_ERROR)?
                };
                let mut found = None;
                for i in offset..=h.len() {
                    self.budget
                        // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                        .text(h[i..].iter().map(|character| character.len_utf8()).sum())?;
                    // SAFETY: The local count/start/tail values are clamped to chars.len(); suffix scan i is bounded by h.len(), so the requested character slices are in bounds.
                    let tail: String = h[i..].iter().collect();
                    let matches = if name == TextFunction::Find {
                        tail.starts_with(&t)
                    } else {
                        wildcard_prefix(&t, &tail, &mut self.budget)?
                    };
                    if matches {
                        found = Some(i + 1);
                        break;
                    }
                }
                Number(found.ok_or(VALUE_ERROR)? as f64)
            }
            TextFunction::Text
            | TextFunction::Char
            | TextFunction::UniChar
            | TextFunction::Value
            | TextFunction::Concat
            | TextFunction::Concatenate => return Err(VALUE_ERROR.into()),
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
            if matches!(
                self.text_policy.source,
                TextSourceVersion::Poi7321209 | TextSourceVersion::Poi7321209OpenJdk21Unicode15
            ) {
                return Ok(poirot_coercionv2::compatibility::coercev2_render_number(
                    *value,
                    poirot_numeric::number_text::SourceVersion::Poi7321209,
                ));
            }
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
