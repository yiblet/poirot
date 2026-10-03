//! Rendered text keeps fill positions outside user-visible Unicode data.
use super::*;
/// Rendered buffer plus literal spans used to preserve source-specific text transformations.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct RenderedText {
    /// Current display buffer before final literal-aware postprocessing.
    pub(super) text: String,
    fills: Vec<(usize, char)>,
}
impl RenderedText {
    /// Wrap text that has no separately tracked literal spans.
    pub(super) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fills: Vec::new(),
        }
    }
    /// Append generated display text.
    pub(super) fn push_str(&mut self, text: &str) {
        self.text.push_str(text);
    }
    /// Append source literal text and record its protected span.
    pub(super) fn push_literal(&mut self, token: &Token) {
        if let Token::Fill(c) = token {
            self.fills.push((self.text.len(), *c));
        } else {
            self.push_str(&literal(token));
        }
    }
    /// Borrow the protected literal ranges in the output buffer.
    pub(super) fn literals(tokens: &[Token]) -> Self {
        let mut output = Self::default();
        for token in tokens {
            output.push_literal(token);
        }
        output
    }
    /// Append another rendered buffer, translating its literal spans.
    pub(super) fn append(&mut self, other: &Self) {
        let offset = self.text.len();
        self.fills
            .extend(other.fills.iter().map(|(pos, c)| (offset + pos, *c)));
        self.text.push_str(&other.text);
    }
    /// Consume rendering state and return final display text.
    pub(super) fn finish(
        mut self,
        width: Option<usize>,
        max_output: usize,
    ) -> Result<String, FormatError> {
        let maximum = max_output.min(32767);
        let count = self.text.chars().count();
        if self.fills.is_empty() {
            if count > maximum {
                return Err(resource_error("formatted text exceeds cell limit"));
            }
            return Ok(self.text);
        }
        let width = width.ok_or_else(|| error("fill requires explicit character width"))?;
        if self.fills.len() > 1 {
            return Err(error("multiple fill directives unsupported"));
        }
        if width > 32767 {
            return Err(resource_error("width exceeds resource limit"));
        }
        if count.max(width) > maximum {
            return Err(resource_error("formatted text exceeds cell limit"));
        }
        // SAFETY: The empty-fill branch returned above, so at least one fill remains.
        let (position, c) = self.fills[0];
        if !self.text.is_char_boundary(position) {
            return Err(error("invalid tracked fill position"));
        }
        let repeat = width.saturating_sub(count);
        // SAFETY: width is at most 32767 and repeat <= width; a char uses at most four UTF-8 bytes.
        let fill = c.to_string().repeat(repeat);
        // SAFETY: is_char_boundary(position) succeeded above and self.text has not changed since that check.
        self.text.insert_str(position, &fill);
        Ok(self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracked_positions_preserve_arbitrary_unicode_and_use_character_width() {
        let mut output = RenderedText::plain("é😀\u{10ffff}");
        output.push_literal(&Token::Fill('🦀'));
        output.push_str("\u{10ffff}Z");
        assert_eq!(
            output.finish(Some(7), 32767).unwrap(),
            "é😀\u{10ffff}🦀🦀\u{10ffff}Z"
        );
        let mut left = RenderedText::plain("é");
        let mut right = RenderedText::default();
        right.push_literal(&Token::Fill('x'));
        right.push_str("😀");
        left.append(&right);
        assert_eq!(left.finish(Some(4), 32767).unwrap(), "éxx😀");
    }
    #[test]
    fn fill_checks_width_multiplicity_and_output_budget_before_expansion() {
        let mut output = RenderedText::plain("a");
        output.push_literal(&Token::Fill('x'));
        output.push_literal(&Token::Fill('y'));
        assert!(
            output
                .clone()
                .finish(None, 32767)
                .unwrap_err()
                .message
                .contains("explicit character width")
        );
        assert!(
            output
                .finish(Some(10), 32767)
                .unwrap_err()
                .message
                .contains("multiple fill")
        );
        let mut output = RenderedText::plain("😀");
        output.push_literal(&Token::Fill('x'));
        assert!(
            output
                .clone()
                .finish(Some(usize::MAX), 32767)
                .unwrap_err()
                .message
                .contains("width exceeds")
        );
        assert!(
            output
                .clone()
                .finish(Some(11), 10)
                .unwrap_err()
                .message
                .contains("cell limit")
        );
        assert_eq!(output.finish(Some(0), 10).unwrap(), "😀");
        assert!(RenderedText::plain("abc").finish(None, 2).is_err());
    }
    #[test]
    fn public_number_render_rejects_real_fill_without_reserving_any_character() {
        let program = compile_format("0*x", FormatCompilePolicy::default()).unwrap();
        let number = program.sections()[0].number_plan().unwrap();
        assert!(
            number
                .render(1.)
                .unwrap_err()
                .message
                .contains("explicit character width")
        );
        let program = compile_format("\"\u{10ffff}\"0", FormatCompilePolicy::default()).unwrap();
        assert_eq!(
            program.sections()[0]
                .number_plan()
                .unwrap()
                .render(1.)
                .unwrap(),
            "\u{10ffff}1"
        );
        assert!(
            program.sections()[0]
                .number_plan()
                .unwrap()
                .render(f64::INFINITY)
                .is_err()
        );
        let context = FormatRenderContext {
            width: Some(4),
            ..FormatRenderContext::default()
        };
        let filled = compile_format("0*x", FormatCompilePolicy::default()).unwrap();
        assert_eq!(
            render_value(&CellValue::Number(1.), &filled, context)
                .unwrap()
                .text,
            "1xxx"
        );
    }
}
