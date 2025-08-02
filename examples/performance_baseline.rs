//! Performance Baseline Measurement Tool
//!
//! Quick performance measurements for TCP command architecture
//! to establish baselines without full criterion overhead.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tasker_core::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, WorkerCapabilities,
};

fn main() {
    println!("🎯 TCP Command Architecture Performance Baseline");
    println!("=".repeat(60));

    // Warm up
    for _ in 0..100 {
        create_sample_command();
    }

    measure_command_creation();
    measure_json_serialization();
    measure_json_deserialization();
    measure_payload_size_impact();

    println!("\n✅ Performance baseline measurements complete!");
}

fn create_sample_command() -> Command {
    let worker_capabilities = WorkerCapabilities {
        worker_id: "baseline_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["baseline".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
        connection_info: None,
        runtime_info: None,
        supported_tasks: None,
    };

    Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities,
        },
        CommandSource::RustServer {
            id: "baseline".to_string(),
        },
    )
}

fn measure_command_creation() {
    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _command = create_sample_command();
    }

    let duration = start.elapsed();
    let avg_ns = duration.as_nanos() / iterations;

    println!("📊 Command Creation:");
    println!("   Total: {:?} for {} iterations", duration, iterations);
    println!("   Average: {}ns per command", avg_ns);
    println!(
        "   Rate: {:.0} commands/sec",
        1_000_000_000.0 / avg_ns as f64
    );
}

fn measure_json_serialization() {
    let command = create_sample_command();
    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _json = serde_json::to_string(&command).unwrap();
    }

    let duration = start.elapsed();
    let avg_ns = duration.as_nanos() / iterations;

    // Get size information
    let json = serde_json::to_string(&command).unwrap();
    let size_bytes = json.len();

    println!("\n📊 JSON Serialization:");
    println!("   Total: {:?} for {} iterations", duration, iterations);
    println!("   Average: {}ns per serialization", avg_ns);
    println!(
        "   Rate: {:.0} serializations/sec",
        1_000_000_000.0 / avg_ns as f64
    );
    println!("   JSON size: {} bytes", size_bytes);
    println!(
        "   Throughput: {:.2} MB/sec",
        (size_bytes as f64 * 1_000_000_000.0 / avg_ns as f64) / 1_000_000.0
    );
}

fn measure_json_deserialization() {
    let command = create_sample_command();
    let json = serde_json::to_string(&command).unwrap();
    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _: Command = serde_json::from_str(&json).unwrap();
    }

    let duration = start.elapsed();
    let avg_ns = duration.as_nanos() / iterations;

    println!("\n📊 JSON Deserialization:");
    println!("   Total: {:?} for {} iterations", duration, iterations);
    println!("   Average: {}ns per deserialization", avg_ns);
    println!(
        "   Rate: {:.0} deserializations/sec",
        1_000_000_000.0 / avg_ns as f64
    );
}

fn measure_payload_size_impact() {
    println!("\n📊 Payload Size Impact:");

    for namespace_count in [1, 5, 10, 25, 50] {
        let namespaces: Vec<String> = (0..namespace_count)
            .map(|i| format!("namespace_{}", i))
            .collect();

        let worker_capabilities = WorkerCapabilities {
            worker_id: format!("worker_with_{}_namespaces", namespace_count),
            max_concurrent_steps: 10,
            supported_namespaces: namespaces,
            step_timeout_ms: 30000,
            supports_retries: true,
            language_runtime: "rust".to_string(),
            version: "1.0.0".to_string(),
            custom_capabilities: HashMap::new(),
            connection_info: None,
            runtime_info: None,
            supported_tasks: None,
        };

        let command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities,
            },
            CommandSource::RustServer {
                id: "baseline".to_string(),
            },
        );

        // Measure serialization time
        let iterations = 1_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _json = serde_json::to_string(&command).unwrap();
        }

        let duration = start.elapsed();
        let avg_ns = duration.as_nanos() / iterations;
        let json_size = serde_json::to_string(&command).unwrap().len();

        println!(
            "   {} namespaces: {}ns avg, {} bytes, {:.0} ops/sec",
            namespace_count,
            avg_ns,
            json_size,
            1_000_000_000.0 / avg_ns as f64
        );
    }
}
