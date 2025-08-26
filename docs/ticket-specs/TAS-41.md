# TAS-41: Pure Rust Worker Implementation

## Executive Summary

Create a standalone, production-ready Rust worker (`tasker-worker-rust/`) that leverages the worker foundation from TAS-40 to implement step handlers in pure Rust. This worker will replicate all workflow patterns currently tested in Ruby while demonstrating the foundation's language-agnostic capabilities and achieving 1000+ steps/second performance targets.

## Context and Dependencies

### Prerequisites
- **TAS-40 Complete**: Worker foundation (`tasker-worker-foundation/`) must be implemented
- **Workspace Structure**: Multi-workspace architecture established per TAS-40-preamble
- **Shared Components**: `tasker-shared/` workspace with common models and configuration

### Validation Goals
- **Foundation Proof**: Demonstrate worker foundation works with non-Ruby language
- **Performance Baseline**: Establish performance benchmarks for step processing
- **Pattern Replication**: Implement same workflow patterns as Ruby examples
- **Production Readiness**: Full Docker deployment and Kubernetes integration

## Current Ruby Workflow Patterns

The Ruby implementation in `workers/ruby/spec/handlers/examples/` provides these workflow patterns that must be replicated:

### Linear Workflow (`linear_workflow/`)
- **Task Template**: `config/linear_workflow_handler.yaml`
- **Handler**: Sequential step processing with dependency chains
- **Steps**: `linear_step_1` → `linear_step_2` → `linear_step_3` → `linear_step_4`
- **Pattern**: Simple sequential execution with step result passing

### Diamond Workflow (`diamond_workflow/`)
- **Task Template**: `config/diamond_workflow_handler.yaml`
- **Handler**: Fork-join pattern with parallel branches
- **Steps**: `diamond_start` → [`diamond_branch_b`, `diamond_branch_c`] → `diamond_end`
- **Pattern**: Parallel execution with synchronization point

### Tree Workflow (`tree_workflow/`)
- **Task Template**: `config/tree_workflow_handler.yaml`
- **Handler**: Complex branching with multiple convergence points
- **Steps**: Tree structure with multiple leaf nodes and convergence
- **Pattern**: Complex dependency management and parallel processing

### Mixed DAG Workflow (`mixed_dag_workflow/`)
- **Task Template**: `config/mixed_dag_workflow_handler.yaml`
- **Handler**: Complex directed acyclic graph with multiple patterns
- **Steps**: Combination of sequential, parallel, and convergence patterns
- **Pattern**: Real-world complex workflow simulation

### Order Fulfillment Workflow (`order_fulfillment/`)
- **Task Template**: `config/order_fulfillment_handler.yaml`
- **Handler**: Business workflow with external system integration
- **Steps**: `validate_order` → `reserve_inventory` → `process_payment` → `ship_order`
- **Pattern**: Business logic with error handling and rollback capabilities

## Implementation Plan

### Phase 1: Workspace Setup and Foundation Integration (Week 5.1)

#### 1.1 Create Rust Worker Workspace
```bash
# Create workspace structure
mkdir -p tasker-worker-rust/{src,config,tests,docker}
cd tasker-worker-rust

# Initialize Cargo.toml
cat > Cargo.toml << EOF
[package]
name = "tasker-worker-rust"
version = "0.1.0"
edition = "2021"

[dependencies]
tasker-worker-foundation = { path = "../tasker-worker-foundation" }
tasker-shared = { path = "../tasker-shared" }
tokio = { workspace = true }
serde = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }

# Rust-specific dependencies
async-trait = "0.1"
futures = "0.3"
once_cell = "1.19"

[features]
default = ["all-workflows"]
all-workflows = ["linear", "diamond", "tree", "mixed-dag", "order-fulfillment"]
linear = []
diamond = []
tree = []
mixed-dag = []
order-fulfillment = []
EOF
```

#### 1.2 Core Rust Worker Architecture
```rust
// tasker-worker-rust/src/lib.rs
pub mod worker;
pub mod handlers;
pub mod workflows;
pub mod config;

pub use worker::RustWorker;

// tasker-worker-rust/src/worker/mod.rs
use std::sync::Arc;
use anyhow::Result;
use tracing::{info, error};

use tasker_worker_foundation::{WorkerCore, FfiBridge};
use tasker_shared::{
    config::ConfigManager,
    types::{StepExecutionRequest, StepExecutionResult},
};

use crate::handlers::HandlerRegistry;

/// Pure Rust worker implementation using worker foundation
pub struct RustWorker {
    worker_core: Arc<WorkerCore>,
    handler_registry: Arc<HandlerRegistry>,
}

impl RustWorker {
    /// Create new Rust worker instance
    pub async fn new() -> Result<Self> {
        info!("🚀 Initializing RustWorker...");

        // Initialize worker foundation
        let worker_core = Arc::new(WorkerCore::new().await?);

        // Initialize handler registry with Rust implementations
        let handler_registry = Arc::new(HandlerRegistry::new().await?);

        // Register native Rust execution with foundation
        worker_core.register_native_executor(Box::new(RustExecutor::new(
            handler_registry.clone()
        ))).await?;

        info!("✅ RustWorker initialized successfully");

        Ok(Self {
            worker_core,
            handler_registry,
        })
    }

    /// Start the Rust worker
    pub async fn start(&self) -> Result<()> {
        info!("🔄 Starting RustWorker...");

        // Start the worker foundation
        self.worker_core.start().await?;

        info!("✅ RustWorker started successfully");
        Ok(())
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Shutting down RustWorker...");

        self.worker_core.shutdown().await?;

        info!("✅ RustWorker shutdown completed");
        Ok(())
    }
}

/// Native Rust step executor
pub struct RustExecutor {
    handler_registry: Arc<HandlerRegistry>,
}

impl RustExecutor {
    pub fn new(handler_registry: Arc<HandlerRegistry>) -> Self {
        Self { handler_registry }
    }
}

#[async_trait::async_trait]
impl StepExecutor for RustExecutor {
    async fn execute_step(&self, request: StepExecutionRequest) -> Result<StepExecutionResult> {
        // Look up handler for this step
        let handler = self.handler_registry
            .get_handler(&request.step_name)
            .ok_or_else(|| anyhow::anyhow!("No handler found for step: {}", request.step_name))?;

        // Execute step with handler
        handler.execute(request).await
    }
}
```

### Phase 2: Handler Registry and Base Patterns (Week 5.2)

#### 2.1 Handler Registry System
```rust
// tasker-worker-rust/src/handlers/mod.rs
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;

use tasker_shared::types::{StepExecutionRequest, StepExecutionResult};

/// Base trait for all Rust step handlers
#[async_trait]
pub trait StepHandler: Send + Sync {
    async fn execute(&self, request: StepExecutionRequest) -> Result<StepExecutionResult>;
    fn step_name(&self) -> &str;
    fn description(&self) -> &str;
}

/// Registry for managing step handlers
pub struct HandlerRegistry {
    handlers: HashMap<String, Arc<dyn StepHandler>>,
}

impl HandlerRegistry {
    pub async fn new() -> Result<Self> {
        let mut handlers: HashMap<String, Arc<dyn StepHandler>> = HashMap::new();

        // Register all workflow handlers
        Self::register_linear_handlers(&mut handlers)?;
        Self::register_diamond_handlers(&mut handlers)?;
        Self::register_tree_handlers(&mut handlers)?;
        Self::register_mixed_dag_handlers(&mut handlers)?;
        Self::register_order_fulfillment_handlers(&mut handlers)?;

        Ok(Self { handlers })
    }

    pub fn get_handler(&self, step_name: &str) -> Option<Arc<dyn StepHandler>> {
        self.handlers.get(step_name).cloned()
    }

    fn register_linear_handlers(handlers: &mut HashMap<String, Arc<dyn StepHandler>>) -> Result<()> {
        use crate::workflows::linear::*;

        handlers.insert("linear_step_1".to_string(), Arc::new(LinearStep1Handler::new()));
        handlers.insert("linear_step_2".to_string(), Arc::new(LinearStep2Handler::new()));
        handlers.insert("linear_step_3".to_string(), Arc::new(LinearStep3Handler::new()));
        handlers.insert("linear_step_4".to_string(), Arc::new(LinearStep4Handler::new()));

        Ok(())
    }

    // Similar registration for other workflow types...
}
```

#### 2.2 Base Handler Implementation Pattern
```rust
// tasker-worker-rust/src/handlers/base.rs
use anyhow::Result;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use tasker_shared::types::{StepExecutionRequest, StepExecutionResult};

/// Helper functions for common step handler patterns
pub struct HandlerUtils;

impl HandlerUtils {
    /// Create successful step result
    pub fn success_result(data: Value) -> StepExecutionResult {
        StepExecutionResult {
            success: true,
            data,
            error: None,
            metadata: Self::create_metadata(),
            duration_ms: 0, // Set by caller
        }
    }

    /// Create error step result
    pub fn error_result(error_message: String, retryable: bool) -> StepExecutionResult {
        StepExecutionResult {
            success: false,
            data: Value::Null,
            error: Some(StepExecutionError {
                message: error_message,
                error_type: if retryable { "RetryableError" } else { "PermanentError" }.to_string(),
                details: None,
            }),
            metadata: Self::create_metadata(),
            duration_ms: 0,
        }
    }

    /// Extract sequence results by step name
    pub fn get_sequence_result(request: &StepExecutionRequest, step_name: &str) -> Option<Value> {
        // Parse sequence data to find results from previous steps
        for step_result in &request.sequence_data {
            if let Some(name) = step_result.get("step_name") {
                if name.as_str() == Some(step_name) {
                    return step_result.get("result").cloned();
                }
            }
        }
        None
    }

    fn create_metadata() -> ExecutionMetadata {
        ExecutionMetadata {
            executed_at: chrono::Utc::now(),
            execution_environment: "rust".to_string(),
            memory_usage_kb: None, // Can be populated if needed
            cpu_time_ms: None,
        }
    }
}

/// Macro for creating simple step handlers
macro_rules! create_step_handler {
    ($handler_name:ident, $step_name:expr, $description:expr, $execute_fn:expr) => {
        pub struct $handler_name;

        impl $handler_name {
            pub fn new() -> Self {
                Self
            }
        }

        #[async_trait::async_trait]
        impl StepHandler for $handler_name {
            async fn execute(&self, request: StepExecutionRequest) -> Result<StepExecutionResult> {
                let start_time = std::time::Instant::now();

                let result = $execute_fn(request).await;

                let duration_ms = start_time.elapsed().as_millis() as u64;

                match result {
                    Ok(mut success_result) => {
                        success_result.duration_ms = duration_ms;
                        Ok(success_result)
                    }
                    Err(e) => {
                        let mut error_result = HandlerUtils::error_result(e.to_string(), true);
                        error_result.duration_ms = duration_ms;
                        Ok(error_result)
                    }
                }
            }

            fn step_name(&self) -> &str {
                $step_name
            }

            fn description(&self) -> &str {
                $description
            }
        }
    };
}

pub(crate) use create_step_handler;
```

### Phase 3: Workflow Implementation (Week 5.3)

#### 3.1 Linear Workflow Implementation
```rust
// tasker-worker-rust/src/workflows/linear/mod.rs
use anyhow::Result;
use serde_json::{json, Value};

use tasker_shared::types::{StepExecutionRequest, StepExecutionResult};
use crate::handlers::{StepHandler, base::{HandlerUtils, create_step_handler}};

// Linear Step 1: Generate initial data
create_step_handler!(
    LinearStep1Handler,
    "linear_step_1",
    "Generate initial data for linear workflow",
    |request: StepExecutionRequest| async move {
        // Extract even_number from task context
        let even_number = request.task_data
            .get("even_number")
            .and_then(|v| v.as_i64())
            .unwrap_or(6);

        let result_data = json!({
            "generated_value": even_number * 2,
            "step_1_timestamp": chrono::Utc::now().to_rfc3339(),
            "processing_info": "Generated by Rust linear step 1"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);

// Linear Step 2: Process data from step 1
create_step_handler!(
    LinearStep2Handler,
    "linear_step_2",
    "Process data from linear step 1",
    |request: StepExecutionRequest| async move {
        // Get result from step 1
        let step_1_result = HandlerUtils::get_sequence_result(&request, "linear_step_1")
            .ok_or_else(|| anyhow::anyhow!("Missing result from linear_step_1"))?;

        let generated_value = step_1_result
            .get("generated_value")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let result_data = json!({
            "processed_value": generated_value + 10,
            "step_2_timestamp": chrono::Utc::now().to_rfc3339(),
            "previous_step_value": generated_value,
            "processing_info": "Processed by Rust linear step 2"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);

// Linear Step 3: Transform data
create_step_handler!(
    LinearStep3Handler,
    "linear_step_3",
    "Transform data from linear step 2",
    |request: StepExecutionRequest| async move {
        // Get result from step 2
        let step_2_result = HandlerUtils::get_sequence_result(&request, "linear_step_2")
            .ok_or_else(|| anyhow::anyhow!("Missing result from linear_step_2"))?;

        let processed_value = step_2_result
            .get("processed_value")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let result_data = json!({
            "transformed_value": processed_value * 3,
            "step_3_timestamp": chrono::Utc::now().to_rfc3339(),
            "transformation_applied": "multiply_by_3",
            "processing_info": "Transformed by Rust linear step 3"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);

// Linear Step 4: Finalize workflow
create_step_handler!(
    LinearStep4Handler,
    "linear_step_4",
    "Finalize linear workflow",
    |request: StepExecutionRequest| async move {
        // Get results from all previous steps
        let step_1_result = HandlerUtils::get_sequence_result(&request, "linear_step_1");
        let step_2_result = HandlerUtils::get_sequence_result(&request, "linear_step_2");
        let step_3_result = HandlerUtils::get_sequence_result(&request, "linear_step_3");

        let final_value = step_3_result
            .and_then(|r| r.get("transformed_value"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let result_data = json!({
            "final_result": final_value,
            "workflow_summary": {
                "step_1_executed": step_1_result.is_some(),
                "step_2_executed": step_2_result.is_some(),
                "step_3_executed": step_3_result.is_some(),
                "total_steps": 4
            },
            "completion_timestamp": chrono::Utc::now().to_rfc3339(),
            "processing_info": "Finalized by Rust linear step 4"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);
```

#### 3.2 Diamond Workflow Implementation
```rust
// tasker-worker-rust/src/workflows/diamond/mod.rs
use anyhow::Result;
use serde_json::{json, Value};

use tasker_shared::types::{StepExecutionRequest, StepExecutionResult};
use crate::handlers::{StepHandler, base::{HandlerUtils, create_step_handler}};

// Diamond Start: Initialize parallel work
create_step_handler!(
    DiamondStartHandler,
    "diamond_start",
    "Initialize diamond workflow with parallel branches",
    |request: StepExecutionRequest| async move {
        let base_value = request.task_data
            .get("base_value")
            .and_then(|v| v.as_i64())
            .unwrap_or(10);

        let result_data = json!({
            "branch_data": {
                "value_for_b": base_value * 2,
                "value_for_c": base_value * 3
            },
            "start_timestamp": chrono::Utc::now().to_rfc3339(),
            "processing_info": "Started by Rust diamond start"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);

// Diamond Branch B: Process branch B work
create_step_handler!(
    DiamondBranchBHandler,
    "diamond_branch_b",
    "Process diamond workflow branch B",
    |request: StepExecutionRequest| async move {
        let start_result = HandlerUtils::get_sequence_result(&request, "diamond_start")
            .ok_or_else(|| anyhow::anyhow!("Missing result from diamond_start"))?;

        let value_for_b = start_result
            .get("branch_data")
            .and_then(|d| d.get("value_for_b"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // Simulate some processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let result_data = json!({
            "branch_b_result": value_for_b + 100,
            "branch_b_timestamp": chrono::Utc::now().to_rfc3339(),
            "processing_info": "Processed by Rust diamond branch B"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);

// Diamond Branch C: Process branch C work (parallel to B)
create_step_handler!(
    DiamondBranchCHandler,
    "diamond_branch_c",
    "Process diamond workflow branch C",
    |request: StepExecutionRequest| async move {
        let start_result = HandlerUtils::get_sequence_result(&request, "diamond_start")
            .ok_or_else(|| anyhow::anyhow!("Missing result from diamond_start"))?;

        let value_for_c = start_result
            .get("branch_data")
            .and_then(|d| d.get("value_for_c"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // Simulate different processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

        let result_data = json!({
            "branch_c_result": value_for_c * 2,
            "branch_c_timestamp": chrono::Utc::now().to_rfc3339(),
            "processing_info": "Processed by Rust diamond branch C"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);

// Diamond End: Combine results from both branches
create_step_handler!(
    DiamondEndHandler,
    "diamond_end",
    "Combine results from diamond workflow branches",
    |request: StepExecutionRequest| async move {
        // Wait for both branches to complete
        let branch_b_result = HandlerUtils::get_sequence_result(&request, "diamond_branch_b")
            .ok_or_else(|| anyhow::anyhow!("Missing result from diamond_branch_b"))?;
        let branch_c_result = HandlerUtils::get_sequence_result(&request, "diamond_branch_c")
            .ok_or_else(|| anyhow::anyhow!("Missing result from diamond_branch_c"))?;

        let b_value = branch_b_result
            .get("branch_b_result")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let c_value = branch_c_result
            .get("branch_c_result")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let result_data = json!({
            "final_result": b_value + c_value,
            "branch_results": {
                "branch_b": b_value,
                "branch_c": c_value
            },
            "end_timestamp": chrono::Utc::now().to_rfc3339(),
            "processing_info": "Combined by Rust diamond end"
        });

        Ok(HandlerUtils::success_result(result_data))
    }
);
```

### Phase 4: Task Template Configuration (Week 5.4)

#### 4.1 Rust Worker Task Templates
Since we use TOML for configuration but YAML for task templates, we'll maintain YAML for task templates but ensure our Rust handlers match:

```yaml
# tasker-worker-rust/config/workflows/linear_workflow_handler.yaml
namespace_name: linear_workflow
name: mathematical_sequence
version: 1.0.0
description: Linear workflow implementation in pure Rust
task_handler_class: RustLinearWorkflowHandler
language: rust

step_templates:
  - name: linear_step_1
    description: Generate initial data for linear workflow
    handler_class: LinearStep1Handler
    execution_environment: rust
    timeout_seconds: 30
    max_retries: 3

  - name: linear_step_2
    description: Process data from linear step 1
    handler_class: LinearStep2Handler
    execution_environment: rust
    timeout_seconds: 30
    max_retries: 3
    dependencies:
      - linear_step_1

  - name: linear_step_3
    description: Transform data from linear step 2
    handler_class: LinearStep3Handler
    execution_environment: rust
    timeout_seconds: 30
    max_retries: 3
    dependencies:
      - linear_step_2

  - name: linear_step_4
    description: Finalize linear workflow
    handler_class: LinearStep4Handler
    execution_environment: rust
    timeout_seconds: 30
    max_retries: 3
    dependencies:
      - linear_step_3

workflow_steps:
  - step_template: linear_step_1
    context_key: even_number

  - step_template: linear_step_2
    dependencies: ["linear_step_1"]

  - step_template: linear_step_3
    dependencies: ["linear_step_2"]

  - step_template: linear_step_4
    dependencies: ["linear_step_3"]
```

#### 4.2 Worker Configuration TOML
```toml
# tasker-worker-rust/config/worker.toml
[worker]
worker_type = "rust_native"
namespaces = ["linear_workflow", "diamond_workflow", "tree_workflow", "mixed_dag_workflow", "order_fulfillment"]
max_concurrent_steps = 100
performance_target_steps_per_second = 1000

[worker.rust_runtime]
enabled = true
tokio_worker_threads = 8
async_runtime = "multi_threaded"
stack_size_kb = 2048

[worker.task_templates]
template_directory = "config/workflows"
auto_reload = true
validation_enabled = true

[worker.metrics]
prometheus_enabled = true
prometheus_port = 9091
performance_monitoring = true
step_timing_enabled = true
memory_monitoring = true
```

### Phase 5: Testing Infrastructure (Week 5.5)

#### 5.1 Comprehensive Test Suite
```rust
// tasker-worker-rust/tests/integration/workflow_tests.rs
use std::time::Duration;
use tokio::time::timeout;
use anyhow::Result;

use tasker_worker_rust::RustWorker;
use tasker_shared::{
    models::{Task, TaskRequest},
    types::StepExecutionRequest,
};

/// Test linear workflow execution
#[tokio::test]
async fn test_linear_workflow_execution() -> Result<()> {
    // Initialize Rust worker
    let worker = RustWorker::new().await?;
    worker.start().await?;

    // Create linear workflow task
    let task_request = TaskRequest {
        namespace: "linear_workflow".to_string(),
        name: "mathematical_sequence".to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::json!({
            "even_number": 6,
            "test_run_id": uuid::Uuid::new_v4().to_string()
        }),
        initiator: "rust_integration_test".to_string(),
        source_system: "test_suite".to_string(),
        reason: "Test linear workflow execution".to_string(),
        priority: Some(5),
        claim_timeout_seconds: Some(300),
    };

    // Submit task and wait for completion
    let task_result = submit_task_and_wait(&worker, task_request, Duration::from_secs(30)).await?;

    // Verify final result
    assert_eq!(task_result.status, "completed");
    assert!(task_result.final_result.get("final_result").is_some());

    worker.shutdown().await?;
    Ok(())
}

/// Test diamond workflow parallel execution
#[tokio::test]
async fn test_diamond_workflow_execution() -> Result<()> {
    let worker = RustWorker::new().await?;
    worker.start().await?;

    let task_request = TaskRequest {
        namespace: "diamond_workflow".to_string(),
        name: "parallel_processing".to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::json!({
            "base_value": 10,
            "test_run_id": uuid::Uuid::new_v4().to_string()
        }),
        initiator: "rust_integration_test".to_string(),
        source_system: "test_suite".to_string(),
        reason: "Test diamond workflow execution".to_string(),
        priority: Some(5),
        claim_timeout_seconds: Some(300),
    };

    let start_time = std::time::Instant::now();
    let task_result = submit_task_and_wait(&worker, task_request, Duration::from_secs(30)).await?;
    let execution_time = start_time.elapsed();

    // Verify parallel execution was efficient
    assert!(execution_time < Duration::from_secs(5));
    assert_eq!(task_result.status, "completed");

    // Verify both branches were executed
    let final_result = task_result.final_result;
    assert!(final_result.get("branch_results").is_some());

    worker.shutdown().await?;
    Ok(())
}

/// Performance test - 1000+ steps/second target
#[tokio::test]
async fn test_performance_target() -> Result<()> {
    let worker = RustWorker::new().await?;
    worker.start().await?;

    let start_time = std::time::Instant::now();
    let mut tasks = Vec::new();

    // Submit 100 linear workflow tasks
    for i in 0..100 {
        let task_request = TaskRequest {
            namespace: "linear_workflow".to_string(),
            name: "mathematical_sequence".to_string(),
            version: "1.0.0".to_string(),
            context: serde_json::json!({
                "even_number": 6,
                "test_run_id": format!("perf_test_{}", i)
            }),
            initiator: "performance_test".to_string(),
            source_system: "test_suite".to_string(),
            reason: "Performance testing".to_string(),
            priority: Some(5),
            claim_timeout_seconds: Some(300),
        };

        tasks.push(submit_task(&worker, task_request).await?);
    }

    // Wait for all tasks to complete
    for task in tasks {
        wait_for_task_completion(&worker, task.task_id, Duration::from_secs(60)).await?;
    }

    let total_time = start_time.elapsed();
    let steps_processed = 100 * 4; // 4 steps per linear workflow
    let steps_per_second = steps_processed as f64 / total_time.as_secs_f64();

    println!("Performance: {:.2} steps/second", steps_per_second);

    // Verify we meet our 1000+ steps/second target
    assert!(steps_per_second >= 1000.0,
           "Performance target not met: {:.2} steps/second < 1000",
           steps_per_second);

    worker.shutdown().await?;
    Ok(())
}

// Helper functions for test utilities
async fn submit_task_and_wait(
    worker: &RustWorker,
    task_request: TaskRequest,
    timeout_duration: Duration,
) -> Result<TaskResult> {
    let task = submit_task(worker, task_request).await?;
    timeout(timeout_duration, wait_for_task_completion(worker, task.task_id, timeout_duration)).await??
}

async fn submit_task(worker: &RustWorker, task_request: TaskRequest) -> Result<Task> {
    // Implementation would call orchestration API to submit task
    todo!("Implement task submission via orchestration API")
}

async fn wait_for_task_completion(
    worker: &RustWorker,
    task_id: String,
    timeout_duration: Duration,
) -> Result<TaskResult> {
    // Implementation would poll task status until completion
    todo!("Implement task completion waiting")
}
```

### Phase 6: Docker and Deployment (Week 6)

#### 6.1 Production Docker Configuration
```dockerfile
# tasker-worker-rust/docker/Dockerfile
FROM rust:1.75 as builder

WORKDIR /app

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./
COPY tasker-shared/ ./tasker-shared/
COPY tasker-worker-foundation/ ./tasker-worker-foundation/
COPY tasker-worker-rust/ ./tasker-worker-rust/

# Build the Rust worker
RUN cargo build --release --package tasker-worker-rust

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -u 1000 tasker

# Copy binary and configuration
COPY --from=builder /app/target/release/tasker-worker-rust /usr/local/bin/
COPY tasker-worker-rust/config/ /etc/tasker/config/

# Set permissions
RUN chown -R tasker:tasker /etc/tasker
USER tasker

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8081/health || exit 1

EXPOSE 8081

CMD ["tasker-worker-rust"]
```

#### 6.2 Kubernetes Deployment
```yaml
# tasker-worker-rust/deploy/k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tasker-worker-rust
  labels:
    app: tasker-worker-rust
    version: v1.0.0
spec:
  replicas: 3
  selector:
    matchLabels:
      app: tasker-worker-rust
  template:
    metadata:
      labels:
        app: tasker-worker-rust
    spec:
      containers:
      - name: tasker-worker-rust
        image: tasker-worker-rust:latest
        ports:
        - containerPort: 8081
          name: health
        env:
        - name: TASKER_ENV
          value: "production"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tasker-secrets
              key: database-url
        - name: ORCHESTRATION_API_URL
          value: "http://tasker-orchestration:8080"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8081
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8081
          initialDelaySeconds: 5
          periodSeconds: 5
```

## Success Criteria

### Functional Requirements
- ✅ **Workflow Parity**: All Ruby workflow patterns replicated identically
- ✅ **Foundation Integration**: Uses worker foundation without direct infrastructure code
- ✅ **Configuration Compatibility**: Uses same task template formats as Ruby
- ✅ **API Compatibility**: Works with same orchestration API endpoints

### Performance Requirements
- ✅ **Throughput**: Achieves 1000+ steps/second processing rate
- ✅ **Latency**: Step processing latency < 10ms for simple steps
- ✅ **Concurrency**: Handles 100+ concurrent step executions
- ✅ **Resource Efficiency**: Memory usage < 100MB per worker instance

### Production Requirements
- ✅ **Docker Deployment**: Complete containerization with health checks
- ✅ **Kubernetes Ready**: Full K8s deployment with scaling and monitoring
- ✅ **Observability**: Prometheus metrics and structured logging
- ✅ **Graceful Shutdown**: < 5 second shutdown with work completion

### Quality Requirements
- ✅ **Test Coverage**: > 90% test coverage including integration tests
- ✅ **Documentation**: Complete API documentation and deployment guides
- ✅ **Error Handling**: Comprehensive error handling and recovery
- ✅ **Monitoring**: Full metrics collection and health monitoring

This pure Rust worker implementation validates the worker foundation architecture while providing a high-performance baseline for step processing across the Tasker ecosystem.
