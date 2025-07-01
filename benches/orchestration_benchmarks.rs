use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tasker_core::TaskerConfig;

fn benchmark_config_creation(c: &mut Criterion) {
    c.bench_function("config_creation", |b| {
        b.iter(|| TaskerConfig::default())
    });
}

fn benchmark_config_from_env(c: &mut Criterion) {
    c.bench_function("config_from_env", |b| {
        b.iter(|| TaskerConfig::from_env())
    });
}

criterion_group!(benches, benchmark_config_creation, benchmark_config_from_env);
criterion_main!(benches);