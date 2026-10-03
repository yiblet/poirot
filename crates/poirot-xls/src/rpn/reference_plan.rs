//! Immutable reference-subexpression plan; one classification walk per algebra subtree.
//! Static tArea fastpath remains exact; dynamic algebra uses pinned Mem wrappers.
use super::*;
/// Resolved static rectangle retained while selecting reference-class BIFF tokens.
#[derive(Clone, Copy)]
pub(super) struct Area {
    sheet: Option<usize>,
    r0: u16,
    r1: u16,
    c0: u16,
    c1: u16,
}

/// Reference expression plan separating static geometry, operands, and reference algebra.
pub(super) enum ReferencePlan<'a> {
    /// Statically resolved reference with directly encodable geometry.
    Static {
        /// Original syntax whose BIFF reference semantics must be retained.
        expression: &'a Expr,
        /// Known reference geometry when it can be inferred.
        area: Area,
    },
    /// Reference-valued operand requiring expression emission.
    Operand {
        /// Original syntax whose BIFF reference semantics must be retained.
        expression: &'a Expr,
        /// Known reference geometry when it can be inferred.
        area: Option<Area>,
        /// Whether BIFF memory-function framing is required.
        mem_func: bool,
    },
    /// Range, union, or intersection composed from reference subplans.
    Algebra {
        /// Original syntax whose BIFF reference semantics must be retained.
        expression: &'a Expr,
        /// Reference-combination operation to encode.
        op: BinaryOp,
        /// Left reference subplan.
        left: Box<Self>,
        /// Right reference subplan.
        right: Box<Self>,
        /// Whether BIFF memory-function framing is required.
        mem_func: bool,
    },
}
impl<'a> ReferencePlan<'a> {
    /// Validate reference syntax and infer supported static geometry.
    pub(super) fn new(w: &Workbook, expression: &'a Expr) -> Result<Self, XlsError> {
        if let Expr::Binary {
            op: op @ (BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection),
            left,
            right,
        } = expression
        {
            let left = Self::new(w, left)?;
            let right = Self::new(w, right)?;
            let area = if *op == BinaryOp::Range {
                match (left.area(), right.area()) {
                    (Some(a), Some(b)) => Some(merge(a, b)?),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(area) = area {
                return Ok(Self::Static { expression, area });
            }
            let mem_func = left.mem_func() || right.mem_func();
            Ok(Self::Algebra {
                expression,
                op: *op,
                left: Box::new(left),
                right: Box::new(right),
                mem_func,
            })
        } else {
            let area = primitive(w, expression)?;
            let mem_func = match expression {
                Expr::Function { .. }
                | Expr::Name(_)
                | Expr::QualifiedName { .. }
                | Expr::SheetSpan(_) => true,
                Expr::Cell(cell) => cell.sheet.is_some(),
                _ => false,
            };
            Ok(Self::Operand {
                expression,
                area,
                mem_func,
            })
        }
    }
    fn area(&self) -> Option<Area> {
        match self {
            Self::Static { area, .. } => Some(*area),
            Self::Operand { area, .. } => *area,
            Self::Algebra { .. } => None,
        }
    }
    fn mem_func(&self) -> bool {
        match self {
            Self::Static { area, .. } => area.sheet.is_some(),
            Self::Operand { mem_func, .. } | Self::Algebra { mem_func, .. } => *mem_func,
        }
    }
    fn expression(&self) -> &Expr {
        match self {
            Self::Static { expression, .. }
            | Self::Operand { expression, .. }
            | Self::Algebra { expression, .. } => expression,
        }
    }
}
/// Emit the selected BIFF reference token structure from a validated plan.
pub(super) fn emit_plan(
    w: &Workbook,
    s: usize,
    plan: &ReferencePlan<'_>,
    class: u8,
    budget: usize,
    addins: &addins::AddInPlan,
    out: &mut Vec<u8>,
) -> Result<(), XlsError> {
    match plan {
        ReferencePlan::Operand { expression, .. } => {
            emit(w, s, expression, class, budget, addins, out)?
        }
        ReferencePlan::Static { area, .. } => {
            if let Some(sheet) = area.sheet {
                out.push(0x3b + class);
                short(out, sheet as u16);
            } else {
                out.push(0x25 + class);
            }
            short(out, area.r0);
            short(out, area.r1);
            short(out, area.c0);
            short(out, area.c1);
        }
        ReferencePlan::Algebra {
            op,
            left,
            right,
            mem_func,
            ..
        } => {
            let start = out.len();
            let prefix = if *mem_func { 3 } else { 7 };
            if start.checked_add(prefix).is_none_or(|n| n > budget) {
                return Err(XlsError::ResourceLimit(format!(
                    "formula token bytes exceeds export limit {budget}"
                )));
            }
            // MemFunc sid=0x29 has u16 length; MemArea sid=0x26 has reserved u32 then length.
            out.push(if *mem_func { 0x29 } else { 0x26 });
            if !*mem_func {
                long(out, 0);
            }
            short(out, 0);
            for (right_side, child_plan) in [(false, left.as_ref()), (true, right.as_ref())] {
                emit_plan(w, s, child_plan, 0, budget, addins, out)?;
                if needs_parentheses(
                    child_plan.expression(),
                    GroupingContext::BinaryChild {
                        parent: *op,
                        right: right_side,
                    },
                ) {
                    out.push(0x15);
                }
                check_budget(out, budget)?;
            }
            out.push(match op {
                BinaryOp::Range => 0x11,
                BinaryOp::Union => 0x10,
                BinaryOp::Intersection => 0x0f,
                _ => return Err(fail("unsupported reference operator")),
            });
            let length = u16::try_from(out.len() - start - prefix)
                .map_err(|_| fail("reference subexpression exceeds BIFF length"))?;
            // SAFETY: This function appends the two-byte placeholder after start and never removes bytes; start + prefix - 2..start + prefix is that placeholder.
            out[start + prefix - 2..start + prefix].copy_from_slice(&length.to_le_bytes());
        }
    }
    check_budget(out, budget)
}
fn primitive(w: &Workbook, e: &Expr) -> Result<Option<Area>, XlsError> {
    match e {
        Expr::Cell(c) => {
            if c.row >= 65536 || c.column >= 256 {
                return Err(fail("range endpoint exceeds XLS grid"));
            }
            let col = column(c.column as u16, c.row_absolute, c.column_absolute);
            Ok(Some(Area {
                sheet: c.sheet.as_ref().map(|n| sheet(w, n)).transpose()?,
                r0: c.row as u16,
                r1: c.row as u16,
                c0: col,
                c1: col,
            }))
        }
        Expr::Column {
            column: c,
            absolute,
            sheet: name,
        } => {
            if *c >= 256 {
                return Err(fail("range column exceeds XLS grid"));
            }
            let col = column(*c as u16, true, *absolute);
            Ok(Some(Area {
                sheet: name.as_ref().map(|n| sheet(w, n)).transpose()?,
                r0: 0,
                r1: 65535,
                c0: col,
                c1: col,
            }))
        }
        Expr::Row {
            row,
            absolute,
            sheet: name,
        } => {
            if *row >= 65536 {
                return Err(fail("range row exceeds XLS grid"));
            }
            Ok(Some(Area {
                sheet: name.as_ref().map(|n| sheet(w, n)).transpose()?,
                r0: *row as u16,
                r1: *row as u16,
                c0: column(0, *absolute, true),
                c1: column(255, *absolute, true),
            }))
        }
        _ => Ok(None),
    }
}

fn merge(a: Area, b: Area) -> Result<Area, XlsError> {
    if a.sheet != b.sheet {
        return Err(fail("range spans distinct worksheets"));
    }
    Ok(Area {
        sheet: a.sheet,
        r0: a.r0.min(b.r0),
        r1: a.r1.max(b.r1),
        c0: (if (a.c0 & 255) <= (b.c0 & 255) {
            a.c0 & 0x40ff
        } else {
            b.c0 & 0x40ff
        }) | (if a.r0 <= b.r0 {
            a.c0 & 0x8000
        } else {
            b.c0 & 0x8000
        }),
        c1: (if (a.c1 & 255) > (b.c1 & 255) {
            a.c1 & 0x40ff
        } else {
            b.c1 & 0x40ff
        }) | (if a.r1 > b.r1 {
            a.c1 & 0x8000
        } else {
            b.c1 & 0x8000
        }),
    })
}

/// Array-class parameters can transport references without implementing array constants.
/// Function selection follows immutable return metadata, never function-name spelling.
pub(super) fn array_reference(expression: &Expr) -> Result<bool, XlsError> {
    Ok(match expression {
        Expr::Cell(_) | Expr::Name(_) | Expr::QualifiedName { .. } | Expr::SheetSpan(_) => true,
        Expr::Binary {
            op: BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection,
            ..
        } => true,
        Expr::Function { name, .. } => functions::metadata(name)?
            .is_some_and(|m| m.result == functions::OperandClass::Reference),
        _ => false,
    })
}
