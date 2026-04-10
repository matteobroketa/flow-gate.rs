use flow_gate_core::{
    EllipsoidDimension, EllipsoidGate, EventMatrix, FASinhTransform, Gate, GateId,
    HyperlogTransform, LinearTransform, LogarithmicTransform, MatrixLayout, MatrixView,
    ParameterName, Transform,
};

fn assert_monotone_over_range<T: Transform>(transform: &T, start: f64, end: f64, steps: usize) {
    let step = (end - start) / steps as f64;
    let mut last = transform.apply(start);
    for i in 1..=steps {
        let x = start + step * i as f64;
        let y = transform.apply(x);
        assert!(
            y + 1e-12 >= last || (y.is_nan() && last.is_nan()),
            "non-monotone at x={x}: y={y}, last={last}"
        );
        last = y;
    }
}

#[test]
fn transforms_monotone_over_10000_points() {
    let fasinh = FASinhTransform::new(262_144.0, 4.5, 0.0).expect("fasinh");
    assert_monotone_over_range(&fasinh, -262_144.0, 262_144.0, 10_000);

    let linear = LinearTransform::new(262_144.0, 1000.0).expect("linear");
    assert_monotone_over_range(&linear, -1_000.0, 262_144.0, 10_000);

    let log = LogarithmicTransform::new(262_144.0, 4.5).expect("log");
    assert_monotone_over_range(&log, f64::MIN_POSITIVE, 262_144.0, 10_000);

    let hyperlog = HyperlogTransform::new(262_144.0, 0.5, 4.5, 0.0).expect("hyperlog");
    assert_monotone_over_range(&hyperlog, -262_144.0, 262_144.0, 10_000);
}

#[test]
fn ellipsoid_contains_rejects_infinities() {
    let gate = EllipsoidGate::new(
        GateId::from("e1"),
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
        2.0,
    )
    .expect("gate");

    assert!(!gate.contains(&[f64::INFINITY, 0.0]));
    assert!(!gate.contains(&[0.0, f64::NEG_INFINITY]));
}

#[test]
fn from_view_supports_row_major_numpy_layout() {
    let data = [1.0_f64, 2.0, 3.0, 4.0, 5.0, 6.0];
    // 2x3 row-major:
    // [1,2,3]
    // [4,5,6]
    // SAFETY: data points to 6 valid f64 values for the entire test scope.
    let view = unsafe { MatrixView::from_raw(data.as_ptr(), 2, 3, MatrixLayout::RowMajor) };
    let matrix = EventMatrix::from_view(
        view,
        vec![
            ParameterName::from("A"),
            ParameterName::from("B"),
            ParameterName::from("C"),
        ],
    )
    .expect("matrix view");
    assert_eq!(matrix.value_at(0, 0).unwrap(), 1.0);
    assert_eq!(matrix.value_at(0, 2).unwrap(), 3.0);
    assert_eq!(matrix.value_at(1, 0).unwrap(), 4.0);
    assert_eq!(matrix.value_at(1, 2).unwrap(), 6.0);
}

#[test]
fn from_view_supports_col_major_r_layout() {
    let data = [1.0_f64, 4.0, 2.0, 5.0, 3.0, 6.0];
    // 2x3 column-major:
    // [1,2,3]
    // [4,5,6]
    // SAFETY: data points to 6 valid f64 values for the entire test scope.
    let view = unsafe { MatrixView::from_raw(data.as_ptr(), 2, 3, MatrixLayout::ColMajor) };
    let matrix = EventMatrix::from_view(
        view,
        vec![
            ParameterName::from("A"),
            ParameterName::from("B"),
            ParameterName::from("C"),
        ],
    )
    .expect("matrix view");
    assert_eq!(matrix.value_at(0, 0).unwrap(), 1.0);
    assert_eq!(matrix.value_at(0, 2).unwrap(), 3.0);
    assert_eq!(matrix.value_at(1, 0).unwrap(), 4.0);
    assert_eq!(matrix.value_at(1, 2).unwrap(), 6.0);
}
