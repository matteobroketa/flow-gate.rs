use flow_gate_core::{
    FASinhTransform, HyperlogTransform, LinearTransform, LogarithmicTransform, Transform,
};

#[test]
fn logarithmic_nonpositive_is_nan() {
    let t = LogarithmicTransform::new(262_144.0, 4.5).expect("valid transform");
    for i in -1000..=0 {
        let x = i as f64;
        assert!(
            t.apply(x).is_nan(),
            "expected NaN for x={x}, got {}",
            t.apply(x)
        );
    }
}

#[test]
fn logarithmic_t_maps_to_one() {
    let t = LogarithmicTransform::new(262_144.0, 4.5).expect("valid transform");
    assert!((t.apply(262_144.0) - 1.0).abs() <= 1e-15);
}

#[test]
fn linear_endpoints() {
    let t = LinearTransform::new(200.0, 50.0).expect("valid linear transform");
    assert!((t.apply(-50.0) - 0.0).abs() <= 1e-15);
    assert!((t.apply(200.0) - 1.0).abs() <= 1e-15);
}

#[test]
fn fasinh_roundtrip_sample() {
    let t = FASinhTransform::new(262_144.0, 4.5, 0.0).expect("valid fasinh transform");
    let points = [-200_000.0, -1_000.0, -1.0, 0.0, 1.0, 1_000.0, 200_000.0];
    for x in points {
        let y = t.apply(x);
        let x2 = t.invert(y);
        let rel = (x - x2).abs() / x.abs().max(1.0);
        assert!(
            rel < 1e-9,
            "round-trip rel error too large for x={x}: {rel:e}"
        );
    }
}

#[test]
fn fasinh_non_finite_is_nan() {
    let t = FASinhTransform::new(262_144.0, 4.5, 0.0).expect("valid fasinh transform");
    assert!(t.apply(f64::NAN).is_nan());
    assert!(t.apply(f64::INFINITY).is_nan());
    assert!(t.apply(f64::NEG_INFINITY).is_nan());
}

#[test]
fn hyperlog_roundtrip_sample() {
    let t = HyperlogTransform::new(262_144.0, 0.5, 4.5, 0.0).expect("valid hyperlog transform");
    let points = [-20_000.0, -500.0, -2.0, 0.0, 2.0, 500.0, 20_000.0];
    for x in points {
        let y = t.apply(x);
        let x2 = t.invert(y);
        let rel = (x - x2).abs() / x.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "hyperlog round-trip rel error too large for x={x}: {rel:e}"
        );
    }
}
