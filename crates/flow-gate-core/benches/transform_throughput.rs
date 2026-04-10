use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use flow_gate_core::{FASinhTransform, LogicleParams, LogicleTransform, Transform};

fn bench_fasinh_apply_batch(c: &mut Criterion) {
    let transform = FASinhTransform::new(262_144.0, 4.5, 0.0).expect("fasinh");
    let input: Vec<f64> = (0..1_000_000).map(|i| i as f64 - 500_000.0).collect();
    let mut out = vec![0.0_f64; input.len()];

    let mut group = c.benchmark_group("transforms");
    group.throughput(Throughput::Elements(input.len() as u64));
    group.bench_function("fasinh_apply_batch_1m", |b| {
        b.iter(|| transform.apply_batch(black_box(&input), black_box(&mut out)))
    });
    group.finish();
}

fn bench_logicle_apply_batch(c: &mut Criterion) {
    let transform = LogicleTransform {
        params: LogicleParams {
            t: 262_144.0,
            w: 0.5,
            m: 4.5,
            a: 0.0,
        },
    };
    let input: Vec<f64> = (0..1_000_000).map(|i| i as f64 - 250_000.0).collect();
    let mut out = vec![0.0_f64; input.len()];

    let mut group = c.benchmark_group("transforms");
    group.throughput(Throughput::Elements(input.len() as u64));
    group.bench_function("logicle_apply_batch_1m", |b| {
        b.iter(|| transform.apply_batch(black_box(&input), black_box(&mut out)))
    });
    group.finish();
}

criterion_group!(benches, bench_fasinh_apply_batch, bench_logicle_apply_batch);
criterion_main!(benches);
