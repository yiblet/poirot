//! Deferred text call policy owned by the evaluator adapter.
/// Pure text helpers and their source-compatible value contracts.
pub use poirot_functions_text::*;

/// Compatibility rules for deferred text operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticsPolicy {
    /// Pinned Apache POI behavior.
    PinnedPoi,
    /// Documented Excel behavior.
    ExcelDocumentation,
}

/// Deferred text operations executed through this host adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    /// Parse text using explicit decimal and grouping separators.
    NumberValue,
    /// Capitalize words using the selected text semantics.
    Proper,
    /// Render Roman numeral text.
    Roman,
    /// Decode Roman numeral text.
    Arabic,
    /// Join selected text members with delimiters.
    TextJoin,
}
#[derive(Clone, Debug, PartialEq)]
/// A selected text-family operation and its deferred host argument handles.
pub struct Call<E> {
    /// Selected text operation.
    pub function: Function,
    /// Deferred host operand handles in source order.
    pub arguments: poirot_functions_core::ParamVec<E>,
}
/// Text failures retaining spreadsheet errors, operational aborts, and source faults.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure<X, O = ()> {
    /// Catchable spreadsheet error.
    Spreadsheet(X),
    /// Host operational abort.
    Operational(O),
    /// Required output growth bound is unavailable.
    Unsupported(Unsupported),
    /// Exceptional pinned-source coercion behavior.
    SourceException(HostFault),
}
impl<X, O> From<X> for Failure<X, O> {
    fn from(error: X) -> Self {
        Self::Spreadsheet(error)
    }
}
/// Typed adapter result retaining spreadsheet and operational failure domains.
pub type TextResult<T, X, O = ()> = Result<T, Failure<X, O>>;
/// Reasons text output capacity cannot be safely admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// Host cannot establish an output size bound.
    UnknownOutputGrowth,
    /// Output size arithmetic overflowed.
    SizeOverflow,
}
/// Numeric or text result from a deferred text operation.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Parsed numeric value.
    Number(f64),
    /// Constructed text.
    Text(String),
}
/// Rows selected when traversing a text operand area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaSelection {
    /// Traverse every row in source order.
    AllRows,
    /// Traverse only the last row, as required for delimiter areas.
    LastRow,
}
#[derive(Clone, Debug)]
/// Explicit numeric locale plus pinned NUMBERVALUE UTF-16 separator defaults.
/// Documented semantics use the locale; pinned defaults remain `.` and `,`.
pub struct Policy<'a> {
    /// Numeric locale for documented argument parsing.
    pub number_locale: &'a poirot_parsing::number::NumberLocaleV2,
    /// Default decimal separator as one UTF-16 unit.
    pub decimal_separator: u16,
    /// Default grouping separator as one UTF-16 unit.
    pub grouping_separator: u16,
}
impl Default for Policy<'_> {
    fn default() -> Self {
        Self {
            number_locale: &poirot_parsing::number::NumberLocaleV2::EN_US,
            decimal_separator: b'.' as u16,
            grouping_separator: b',' as u16,
        }
    }
}

/// `single` implements OperandResolver.getSingleValue, including source-cell implicit
/// intersection and error propagation. `values` uses row-major AreaEval traversal;
/// scalar/reference inputs use `single`. Delimiter areas select their last row.
/// Strings use POI NumberToTextConverter; missing text items are a host source fault
/// (POI coerceValueToString rejects MissingArgEval), while delimiter missing is empty.
pub trait Host<E> {
    /// Engine scalar channel preserving omitted-argument identity.
    type Scalar;
    /// Engine spreadsheet error type.
    type Error;
    /// Host operational failure type.
    type Operational;
    /// Check text capacity before allocation or copying.
    fn check_text(&mut self, bytes: usize) -> Result<(), Self::Operational>;
    /// Check collection capacity before materializing members.
    fn check_collection(&mut self, elements: usize) -> Result<(), Self::Operational>;
    /// Admit helper work before performing it.
    fn charge_work(&mut self, units: usize) -> Result<(), Self::Operational>;
    /// Map a text helper error into the engine's spreadsheet error type.
    fn error(&self, error: Error) -> Self::Error;
    /// Resolve the caller-selected scalar without traversing the whole operand.
    fn single(
        &mut self,
        expression: &E,
    ) -> TextResult<Self::Scalar, Self::Error, Self::Operational>;
    /// Inspect omitted arguments without applying scalar conversion.
    fn is_missing(&self, _scalar: &Self::Scalar) -> bool {
        false
    }
    /// Record consumption of the configured numeric locale.
    fn read_number_locale(&mut self) {}
    /// Bound selected member count without materializing its values.
    fn values_bound(&self, expression: &E, selection: AreaSelection) -> Option<usize>;
    /// Bound text conversion storage before constructing the string.
    fn string_bound(&self, scalar: &Self::Scalar) -> Option<usize>;
    /// Materialize only selected area rows in source traversal order.
    fn values(
        &mut self,
        expression: &E,
        selection: AreaSelection,
    ) -> TextResult<Vec<Self::Scalar>, Self::Error, Self::Operational>;
    /// Convert a text item using source-compatible scalar formatting.
    fn coercev2_text_string(
        &self,
        scalar: &Self::Scalar,
    ) -> TextResult<String, Self::Error, Self::Operational>;
    /// Convert a delimiter, treating omitted delimiters according to source rules.
    fn coercev2_text_delimiter(
        &self,
        scalar: &Self::Scalar,
    ) -> TextResult<String, Self::Error, Self::Operational>;
    /// Coerce a scalar to the source's integer argument representation.
    fn coercev2_text_integer(
        &self,
        scalar: &Self::Scalar,
    ) -> TextResult<i32, Self::Error, Self::Operational>;
    /// Decode a scalar through the central ARABIC argument policy.
    fn coercev2_arabic_integer(
        &self,
        scalar: &Self::Scalar,
    ) -> TextResult<i32, Self::Error, Self::Operational>;
    /// Inspect the source scalar without numeric or text coercion.
    fn boolean_value(&self, _scalar: &Self::Scalar) -> Option<bool> {
        None
    }
    /// Apply logical coercion for text operation options.
    fn coercev2_text_boolean(
        &self,
        scalar: &Self::Scalar,
    ) -> TextResult<bool, Self::Error, Self::Operational>;
    /// Locale.ROOT per-UTF16-unit case mapping and Character.isLetter category.
    /// Engine supplies a policy pinned to its Java Unicode baseline.
    fn proper_output_bound(&self, text: &str) -> Option<usize> {
        text.len().checked_mul(3)
    }
    /// Apply the host's selected proper-case mapping after output admission.
    fn proper(&self, text: &str) -> String {
        proper(text)
    }
}

/// Execute a deferred text call under pinned compatibility rules.
pub fn evaluate<E, H: Host<E>>(
    call: &Call<E>,
    policy: &Policy,
    host: &mut H,
) -> TextResult<Value, H::Error, H::Operational> {
    evaluate_with_policy(call, policy, host, SemanticsPolicy::PinnedPoi)
}
/// Execute a deferred text call under explicit compatibility and separator policies.
pub fn evaluate_with_policy<E, H: Host<E>>(
    call: &Call<E>,
    policy: &Policy,
    h: &mut H,
    semantics: SemanticsPolicy,
) -> TextResult<Value, H::Error, H::Operational> {
    let a = &call.arguments;
    let fail = |e| Failure::Spreadsheet(e);
    match call.function {
        Function::Proper => {
            if a.len() != 1 {
                return Err(fail(h.error(Error::Value)));
            }
            // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
            let v = h.single(&a[0])?;
            let text = checked_string(h, &v, false)?;
            let bound = h
                .proper_output_bound(&text)
                .ok_or(Failure::Unsupported(Unsupported::UnknownOutputGrowth))?;
            h.check_text(bound).map_err(Failure::Operational)?;
            h.charge_work(text.len()).map_err(Failure::Operational)?;
            Ok(Value::Text(h.proper(&text)))
        }
        Function::Arabic => {
            if a.len() != 1 {
                return Err(fail(h.error(Error::Value)));
            }
            let value = h.single(a.first().ok_or_else(|| fail(h.error(Error::Value)))?)?;
            let bound = h
                .string_bound(&value)
                .ok_or(Failure::Unsupported(Unsupported::UnknownOutputGrowth))?;
            h.check_text(bound).map_err(Failure::Operational)?;
            h.charge_work(bound).map_err(Failure::Operational)?;
            Ok(Value::Number(f64::from(h.coercev2_arabic_integer(&value)?)))
        }
        Function::Roman => {
            let documented = semantics == SemanticsPolicy::ExcelDocumentation;
            if !poirot_coercionv2::roman::coercev2_roman_arity(a.len(), documented) {
                return Err(fail(h.error(Error::Value)));
            }
            let number = h
                .single(a.first().ok_or_else(|| fail(h.error(Error::Value)))?)
                .and_then(|v| h.coercev2_text_integer(&v))
                .map_err(|e| match e {
                    Failure::Spreadsheet(_) => fail(h.error(Error::Value)),
                    other => other,
                })?;
            if !(0..=3999).contains(&number) {
                return Err(fail(h.error(Error::Value)));
            }
            if number == 0 {
                return Ok(Value::Text(String::new()));
            }
            let form = a
                .get(1)
                .map(|argument| {
                    h.single(argument).and_then(|v| {
                        if let Some(form) = poirot_coercionv2::roman::coercev2_roman_form_default(
                            h.is_missing(&v),
                            h.boolean_value(&v),
                            documented,
                        ) {
                            Ok(form)
                        } else {
                            h.coercev2_text_integer(&v)
                        }
                    })
                })
                .unwrap_or_else(|| {
                    poirot_coercionv2::roman::coercev2_roman_form_default(true, None, documented)
                        .ok_or_else(|| fail(h.error(Error::Value)))
                })
                .map_err(|e| match e {
                    Failure::Spreadsheet(_) => fail(h.error(Error::Num)),
                    other => other,
                })?;
            if !(0..=4).contains(&form) {
                return Err(fail(h.error(Error::Value)));
            }
            h.check_text(16).map_err(Failure::Operational)?;
            h.charge_work(512).map_err(Failure::Operational)?;
            Ok(Value::Text(roman(number as u16, form as u8)))
        }
        Function::NumberValue => {
            // POI leaves text null for other counts and then parses "0".
            if !(1..=3).contains(&a.len()) {
                return Ok(Value::Number(0.));
            }
            // Resolve every argument before coercing strings, as the source does.
            h.check_collection(a.len()).map_err(Failure::Operational)?;
            let mut scalars = Vec::new();
            for arg in a {
                scalars.push(h.single(arg)?);
            }
            // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
            let text = checked_string(h, &scalars[0], false)?;
            if semantics == SemanticsPolicy::ExcelDocumentation {
                let decimal = scalars
                    .get(1)
                    .filter(|s| !h.is_missing(s))
                    .map(|s| checked_string(h, s, false))
                    .transpose()?;
                let grouping = scalars
                    .get(2)
                    .filter(|s| !h.is_missing(s))
                    .map(|s| checked_string(h, s, false))
                    .transpose()?;
                if decimal.is_none() || grouping.is_none() {
                    h.read_number_locale();
                }
                let bound = text
                    .len()
                    .checked_add(1)
                    .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?;
                h.check_text(bound).map_err(Failure::Operational)?;
                h.check_collection(
                    bound
                        .checked_mul(3)
                        .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?,
                )
                .map_err(Failure::Operational)?;
                h.charge_work(
                    bound
                        .checked_mul(8)
                        .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?,
                )
                .map_err(Failure::Operational)?;
                return match numeric::coercev2_number_value_excel(
                    &text,
                    policy.number_locale,
                    decimal.as_deref(),
                    grouping.as_deref(),
                ) {
                    Ok(number) => Ok(Value::Number(number)),
                    Err(numeric::ParseFailure::Spreadsheet(e)) => Err(fail(h.error(e))),
                    Err(numeric::ParseFailure::Source(e)) => Err(Failure::SourceException(e)),
                };
            }
            let mut separator =
                |index: usize, default: u16| -> TextResult<u16, H::Error, H::Operational> {
                    if index >= scalars.len() {
                        return Ok(default);
                    }
                    // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
                    checked_string(h, &scalars[index], false)?
                        .encode_utf16()
                        .next()
                        .ok_or(Failure::SourceException(HostFault::EmptySeparator))
                };
            let dec = separator(1, policy.decimal_separator)?;
            let group = separator(2, policy.grouping_separator)?;
            let bound = text
                .len()
                .checked_add(1)
                .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?;
            h.check_text(bound).map_err(Failure::Operational)?;
            h.check_collection(
                bound
                    .checked_mul(3)
                    .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?,
            )
            .map_err(Failure::Operational)?;
            h.charge_work(
                bound
                    .checked_mul(8)
                    .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?,
            )
            .map_err(Failure::Operational)?;
            match numeric::coercev2_number_value(&text, dec, group) {
                Ok(n) => Ok(Value::Number(n)),
                Err(numeric::ParseFailure::Spreadsheet(e)) => Err(fail(h.error(e))),
                Err(numeric::ParseFailure::Source(e)) => Err(Failure::SourceException(e)),
            }
        }
        Function::TextJoin => {
            if !(3..=254).contains(&a.len()) {
                return Err(fail(h.error(Error::Value)));
            }
            // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
            let delimiters = checked_values(h, &a[0], AreaSelection::LastRow)?;
            // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
            let ignore = h.single(&a[1])?;
            let ignore = h.coercev2_text_boolean(&ignore)?;
            let mut texts = Vec::new();
            let mut text_bytes = 0usize;
            // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
            for arg in &a[2..] {
                for v in checked_values(h, arg, AreaSelection::AllRows)? {
                    let text = checked_string(h, &v, false)?;
                    if !ignore || !text.is_empty() {
                        text_bytes = text_bytes
                            .checked_add(text.len())
                            .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?;
                        h.check_text(text_bytes).map_err(Failure::Operational)?;
                        h.check_collection(
                            texts
                                .len()
                                .checked_add(1)
                                .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?,
                        )
                        .map_err(Failure::Operational)?;
                        texts.push(text);
                    }
                }
            }
            // Source delays delimiter string coercion until all text is collected.
            h.check_collection(delimiters.len())
                .map_err(Failure::Operational)?;
            let mut rendered = Vec::with_capacity(delimiters.len());
            for v in &delimiters {
                rendered.push(checked_string(h, v, true)?);
            }
            let delimiters = rendered;
            let mut bytes = 0usize;
            for (i, text) in texts.iter().enumerate() {
                bytes = bytes
                    .checked_add(text.len())
                    .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?;
                if i > 0 && !delimiters.is_empty() {
                    bytes = bytes
                        // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
                        .checked_add(delimiters[(i - 1) % delimiters.len()].len())
                        .ok_or(Failure::Unsupported(Unsupported::SizeOverflow))?;
                }
            }
            h.check_text(bytes).map_err(Failure::Operational)?;
            h.charge_work(bytes).map_err(Failure::Operational)?;
            let mut output = String::with_capacity(bytes);
            for (i, text) in texts.iter().enumerate() {
                if i > 0 && !delimiters.is_empty() {
                    // SAFETY: Each argument branch checks its count locally; scalars is filled once per argument, separator checks index < scalars.len(), and delimiter indexing is guarded by nonempty and uses modulo its length.
                    output.push_str(&delimiters[(i - 1) % delimiters.len()]);
                }
                output.push_str(text);
            }
            Ok(Value::Text(output))
        }
    }
}

fn checked_string<E, H: Host<E>>(
    h: &mut H,
    v: &H::Scalar,
    delimiter: bool,
) -> TextResult<String, H::Error, H::Operational> {
    let bytes = h
        .string_bound(v)
        .ok_or(Failure::Unsupported(Unsupported::UnknownOutputGrowth))?;
    h.check_text(bytes).map_err(Failure::Operational)?;
    h.charge_work(bytes).map_err(Failure::Operational)?;
    if delimiter {
        h.coercev2_text_delimiter(v)
    } else {
        h.coercev2_text_string(v)
    }
}
fn checked_values<E, H: Host<E>>(
    h: &mut H,
    e: &E,
    s: AreaSelection,
) -> TextResult<Vec<H::Scalar>, H::Error, H::Operational> {
    let count = h
        .values_bound(e, s)
        .ok_or(Failure::Unsupported(Unsupported::UnknownOutputGrowth))?;
    h.check_collection(count).map_err(Failure::Operational)?;
    h.charge_work(count).map_err(Failure::Operational)?;
    h.values(e, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Test input channel for scalar, reference, and deferred text-policy cases.
    #[derive(Clone)]
    enum S {
        Text(&'static str),
        Int(i32),
        Bool(bool),
        Error,
        Operational,
        Unsupported,
        Area(Vec<Vec<S>>),
    }
    /// Test host counting forced text operands and exposing controlled failures.
    struct Adapter {
        seen: usize,
    }
    impl Host<S> for Adapter {
        /// Scalar projection exposed by this text host.
        type Scalar = S;
        /// Spreadsheet error identity exposed by this text host.
        type Error = Error;
        /// Host abort channel kept outside catchable spreadsheet values.
        type Operational = ();
        fn check_text(&mut self, _: usize) -> Result<(), ()> {
            Ok(())
        }
        fn check_collection(&mut self, _: usize) -> Result<(), ()> {
            Ok(())
        }
        fn charge_work(&mut self, _: usize) -> Result<(), ()> {
            Ok(())
        }
        fn error(&self, e: Error) -> Error {
            e
        }
        fn single(&mut self, e: &S) -> Result<S, Failure<Error>> {
            self.seen += 1;
            match e {
                S::Error => Err(Failure::Spreadsheet(Error::Value)),
                S::Operational => Err(Failure::Operational(())),
                S::Unsupported => Err(Failure::Unsupported(Unsupported::UnknownOutputGrowth)),
                _ => Ok(e.clone()),
            }
        }
        fn values_bound(&self, e: &S, s: AreaSelection) -> Option<usize> {
            Some(match e {
                S::Area(rows) => {
                    if s == AreaSelection::LastRow {
                        rows.last()?.len()
                    } else {
                        rows.iter().map(Vec::len).sum()
                    }
                }
                _ => 1,
            })
        }
        fn string_bound(&self, s: &S) -> Option<usize> {
            Some(match s {
                S::Text(t) => t.len(),
                S::Int(_) => 12,
                _ => 0,
            })
        }
        fn values(&mut self, e: &S, selection: AreaSelection) -> Result<Vec<S>, Failure<Error>> {
            match e {
                S::Area(rows) => {
                    let rows = if selection == AreaSelection::LastRow {
                        &rows[rows.len() - 1..]
                    } else {
                        rows.as_slice()
                    };
                    rows.iter().flatten().map(|v| self.single(v)).collect()
                }
                _ => Ok(vec![self.single(e)?]),
            }
        }
        fn coercev2_text_string(&self, s: &S) -> Result<String, Failure<Error>> {
            match s {
                S::Text(t) => Ok((*t).into()),
                S::Int(n) => Ok(n.to_string()),
                _ => Err(Failure::Spreadsheet(Error::Value)),
            }
        }
        fn coercev2_text_delimiter(&self, s: &S) -> Result<String, Failure<Error>> {
            self.coercev2_text_string(s)
        }
        fn coercev2_text_integer(&self, s: &S) -> Result<i32, Failure<Error>> {
            match s {
                S::Int(n) => Ok(*n),
                S::Bool(b) => Ok(i32::from(*b)),
                _ => Err(Failure::Spreadsheet(Error::Value)),
            }
        }
        fn coercev2_arabic_integer(&self, scalar: &S) -> Result<i32, Failure<Error>> {
            if let S::Text(text) = scalar {
                poirot_coercionv2::roman::coercev2_arabic_text(text)
                    .map_err(|_| Failure::Spreadsheet(Error::Value))
            } else {
                Err(Failure::Spreadsheet(Error::Value))
            }
        }
        fn boolean_value(&self, scalar: &S) -> Option<bool> {
            match scalar {
                S::Bool(b) => Some(*b),
                _ => None,
            }
        }
        fn coercev2_text_boolean(&self, s: &S) -> Result<bool, Failure<Error>> {
            match s {
                S::Bool(b) => Ok(*b),
                _ => Err(Failure::Spreadsheet(Error::Value)),
            }
        }
        fn proper(&self, s: &str) -> String {
            s.into()
        }
    }
    #[test]
    fn roman_policy_inspects_boolean_source_without_reinterpreting_numeric_styles() {
        for (scalar, excel, pinned) in [
            (S::Bool(false), "ID", "CDXCIX"),
            (S::Bool(true), "CDXCIX", "LDVLIV"),
            (S::Int(0), "CDXCIX", "CDXCIX"),
            (S::Int(4), "ID", "ID"),
        ] {
            let call = Call {
                function: Function::Roman,
                arguments: vec![S::Int(499), scalar].into(),
            };
            assert_eq!(
                evaluate_with_policy(
                    &call,
                    &Policy::default(),
                    &mut Adapter { seen: 0 },
                    SemanticsPolicy::ExcelDocumentation
                ),
                Ok(Value::Text(excel.into()))
            );
            assert_eq!(
                evaluate(&call, &Policy::default(), &mut Adapter { seen: 0 }),
                Ok(Value::Text(pinned.into()))
            );
        }
    }
    #[test]
    fn roman_skips_form_after_zero_and_masks_errors() {
        let mut h = Adapter { seen: 0 };
        let call = |arguments: Vec<S>| Call {
            function: Function::Roman,
            arguments: arguments.into(),
        };
        assert_eq!(
            evaluate(&call(vec![S::Int(0), S::Error]), &Policy::default(), &mut h),
            Ok(Value::Text(String::new()))
        );
        assert_eq!(h.seen, 1);
        assert_eq!(
            evaluate(&call(vec![S::Int(1), S::Error]), &Policy::default(), &mut h),
            Err(Failure::Spreadsheet(Error::Num))
        );
    }
    #[test]
    fn roman_preserves_non_spreadsheet_failures() {
        let mut h = Adapter { seen: 0 };
        for (input, expected) in [
            (S::Operational, Failure::Operational(())),
            (
                S::Unsupported,
                Failure::Unsupported(Unsupported::UnknownOutputGrowth),
            ),
        ] {
            let call = Call {
                function: Function::Roman,
                arguments: vec![S::Int(1), input].into(),
            };
            assert_eq!(evaluate(&call, &Policy::default(), &mut h), Err(expected));
        }
    }
    #[test]
    fn textjoin_last_delimiter_row_cycles_and_empty_filter() {
        let mut h = Adapter { seen: 0 };
        let call = Call {
            function: Function::TextJoin,
            arguments: vec![
                S::Area(vec![
                    vec![S::Text("unused")],
                    vec![S::Text(","), S::Text(";")],
                ]),
                S::Bool(true),
                S::Area(vec![
                    vec![S::Text("A"), S::Text("")],
                    vec![S::Text("B"), S::Text("C"), S::Text("D")],
                ]),
            ]
            .into(),
        };
        assert_eq!(
            evaluate(&call, &Policy::default(), &mut h),
            Ok(Value::Text("A,B;C,D".into()))
        );
    }
    #[test]
    fn roman_replacement_order() {
        assert_eq!(
            (0..=4).map(|f| roman(499, f)).collect::<Vec<_>>(),
            ["CDXCIX", "LDVLIV", "XDIX", "VDIV", "ID"]
        );
        assert_eq!(roman(999, 4), "IM");
    }
    #[test]
    fn proper_char_word_boundaries() {
        assert_eq!(proper("ßTRAẞE 12hELLO's WORLD"), "SStraße 12Hello'S World");
        assert_eq!(proper("a\u{301}BC"), "A\u{301}Bc");
        assert_eq!(proper("\u{10428}abc"), "\u{10428}Abc");
    }
}
