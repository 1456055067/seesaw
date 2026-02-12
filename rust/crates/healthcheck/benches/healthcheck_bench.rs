use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use healthcheck::{
    checkers::{DnsChecker, HealthChecker, HttpChecker, TcpChecker},
    monitor::HealthCheckMonitor,
    types::{CheckType, HealthCheckConfig},
};
use std::hint::black_box;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

fn tcp_check_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("tcp_check");

    // Benchmark TCP check against a non-existent port (measures failure path)
    let checker = Arc::new(TcpChecker::new(
        "127.0.0.1:1".parse::<SocketAddr>().unwrap(),
        Duration::from_millis(100),
    ));

    group.bench_function("tcp_connection_refused", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| rt.block_on(async { black_box(checker.check().await) }));
    });

    group.finish();
}

fn http_check_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("http_check");

    // Benchmark HTTP check against a non-existent server (measures failure path)
    let checker = Arc::new(
        HttpChecker::new(
            "http://127.0.0.1:1/health".to_string(),
            reqwest::Method::GET,
            vec![200],
            Duration::from_millis(100),
        )
        .unwrap(),
    );

    group.bench_function("http_connection_error", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| rt.block_on(async { black_box(checker.check().await) }));
    });

    group.finish();
}

fn dns_check_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("dns_check");
    group.sample_size(10); // DNS checks are slower, use fewer samples

    // Benchmark DNS check for localhost (measures success path)
    let checker = Arc::new(DnsChecker::new(
        "localhost".to_string(),
        vec![],
        Duration::from_secs(1),
    ));

    group.bench_function("dns_localhost_resolution", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| rt.block_on(async { black_box(checker.check().await) }));
    });

    group.finish();
}

fn monitor_overhead_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("monitor_overhead");

    // Benchmark monitor creation and single check
    group.bench_function("monitor_create", |b| {
        b.iter(|| {
            let checker = Arc::new(TcpChecker::new(
                "127.0.0.1:1".parse::<SocketAddr>().unwrap(),
                Duration::from_millis(100),
            ));

            let config = HealthCheckConfig {
                target: "127.0.0.1:1".to_string(),
                timeout: Duration::from_millis(100),
                interval: Duration::from_secs(1),
                rise: 2,
                fall: 2,
                check_type: CheckType::Tcp,
            };

            black_box(HealthCheckMonitor::new(checker, config))
        });
    });

    group.finish();
}

fn concurrent_checks_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_checks");
    group.sample_size(10); // Concurrent tests are expensive

    for count in [1, 10, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            b.iter(|| {
                rt.block_on(async move {
                    let mut handles = vec![];

                    for _ in 0..count {
                        let checker = Arc::new(TcpChecker::new(
                            "127.0.0.1:1".parse::<SocketAddr>().unwrap(),
                            Duration::from_millis(100),
                        ));

                        handles.push(tokio::spawn(async move { checker.check().await }));
                    }

                    for handle in handles {
                        black_box(handle.await.unwrap());
                    }
                })
            });
        });
    }

    group.finish();
}

fn checker_type_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("checker_comparison");

    // TCP checker
    let tcp_checker = Arc::new(TcpChecker::new(
        "127.0.0.1:1".parse::<SocketAddr>().unwrap(),
        Duration::from_millis(100),
    ));

    group.bench_function("tcp", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| rt.block_on(async { black_box(tcp_checker.check().await) }));
    });

    // HTTP checker
    let http_checker = Arc::new(
        HttpChecker::new(
            "http://127.0.0.1:1/".to_string(),
            reqwest::Method::GET,
            vec![200],
            Duration::from_millis(100),
        )
        .unwrap(),
    );

    group.bench_function("http", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| rt.block_on(async { black_box(http_checker.check().await) }));
    });

    // DNS checker
    let dns_checker = Arc::new(DnsChecker::new(
        "localhost".to_string(),
        vec![],
        Duration::from_secs(1),
    ));

    group.bench_function("dns", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| rt.block_on(async { black_box(dns_checker.check().await) }));
    });

    group.finish();
}

criterion_group!(
    benches,
    tcp_check_benchmark,
    http_check_benchmark,
    dns_check_benchmark,
    monitor_overhead_benchmark,
    concurrent_checks_benchmark,
    checker_type_comparison,
);

criterion_main!(benches);
