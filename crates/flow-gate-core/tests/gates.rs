use flow_gate_core::{
    BooleanGate, BooleanOp, BooleanOperand, EllipsoidCovariance, EllipsoidDimension, EllipsoidGate,
    EventMatrix, Gate, GateId, GateKind, GateRegistry, FlowGateError, ParameterName,
    PolygonDimension, PolygonGate, RectangleDimension, RectangleGate,
};
use indexmap::IndexMap;

#[test]
fn rectangle_rejects_nan_coords() {
    let gate = RectangleGate::new(
        GateId::from("rect"),
        None,
        vec![RectangleDimension {
            parameter: ParameterName::from("FSC-A"),
            transform: None,
            min: Some(0.0),
            max: Some(10.0),
        }],
    )
    .expect("valid rectangle");

    assert!(!gate.contains(&[f64::NAN]));
}

#[test]
fn polygon_boundary_is_inside() {
    let gate = PolygonGate::new(
        GateId::from("poly"),
        None,
        PolygonDimension {
            parameter: ParameterName::from("X"),
            transform: None,
        },
        PolygonDimension {
            parameter: ParameterName::from("Y"),
            transform: None,
        },
        vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)],
    )
    .expect("valid polygon");

    assert!(gate.contains(&[1.0, 0.0]));
}

#[test]
fn ellipsoid_mean_is_inside() {
    let gate = EllipsoidGate::new(
        GateId::from("ell"),
        None,
        vec![
            EllipsoidDimension {
                parameter: ParameterName::from("X"),
                transform: None,
            },
            EllipsoidDimension {
                parameter: ParameterName::from("Y"),
                transform: None,
            },
        ],
        vec![0.0, 0.0],
        &[1.0, 0.0, 1.0],
        1.0,
    )
    .expect("valid ellipsoid");

    assert!(gate.contains(&[0.0, 0.0]));
}

#[test]
fn boolean_not_requires_exactly_one_operand() {
    let err = BooleanGate::new(
        GateId::from("b1"),
        None,
        BooleanOp::Not,
        vec![
            BooleanOperand {
                gate_id: GateId::from("a"),
                complement: false,
            },
            BooleanOperand {
                gate_id: GateId::from("b"),
                complement: false,
            },
        ],
    )
    .expect_err("expected not arity error");

    match err {
        FlowGateError::BooleanNotArity(_, n) => assert_eq!(n, 2),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn covariance_non_positive_definite_is_error() {
    let err = EllipsoidCovariance::from_upper_triangular(&[1.0, 2.0, 1.0], 2)
        .expect_err("expected SPD error");
    assert!(matches!(err, FlowGateError::NotPositiveDefinite));
}

#[test]
fn general_ellipsoid_accepts_official_non_symmetric_matrix() {
    let gate = EllipsoidGate::new_general_covariance(
        GateId::from("ell_general"),
        None,
        vec![
            EllipsoidDimension {
                parameter: ParameterName::from("FL3-H"),
                transform: None,
            },
            EllipsoidDimension {
                parameter: ParameterName::from("FL4-H"),
                transform: None,
            },
            EllipsoidDimension {
                parameter: ParameterName::from("FL1-H"),
                transform: None,
            },
        ],
        vec![40.3, 30.6, 20.8],
        &[
            2.5, 7.5, 17.5, //
            7.5, 7.0, 13.5, //
            15.5, 13.5, 4.3,
        ],
        1.0,
    )
    .expect("general covariance must parse");

    // Center point is always inside.
    assert!(gate.contains(&[40.3, 30.6, 20.8]));
    // A distant point from official-scale values should be outside.
    assert!(!gate.contains(&[60.3, 50.6, 40.8]));
}

#[test]
fn spd_general_and_strict_modes_are_numerically_equivalent() {
    let dimensions = vec![
        EllipsoidDimension {
            parameter: ParameterName::from("X"),
            transform: None,
        },
        EllipsoidDimension {
            parameter: ParameterName::from("Y"),
            transform: None,
        },
    ];
    let mean = vec![1.5, -2.0];
    let spd_full = [5.0, 1.5, 1.5, 3.0];

    let strict = EllipsoidGate::new(
        GateId::from("strict"),
        None,
        dimensions.clone(),
        mean.clone(),
        &spd_full,
        1.0,
    )
    .expect("strict gate");
    let general = EllipsoidGate::new_general_covariance(
        GateId::from("general"),
        None,
        dimensions,
        mean,
        &spd_full,
        1.0,
    )
    .expect("general gate");

    let probe_points = [
        [1.5, -2.0],
        [2.0, -2.0],
        [0.0, 0.0],
        [4.0, 1.0],
        [10.0, -10.0],
    ];
    for point in probe_points {
        assert_eq!(
            strict.contains(&point),
            general.contains(&point),
            "classification mismatch at point {:?}",
            point
        );
    }
}

#[test]
fn rectangle_open_bounds_behave_as_infinite() {
    let gate = RectangleGate::new(
        GateId::from("rect_open"),
        None,
        vec![RectangleDimension {
            parameter: ParameterName::from("FSC-A"),
            transform: None,
            min: None,
            max: None,
        }],
    )
    .expect("valid rectangle");

    assert!(gate.contains(&[-1.0e12]));
    assert!(gate.contains(&[1.0e12]));
}

#[test]
fn polygon_requires_three_vertices() {
    let err = PolygonGate::new(
        GateId::from("poly_bad"),
        None,
        PolygonDimension {
            parameter: ParameterName::from("X"),
            transform: None,
        },
        PolygonDimension {
            parameter: ParameterName::from("Y"),
            transform: None,
        },
        vec![(0.0, 0.0), (1.0, 0.0)],
    )
    .expect_err("expected invalid polygon");

    assert!(matches!(err, FlowGateError::InvalidGate(_)));
}

#[test]
fn gates_reject_non_finite_coordinates() {
    let rectangle = RectangleGate::new(
        GateId::from("rect"),
        None,
        vec![RectangleDimension {
            parameter: ParameterName::from("X"),
            transform: None,
            min: Some(0.0),
            max: Some(10.0),
        }],
    )
    .expect("rectangle");
    assert!(!rectangle.contains(&[f64::INFINITY]));
    assert!(!rectangle.contains(&[f64::NEG_INFINITY]));
    assert!(!rectangle.contains(&[f64::NAN]));

    let polygon = PolygonGate::new(
        GateId::from("poly"),
        None,
        PolygonDimension {
            parameter: ParameterName::from("X"),
            transform: None,
        },
        PolygonDimension {
            parameter: ParameterName::from("Y"),
            transform: None,
        },
        vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)],
    )
    .expect("polygon");
    assert!(!polygon.contains(&[f64::NAN, 1.0]));
    assert!(!polygon.contains(&[1.0, f64::INFINITY]));
    assert!(!polygon.contains(&[f64::NEG_INFINITY, 1.0]));

    let ellipsoid = EllipsoidGate::new(
        GateId::from("ell"),
        None,
        vec![
            EllipsoidDimension {
                parameter: ParameterName::from("X"),
                transform: None,
            },
            EllipsoidDimension {
                parameter: ParameterName::from("Y"),
                transform: None,
            },
        ],
        vec![0.0, 0.0],
        &[1.0, 0.0, 1.0],
        1.0,
    )
    .expect("ellipsoid");
    assert!(!ellipsoid.contains(&[f64::INFINITY, 0.0]));
    assert!(!ellipsoid.contains(&[0.0, f64::NAN]));
}

#[test]
fn gate_registry_rejects_cyclic_boolean_references() {
    let a = BooleanGate::new(
        GateId::from("A"),
        None,
        BooleanOp::And,
        vec![BooleanOperand {
            gate_id: GateId::from("B"),
            complement: false,
        }],
    )
    .expect("gate A");
    let b = BooleanGate::new(
        GateId::from("B"),
        None,
        BooleanOp::And,
        vec![BooleanOperand {
            gate_id: GateId::from("A"),
            complement: false,
        }],
    )
    .expect("gate B");

    let mut gates = IndexMap::new();
    gates.insert(GateId::from("A"), GateKind::Boolean(a));
    gates.insert(GateId::from("B"), GateKind::Boolean(b));

    let err = GateRegistry::new(gates).expect_err("expected cycle error");
    assert!(matches!(err, FlowGateError::CyclicGateReference(_)));
}

#[test]
fn child_gate_membership_is_masked_by_parent() {
    let parent = RectangleGate::new(
        GateId::from("P"),
        None,
        vec![RectangleDimension {
            parameter: ParameterName::from("X"),
            transform: None,
            min: Some(0.0),
            max: Some(10.0),
        }],
    )
    .expect("parent");
    let child = RectangleGate::new(
        GateId::from("C"),
        Some(GateId::from("P")),
        vec![RectangleDimension {
            parameter: ParameterName::from("X"),
            transform: None,
            min: Some(5.0),
            max: Some(15.0),
        }],
    )
    .expect("child");
    let mut gates = IndexMap::new();
    gates.insert(GateId::from("P"), GateKind::Rectangle(parent));
    gates.insert(GateId::from("C"), GateKind::Rectangle(child));
    let registry = GateRegistry::new(gates).expect("registry");

    let matrix =
        EventMatrix::from_columns(vec![vec![4.0, 6.0, 12.0]], vec![ParameterName::from("X")])
            .expect("matrix");
    let out = registry.classify_all(&matrix).expect("classify");
    let child_bits = out.get(&GateId::from("C")).expect("child");
    assert_eq!(child_bits.len(), 3);
    assert!(!child_bits[0]);
    assert!(child_bits[1]);
    assert!(!child_bits[2]);
}
