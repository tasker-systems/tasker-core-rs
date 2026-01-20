# TAS-73: Multi-Instance Deployment Infrastructure Design

**Status:** Design Proposal
**Priority:** P0 (required for multi-instance testing)
**Branch:** `jcoletaylor/tas-73-resiliency-and-redundancy-ensuring-atomicity-and-idempotency`

---

## Executive Summary

This document designs the infrastructure needed to run multiple orchestration and worker instances locally for testing horizontal scaling, race conditions, and concurrent processing scenarios. The design prioritizes:

1. **Reusability**: Same infrastructure will support TAS-73 stress tests and future benchmarking tickets
2. **Ergonomics**: Integration with existing cargo-make and dotenv tooling
3. **Flexibility**: Support both "1-of-each" (current) and "N-of-each" modes
4. **Observability**: Clear instance identification in logs and metrics

---

## Current State Analysis

### What Exists

| Capability | Status | Location |
|------------|--------|----------|
| PID-based service tracking | ✅ Works | `.pids/`, `cargo-make/scripts/service-*.sh` |
| Per-service .env generation | ✅ Works | `cargo-make/scripts/setup-env.sh` |
| Health check endpoints | ✅ Works | `/health`, `/health/detailed` |
| Static port configuration | ⚠️ Hardcoded | `Makefile.toml` (8080, 8081, 8082...) |
| Docker Compose replicas | ✅ Exists | `docker-compose.prod.yml` |
| Split-database mode | ✅ Works | TAS-78 implementation |
| IntegrationTestManager | ⚠️ Single-instance | `tests/common/integration_test_manager.rs` |

### Critical Gaps

| Gap | Impact | Severity |
|-----|--------|----------|
| No `TASKER_INSTANCE_ID` concept | Cannot differentiate instances | High |
| Worker ID randomly generated | Ignores TOML config, no instance control | High |
| Static service discovery in tests | Cannot test multiple endpoints | High |
| No round-robin load distribution | Cannot validate concurrent behavior | Medium |
| Hardcoded ports in Makefile.toml | Cannot start N instances dynamically | Medium |

---

## Design Overview

### Instance Identification Model

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Instance Identification                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  TASKER_INSTANCE_ID = "orchestration-1"                                 │
│                        ↓                                                │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ Used in:                                                         │   │
│  │  • Logging: [orchestration-1] INFO processing task abc           │   │
│  │  • Metrics: tasker_tasks_processed{instance="orchestration-1"}   │   │
│  │  • PID files: .pids/orchestration-1.pid                         │   │
│  │  • Log files: .logs/orchestration-1.log                         │   │
│  │  • Port derivation: base_port + instance_number                  │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Instance ID Format: {service_type}-{instance_number}                   │
│  Examples: orchestration-1, orchestration-2, worker-rust-1, worker-rust-2│
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Port Allocation Strategy

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Port Allocation Ranges                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Orchestration Instances:  8080-8089 (up to 10 instances)               │
│    orchestration-1: 8080                                                │
│    orchestration-2: 8081                                                │
│    orchestration-3: 8082                                                │
│                                                                         │
│  Rust Worker Instances:    8100-8109 (up to 10 instances)               │
│    worker-rust-1: 8100                                                  │
│    worker-rust-2: 8101                                                  │
│    worker-rust-3: 8102                                                  │
│                                                                         │
│  Ruby Worker Instances:    8200-8209 (up to 10 instances)               │
│  Python Worker Instances:  8300-8309 (up to 10 instances)               │
│  TypeScript Worker:        8400-8409 (up to 10 instances)               │
│                                                                         │
│  Single-Instance Mode (legacy compatibility):                           │
│    orchestration: 8080                                                  │
│    rust-worker:   8081 (unchanged for backward compat)                  │
│    ruby-worker:   8082                                                  │
│    python-worker: 8083                                                  │
│    ts-worker:     8085                                                  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Component 1: Configuration Enhancements

### 1.1 New Environment Variables

Add to `config_loader.rs` allowlist:

```rust
// tasker-shared/src/config/config_loader.rs

const ALLOWED_ENV_VARS: &[(&str, &str)] = &[
    // ... existing vars ...
    ("TASKER_INSTANCE_ID", r"^[a-z0-9\-]+$"),           // Instance identifier
    ("TASKER_INSTANCE_PORT", r"^[0-9]{4,5}$"),         // Override port
    ("TASKER_WORKER_ID", r"^[a-z0-9\-]+$"),            // Worker identifier (fix for bootstrap)
];
```

### 1.2 Worker Bootstrap Fix

**File:** `tasker-worker/src/bootstrap.rs`

Currently generates random worker_id, ignoring configuration:
```rust
// BEFORE (broken):
worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
```

Fix to respect configuration and instance ID:
```rust
// AFTER (fixed):
worker_id: std::env::var("TASKER_WORKER_ID")
    .or_else(|_| std::env::var("TASKER_INSTANCE_ID"))
    .unwrap_or_else(|_| config.worker.worker_id.clone()),
```

### 1.3 Instance-Aware Logging

**File:** `tasker-shared/src/logging.rs` (or appropriate location)

Add instance ID to log format when available:
```rust
// If TASKER_INSTANCE_ID is set, include in log prefix
// Format: [instance-id] LEVEL target: message
```

---

## Component 2: Cargo-Make Infrastructure

### 2.1 New Environment Files

**File:** `config/dotenv/multi-instance.env`
```bash
# Multi-instance mode settings
TASKER_MULTI_INSTANCE_MODE=true
TASKER_ORCHESTRATION_INSTANCES=2
TASKER_WORKER_RUST_INSTANCES=2

# Base ports (instances add offset)
TASKER_ORCHESTRATION_BASE_PORT=8080
TASKER_WORKER_RUST_BASE_PORT=8100
TASKER_WORKER_RUBY_BASE_PORT=8200
TASKER_WORKER_PYTHON_BASE_PORT=8300
TASKER_WORKER_TS_BASE_PORT=8400
```

### 2.2 Multi-Instance Startup Script

**File:** `cargo-make/scripts/multi-deploy/start-cluster.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Multi-instance cluster startup script
# Usage: start-cluster.sh <service-type> <count> [base-port]
#
# Examples:
#   start-cluster.sh orchestration 3        # Start 3 orchestration instances
#   start-cluster.sh worker-rust 2          # Start 2 rust worker instances

SERVICE_TYPE="${1:?Service type required (orchestration, worker-rust, etc.)}"
COUNT="${2:?Instance count required}"
BASE_PORT="${3:-}"

SCRIPTS_DIR="$(dirname "$0")/.."
PROJECT_ROOT="$(cd "$SCRIPTS_DIR/../.." && pwd)"

# Determine base port from service type if not specified
if [ -z "$BASE_PORT" ]; then
    case "$SERVICE_TYPE" in
        orchestration) BASE_PORT=8080 ;;
        worker-rust)   BASE_PORT=8100 ;;
        worker-ruby)   BASE_PORT=8200 ;;
        worker-python) BASE_PORT=8300 ;;
        worker-ts)     BASE_PORT=8400 ;;
        *) echo "Unknown service type: $SERVICE_TYPE"; exit 1 ;;
    esac
fi

# Determine binary and package
case "$SERVICE_TYPE" in
    orchestration)
        BINARY="tasker-server"
        PACKAGE="tasker-orchestration"
        ;;
    worker-rust)
        BINARY="rust-worker"
        PACKAGE="workers/rust"
        ;;
    worker-ruby)
        # Ruby worker startup logic
        BINARY="ruby-worker"
        PACKAGE="workers/ruby"
        ;;
    worker-python)
        BINARY="python-worker"
        PACKAGE="workers/python"
        ;;
    worker-ts)
        BINARY="typescript-worker"
        PACKAGE="workers/typescript"
        ;;
esac

echo "Starting $COUNT $SERVICE_TYPE instance(s) from base port $BASE_PORT..."

for i in $(seq 1 "$COUNT"); do
    INSTANCE_ID="${SERVICE_TYPE}-${i}"
    PORT=$((BASE_PORT + i - 1))
    PID_FILE="${PROJECT_ROOT}/.pids/${INSTANCE_ID}.pid"
    LOG_FILE="${PROJECT_ROOT}/.logs/${INSTANCE_ID}.log"

    # Check if instance already running
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            echo "  $INSTANCE_ID: already running (PID $PID, port $PORT)"
            continue
        fi
        rm -f "$PID_FILE"
    fi

    mkdir -p "$(dirname "$PID_FILE")" "$(dirname "$LOG_FILE")"

    # Start the instance
    echo "  Starting $INSTANCE_ID on port $PORT..."

    TASKER_INSTANCE_ID="$INSTANCE_ID" \
    TASKER_INSTANCE_PORT="$PORT" \
    TASKER_WEB_BIND_ADDRESS="0.0.0.0:$PORT" \
    TASKER_WORKER_ID="$INSTANCE_ID" \
    cargo run --release -p "$PACKAGE" --bin "$BINARY" \
        >> "$LOG_FILE" 2>&1 &

    echo $! > "$PID_FILE"
    echo "  $INSTANCE_ID: started (PID $!, port $PORT, log: $LOG_FILE)"
done

echo "Done. Use 'cargo make cluster-status' to check status."
```

### 2.3 Cluster Status Script

**File:** `cargo-make/scripts/multi-deploy/cluster-status.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Display status of all running instances

PROJECT_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PID_DIR="${PROJECT_ROOT}/.pids"

if [ ! -d "$PID_DIR" ]; then
    echo "No instances running (no .pids directory)"
    exit 0
fi

echo "Instance Status:"
echo "─────────────────────────────────────────────────────────────"
printf "%-20s %-10s %-10s %-10s\n" "INSTANCE" "STATUS" "PID" "PORT"
echo "─────────────────────────────────────────────────────────────"

for pid_file in "$PID_DIR"/*.pid; do
    [ -f "$pid_file" ] || continue

    INSTANCE=$(basename "$pid_file" .pid)
    PID=$(cat "$pid_file")

    # Extract port from instance ID (e.g., orchestration-1 -> 8080)
    # This is a heuristic; actual port may differ
    case "$INSTANCE" in
        orchestration-*) BASE=8080; NUM=${INSTANCE##*-}; PORT=$((BASE + NUM - 1)) ;;
        worker-rust-*)   BASE=8100; NUM=${INSTANCE##*-}; PORT=$((BASE + NUM - 1)) ;;
        worker-ruby-*)   BASE=8200; NUM=${INSTANCE##*-}; PORT=$((BASE + NUM - 1)) ;;
        worker-python-*) BASE=8300; NUM=${INSTANCE##*-}; PORT=$((BASE + NUM - 1)) ;;
        worker-ts-*)     BASE=8400; NUM=${INSTANCE##*-}; PORT=$((BASE + NUM - 1)) ;;
        # Legacy single-instance names
        orchestration)   PORT=8080 ;;
        rust-worker)     PORT=8081 ;;
        ruby-worker)     PORT=8082 ;;
        python-worker)   PORT=8083 ;;
        ts-worker)       PORT=8085 ;;
        *) PORT="?" ;;
    esac

    if kill -0 "$PID" 2>/dev/null; then
        # Check health endpoint
        HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$PORT/health" 2>/dev/null || echo "000")
        if [ "$HTTP_CODE" = "200" ]; then
            STATUS="🟢 healthy"
        else
            STATUS="🟡 starting"
        fi
    else
        STATUS="🔴 stopped"
        rm -f "$pid_file"
    fi

    printf "%-20s %-10s %-10s %-10s\n" "$INSTANCE" "$STATUS" "$PID" "$PORT"
done

echo "─────────────────────────────────────────────────────────────"
```

### 2.4 Stop Cluster Script

**File:** `cargo-make/scripts/multi-deploy/stop-cluster.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# Stop cluster instances
# Usage: stop-cluster.sh [service-type]
#   stop-cluster.sh              # Stop all instances
#   stop-cluster.sh orchestration # Stop only orchestration instances

SERVICE_FILTER="${1:-}"
PROJECT_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PID_DIR="${PROJECT_ROOT}/.pids"

if [ ! -d "$PID_DIR" ]; then
    echo "No instances to stop"
    exit 0
fi

for pid_file in "$PID_DIR"/*.pid; do
    [ -f "$pid_file" ] || continue

    INSTANCE=$(basename "$pid_file" .pid)

    # Apply filter if specified
    if [ -n "$SERVICE_FILTER" ] && [[ ! "$INSTANCE" =~ ^${SERVICE_FILTER} ]]; then
        continue
    fi

    PID=$(cat "$pid_file")

    if kill -0 "$PID" 2>/dev/null; then
        echo "Stopping $INSTANCE (PID $PID)..."
        kill -TERM "$PID" 2>/dev/null || true

        # Wait up to 10 seconds for graceful shutdown
        for _ in $(seq 1 20); do
            if ! kill -0 "$PID" 2>/dev/null; then
                break
            fi
            sleep 0.5
        done

        # Force kill if still running
        if kill -0 "$PID" 2>/dev/null; then
            echo "  Force killing $INSTANCE..."
            kill -9 "$PID" 2>/dev/null || true
        fi
    fi

    rm -f "$pid_file"
    echo "  $INSTANCE stopped"
done
```

### 2.5 Makefile.toml Tasks

**Add to:** `Makefile.toml`

```toml
# =============================================================================
# Multi-Instance Cluster Tasks (TAS-73)
# =============================================================================

[tasks.cluster-start-orchestration]
description = "Start N orchestration instances"
script = '''
COUNT="${TASKER_ORCHESTRATION_INSTANCES:-2}"
bash "${SCRIPTS_DIR}/multi-deploy/start-cluster.sh" orchestration "$COUNT"
'''
dependencies = ["build"]

[tasks.cluster-start-workers]
description = "Start N worker instances of each type"
script = '''
RUST_COUNT="${TASKER_WORKER_RUST_INSTANCES:-2}"
bash "${SCRIPTS_DIR}/multi-deploy/start-cluster.sh" worker-rust "$RUST_COUNT"
'''
dependencies = ["build"]

[tasks.cluster-start]
description = "Start full multi-instance cluster"
dependencies = ["cluster-start-orchestration", "cluster-start-workers"]

[tasks.cluster-status]
description = "Show status of all cluster instances"
script = '''
bash "${SCRIPTS_DIR}/multi-deploy/cluster-status.sh"
'''

[tasks.cluster-stop]
description = "Stop all cluster instances"
script = '''
bash "${SCRIPTS_DIR}/multi-deploy/stop-cluster.sh"
'''

[tasks.cluster-stop-orchestration]
description = "Stop orchestration instances only"
script = '''
bash "${SCRIPTS_DIR}/multi-deploy/stop-cluster.sh" orchestration
'''

[tasks.cluster-stop-workers]
description = "Stop worker instances only"
script = '''
bash "${SCRIPTS_DIR}/multi-deploy/stop-cluster.sh" worker-rust
bash "${SCRIPTS_DIR}/multi-deploy/stop-cluster.sh" worker-ruby
bash "${SCRIPTS_DIR}/multi-deploy/stop-cluster.sh" worker-python
bash "${SCRIPTS_DIR}/multi-deploy/stop-cluster.sh" worker-ts
'''

[tasks.cluster-logs]
description = "Tail logs from all instances"
script = '''
tail -f .logs/*.log
'''

[tasks.cluster-logs-orchestration]
description = "Tail logs from orchestration instances"
script = '''
tail -f .logs/orchestration-*.log
'''
```

---

## Component 3: Test Infrastructure

### 3.1 OrchestrationCluster Abstraction

**File:** `tests/common/orchestration_cluster.rs`

```rust
use reqwest::Client;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tasker_client::api::OrchestrationApiClient;
use uuid::Uuid;

/// Load balancing strategy for distributing requests across instances
#[derive(Debug, Clone, Default)]
pub enum LoadBalancingStrategy {
    #[default]
    RoundRobin,
    Random,
    FirstHealthy,
    Sticky(Uuid), // Stick to instance based on task UUID hash
}

/// Configuration for an orchestration cluster
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    pub orchestration_urls: Vec<String>,
    pub worker_urls: Vec<String>,
    pub load_balancing: LoadBalancingStrategy,
    pub health_timeout: Duration,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            orchestration_urls: vec!["http://localhost:8080".to_string()],
            worker_urls: vec!["http://localhost:8100".to_string()],
            load_balancing: LoadBalancingStrategy::RoundRobin,
            health_timeout: Duration::from_secs(5),
        }
    }
}

impl ClusterConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        let orchestration_urls = Self::parse_urls_from_env(
            "TASKER_TEST_ORCHESTRATION_URLS",
            &["http://localhost:8080"],
        );

        let worker_urls = Self::parse_urls_from_env(
            "TASKER_TEST_WORKER_URLS",
            &["http://localhost:8100"],
        );

        Self {
            orchestration_urls,
            worker_urls,
            ..Default::default()
        }
    }

    /// Create config for N orchestration instances starting at base port
    pub fn with_orchestration_instances(count: usize, base_port: u16) -> Self {
        let orchestration_urls = (0..count)
            .map(|i| format!("http://localhost:{}", base_port + i as u16))
            .collect();

        Self {
            orchestration_urls,
            ..Default::default()
        }
    }

    fn parse_urls_from_env(var_name: &str, defaults: &[&str]) -> Vec<String> {
        std::env::var(var_name)
            .map(|s| s.split(',').map(String::from).collect())
            .unwrap_or_else(|_| defaults.iter().map(|s| s.to_string()).collect())
    }
}

/// Manages a cluster of orchestration instances for testing
pub struct OrchestrationCluster {
    clients: Vec<OrchestrationApiClient>,
    config: ClusterConfig,
    round_robin_counter: AtomicUsize,
}

impl OrchestrationCluster {
    /// Create a new cluster from configuration
    pub async fn new(config: ClusterConfig) -> anyhow::Result<Self> {
        let http_client = Client::builder()
            .timeout(config.health_timeout)
            .build()?;

        let clients: Vec<OrchestrationApiClient> = config
            .orchestration_urls
            .iter()
            .map(|url| OrchestrationApiClient::new(url.clone(), http_client.clone()))
            .collect();

        Ok(Self {
            clients,
            config,
            round_robin_counter: AtomicUsize::new(0),
        })
    }

    /// Create cluster with N instances at default ports
    pub async fn with_instances(count: usize) -> anyhow::Result<Self> {
        let config = ClusterConfig::with_orchestration_instances(count, 8080);
        Self::new(config).await
    }

    /// Wait for all instances to become healthy
    pub async fn wait_for_healthy(&self, timeout: Duration) -> anyhow::Result<()> {
        let deadline = std::time::Instant::now() + timeout;

        for (i, client) in self.clients.iter().enumerate() {
            while std::time::Instant::now() < deadline {
                match client.health().await {
                    Ok(health) if health.status == "healthy" => break,
                    _ => tokio::time::sleep(Duration::from_millis(500)).await,
                }
            }

            // Final check
            let health = client.health().await?;
            if health.status != "healthy" {
                anyhow::bail!(
                    "Instance {} ({}) did not become healthy within timeout",
                    i,
                    self.config.orchestration_urls[i]
                );
            }
        }

        Ok(())
    }

    /// Get a client using the configured load balancing strategy
    pub fn get_client(&self) -> &OrchestrationApiClient {
        match &self.config.load_balancing {
            LoadBalancingStrategy::RoundRobin => {
                let idx = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
                &self.clients[idx % self.clients.len()]
            }
            LoadBalancingStrategy::Random => {
                let idx = rand::random::<usize>() % self.clients.len();
                &self.clients[idx]
            }
            LoadBalancingStrategy::FirstHealthy => {
                // For now, just return first (could add health checking)
                &self.clients[0]
            }
            LoadBalancingStrategy::Sticky(uuid) => {
                let idx = (uuid.as_u128() as usize) % self.clients.len();
                &self.clients[idx]
            }
        }
    }

    /// Get all clients (for parallel operations or validation)
    pub fn all_clients(&self) -> &[OrchestrationApiClient] {
        &self.clients
    }

    /// Get specific client by index
    pub fn client(&self, index: usize) -> Option<&OrchestrationApiClient> {
        self.clients.get(index)
    }

    /// Number of instances in the cluster
    pub fn instance_count(&self) -> usize {
        self.clients.len()
    }
}
```

### 3.2 MultiInstanceTestManager

**File:** `tests/common/multi_instance_test_manager.rs`

```rust
use super::orchestration_cluster::{ClusterConfig, OrchestrationCluster};
use std::time::Duration;

/// Test manager for multi-instance scenarios
pub struct MultiInstanceTestManager {
    pub cluster: OrchestrationCluster,
    pub worker_urls: Vec<String>,
}

impl MultiInstanceTestManager {
    /// Setup with N orchestration instances and M worker instances
    pub async fn setup(
        orchestration_count: usize,
        worker_count: usize,
    ) -> anyhow::Result<Self> {
        let cluster = OrchestrationCluster::with_instances(orchestration_count).await?;

        // Wait for instances to become healthy
        cluster.wait_for_healthy(Duration::from_secs(30)).await?;

        let worker_urls = (0..worker_count)
            .map(|i| format!("http://localhost:{}", 8100 + i))
            .collect();

        Ok(Self {
            cluster,
            worker_urls,
        })
    }

    /// Setup from environment (for CI/external service mode)
    pub async fn setup_from_env() -> anyhow::Result<Self> {
        let config = ClusterConfig::from_env();
        let cluster = OrchestrationCluster::new(config.clone()).await?;

        cluster.wait_for_healthy(Duration::from_secs(30)).await?;

        Ok(Self {
            cluster,
            worker_urls: config.worker_urls,
        })
    }

    /// Create N tasks concurrently across the cluster
    pub async fn create_tasks_concurrent(
        &self,
        requests: Vec<TaskRequest>,
    ) -> anyhow::Result<Vec<TaskResponse>> {
        use futures::future::join_all;

        let futures: Vec<_> = requests
            .into_iter()
            .map(|req| {
                let client = self.cluster.get_client();
                async move { client.create_task(req).await }
            })
            .collect();

        let results = join_all(futures).await;
        results.into_iter().collect()
    }

    /// Verify task state is consistent across all orchestration instances
    pub async fn verify_task_consistency(&self, task_uuid: Uuid) -> anyhow::Result<()> {
        let mut states = Vec::new();

        for client in self.cluster.all_clients() {
            let task = client.get_task(task_uuid).await?;
            states.push(task.state.clone());
        }

        // All instances should report the same state
        let first_state = &states[0];
        for (i, state) in states.iter().enumerate() {
            if state != first_state {
                anyhow::bail!(
                    "State inconsistency: instance 0 reports {:?}, instance {} reports {:?}",
                    first_state,
                    i,
                    state
                );
            }
        }

        Ok(())
    }
}
```

### 3.3 Extended IntegrationTestManager

**File:** `tests/common/integration_test_manager.rs` (modifications)

```rust
// Add to existing IntegrationTestManager

impl IntegrationTestManager {
    /// Setup with multiple orchestration endpoints (new method)
    pub async fn setup_multi_orchestration(
        orchestration_urls: Vec<String>,
    ) -> Result<MultiInstanceTestManager> {
        let config = ClusterConfig {
            orchestration_urls,
            ..Default::default()
        };

        let cluster = OrchestrationCluster::new(config).await?;
        cluster.wait_for_healthy(Duration::from_secs(30)).await?;

        Ok(MultiInstanceTestManager {
            cluster,
            worker_urls: vec![], // Can be configured separately
        })
    }
}
```

---

## Component 4: Example Tests

### 4.1 Step Claiming Contention Test

**File:** `tests/integration/concurrency/step_claiming_contention_test.rs`

```rust
//! Tests that multiple workers competing for the same queue
//! correctly claim steps without double-processing.

use common::{MultiInstanceTestManager, LifecycleTestManager};
use std::collections::HashSet;
use std::time::Duration;

#[tokio::test]
async fn test_multiple_workers_no_duplicate_claims() -> anyhow::Result<()> {
    // Setup: 2 orchestration instances, 3 worker instances
    let manager = MultiInstanceTestManager::setup(2, 3).await?;

    // Create a task with many steps (fan-out pattern)
    let task_request = create_fan_out_task_request(10); // 10 parallel steps
    let task_response = manager.cluster.get_client().create_task(task_request).await?;

    // Wait for completion
    wait_for_task_completion(
        manager.cluster.get_client(),
        task_response.task_uuid,
        Duration::from_secs(60),
    ).await?;

    // Verify: Each step was processed exactly once
    let steps = manager.cluster.get_client()
        .get_task_steps(task_response.task_uuid)
        .await?;

    let mut processed_by: HashSet<String> = HashSet::new();
    for step in steps {
        assert_eq!(step.state, "complete", "Step {} not complete", step.step_uuid);

        // Check transition history for duplicate InProgress transitions
        let transitions = get_step_transitions(step.step_uuid).await?;
        let in_progress_count = transitions
            .iter()
            .filter(|t| t.to_state == "in_progress")
            .count();

        assert_eq!(
            in_progress_count, 1,
            "Step {} was claimed {} times (expected 1)",
            step.step_uuid, in_progress_count
        );

        if let Some(processor) = &step.processor_uuid {
            processed_by.insert(processor.clone());
        }
    }

    // Verify work was distributed across workers
    assert!(
        processed_by.len() > 1,
        "All steps processed by single worker; expected distribution"
    );

    Ok(())
}
```

### 4.2 Orchestrator Race Test

**File:** `tests/integration/concurrency/orchestrator_race_test.rs`

```rust
//! Tests that multiple orchestrators processing the same task
//! coordinate correctly without conflicts.

#[tokio::test]
async fn test_concurrent_orchestrators_same_task() -> anyhow::Result<()> {
    // Setup: 3 orchestration instances
    let manager = MultiInstanceTestManager::setup(3, 1).await?;

    // Create a task via instance 1
    let task_request = create_simple_task_request();
    let task_response = manager.cluster.client(0).unwrap()
        .create_task(task_request)
        .await?;

    // Query task state from all instances concurrently
    let futures: Vec<_> = manager.cluster.all_clients()
        .iter()
        .map(|client| client.get_task(task_response.task_uuid))
        .collect();

    let results = futures::future::join_all(futures).await;

    // All instances should see the same task
    for result in results {
        let task = result?;
        assert_eq!(task.task_uuid, task_response.task_uuid);
    }

    // Wait for completion via round-robin queries
    wait_for_task_completion_via_cluster(
        &manager.cluster,
        task_response.task_uuid,
        Duration::from_secs(30),
    ).await?;

    // Verify final state consistency
    manager.verify_task_consistency(task_response.task_uuid).await?;

    // Verify only one Complete transition exists
    let transitions = get_task_transitions(task_response.task_uuid).await?;
    let complete_count = transitions
        .iter()
        .filter(|t| t.to_state == "complete")
        .count();

    assert_eq!(
        complete_count, 1,
        "Task finalized {} times (expected 1)",
        complete_count
    );

    Ok(())
}
```

---

## Implementation Plan

### Phase 1: Configuration Foundation (Day 1)

1. Add `TASKER_INSTANCE_ID`, `TASKER_INSTANCE_PORT`, `TASKER_WORKER_ID` to config_loader.rs allowlist
2. Fix worker bootstrap to respect configured/env worker_id
3. Add instance ID to logging format
4. Create `config/dotenv/multi-instance.env`

### Phase 2: Cargo-Make Scripts (Day 1-2)

1. Create `cargo-make/scripts/multi-deploy/` directory
2. Implement `start-cluster.sh`, `stop-cluster.sh`, `cluster-status.sh`
3. Add cluster tasks to `Makefile.toml`
4. Test basic multi-instance startup/shutdown

### Phase 3: Test Infrastructure (Day 2-3)

1. Create `tests/common/orchestration_cluster.rs`
2. Create `tests/common/multi_instance_test_manager.rs`
3. Add multi-instance setup methods to existing IntegrationTestManager
4. Create module exports in `tests/common/mod.rs`

### Phase 4: Initial Tests (Day 3-4)

1. Implement `step_claiming_contention_test.rs`
2. Implement `orchestrator_race_test.rs`
3. Implement `duplicate_result_test.rs`
4. Validate all tests pass with N=2 instances

---

## Success Criteria

1. **Instance isolation**: Each instance has unique ID, logs to separate file, binds to unique port
2. **Clean startup/shutdown**: `cargo make cluster-start` and `cargo make cluster-stop` work reliably
3. **Test infrastructure ready**: `MultiInstanceTestManager` can setup N orchestrations + M workers
4. **Round-robin works**: Requests distributed across instances
5. **State consistency**: All instances report same task/step states
6. **No duplicate processing**: Steps claimed exactly once despite multiple workers
7. **Backward compatible**: Single-instance mode (`cargo make services-start`) still works

---

## Open Questions

1. **Instance discovery for workers**: Should workers discover orchestration instances via config, or use a fixed list?
   - **Proposal**: Use `TASKER_ORCHESTRATION_URL` for single-instance, `TASKER_ORCHESTRATION_URLS` (comma-separated) for multi-instance

2. **Health check aggregation**: Should `cluster-status` report aggregate health or per-instance?
   - **Proposal**: Per-instance status with summary line

3. **Log aggregation**: Should we implement log aggregation for multi-instance, or rely on `tail -f .logs/*.log`?
   - **Proposal**: Start with `tail -f`, add structured logging later if needed

---

## References

- `docs/ticket-specs/TAS-73/research-findings.md` - Research findings
- `docs/architecture/deployment-patterns.md` - Existing deployment documentation
- `cargo-make/scripts/service-*.sh` - Existing service management
- `tests/common/integration_test_manager.rs` - Current test infrastructure
