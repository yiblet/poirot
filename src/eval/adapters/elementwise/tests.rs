use super::argument_role::ArgumentRole;
use super::broadcast::Broadcast;
use super::fixed_scalar::FixedScalar;
use super::member_context::MemberContext;
use super::optional::Optional;
use super::tuple_signature::Signature;
use super::whole_collection::WholeCollection;
use super::with_demand::{Deferred, WithDemand};
use crate::eval::compile;
use crate::eval::config::EvaluationInputs;
use crate::eval::host::{CallHost, SelectionAccess};
use crate::eval::session::Session;
use crate::eval::values::Value;
use crate::model::CellValue::{Blank, Number};
use crate::model::{CellAddress, Workbook};

#[test]
fn absent_optional_deferred_shapes_do_not_admit_or_prepare_sources() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let arguments = vec![compile::bound::bind(
        &poirot_parsing::formula::parse_formula("{1;2}").unwrap(),
    )];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host =
        CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
    /// Trailing optional roles retain absence outside their demand policy.
    type Roles = (
        Broadcast,
        Optional<Deferred<Broadcast>>,
        Optional<Deferred<FixedScalar>>,
        Optional<Deferred<WholeCollection>>,
    );
    let (lift, (_, broadcast, scalar, collection)) = Roles::admit(&mut host).unwrap();
    assert_eq!(lift.dimensions(), (2, 1));
    assert_eq!(broadcast.argument(), 1);
    assert_eq!(scalar.argument(), 2);
    assert_eq!(collection.argument(), 3);
    assert!(broadcast.0.is_none());
    assert!(scalar.0.is_none());
    assert!(collection.0.is_none());
}

#[test]
fn optional_deferred_shapes_keep_omission_present_and_preserve_value_channels() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let blank_reference =
        compile::bound::bind(&poirot_parsing::formula::parse_formula("A1").unwrap());
    // Each shape keeps omission present; selectors inspect omission before preparation.
    for (case, arguments) in [
        vec![compile::bound::Expression::Missing; 3],
        vec![blank_reference; 3],
        vec![compile::bound::Expression::Number(7.0); 3],
    ]
    .into_iter()
    .enumerate()
    {
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        /// Optional roles distinguish absent slots from explicitly supplied sources.
        type Roles = (
            Optional<Deferred<Broadcast>>,
            Optional<Deferred<FixedScalar>>,
            Optional<Deferred<WholeCollection>>,
        );
        let (lift, (broadcast, scalar, collection)) = Roles::admit(&mut host).unwrap();
        assert_eq!(lift.dimensions(), (1, 1));
        assert_eq!(
            SelectionAccess::new(&mut host).is_missing(broadcast.argument()),
            case == 0
        );
        assert_eq!(
            SelectionAccess::new(&mut host).is_missing(scalar.argument()),
            case == 0
        );
        assert_eq!(
            SelectionAccess::new(&mut host).is_missing(collection.argument()),
            case == 0
        );
        let values = [
            broadcast
                .0
                .unwrap()
                .prepare(&mut host)
                .unwrap()
                .into_value(),
            scalar.0.unwrap().prepare(&mut host).unwrap().into_value(),
            collection
                .0
                .unwrap()
                .prepare(&mut host)
                .unwrap()
                .into_value(),
        ];
        for value in values {
            assert!(match case {
                // The host's ordinary expression path projects omission to blank.
                0 => matches!(value, Value::Scalar(Blank)),
                1 => matches!(value, Value::Refs(_)),
                2 => matches!(value, Value::Scalar(Number(7.0))),
                _ => unreachable!(),
            });
        }
    }
}

#[test]
fn eager_fixed_and_collection_shapes_do_not_contribute_geometry_or_shift_broadcast_ordinals() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let arguments: Vec<_> = ["{1;2}", "{3;4;5}", "{6,7,8}", "{10,20}"]
        .into_iter()
        .map(|formula| {
            compile::bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
        })
        .collect();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host =
        CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
    /// Only the first and fourth eager shapes contribute broadcast members.
    type Roles = (
        Broadcast,
        WithDemand<FixedScalar>,
        WithDemand<WholeCollection>,
        Broadcast,
    );
    let (lift, (first, scalar, collection, last)) = Roles::admit(&mut host).unwrap();
    assert_eq!(lift.dimensions(), (2, 2));
    assert_eq!(scalar.role().argument(), 1);
    assert_eq!(collection.role().argument(), 2);
    let mut context = MemberContext {
        host: &mut host,
        lift: &lift,
        row: 1,
        column: 1,
    };
    assert_eq!(first.read(&mut context).unwrap().value, Number(2.0));
    assert_eq!(last.read(&mut context).unwrap().value, Number(20.0));
}

#[test]
fn deferred_and_nonbroadcast_roles_do_not_shift_eager_member_ordinals() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let arguments: Vec<_> = ["{1;2}", "RAND()", "{3;4;5}", "{10,20}"]
        .into_iter()
        .map(|formula| {
            compile::bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
        })
        .collect();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    {
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        /// Deferred broadcast and fixed roles leave eager member ordinals contiguous.
        type Roles = (Broadcast, Deferred<Broadcast>, FixedScalar, Broadcast);
        let (lift, (first, deferred, fixed, last)) = Roles::admit(&mut host).unwrap();
        assert_eq!(lift.dimensions(), (2, 2));
        assert_eq!(deferred.argument(), 1);
        assert_eq!(fixed.argument(), 2);
        let mut context = MemberContext {
            host: &mut host,
            lift: &lift,
            row: 1,
            column: 1,
        };
        assert_eq!(first.read(&mut context).unwrap().value, Number(2.0));
        assert_eq!(last.read(&mut context).unwrap().value, Number(20.0));
    }
    assert!(
        session
            .state
            .runtime
            .budget
            .usage
            .random_accesses
            .is_empty()
    );
}

#[test]
fn demand_is_orthogonal_to_shape_and_deferred_broadcast_skips_admission() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let arguments: Vec<_> = ["{1;2}", "RAND()", "RAND()", "RAND()"]
        .into_iter()
        .map(|formula| {
            compile::bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
        })
        .collect();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    {
        let mut host =
            CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
        /// One eager broadcast and three independently deferred argument shapes.
        type Roles = (
            WithDemand<Broadcast>,
            Deferred<Broadcast>,
            Deferred<FixedScalar>,
            Deferred<WholeCollection>,
        );
        let (lift, (eager, broadcast, scalar, collection)) = Roles::admit(&mut host).unwrap();
        assert_eq!(lift.dimensions(), (2, 1));
        assert_eq!(eager.argument(), 0);
        assert_eq!(broadcast.argument(), 1);
        assert_eq!(scalar.argument(), 2);
        assert_eq!(collection.argument(), 3);
        // No deferred shape was admitted: none of the three RAND expressions ran.
        let _ = (broadcast, scalar, collection);
    }
    assert!(
        session
            .state
            .runtime
            .budget
            .usage
            .random_accesses
            .is_empty()
    );
    let mut host =
        CallHost::new_inheriting(&mut session, 0, CellAddress { row: 0, col: 0 }, &arguments);
    let broadcast = Deferred::<Broadcast>::bind(1, 0, true)
        .prepare(&mut host)
        .unwrap();
    assert_eq!(broadcast.shape().unwrap().rows, 1);
    let scalar = Deferred::<FixedScalar>::bind(2, 0, true)
        .prepare(&mut host)
        .unwrap();
    let collection = Deferred::<WholeCollection>::bind(3, 0, true)
        .prepare(&mut host)
        .unwrap();
    for value in [
        broadcast.into_value(),
        scalar.into_value(),
        collection.into_value(),
    ] {
        assert!(matches!(value, Value::Scalar(Number(n)) if (0.0..1.0).contains(&n)));
    }
    drop(host);
    assert_eq!(
        session.state.runtime.budget.usage.random_accesses[0].draws,
        3
    );
}
