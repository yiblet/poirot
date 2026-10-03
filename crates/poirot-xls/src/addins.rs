//! Bounded HSSF add-in identities. This is an export record plan, not a UDF registry.
//! POI LinkTable.addNameXPtg uses an add-in SUPBOOK, workbook-scope EXTERNSHEET
//! (-2, -2), and an EXTERNNAME whose formula is the single #REF! error token.
use super::*;
use poirot_formula::Expr;

/// Deduplicated external-name and sheet records required for BIFF add-in formula tokens.
#[derive(Debug, Default)]
pub(super) struct AddInPlan {
    names: Vec<String>,
}

/// Only functions with a separately implemented evaluator are in this bounded slice.
fn arity(name: &str) -> Option<(usize, usize)> {
    // POI's ATP finder strips only the exact lowercase modern prefix. A mixed
    // prefix may become a defined function NAME instead of an EXTERNNAME.
    if name
        .strip_prefix("_xlfn.")
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case("IFERROR"))
    {
        return Some((2, 2));
    }
    match name.to_ascii_uppercase().as_str() {
        "EDATE" | "IFERROR" => Some((2, 2)),
        "YEARFRAC" => Some((2, 3)),
        _ => None,
    }
}
impl AddInPlan {
    fn function(&mut self, name: &str, argument_count: usize) -> Result<(), XlsError> {
        if name.eq_ignore_ascii_case("_xlfn.IFERROR")
            && !name.starts_with("_xlfn.")
            && self.index(name).is_none()
        {
            return Err(XlsError::Unsupported(format!(
                "function {name} requires a defined function NAME; unsupported"
            )));
        }
        if let Some((min, max)) = arity(name).or_else(|| {
            self.index(name)
                .and_then(|index| self.names.get(index).and_then(|name| arity(name)))
        }) {
            if !(min..=max).contains(&argument_count) {
                return Err(fail(&format!(
                    "{name} argument count outside supported add-in metadata"
                )));
            }
            if self.index(name).is_none() {
                // BIFF NameX name number is an unsigned 16-bit, one-based slot.
                if self.names.len() == u16::MAX as usize {
                    return Err(XlsError::ResourceLimit(
                        "add-in name slots exceed BIFF limit".into(),
                    ));
                }
                self.names.push(name.to_owned());
            }
        }
        Ok(())
    }
    /// Use parser-attached node metadata; AST name normalization stays unchanged.
    pub(super) fn visit_parsed(
        &mut self,
        parsed: &poirot_formula::ParsedFormula,
    ) -> Result<(), XlsError> {
        for call in parsed.function_invocations() {
            let name = parsed
                .source()
                .get(call.name_span.start..call.name_span.end)
                .ok_or_else(|| fail("function spelling span outside parsed source"))?;
            if parsed
                .source()
                .get(call.expression_span.start..call.expression_span.end)
                .is_none()
                || call.expression_span.start != call.name_span.start
                || call.name_span.end > call.expression_span.end
            {
                return Err(fail("function invocation span outside parsed source"));
            }
            self.function(name, call.argument_count)?;
        }
        Ok(())
    }
    /// Collect add-in references from formula syntax before assigning record indexes.
    #[cfg(test)]
    pub(super) fn visit(&mut self, expr: &Expr) -> Result<(), XlsError> {
        match expr {
            Expr::Function { name, args } => {
                self.function(name, args.len())?;
                for arg in args {
                    self.visit(arg)?;
                }
            }
            Expr::Unary { operand, .. } => self.visit(operand)?,
            Expr::Binary { left, right, .. } => {
                self.visit(left)?;
                self.visit(right)?;
            }
            // Array constants remain explicitly unsupported by the token compiler.
            _ => {}
        }
        Ok(())
    }
    /// Resolve a collected add-in spelling to its planned BIFF index.
    pub(super) fn index(&self, name: &str) -> Option<usize> {
        self.names
            .iter()
            .position(|stored| stored.eq_ignore_ascii_case(name))
    }
    /// Borrow indexes referenced by emitted add-in formula tokens.
    pub(super) fn referenced_indexes(&self, expr: &Expr) -> Vec<usize> {
        fn visit(plan: &AddInPlan, expr: &Expr, indexes: &mut Vec<usize>) {
            match expr {
                Expr::Function { name, args } => {
                    if let Some(index) = plan.index(name)
                        && !indexes.contains(&index)
                    {
                        indexes.push(index);
                    }
                    for arg in args {
                        visit(plan, arg, indexes);
                    }
                }
                Expr::Unary { operand, .. } => visit(plan, operand, indexes),
                Expr::Binary { left, right, .. } => {
                    visit(plan, left, indexes);
                    visit(plan, right, indexes);
                }
                _ => {}
            }
        }
        let mut indexes = Vec::new();
        visit(self, expr, &mut indexes);
        indexes
    }
    /// Iterate add-in names in their planned record order.
    pub(super) fn names(&self) -> &[String] {
        &self.names
    }
    /// Whether no add-in records are required.
    pub(super) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
    /// Estimate encoded add-in record bytes before stream admission.
    pub(super) fn predicted_bytes(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            // SUPBOOK (8), one additional EXTERNSHEET reference (6), EXTERNNAME
            // header (4), flags/ixals/reserved (6), short string (2), #REF formula (4).
            14 + self.names.iter().map(|name| 16 + name.len()).sum::<usize>()
        }
    }
    /// Encode the admitted external-book/name records into the workbook stream.
    pub(super) fn encode_records(&self, out: &mut Vec<u8>) -> Result<(), XlsError> {
        if self.is_empty() {
            return Ok(());
        }
        record(out, 0x01ae, &[1, 0, 1, 0x3a])?;
        for name in &self.names {
            let mut bytes = vec![0; 6];
            bytes.extend([name.len() as u8, 0]);
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend([2, 0, 0x1c, 0x17]);
            record(out, 0x0023, &bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_attached_calls_exclude_escaped_literals_and_quoted_sheets() {
        let parsed = poirot_formula::parse_formula_with_policy(
            r#"IFERROR("edate(""1"",2)",'YEARFRAC(2,3)'!A1)+edate(1,2)"#,
            poirot_formula::FormulaParsePolicy::default(),
        )
        .unwrap();
        let mut plan = AddInPlan::default();
        plan.visit_parsed(&parsed).unwrap();
        assert_eq!(plan.names(), &["IFERROR", "edate"]);
    }
    #[test]
    fn prefix_order_preserves_external_record_kind_boundary() {
        let mut plan = AddInPlan::default();
        plan.visit(&parse_formula("_xlfn.IFERROR(1,2)").unwrap())
            .unwrap();
        plan.visit(&parse_formula("_XLFN.IFERROR(3,4)").unwrap())
            .unwrap();
        assert_eq!(plan.names(), &["_xlfn.IFERROR"]);
        assert_eq!(
            plan.referenced_indexes(&parse_formula("_XLFN.IFERROR(3,4)").unwrap()),
            vec![0]
        );
        assert!(
            AddInPlan::default()
                .visit(&parse_formula("_XLFN.IFERROR(1,2)").unwrap())
                .is_err()
        );
        assert!(
            plan.visit(&parse_formula("_XLFN.IFERROR(1)").unwrap())
                .is_err()
        );
    }
    #[test]
    fn identities_reuse_first_spelling_and_biff_name_slot() {
        let mut plan = AddInPlan::default();
        plan.visit_parsed(
            &poirot_formula::parse_formula_with_policy(
                "edate(1,2)+EDATE(3,4)+YEARFRAC(1,2)",
                poirot_formula::FormulaParsePolicy::default(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(plan.names(), &["edate", "YEARFRAC"]);
        assert_eq!(plan.index("EDATE"), Some(0));
        let mut records = Vec::new();
        plan.encode_records(&mut records).unwrap();
        assert_eq!(&records[..8], &[0xae, 1, 4, 0, 1, 0, 1, 0x3a]);
        assert_eq!(
            &records[8..],
            &[
                0x23, 0, 17, 0, 0, 0, 0, 0, 0, 0, 5, 0, b'e', b'd', b'a', b't', b'e', 2, 0, 0x1c,
                0x17, 0x23, 0, 20, 0, 0, 0, 0, 0, 0, 0, 8, 0, b'Y', b'E', b'A', b'R', b'F', b'R',
                b'A', b'C', 2, 0, 0x1c, 0x17
            ]
        );
        assert_eq!(records.len() + 6, plan.predicted_bytes());
    }
}
