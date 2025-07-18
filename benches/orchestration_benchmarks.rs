use criterion::{criterion_group, criterion_main, Criterion};
use tasker_core::TaskerConfig;

fn benchmark_config_creation(c: &mut Criterion) {
    c.bench_function("config_creation", |b| b.iter(TaskerConfig::default));
}

criterion_group!(benches, benchmark_config_creation);
criterion_main!(benches);
