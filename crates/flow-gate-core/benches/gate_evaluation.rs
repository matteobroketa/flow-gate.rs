use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use flow_gate_core::{
    gate::GateKind, EventMatrix, GateId, GateRegistry, ParameterName, RectangleDimension,
    RectangleGate,
};
use indexmap::IndexMap;

fn bench_rectangle_classify(c: &mut Criterion) {
    let n_events = 1_000_000usize;
    let x: Vec<f64> = (0..n_events).map(|i| (i % 2000) as f64 - 1000.0).collect();
    let y: Vec<f64> = (0..n_events).map(|i| (i % 3000) as f64 - 1500.0).collect();
    let matrix = EventMatrix::from_columns(
        vec![x, y],
        vec![ParameterName::from("X"), ParameterName::from("Y")],
    )
    .expect("matrix");

    let gate = RectangleGate::new(
        GateId::from("R1"),
        None,
        vec![
            RectangleDimension {
                parameter: ParameterName::from("X"),
                transform: None,
                min: Some(-100.0),
                max: Some(100.0),
            },
            RectangleDimension {
                parameter: ParameterName::from("Y"),
                transform: None,
                min: Some(-100.0),
                max: Some(100.0),
            },
        ],
    )
    .expect("gate");
    let mut gates = IndexMap::new();
    gates.insert(GateId::from("R1"), GateKind::Rectangle(gate));
    let registry = GateRegistry::new(gates).expect("registry");

    let mut group = c.benchmark_group("gates");
    group.throughput(Throughput::Elements(n_events as u64));
    group.bench_function("rectangle_classify_2d_1m", |b| {
        b.iter(|| black_box(registry.classify_all(black_box(&matrix)).expect("classify")))
    });
    group.finish();
}

criterion_group!(benches, bench_rectangle_classify);
criterion_main!(benches);
