//! Stack execution of owned instructions; calls borrow lazy blocks through family hosts.
//! The unit-test tree oracle remains to check value, read, observation and budget parity.
use super::*;
use crate::bytecode::{CallTarget, Compiled, Opcode, ReferenceKind};
use crate::executable::ReferenceRef;
use crate::execution_observation::{Operation, OperationKind, observed_value};

/// Active operation and prior observation context restored when a bytecode expression returns.
type ObservationFrame = (Operation, Option<(Operation, u64)>);
impl Session<'_> {
    /// Check extension identity and execute a complete compiled formula block.
    pub(crate) fn bytecode_expr(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
    ) -> EvalResult<Value> {
        if program
            .extension_identity
            .as_ref()
            .is_some_and(|identity| self.extensions.is_none_or(|r| &r.identity() != identity))
        {
            return Err(EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::UserDefinedFunction,
                source: Some(EvaluationTarget { sheet, address: at }),
                detail: "extension registry identity changed; rebind the formula plan".into(),
            });
        }
        self.execute_bytecode(sheet, at, program, 0)
    }
    /// Execute a bytecode block with shared budgets and lazy family adapters.
    pub(super) fn execute_bytecode(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
        start: usize,
    ) -> EvalResult<Value> {
        let mut values = self.bytecode_stacks.pop().unwrap_or_default();
        let observed = self.execution_observer.is_some();
        let mut frames: Vec<ObservationFrame> = Vec::new();
        let mut entered = 0usize;
        let result = (|| {
            let mut ip = start;
            loop {
                let instruction = *program
                    .template
                    .code
                    .get(ip)
                    .ok_or(EvaluationFailure::StalePlan)?;
                match instruction.opcode {
                    Opcode::Enter => {
                        self.budget.charge(WorkKind::Expression, 1)?;
                        self.budget.enter_depth()?;
                        if observed {
                            let handler = program
                                .template
                                .code
                                .get(ip + 1)
                                .filter(|next| next.opcode == Opcode::Invoke)
                                .and_then(|next| self.bytecode_handler(program, next.a as usize));
                            match self.observation_begin(
                                EvaluationTarget { sheet, address: at },
                                OperationKind::Expression { handler },
                            ) {
                                Ok(frame) => frames.push(frame),
                                Err(error) => {
                                    self.budget.leave_depth();
                                    return Err(error);
                                }
                            }
                        }
                        entered += 1;
                    }
                    Opcode::Leave => {
                        entered = entered.checked_sub(1).ok_or(EvaluationFailure::StalePlan)?;
                        let notification = if let Some((operation, previous)) = frames.pop() {
                            self.observation_end(
                                operation,
                                previous,
                                Ok(observed_value(
                                    values.last().ok_or(EvaluationFailure::StalePlan)?,
                                )),
                            )
                        } else {
                            Ok(())
                        };
                        self.budget.leave_depth();
                        notification?;
                        if entered == 0 {
                            return values.pop().ok_or(EvaluationFailure::StalePlan);
                        }
                    }
                    Opcode::Number => values.push(
                        self.numeric_policy
                            .number_ptg(f64::from_bits(
                                u64::from(instruction.a) | u64::from(instruction.b) << 32,
                            ))?
                            .into(),
                    ),
                    Opcode::Text => {
                        let text = program.text(instruction.a, instruction.b)?;
                        self.budget.text(text.len())?;
                        values.push(Text(text.to_owned()).into());
                    }
                    Opcode::Boolean => values.push(Boolean(instruction.a != 0).into()),
                    Opcode::Error => values.push(
                        Error(
                            program
                                .text(instruction.a, instruction.b)?
                                .parse()
                                .unwrap_or(VALUE_ERROR),
                        )
                        .into(),
                    ),
                    Opcode::Missing => values.push(Blank.into()),
                    Opcode::Reference => values.push(self.bytecode_reference(
                        sheet,
                        at,
                        program,
                        instruction.a as usize,
                    )?),
                    Opcode::Name => values.push(self.defined_name(
                        sheet,
                        sheet,
                        at,
                        program.text(instruction.a, instruction.b)?,
                    )?),
                    Opcode::QualifiedName => {
                        let mut bytes = Reader(
                            program
                                .template
                                .operands
                                .get(
                                    instruction.a as usize
                                        ..instruction
                                            .a
                                            .checked_add(instruction.b)
                                            .ok_or(EvaluationFailure::StalePlan)?
                                            as usize,
                                )
                                .ok_or(EvaluationFailure::StalePlan)?,
                        );
                        let name_sheet = bytes.string()?;
                        let name = std::str::from_utf8(bytes.0)
                            .map_err(|_| EvaluationFailure::StalePlan)?;
                        values.push(match self.sheet_named(sheet, Some(name_sheet))? {
                            Some(scope) => self.defined_name(sheet, scope, at, name)?,
                            None => err(Ref),
                        });
                    }
                    Opcode::Unary => {
                        let value = values.pop().ok_or(EvaluationFailure::StalePlan)?;
                        values.push(self.unary_value(
                            sheet,
                            at,
                            decode_unary(instruction.a)?,
                            value,
                        )?);
                    }
                    Opcode::Binary => {
                        let right = values.pop().ok_or(EvaluationFailure::StalePlan)?;
                        let left = values.pop().ok_or(EvaluationFailure::StalePlan)?;
                        values.push(self.binary_values(
                            sheet,
                            at,
                            decode_binary(instruction.a)?,
                            left,
                            right,
                        )?);
                    }
                    Opcode::ArrayAllocate => self.budget.allocation(instruction.a as usize)?,
                    Opcode::Scalarize => {
                        let value = values.pop().ok_or(EvaluationFailure::StalePlan)?;
                        values.push(self.scalar(at, value)?.into());
                    }
                    Opcode::Array => {
                        let count = (instruction.a as usize)
                            .checked_mul(instruction.b as usize)
                            .ok_or(EvaluationFailure::StalePlan)?;
                        let offset = values
                            .len()
                            .checked_sub(count)
                            .ok_or(EvaluationFailure::StalePlan)?;
                        // SAFETY: offset was obtained by checked subtraction from values.len(); the vector has not changed since that check.
                        let mut elements = values.drain(offset..);
                        let mut rows = Vec::new();
                        for _ in 0..instruction.a {
                            let mut row = Vec::new();
                            for _ in 0..instruction.b {
                                let Some(Value::Scalar(value)) = elements.next() else {
                                    return Err(EvaluationFailure::StalePlan);
                                };
                                row.push(value);
                            }
                            rows.push(row);
                        }
                        drop(elements);
                        values.push(Value::Array(ArrayView::anchored(
                            rows,
                            Some(CellAddress { row: 0, col: 0 }),
                        )));
                    }
                    Opcode::Invoke => {
                        let args = ArgumentsRef::Bytecode {
                            program,
                            first: ip + 2,
                            count: instruction.b as usize,
                        };
                        values.push(self.bytecode_call(
                            sheet,
                            at,
                            program,
                            instruction.a as usize,
                            args,
                        )?);
                        ip = instruction.c as usize;
                        continue;
                    }
                    Opcode::Jump => {
                        ip = instruction.c as usize;
                        continue;
                    }
                    Opcode::YieldArgument | Opcode::Argument => {
                        return Err(EvaluationFailure::StalePlan);
                    }
                }
                ip += 1;
            }
        })();
        if let Err(error) = &result {
            while entered != 0 {
                if let Some((operation, previous)) = frames.pop() {
                    // Existing operational failures dominate observer failures.
                    let _ = self.observation_end(operation, previous, Err(error));
                }
                self.budget.leave_depth();
                entered -= 1;
            }
        }
        // Every active invocation owns its buffer, including nested cells and lazy
        // arguments. Only empty buffers return to this run's reuse pool.
        values.clear();
        self.bytecode_stacks.push(values);
        result
    }
    fn bytecode_handler(&self, program: &Compiled, index: usize) -> Option<FunctionId> {
        match &program.calls.get(index)?.target {
            CallTarget::Builtin(handler) => Some(*handler),
            CallTarget::Unresolved
                if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation =>
            {
                registry::resolve_excel_function(&poirot_formula::FunctionName::from(
                    program.resolved_spelling(index).ok()?,
                ))
            }
            _ => None,
        }
    }
    fn bytecode_call(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
        index: usize,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        let call = program
            .calls
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let handler = self.bytecode_handler(program, index);
        let parsed_spelling = program.source_spelling(index)?;
        let resolved_spelling = program.resolved_spelling(index)?;
        self.admit_resolved_call(
            call_admission::ResolvedCall {
                invocation: handler.map(|handler| ConsumedBuiltinInvocation {
                    target: EvaluationTarget { sheet, address: at },
                    handler,
                    parsed_spelling,
                    resolved_spelling,
                    argument_count: args.len(),
                }),
                read_builtin_policy: matches!(call.target, CallTarget::Unresolved),
            },
            call_admission::AdmissionOrder::ObserverBeforePolicy,
        )?;
        if let Some(handler) = handler {
            return adapters::execute(self, EvaluationTarget { sheet, address: at }, handler, args);
        }
        match &call.target {
            CallTarget::Extension(binding) => extensions::execute(self, sheet, at, binding, args),
            _ => {
                self.read_lazy(|| DependencyRead::OpaqueProvider(parsed_spelling.to_owned()));
                Ok(err(Name))
            }
        }
    }
    fn bytecode_reference(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
        slot: usize,
    ) -> EvalResult<Value> {
        let binding = *program
            .references
            .slots
            .get(slot)
            .ok_or(EvaluationFailure::StalePlan)?;
        let mut reader = Reader(
            program
                .references
                .data
                .get(
                    binding.offset as usize
                        ..binding
                            .offset
                            .checked_add(binding.length)
                            .ok_or(EvaluationFailure::StalePlan)?
                            as usize,
                )
                .ok_or(EvaluationFailure::StalePlan)?,
        );
        match binding.kind {
            ReferenceKind::Cell => {
                let row = reader.word()?;
                let column = reader.word()?;
                reader.word()?;
                reader.word()?;
                let named = reader.optional()?;
                self.reference_atom(
                    sheet,
                    at,
                    ReferenceRef::Cell {
                        row,
                        column,
                        sheet: named,
                    },
                )
            }
            ReferenceKind::Column | ReferenceKind::Row => {
                let index = reader.word()?;
                reader.word()?;
                let named = reader.optional()?;
                self.reference_atom(
                    sheet,
                    at,
                    match binding.kind {
                        ReferenceKind::Column => ReferenceRef::Column {
                            column: index,
                            sheet: named,
                        },
                        _ => ReferenceRef::Row {
                            row: index,
                            sheet: named,
                        },
                    },
                )
            }
            ReferenceKind::SheetSpan => {
                let first_sheet = reader.string()?;
                let last_sheet = reader.string()?;
                let start = reader.cell()?;
                let end = if reader.word()? != 0 {
                    Some(reader.cell()?)
                } else {
                    None
                };
                self.reference_atom(
                    sheet,
                    at,
                    ReferenceRef::SheetSpan {
                        first_sheet,
                        last_sheet,
                        start: &start,
                        end: end.as_ref(),
                    },
                )
            }
        }
    }
}
impl crate::bytecode::Executable {
    /// Borrow a checked UTF-8 span from operand storage.
    pub(crate) fn text(&self, offset: u32, length: u32) -> EvalResult<&str> {
        let end = offset
            .checked_add(length)
            .ok_or(EvaluationFailure::StalePlan)?;
        let bytes = self
            .template
            .operands
            .get(offset as usize..end as usize)
            .ok_or(EvaluationFailure::StalePlan)?;
        std::str::from_utf8(bytes).map_err(|_| EvaluationFailure::StalePlan)
    }
    /// Borrow a call's parsed function spelling, rejecting invalid packed storage.
    pub fn source_spelling(&self, index: usize) -> EvalResult<&str> {
        let call = self.calls.get(index).ok_or(EvaluationFailure::StalePlan)?;
        let bytes = self
            .template
            .operands
            .get(call.source_offset as usize..)
            .ok_or(EvaluationFailure::StalePlan)?;
        Reader(bytes).string()
    }
    /// Borrow the exact lookup spelling, rejecting an invalid call index.
    pub fn resolved_spelling(&self, index: usize) -> EvalResult<&str> {
        let call = self.calls.get(index).ok_or(EvaluationFailure::StalePlan)?;
        match call.resolved_name.as_deref() {
            Some(name) => Ok(name),
            None => self.source_spelling(index),
        }
    }
}
/// Cursor decoding checked reference words and UTF-8 strings.
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn word(&mut self) -> EvalResult<u32> {
        let (value, rest) = self
            .0
            .split_at_checked(4)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.0 = rest;
        let bytes = value.try_into().map_err(|_| EvaluationFailure::StalePlan)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn string(&mut self) -> EvalResult<&'a str> {
        let count = self.word()? as usize;
        let (text, rest) = self
            .0
            .split_at_checked(count)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.0 = rest;
        std::str::from_utf8(text).map_err(|_| EvaluationFailure::StalePlan)
    }
    fn optional(&mut self) -> EvalResult<Option<&'a str>> {
        if self.word()? != 0 {
            self.string().map(Some)
        } else {
            Ok(None)
        }
    }
    fn cell(&mut self) -> EvalResult<poirot_formula::CellReference> {
        let row = self.word()?;
        let column = self.word()?;
        let row_absolute = self.word()? != 0;
        let column_absolute = self.word()? != 0;
        if self.optional()?.is_some() {
            return Err(EvaluationFailure::StalePlan);
        }
        Ok(poirot_formula::CellReference {
            row,
            column,
            row_absolute,
            column_absolute,
            sheet: None,
        })
    }
}
fn decode_unary(code: u32) -> EvalResult<UnaryOp> {
    Ok(match code {
        0 => UnaryOp::Plus,
        1 => UnaryOp::Minus,
        2 => UnaryOp::Percent,
        3 => UnaryOp::ImplicitIntersection,
        _ => return Err(EvaluationFailure::StalePlan),
    })
}
fn decode_binary(code: u32) -> EvalResult<BinaryOp> {
    Ok(match code {
        0 => BinaryOp::Add,
        1 => BinaryOp::Subtract,
        2 => BinaryOp::Multiply,
        3 => BinaryOp::Divide,
        4 => BinaryOp::Power,
        5 => BinaryOp::Concat,
        6 => BinaryOp::Equal,
        7 => BinaryOp::NotEqual,
        8 => BinaryOp::Less,
        9 => BinaryOp::LessEqual,
        10 => BinaryOp::Greater,
        11 => BinaryOp::GreaterEqual,
        12 => BinaryOp::Range,
        13 => BinaryOp::Union,
        14 => BinaryOp::Intersection,
        _ => return Err(EvaluationFailure::StalePlan),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_observation::{ExecutionEvent, ExecutionObserver};
    #[test]
    fn malformed_reference_encoding_returns_stale_plan() {
        for length in 0..4 {
            assert_eq!(
                Reader(&[0; 4][..length]).word(),
                Err(EvaluationFailure::StalePlan)
            );
        }
        assert_eq!(
            Reader(&[5, 0, 0, 0, b'a']).string(),
            Err(EvaluationFailure::StalePlan)
        );
        assert_eq!(
            Reader(&[1, 0, 0, 0, 0xff]).string(),
            Err(EvaluationFailure::StalePlan)
        );
        assert!(Reader(&[0; 16]).cell().is_err());
        assert!(decode_unary(u32::MAX).is_err());
        assert!(decode_binary(u32::MAX).is_err());
    }
    const AT: CellAddress = CellAddress { row: 3, col: 3 };
    /// Records tree and bytecode execution, dependency, and builtin events for parity checks.
    #[derive(Default)]
    struct Events(Vec<String>);
    impl ExecutionObserver for Events {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            self.0.push(format!("{event:?}"));
            Ok(())
        }
    }
    impl DependencyReadSink for Events {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(format!("{event:?}"));
        }
    }
    impl BuiltinInvocationObserver for Events {
        fn consumed(&mut self, call: ConsumedBuiltinInvocation<'_>) -> EvalResult<()> {
            self.0.push(format!(
                "{:?} {:?} {} {} {}",
                call.target,
                call.handler,
                call.parsed_spelling,
                call.resolved_spelling,
                call.argument_count
            ));
            Ok(())
        }
    }
    fn workbook() -> Workbook {
        let mut workbook = Workbook::new();
        for (index, sheet) in ["One", "Two", "Three"].into_iter().enumerate() {
            workbook.add_sheet(sheet).unwrap();
            workbook
                .sheet_mut(index)
                .unwrap()
                .set_value("A1".parse().unwrap(), 2.)
                .unwrap();
            workbook
                .sheet_mut(index)
                .unwrap()
                .set_value("B1".parse().unwrap(), 4.)
                .unwrap();
        }
        workbook
    }
    fn snapshot(value: Value) -> String {
        match value {
            Value::Scalar(v) => format!("scalar {v:?}"),
            Value::Missing => "missing".into(),
            Value::Array(v) => format!("array {:?} {:?}", v.anchor, v.values),
            Value::Refs(v) | Value::SheetSpan(v) => format!(
                "refs {:?}",
                v.into_iter()
                    .map(|a| (
                        matches!(a.kind, AreaKind::Cell),
                        a.sheet,
                        a.r0,
                        a.r1,
                        a.c0,
                        a.c1
                    ))
                    .collect::<Vec<_>>()
            ),
            Value::SignedRefs(v) => format!(
                "signed {:?}",
                v.into_iter()
                    .map(|a| (
                        matches!(a.kind, AreaKind::Cell),
                        a.sheet,
                        a.r0,
                        a.r1,
                        a.c0,
                        a.c1
                    ))
                    .collect::<Vec<_>>()
            ),
        }
    }
    fn compare(
        syntax: &poirot_formula::Expr,
        budgets: EvaluationBudgets,
        registry: Option<&extensions::ExtensionRegistry>,
    ) {
        for numeric in [
            NumericOutcomePolicy::LegacyFinite,
            NumericOutcomePolicy::PinnedPoi7321209,
        ] {
            for builtin in [
                BuiltinSemanticsPolicy::PinnedPoi,
                BuiltinSemanticsPolicy::ExcelDocumentation,
            ] {
                compare_configured(
                    syntax,
                    budgets,
                    registry,
                    numeric,
                    builtin,
                    ArrayResultPolicy::ScalarIntersection,
                );
            }
        }
    }
    fn compare_configured(
        syntax: &poirot_formula::Expr,
        budgets: EvaluationBudgets,
        registry: Option<&extensions::ExtensionRegistry>,
        numeric: NumericOutcomePolicy,
        builtin: BuiltinSemanticsPolicy,
        arrays: ArrayResultPolicy,
    ) {
        let workbook = workbook();
        let tree = bound::bind_with_registry(syntax, registry);
        let program = bytecode::compile_with_registry(syntax, registry).unwrap();
        let mut tree_events = Events::default();
        let mut vm_events = Events::default();
        let mut tree_reads = Events::default();
        let mut vm_reads = Events::default();
        let mut tree_calls = Events::default();
        let mut vm_calls = Events::default();
        let (tree_result, tree_usage, tree_extensions) = {
            let mut session = Session::new(
                &workbook,
                EvaluationInputs::default(),
                BudgetMeter::new(budgets).unwrap(),
            );
            session.budget.push_cell(EvaluationTarget {
                sheet: 0,
                address: AT,
            });
            session.extensions = registry;
            session.numeric_policy = numeric;
            session.builtin_policy = builtin;
            session.array_result_policy = arrays;
            session.execution_observer = Some(&mut tree_events);
            session.dependency_sink = Some(&mut tree_reads);
            session.builtin_invocation_observer = Some(&mut tree_calls);
            let result = session.expr(0, AT, &tree).map(snapshot);
            (
                result,
                session.budget.usage.clone(),
                session.extension_trace,
            )
        };
        let (vm_result, vm_usage, vm_extensions) = {
            let mut session = Session::new(
                &workbook,
                EvaluationInputs::default(),
                BudgetMeter::new(budgets).unwrap(),
            );
            session.budget.push_cell(EvaluationTarget {
                sheet: 0,
                address: AT,
            });
            session.extensions = registry;
            session.numeric_policy = numeric;
            session.builtin_policy = builtin;
            session.array_result_policy = arrays;
            session.execution_observer = Some(&mut vm_events);
            session.dependency_sink = Some(&mut vm_reads);
            session.builtin_invocation_observer = Some(&mut vm_calls);
            let result = session.bytecode_expr(0, AT, &program).map(snapshot);
            (
                result,
                session.budget.usage.clone(),
                session.extension_trace,
            )
        };
        assert_eq!(tree_result, vm_result, "{syntax:?}");
        assert_eq!(tree_usage, vm_usage, "{syntax:?}");
        assert_eq!(tree_extensions, vm_extensions, "{syntax:?}");
        assert_eq!(tree_reads.0, vm_reads.0, "{syntax:?}");
        assert_eq!(tree_calls.0, vm_calls.0, "{syntax:?}");
        assert_eq!(tree_events.0, vm_events.0, "{syntax:?}");
    }
    #[test]
    fn array_operator_tree_and_bytecode_match_values_reads_events_and_budgets() {
        for source in [
            "{1,2}+{10;20}",
            "-+{100,200}%",
            "{1,2;3,4}/{1;0}",
            "{1,2}<{2;1}",
            "{1,2}&{\"a\";\"b\"}",
            "{1,2}+{10,20,30}",
            "A1:B2+{10,20}",
            "{#N/A,#VALUE!}+A1:B1",
            "@({1,2}+1)",
            "IF(TRUE,{1,2},UNKNOWN())+1",
            "TRANSPOSE({1,2})*2",
        ] {
            let syntax = poirot_formula::parse_formula(source).unwrap();
            for numeric in [
                NumericOutcomePolicy::LegacyFinite,
                NumericOutcomePolicy::PinnedPoi7321209,
            ] {
                for builtin in [
                    BuiltinSemanticsPolicy::PinnedPoi,
                    BuiltinSemanticsPolicy::ExcelDocumentation,
                ] {
                    for budgets in [
                        EvaluationBudgets::default(),
                        EvaluationBudgets {
                            materialized_values: 3,
                            ..Default::default()
                        },
                        EvaluationBudgets {
                            run_work: 10,
                            ..Default::default()
                        },
                    ] {
                        compare_configured(
                            &syntax,
                            budgets,
                            None,
                            numeric,
                            builtin,
                            ArrayResultPolicy::DynamicSpill,
                        );
                    }
                }
            }
        }
    }

    fn source(source: &str, budgets: EvaluationBudgets) {
        compare(
            &poirot_formula::parse_formula(source)
                .unwrap_or_else(|error| panic!("{source}: {error}")),
            budgets,
            None,
        );
    }
    fn compile(source: &str) -> Compiled {
        bytecode::compile_with_registry(&poirot_formula::parse_formula(source).unwrap(), None)
            .unwrap()
    }

    #[test]
    fn bytecode_stack_reuses_capacity_across_nested_cells_and_lazy_arguments() {
        let mut workbook = workbook();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(
                "A2".parse().unwrap(),
                "10+IF(TRUE,SUM({1,2;3,4}),UNKNOWN())",
                Blank,
            )
            .unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let program = compile("100+IF(TRUE,A2+SUM(One:Three!A1)+SUM({5,6}),UNKNOWN())");
        assert_eq!(
            snapshot(session.bytecode_expr(0, AT, &program).unwrap()),
            "scalar Number(137.0)"
        );
        assert!(session.bytecode_stacks.iter().all(Vec::is_empty));
        let buffers = |session: &Session<'_>| {
            let mut buffers = session
                .bytecode_stacks
                .iter()
                .map(|values| (values.as_ptr() as usize, values.capacity()))
                .collect::<Vec<_>>();
            buffers.sort_unstable();
            buffers
        };
        let allocations = buffers(&session);
        assert!(
            allocations.len() > 1,
            "nested invocations need distinct buffers"
        );
        assert!(allocations.iter().all(|(_, capacity)| *capacity > 0));
        for _ in 0..3 {
            session.cache.clear();
            assert_eq!(
                snapshot(session.bytecode_expr(0, AT, &program).unwrap()),
                "scalar Number(137.0)"
            );
            assert!(session.bytecode_stacks.iter().all(Vec::is_empty));
            assert_eq!(buffers(&session), allocations);
        }
        let fresh = Session::with_context(&workbook, EvaluationInputs::default());
        assert!(fresh.bytecode_stacks.is_empty());
    }

    #[test]
    fn bytecode_stack_pool_returns_empty_buffers_after_budget_failure() {
        let workbook = workbook();
        // Abort after partial operands, including nested lazy arguments and array construction.
        for source in [
            "100+(1+2)",
            "100+IF(TRUE,1+2,UNKNOWN())",
            "100+SUM({1,2;3,4})",
        ] {
            let program = compile(source);
            let limits = (1..25)
                .map(|run_work| EvaluationBudgets {
                    run_work,
                    ..EvaluationBudgets::default()
                })
                .chain((1..8).map(|depth| EvaluationBudgets {
                    depth,
                    ..EvaluationBudgets::default()
                }));
            let mut failures = 0;
            for limits in limits {
                let mut session = Session::new(
                    &workbook,
                    EvaluationInputs::default(),
                    BudgetMeter::new(limits).unwrap(),
                );
                failures += usize::from(session.bytecode_expr(0, AT, &program).is_err());
                assert!(!session.bytecode_stacks.is_empty());
                assert!(
                    session.bytecode_stacks.iter().all(Vec::is_empty),
                    "{source}, {limits:?}"
                );
                session.budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
                assert_eq!(
                    snapshot(session.bytecode_expr(0, AT, &compile("17")).unwrap()),
                    "scalar Number(17.0)"
                );
                assert!(session.bytecode_stacks.iter().all(Vec::is_empty));
            }
            assert!(failures > 0);
        }
    }

    #[test]
    fn bytecode_stack_unwinds_nested_observer_refusals_with_tree_trace_parity() {
        /// Rejects a chosen event ordinal to test nested VM frame unwinding.
        struct Reject {
            events: Vec<String>,
            ordinal: usize,
        }
        impl ExecutionObserver for Reject {
            fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
                self.events.push(format!("{event:?}"));
                if self.events.len() == self.ordinal {
                    Err(EvaluationFailure::StalePlan)
                } else {
                    Ok(())
                }
            }
        }
        let mut workbook = workbook();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(
                "A2".parse().unwrap(),
                "10+IF(TRUE,SUM({1,2;3,4}),UNKNOWN())",
                Blank,
            )
            .unwrap();
        let syntax = poirot_formula::parse_formula("100+IF(TRUE,A2+SUM({5,6}),UNKNOWN())").unwrap();
        let tree = bound::bind_with_registry(&syntax, None);
        let program = bytecode::compile_with_registry(&syntax, None).unwrap();
        let probe = compile("17");
        // Refuse every event in turn, covering Begin, successful Complete and ancestor unwind.
        let run = |vm: bool, ordinal| {
            let mut observer = Reject {
                events: Vec::new(),
                ordinal,
            };
            let (result, usage) = {
                let mut session = Session::with_context(&workbook, EvaluationInputs::default());
                session.execution_observer = Some(&mut observer);
                let result = if vm {
                    session.bytecode_expr(0, AT, &program)
                } else {
                    session.expr(0, AT, &tree)
                }
                .map(snapshot);
                assert!(session.bytecode_stacks.iter().all(Vec::is_empty));
                let usage = session.budget.usage.clone();
                assert_eq!(
                    snapshot(session.bytecode_expr(0, AT, &probe).unwrap()),
                    "scalar Number(17.0)"
                );
                assert!(session.bytecode_stacks.iter().all(Vec::is_empty));
                (result, usage)
            };
            (result, usage, observer.events)
        };
        // The final literal probe emits its own Begin/Complete pair; refuse only
        // main-expression events, then verify the observed probe still succeeds.
        let events = run(true, usize::MAX).2.len() - 2;
        for ordinal in 1..=events {
            assert_eq!(
                run(false, ordinal),
                run(true, ordinal),
                "observer event {ordinal}"
            );
        }
    }

    #[test]
    fn bytecode_vm_matches_tree_families_references_and_lazy_errors() {
        for formula in [
            "1+2*3^2",
            "-0%",
            "+\"hello\"",
            "1/0",
            "\"A\"=\"a\"",
            "\"a\"&2",
            "@A1",
            "{1,2;3,4}",
            "SUM(A1:B1)",
            "SUM(One:Three!A1)",
            "SUM(One:Three!A1:B1)",
            "A1:B1",
            "(A1,B1)",
            "A1:B2 B1:C2",
            "SUM(A:A)",
            "SUM(1:1)",
            "One!missing",
            "missing",
            "IF(TRUE,7,UNKNOWN())",
            "IF(FALSE,1/0,9)",
            "IF(TRUE,,1/0)",
            "IFERROR(1/0,IFNA(#N/A,8))",
            "CHOOSE(2,1/0,9)",
            "IFERROR(A1,UNKNOWN())",
            "UNKNOWN(1/0)",
            "IF(1/0)",
            "DATE(2020,2,3)",
            "AND(TRUE,FALSE)",
            "AVERAGE({1,2;3,4})",
            "ROW(A1)",
            "INDEX(A1:B1,1,2)",
            "MATCH(2,{1,2,3},0)",
            "ISNUMBER(2)",
            "ABS(-3)",
            "LEFT(\"hello\",2)",
            "PV(0.1,2,3)",
            "BIN2DEC(10)",
            "MMULT({1,2},{3;4})",
            "SUMIF(A1:B1,\">2\")",
            "DSUM({\"a\",\"b\";1,2},\"b\",{\"a\";1})",
            "STDEV.S(1,2)",
            "QUARTILE.INC({1,2,3},1)",
            "RANK.EQ(2,{1,2,3})",
            "PERCENTILE.INC({1,2,3},0.5)",
            "_xlfn.IFNA(#N/A,2)",
            "RAND()",
            "RANDBETWEEN(1,4)",
        ] {
            source(formula, EvaluationBudgets::default());
        }
    }
    #[test]
    fn bytecode_vm_matches_tree_budget_admission_order() {
        for formula in [
            "{1,2;3,4}",
            "SUM(A1:B1)+IF(FALSE,UNKNOWN(),5)",
            "IFERROR(1/0,2)",
            "A1+B1",
        ] {
            for work in 1..65 {
                source(
                    formula,
                    EvaluationBudgets {
                        run_work: work,
                        ..EvaluationBudgets::default()
                    },
                );
            }
            for work in 1..65 {
                source(
                    formula,
                    EvaluationBudgets {
                        cell_work: work,
                        ..EvaluationBudgets::default()
                    },
                );
            }
            for allocation in 1..7 {
                source(
                    formula,
                    EvaluationBudgets {
                        materialized_values: allocation,
                        ..EvaluationBudgets::default()
                    },
                );
            }
            for depth in 1..6 {
                source(
                    formula,
                    EvaluationBudgets {
                        depth,
                        ..EvaluationBudgets::default()
                    },
                );
            }
        }
    }
    #[test]
    fn bytecode_vm_preserves_missing_array_provenance_and_depth_truncation() {
        compare(
            &poirot_formula::Expr::Function {
                name: "INDEX".into(),
                args: vec![
                    poirot_formula::Expr::Array(vec![vec![
                        poirot_formula::Expr::Number(1.),
                        poirot_formula::Expr::Missing,
                    ]]),
                    poirot_formula::Expr::Number(1.),
                    poirot_formula::Expr::Number(2.),
                ],
            },
            EvaluationBudgets::default(),
            None,
        );
        let array_syntax = poirot_formula::Expr::Array(vec![vec![
            poirot_formula::parse_formula("A1").unwrap(),
            poirot_formula::parse_formula("IFERROR(1/0,2)").unwrap(),
        ]]);
        for work in 1..32 {
            compare(
                &array_syntax,
                EvaluationBudgets {
                    run_work: work,
                    ..EvaluationBudgets::default()
                },
                None,
            );
        }
        let mut syntax = poirot_formula::Expr::Number(1.);
        for _ in 0..132 {
            syntax = poirot_formula::Expr::Unary {
                op: UnaryOp::Plus,
                operand: Box::new(syntax),
            };
        }
        compare(&syntax, EvaluationBudgets::default(), None);
    }
    #[test]
    fn bytecode_vm_matches_extension_binding_laziness_and_namespace_spelling() {
        use extensions::*;
        for evaluation in [
            ExtensionEvaluationStrategy::Deferred,
            ExtensionEvaluationStrategy::Eager,
        ] {
            let mut registry = ExtensionRegistry::new();
            registry
                .register(
                    ExtensionNamespace::UserDefined,
                    "CUSTOM",
                    "v1",
                    ExtensionContract {
                        arity: registry::Arity {
                            minimum: 1,
                            maximum: 2,
                        },
                        evaluation,
                        capabilities: ExtensionCapabilities {
                            references: true,
                            arrays: true,
                            context: false,
                        },
                        cacheability: ExtensionCacheability::Deterministic,
                    },
                    |_: &mut dyn ExtensionHost| Ok(ExtensionOutput::Argument(0)),
                )
                .unwrap();
            registry
                .set_external_namespace(Some(
                    external_namespace::ExternalFunctionNamespace::from_stored_names(
                        ["CuStOm", "sUm"],
                        4,
                    )
                    .unwrap(),
                ))
                .unwrap();
            for source in [
                "CUSTOM(A1,UNKNOWN())",
                "CUSTOM()",
                "CUSTOM(1,2,UNKNOWN())",
                "CUSTOM({1,2;3,4})",
                "IF(FALSE,CUSTOM(UNKNOWN()),7)",
                "SUM(A1:B1)",
            ] {
                compare(
                    &poirot_formula::parse_formula(source).unwrap(),
                    EvaluationBudgets::default(),
                    Some(&registry),
                );
            }
            let syntax = poirot_formula::Expr::Function {
                name: "unsupported_Ω".into(),
                args: vec![poirot_formula::Expr::Number(1.)],
            };
            compare(&syntax, EvaluationBudgets::default(), Some(&registry));
        }
    }
    #[test]
    fn bytecode_vm_unwinds_observer_rejections_before_next_expression() {
        /// Rejects after a controlled number of accepted events to test session recovery.
        struct Rejecting {
            remaining: usize,
            events: Vec<String>,
        }
        impl ExecutionObserver for Rejecting {
            fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
                self.events.push(format!("{event:?}"));
                if self.remaining == 0 {
                    Err(EvaluationFailure::ResourceLimit {
                        capability: CapabilityId::FunctionAdapter,
                        source: None,
                        detail: "test observer rejected event".into(),
                    })
                } else {
                    self.remaining -= 1;
                    Ok(())
                }
            }
        }
        let workbook = workbook();
        let syntax = poirot_formula::parse_formula("IFERROR(1/0,2)+3").unwrap();
        let tree = bound::bind(&syntax);
        let program = bytecode::compile(&syntax).unwrap();
        for limit in 0..30 {
            let run = |bytecode| {
                let mut observer = Rejecting {
                    remaining: limit,
                    events: Vec::new(),
                };
                let (result, usage) = {
                    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
                    session.execution_observer = Some(&mut observer);
                    let result = if bytecode {
                        session.bytecode_expr(0, AT, &program)
                    } else {
                        session.expr(0, AT, &tree)
                    }
                    .map(snapshot);
                    session.execution_observer = None;
                    assert_eq!(
                        session.expr(0, AT, &Expr::Number(17.)).map(snapshot),
                        Ok("scalar Number(17.0)".into())
                    );
                    (result, session.budget.usage.clone())
                };
                (result, usage, observer.events)
            };
            assert_eq!(run(false), run(true), "observer limit {limit}");
        }
    }
    #[test]
    fn bytecode_vm_without_execution_observer_preserves_budget_and_unwind_parity() {
        let workbook = workbook();
        for source in [
            "1+2*3",
            "SUM(A1:B1)+IFERROR(1/0,2)",
            "IF(FALSE,UNKNOWN(),IF(TRUE,7,1/0))",
            "{1,2;3,4}",
        ] {
            let syntax = poirot_formula::parse_formula(source).unwrap();
            let tree = bound::bind(&syntax);
            let program = bytecode::compile(&syntax).unwrap();
            let budgets = std::iter::once(EvaluationBudgets::default())
                .chain((1..65).map(|run_work| EvaluationBudgets {
                    run_work,
                    ..EvaluationBudgets::default()
                }))
                .chain((1..7).map(|depth| EvaluationBudgets {
                    depth,
                    ..EvaluationBudgets::default()
                }))
                .chain((1..7).map(|materialized_values| EvaluationBudgets {
                    materialized_values,
                    ..EvaluationBudgets::default()
                }));
            for limits in budgets {
                let run = |bytecode| {
                    let mut session = Session::new(
                        &workbook,
                        EvaluationInputs::default(),
                        BudgetMeter::new(limits).unwrap(),
                    );
                    session.budget.push_cell(EvaluationTarget {
                        sheet: 0,
                        address: AT,
                    });
                    assert!(session.execution_observer.is_none());
                    let result = if bytecode {
                        session.bytecode_expr(0, AT, &program)
                    } else {
                        session.expr(0, AT, &tree)
                    }
                    .map(snapshot);
                    let usage = session.budget.usage.clone();
                    // A depth failure must unwind every admitted frame before the
                    // next expression. Exhausted work remains exhausted in both paths.
                    let next = session.expr(0, AT, &Expr::Number(17.)).map(snapshot);
                    (result, usage, next, session.budget.usage.clone())
                };
                assert_eq!(run(false), run(true), "{source}, {limits:?}");
            }
        }
    }
}
