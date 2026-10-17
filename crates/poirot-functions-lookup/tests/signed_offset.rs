use poirot_functions_lookup::*;
/// Test host recording signed OFFSET reads and off-grid behavior.
#[derive(Default)]
struct Book {
    reads: Vec<(u32, u32)>,
}
impl Workbook for Book {
    fn preflight(&mut self, _: BudgetRequest) -> Result<(), Failure> {
        Ok(())
    }
    fn argument(&mut self, _: usize, _: Context) -> Result<Value, Failure> {
        Err(Error::Value.into())
    }
    fn coerce_numeric_text(&mut self, _: &str) -> Result<f64, Failure> {
        Err(Error::Value.into())
    }
    fn coerce_number_to_text(&mut self, n: f64) -> Result<String, Failure> {
        Ok(n.to_string())
    }
    fn cell(&mut self, r: &Reference, _: u32, row: u32, col: u32) -> Result<Scalar, Failure> {
        assert!((r.first_row..=r.last_row).contains(&row));
        assert!((r.first_col..=r.last_col).contains(&col));
        self.reads.push((row, col));
        Ok(Scalar::Number((row * 10 + col) as f64))
    }
    fn signed_cell(
        &mut self,
        r: &SignedReference,
        _: u32,
        row: i32,
        col: i32,
    ) -> Result<Scalar, Failure> {
        assert!((r.first_row..=r.last_row).contains(&row));
        assert!((r.first_col..=r.last_col).contains(&col));
        assert!(row >= 0 && col >= 0);
        self.reads.push((row as u32, col as u32));
        Ok(Scalar::Number((row * 10 + col) as f64))
    }
    fn sheet_index(&mut self, _: &str) -> Result<Option<u32>, Failure> {
        Ok(None)
    }
    fn dynamic_reference(&mut self, _: &DynamicReference, _: Context) -> Result<Value, Failure> {
        Err(Error::Ref.into())
    }
    fn structured_reference(&mut self, _: &str, _: Context) -> Result<Value, Failure> {
        Err(Error::Ref.into())
    }
}
const C: Context = Context {
    sheet: 2,
    row: 0,
    col: 0,
};
fn n(n: f64) -> Value {
    Value::Scalar(Scalar::Number(n))
}
fn base() -> Value {
    Value::Reference(Reference {
        kind: ReferenceKind::Cell,
        workbook: Some("book".into()),
        first_sheet: 2,
        last_sheet: 2,
        first_row: 2,
        last_row: 2,
        first_col: 1,
        last_col: 1,
    })
}
fn signed(w: &mut Book) -> Value {
    evaluate(
        Function::Reference(ReferenceFunction::Offset),
        &[base(), n(0.), n(0.), n(-2.), n(-2.)],
        C,
        w,
    )
    .unwrap()
}
#[test]
fn signed_offset_preserves_original_endpoints_grid_projection_and_identity_without_reads() {
    let mut w = Book::default();
    let value = signed(&mut w);
    let Value::SignedReference(r) = &value else {
        panic!("signed reference required");
    };
    assert_eq!(
        (r.first_row, r.last_row, r.first_col, r.last_col),
        (-1, 2, -2, 1)
    );
    assert_eq!((r.height(), r.width()), (4, 4));
    assert_eq!(r.workbook.as_deref(), Some("book"));
    assert_eq!(r.first_sheet, 2);
    assert!(w.reads.is_empty());
    let nested = evaluate(
        Function::Reference(ReferenceFunction::Offset),
        &[value, n(2.), n(2.), n(2.), n(2.)],
        C,
        &mut w,
    )
    .unwrap();
    let Value::Reference(r) = nested else {
        panic!("nested reference is now in grid");
    };
    assert_eq!(
        (r.first_row, r.last_row, r.first_col, r.last_col),
        (1, 2, 0, 1)
    );
    assert_eq!(r.workbook.as_deref(), Some("book"));
    assert!(w.reads.is_empty());
}
#[test]
fn signed_table_vector_reads_blank_padding_without_out_of_grid_port_calls() {
    let mut w = Book::default();
    let value = evaluate(
        Function::Reference(ReferenceFunction::Offset),
        &[base(), n(0.), n(0.), n(-2.), n(1.)],
        C,
        &mut w,
    )
    .unwrap();
    assert_eq!(
        evaluate(
            Function::Search(Search::XMatch),
            &[n(11.), value, n(0.)],
            C,
            &mut w
        ),
        Ok(n(3.))
    );
    assert_eq!(w.reads, [(0, 1), (1, 1), (2, 1)]);
}
#[test]
fn source_low_bounds_reject_but_short_narrowing_retains_logical_reference() {
    let mut w = Book::default();
    assert_eq!(
        evaluate(
            Function::Reference(ReferenceFunction::Offset),
            &[base(), n(-3.), n(0.), n(-2.), n(-2.)],
            C,
            &mut w
        ),
        Err(Error::Ref.into())
    );
    assert!(
        matches!(evaluate(Function::Reference(ReferenceFunction::Offset),
        &[base(), n(32768.), n(0.)], C, &mut w), Ok(Value::SignedReference(r))
        if (r.first_row, r.last_row, r.first_col, r.last_col) == (-32766,-32766,1,1))
    );
    assert!(w.reads.is_empty());
}

#[test]
fn signed_reference_dto_keeps_unsigned_reference_layout() {
    assert_eq!(
        std::mem::size_of::<SignedReference>(),
        std::mem::size_of::<Reference>()
    );
    assert_eq!(
        std::mem::align_of::<SignedReference>(),
        std::mem::align_of::<Reference>()
    );
}
#[test]
fn public_extreme_base_wrapped_endpoints_default_shape_and_blank_reads() {
    let mut w = Book::default();
    let base = Value::SignedReference(SignedReference {
        kind: ReferenceKind::Area,
        workbook: None,
        first_sheet: 2,
        last_sheet: 2,
        first_row: i32::MAX,
        last_row: i32::MAX,
        first_col: 0,
        last_col: 0,
    });
    let value = evaluate(
        Function::Reference(ReferenceFunction::Offset),
        &[base, n(0.), n(0.), n(2.), n(1.)],
        C,
        &mut w,
    )
    .unwrap();
    let Value::SignedReference(r) = &value else {
        panic!()
    };
    assert_eq!(
        (r.first_row, r.last_row, r.source_height()),
        (i32::MAX, i32::MIN, 2)
    );
    assert_eq!(
        evaluate(
            Function::Search(Search::XMatch),
            &[n(0.), value, n(0.)],
            C,
            &mut w
        ),
        Err(Error::Na.into())
    );
    assert!(w.reads.is_empty());
    let base = Value::SignedReference(SignedReference {
        kind: ReferenceKind::Area,
        workbook: None,
        first_sheet: 2,
        last_sheet: 2,
        first_row: -32768,
        last_row: -32767,
        first_col: 0,
        last_col: 0,
    });
    assert!(
        matches!(evaluate(Function::Reference(ReferenceFunction::Offset), &[base,n(32768.),n(0.)],C,&mut w),
        Ok(Value::SignedReference(r)) if (r.first_row,r.last_row)==(-65536,-65535))
    );
}
#[test]
fn upper_logical_padding_and_integer_coercion_do_not_request_physical_cells() {
    let mut w = Book::default();
    let upper = Value::SignedReference(SignedReference {
        kind: ReferenceKind::Area,
        workbook: None,
        first_sheet: 2,
        last_sheet: 2,
        first_row: 1_048_576,
        last_row: 1_048_576,
        first_col: 0,
        last_col: 0,
    });
    assert_eq!(
        evaluate(
            Function::Search(Search::XMatch),
            &[n(0.), upper, n(0.)],
            C,
            &mut w
        ),
        Err(Error::Na.into())
    );
    for argument in [
        f64::NAN,
        f64::INFINITY,
        (i32::MAX as f64) + 1.,
        (i32::MIN as f64) - 1.,
    ] {
        assert_eq!(
            evaluate(
                Function::Reference(ReferenceFunction::Offset),
                &[base(), n(argument), n(0.)],
                C,
                &mut w
            ),
            Err(Error::Num.into())
        );
    }
    assert!(w.reads.is_empty());
}
