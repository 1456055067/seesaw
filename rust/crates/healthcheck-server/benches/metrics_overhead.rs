// Benchmark to measure metrics overhead
// Compare performance with metrics enabled vs disabled

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use healthcheck_server::metrics::MetricsRegistry;
use std::sync::Arc;
use std::time::Duration;

fn bench_metrics_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics_recording");

    // Benchmark with metrics disabled (None)
    group.bench_function("disabled", |b| {
        let metrics: Option<Arc<MetricsRegistry>> = None;
        b.iter(|| {
            // Simulate typical metric recording pattern from manager.rs
            if let Some(ref m) = metrics {
                m.record_check(black_box(1), black_box("tcp"), black_box("success"), black_box(Duration::from_millis(10)));
                m.update_state(black_box(1), black_box("tcp"), black_box(true));
                m.update_consecutive(black_box(1), black_box("tcp"), black_box(5), black_box(0));
            }
        });
    });

    // Benchmark with metrics enabled
    group.bench_function("enabled", |b| {
        let metrics = Some(Arc::new(MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        )));

        b.iter(|| {
            if let Some(ref m) = metrics {
                m.record_check(black_box(1), black_box("tcp"), black_box("success"), black_box(Duration::from_millis(10)));
                m.update_state(black_box(1), black_box("tcp"), black_box(true));
                m.update_consecutive(black_box(1), black_box("tcp"), black_box(5), black_box(0));
            }
        });
    });

    group.finish();
}

fn bench_individual_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("individual_operations");
    let registry = Arc::new(MetricsRegistry::new(
        &[0.001, 0.01, 0.1, 1.0],
        &[0.01, 0.1, 1.0],
        &[1.0, 10.0, 100.0],
    ));

    group.bench_function("record_check", |b| {
        b.iter(|| {
            registry.record_check(
                black_box(1),
                black_box("tcp"),
                black_box("success"),
                black_box(Duration::from_millis(10)),
            );
        });
    });

    group.bench_function("update_state", |b| {
        b.iter(|| {
            registry.update_state(black_box(1), black_box("tcp"), black_box(true));
        });
    });

    group.bench_function("update_consecutive", |b| {
        b.iter(|| {
            registry.update_consecutive(black_box(1), black_box("tcp"), black_box(5), black_box(0));
        });
    });

    group.bench_function("record_state_transition", |b| {
        b.iter(|| {
            use healthcheck_server::types::State;
            registry.record_state_transition(
                black_box(1),
                black_box("tcp"),
                black_box(State::Unhealthy),
                black_box(State::Healthy),
            );
        });
    });

    group.bench_function("record_batch_sent", |b| {
        b.iter(|| {
            registry.record_batch_sent(
                black_box(10),
                black_box("size_limit"),
                black_box(Duration::from_millis(50)),
            );
        });
    });

    group.bench_function("update_monitor_count", |b| {
        b.iter(|| {
            registry.update_monitor_count(black_box(42));
        });
    });

    group.finish();
}

fn bench_concurrent_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");
    let registry = Arc::new(MetricsRegistry::new(
        &[0.001, 0.01, 0.1, 1.0],
        &[0.01, 0.1, 1.0],
        &[1.0, 10.0, 100.0],
    ));

    // Simulate concurrent access from multiple healthchecks
    for num_concurrent in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_concurrent),
            num_concurrent,
            |b, &num| {
                b.iter(|| {
                    for id in 0..num {
                        registry.record_check(
                            black_box(id as u64),
                            black_box("tcp"),
                            black_box("success"),
                            black_box(Duration::from_millis(10)),
                        );
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_metrics_recording,
    bench_individual_operations,
    bench_concurrent_recording
);
criterion_main!(benches);
