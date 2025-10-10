//! # Event Propagation Latency Benchmarks (TAS-29 Phase 5.4)
//!
//! Measures PostgreSQL LISTEN/NOTIFY event propagation latency.
//!
//! ## What This Measures
//!
//! - **PGMQ Notify Round-Trip**: Time from `pgmq_send_with_notify` to listener receiving notification
//! - **PostgreSQL LISTEN/NOTIFY Overhead**: Built-in PostgreSQL notification mechanism
//! - **Real Event System Latency**: Actual performance in distributed coordination
//!
//! ## Why This Matters
//!
//! Event propagation latency directly affects:
//! - Worker responsiveness to new steps
//! - Orchestration coordination speed
//! - Overall workflow execution time
//!
//! ## Prerequisites
//!
//! **Database must be running**:
//! ```bash
//! docker-compose -f docker/docker-compose.test.yml up -d postgres
//!
//! # Set DATABASE_URL
//! export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
//! ```
//!
//! ## Expected Performance
//!
//! | Metric | Target | Notes |
//! |--------|--------|-------|
//! | Notify latency (p50) | < 5ms | PostgreSQL NOTIFY overhead |
//! | Notify latency (p95) | < 10ms | Network + processing |
//! | Notify latency (p99) | < 20ms | Outliers acceptable |
//!
//! ## Running Benchmarks
//!
//! ```bash
//! DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
//! cargo bench --package tasker-shared --features benchmarks event_propagation
//! ```

use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// ===================================================================================
// SETUP AND UTILITIES
// ===================================================================================

/// Setup runtime and database pool
fn setup_runtime() -> (tokio::runtime::Runtime, PgPool) {
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for event propagation benchmarks");

    let pool = runtime
        .block_on(async { PgPool::connect(&database_url).await })
        .expect("Failed to connect to database");

    (runtime, pool)
}

// ===================================================================================
// BENCHMARKS
// ===================================================================================

/// Benchmark PostgreSQL LISTEN/NOTIFY round-trip latency
///
/// **Approach**:
/// 1. Start listener on test channel in background task
/// 2. Send test message with `pgmq_send_with_notify`
/// 3. Measure time from send until listener receives notification
/// 4. Calculate round-trip latency
///
/// **Note**: This measures the NOTIFICATION latency, not the message itself.
/// The notification is what triggers workers/orchestration to poll the queue.
fn bench_notify_round_trip(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("🔍 EVENT PROPAGATION BENCHMARK");
    eprintln!("{}", "═".repeat(80));
    eprintln!("\nMeasuring PostgreSQL LISTEN/NOTIFY round-trip latency");
    eprintln!("This is the real-time notification mechanism that triggers:");
    eprintln!("  • Workers to poll for new steps");
    eprintln!("  • Orchestration to process results");
    eprintln!("  • Event-driven coordination");
    eprintln!("{}\n", "═".repeat(80));

    let (runtime, pool) = setup_runtime();

    // Create test queue for benchmarking (must follow *_queue pattern for namespace extraction)
    runtime
        .block_on(async {
            sqlx::query("SELECT pgmq.create($1)")
                .bind("benchmark_queue")
                .execute(&pool)
                .await
        })
        .ok(); // Ignore error if queue already exists

    let mut group = c.benchmark_group("event_propagation");
    group.sample_size(20); // Moderate samples for network operations
    group.measurement_time(Duration::from_secs(15));

    group.bench_function("notify_round_trip", |b| {
        b.iter(|| {
            runtime.block_on(async {
                // Create a channel to receive notification timing
                let (tx, mut rx) = mpsc::channel::<Instant>(1);

                // Start listener in background
                let pool_clone = pool.clone();
                let listener_handle = tokio::spawn(async move {
                    // Create listener
                    let mut listener = sqlx::postgres::PgListener::connect_with(&pool_clone)
                        .await
                        .expect("Failed to create listener");

                    // Listen to the namespace-specific channel
                    // Queue "benchmark_queue" -> namespace "benchmark" -> channel "pgmq_message_ready.benchmark"
                    listener
                        .listen("pgmq_message_ready.benchmark")
                        .await
                        .expect("Failed to listen");

                    // Wait for notification
                    if let Ok(_notification) = listener.recv().await {
                        let received_at = Instant::now();
                        // Send receive time back
                        let _ = tx.send(received_at).await;
                    }
                });

                // Give listener time to connect and set up LISTEN
                tokio::time::sleep(Duration::from_millis(10)).await;

                // Send message with notify (this triggers the notification)
                let send_time = Instant::now();
                let _ = sqlx::query("SELECT pgmq_send_with_notify($1, $2, $3)")
                    .bind("benchmark_queue")
                    .bind(json!({"test": "benchmark_data"}))
                    .bind(0) // No delay
                    .execute(&pool)
                    .await
                    .expect("Failed to send with notify");

                // Wait for listener to receive (with timeout)
                let received_at = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                    .await
                    .expect("Timeout waiting for notification")
                    .expect("Channel closed unexpectedly");

                // Clean up listener task
                listener_handle.abort();

                // Return latency
                received_at.duration_since(send_time)
            })
        });
    });

    group.finish();

    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 EVENT PROPAGATION BENCHMARKS COMPLETE");
    eprintln!("{}", "═".repeat(80));
    eprintln!("\nThis latency represents the real-time notification overhead.");
    eprintln!("Lower latency = faster worker response + orchestration coordination");
    eprintln!("{}\n", "═".repeat(80));
}

// ===================================================================================
// CRITERION CONFIGURATION
// ===================================================================================

criterion_group!(benches, bench_notify_round_trip);
criterion_main!(benches);
