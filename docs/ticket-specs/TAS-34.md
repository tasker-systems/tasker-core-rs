# TAS-34: OrchestrationExecutor Trait and OrchestrationLoopCoordinator Architecture

## Executive Summary

This ticket introduces a sophisticated orchestration architecture to replace the current naive tokio async polling loops with a scalable, monitorable, and self-managing executor pool system. The new architecture provides horizontal scaling, health monitoring, backpressure management, and dynamic load balancing for the orchestration system.

## Current State Analysis

### Existing Implementation

The current orchestration system (`src/orchestration/orchestration_system.rs`) uses a simple approach:

```rust
// Current naive implementation
pub async fn start(&self) -> Result<ContinuousOrchestrationSummary> {
    // Simply spawns three independent tokio tasks
    let task_request_handle = tokio::spawn(async move {
        task_request_processor.start_task_request_processing_loop().await
    });
    
    let orchestration_handle = tokio::spawn(async move { 
        orchestration_loop.run_continuous().await 
    });
    
    let step_results_handle = tokio::spawn(async move { 
        step_result_processor.start_processing_loop().await 
    });
    
    // Wait for all three loops
    tokio::join!(task_request_handle, orchestration_handle, step_results_handle)
}
```

### Current Limitations

1. **No Horizontal Scaling**: Each loop is a single task that blocks on processing
2. **Limited Throughput Control**: Only polling intervals can be adjusted
3. **No Health Monitoring**: No way to detect degraded or stuck executors
4. **No Backpressure Management**: Can overwhelm database connection pool
5. **No Dynamic Scaling**: Cannot respond to load changes
6. **Limited Observability**: Basic metrics only, no per-executor tracking
7. **No Graceful Degradation**: All-or-nothing operation

### Existing Assets to Leverage

1. **Circuit Breaker** (`src/resilience/circuit_breaker.rs`): Mature implementation with three states (Closed, Open, Half-Open)
2. **Database Pool**: SQLx PgPool with connection management
3. **pgmq Integration**: Queue-based architecture already in place
4. **Metrics Infrastructure**: Basic metrics collection exists

## Proposed Architecture

### Core Components

#### 1. OrchestrationExecutor Trait

A standard interface for orchestration workers that can be pooled, monitored, and dynamically scaled.

```rust
#[async_trait]
pub trait OrchestrationExecutor: Send + Sync {
    fn id(&self) -> Uuid;
    fn executor_type(&self) -> ExecutorType;
    async fn start(&self) -> Result<(), TaskerError>;
    async fn stop(&self, timeout: Duration) -> Result<(), TaskerError>;
    async fn health(&self) -> ExecutorHealth;
    async fn metrics(&self) -> ExecutorMetrics;
    async fn process_batch(&self) -> Result<ProcessBatchResult, TaskerError>;
    async fn heartbeat(&self) -> Result<(), TaskerError>;
    fn should_continue(&self) -> bool;
    async fn apply_backpressure(&self, factor: f64) -> Result<(), TaskerError>;
}
```

##### Executor Types

```rust
pub enum ExecutorType {
    TaskRequestProcessor,   // Processes incoming task requests
    TaskClaimer,           // Claims ready tasks from queue
    StepEnqueuer,          // Enqueues viable steps
    StepResultProcessor,   // Processes step completion results
    TaskFinalizer,         // Handles task completion
}
```

##### Health States

```rust
pub enum ExecutorHealth {
    Healthy { last_heartbeat, items_processed, avg_processing_time_ms },
    Degraded { last_heartbeat, reason, error_rate },
    Unhealthy { last_seen, reason },
    Starting { started_at },
    Stopping { initiated_at, graceful },
}
```

#### 2. OrchestrationLoopCoordinator

Manages pools of orchestration executors with dynamic scaling and health monitoring.

```rust
pub struct OrchestrationLoopCoordinator {
    executor_pools: Arc<DashMap<ExecutorType, ExecutorPool>>,
    config: CoordinatorConfig,
    db_pool: PgPool,
    circuit_breakers: Arc<DashMap<ExecutorType, Arc<CircuitBreaker>>>,
    scaling_state: Arc<RwLock<ScalingState>>,
    db_semaphore: Arc<Semaphore>,
}
```

##### Key Responsibilities

1. **Pool Management**: Maintains min/max executors per type
2. **Health Monitoring**: Regular health checks and heartbeats
3. **Auto-scaling**: Scales based on load and performance metrics
4. **Backpressure**: Responds to database pool saturation
5. **Circuit Breaking**: Integrates with existing circuit breaker
6. **Metrics Aggregation**: Collects and aggregates executor metrics

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                 OrchestrationLoopCoordinator                 │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                   Health Monitor                       │  │
│  │  - Heartbeat tracking                                 │  │
│  │  - Health state evaluation                            │  │
│  │  - Unhealthy executor removal                         │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │                  Scaling Monitor                       │  │
│  │  - Load evaluation                                    │  │
│  │  - Auto-scaling decisions                             │  │
│  │  - Backpressure application                          │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              Executor Pools                          │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │                                                      │    │
│  │  ┌──────────────────────────────────────────┐      │    │
│  │  │   TaskRequestProcessor Pool (1-N)        │      │    │
│  │  │   [Executor 1] [Executor 2] ... [N]      │      │    │
│  │  └──────────────────────────────────────────┘      │    │
│  │                                                      │    │
│  │  ┌──────────────────────────────────────────┐      │    │
│  │  │   TaskClaimer Pool (1-N)                 │      │    │
│  │  │   [Executor 1] [Executor 2] ... [N]      │      │    │
│  │  └──────────────────────────────────────────┘      │    │
│  │                                                      │    │
│  │  ┌──────────────────────────────────────────┐      │    │
│  │  │   StepEnqueuer Pool (1-N)                │      │    │
│  │  │   [Executor 1] [Executor 2] ... [N]      │      │    │
│  │  └──────────────────────────────────────────┘      │    │
│  │                                                      │    │
│  │  ┌──────────────────────────────────────────┐      │    │
│  │  │   StepResultProcessor Pool (1-N)         │      │    │
│  │  │   [Executor 1] [Executor 2] ... [N]      │      │    │
│  │  └──────────────────────────────────────────┘      │    │
│  │                                                      │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │            Resource Management                         │  │
│  │  - Database pool monitoring                           │  │
│  │  - Connection semaphore                               │  │
│  │  - Circuit breakers per executor type                 │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

### Scaling Algorithm

```rust
async fn determine_scaling_action(
    &self,
    executor_type: ExecutorType,
    pool: &ExecutorPool,
    system_load: &SystemLoad,
) -> ScalingAction {
    let current_count = pool.executors.len();
    let min_count = self.config.min_executors[&executor_type];
    let max_count = self.config.max_executors[&executor_type];
    
    // Calculate utilization
    let utilization = pool.metrics.avg_utilization;
    let target = self.config.target_utilization;
    
    // Scale up conditions
    if utilization > target * 1.2 && current_count < max_count {
        let scale_factor = (utilization / target).min(2.0);
        let new_count = ((current_count as f64) * scale_factor).ceil() as usize;
        let add_count = (new_count - current_count).min(max_count - current_count);
        return ScalingAction::ScaleUp { count: add_count };
    }
    
    // Scale down conditions
    if utilization < target * 0.5 && current_count > min_count {
        let remove_count = ((current_count - min_count) / 2).max(1);
        return ScalingAction::ScaleDown { count: remove_count };
    }
    
    ScalingAction::NoChange
}
```

### Backpressure Management

```rust
async fn apply_backpressure(&self, saturation: f64) -> Result<(), TaskerError> {
    // Calculate backpressure factor (0.0 = full stop, 1.0 = normal)
    let factor = (1.0 - saturation).max(0.1); // Never go below 10%
    
    // Apply to all executors
    for pool in self.executor_pools.iter() {
        for executor in &pool.executors {
            executor.apply_backpressure(factor).await?;
        }
    }
    
    // Executor adjusts its behavior:
    // - Increases polling interval
    // - Reduces batch size
    // - Adds delays between operations
}
```

## Implementation Plan

### Phase 1: Core Traits and Base Implementation

**Files to Create:**
- `src/orchestration/executor/mod.rs` - Module organization
- `src/orchestration/executor/traits.rs` - Core traits
- `src/orchestration/executor/base.rs` - Base implementation
- `src/orchestration/executor/metrics.rs` - Metrics types
- `src/orchestration/executor/health.rs` - Health management

**Key Tasks:**
1. Define `OrchestrationExecutor` trait
2. Implement `BaseExecutor` with state management
3. Create health and metrics tracking
4. Add lifecycle management (start/stop/heartbeat)
5. Integrate with existing circuit breaker

### Phase 2: Coordinator Framework

**Files to Create:**
- `src/orchestration/coordinator/mod.rs` - Coordinator implementation
- `src/orchestration/coordinator/scaling.rs` - Auto-scaling logic
- `src/orchestration/coordinator/monitor.rs` - Monitoring loops
- `src/orchestration/coordinator/pool.rs` - Pool management

**Key Tasks:**
1. Implement `OrchestrationLoopCoordinator`
2. Create executor pool management
3. Add health monitoring loop
4. Implement metrics aggregation
5. Add configuration management

### Phase 3: Concrete Executors

**Files to Create:**
- `src/orchestration/executors/task_request.rs`
- `src/orchestration/executors/task_claim.rs`
- `src/orchestration/executors/step_enqueue.rs`
- `src/orchestration/executors/step_result.rs`

**Key Tasks:**
1. Migrate existing processors to executor pattern
2. Implement `process_batch()` for each type
3. Add executor-specific metrics
4. Handle backpressure per executor type
5. Add comprehensive error handling

### Phase 4: Integration and Migration

**Files to Update:**
- `src/orchestration/orchestration_system.rs` - Use new coordinator
- `src/orchestration/config.rs` - Add coordinator configuration
- `src/orchestration/mod.rs` - Export new modules

**Key Tasks:**
1. Replace existing `start()` method
2. Update configuration structures
3. Add backward compatibility layer
4. Migrate existing tests
5. Add integration tests

## Configuration Schema

```yaml
orchestration:
  coordinator:
    auto_scaling_enabled: true
    target_utilization: 0.75
    scaling_interval_seconds: 30
    health_check_interval_seconds: 10
    scaling_cooldown_seconds: 60
    max_db_pool_usage: 0.85
    
  executor_pools:
    task_request_processor:
      min_executors: 1
      max_executors: 5
      polling_interval_ms: 100
      batch_size: 10
      circuit_breaker_enabled: true
      circuit_breaker_threshold: 5
      
    task_claimer:
      min_executors: 2
      max_executors: 10
      polling_interval_ms: 50
      batch_size: 20
      circuit_breaker_enabled: true
      circuit_breaker_threshold: 3
      
    step_enqueuer:
      min_executors: 2
      max_executors: 8
      polling_interval_ms: 50
      batch_size: 50
      circuit_breaker_enabled: true
      circuit_breaker_threshold: 5
      
    step_result_processor:
      min_executors: 2
      max_executors: 10
      polling_interval_ms: 100
      batch_size: 20
      circuit_breaker_enabled: true
      circuit_breaker_threshold: 3
```

## Testing Strategy

### Unit Tests

1. **Executor Tests**
   - Lifecycle management (start/stop)
   - Health state transitions
   - Metrics collection
   - Backpressure response

2. **Coordinator Tests**
   - Pool management
   - Scaling decisions
   - Health monitoring
   - Circuit breaker integration

### Integration Tests

1. **Scaling Behavior**
   - Scale up under load
   - Scale down when idle
   - Respect min/max boundaries
   - Cooldown periods

2. **Failure Handling**
   - Executor failure recovery
   - Circuit breaker activation
   - Graceful degradation
   - Database pool exhaustion

3. **Performance Tests**
   - Throughput improvements
   - Latency under load
   - Resource utilization
   - Concurrent executor performance

## Monitoring and Observability

### Metrics to Track

**Per Executor:**
- Items processed per second
- Average processing time
- Error rate
- Health state duration
- Resource usage (memory, connections)

**Per Pool:**
- Total throughput
- Average utilization
- Healthy vs unhealthy ratio
- Scaling events
- Queue depths

**System Level:**
- Database pool saturation
- Total active executors
- Circuit breaker states
- Backpressure events
- Overall error rate

### Health Checks

```rust
GET /health/orchestration
{
  "status": "healthy",
  "coordinator": {
    "running": true,
    "uptime_seconds": 3600
  },
  "executor_pools": {
    "task_request_processor": {
      "healthy": 3,
      "unhealthy": 0,
      "throughput": 150.5
    },
    "task_claimer": {
      "healthy": 5,
      "unhealthy": 1,
      "throughput": 320.2
    }
  },
  "database": {
    "pool_size": 50,
    "active_connections": 35,
    "saturation": 0.70
  }
}
```

## Success Criteria

1. **Throughput Improvement**: 3-5x increase in task processing throughput
2. **Horizontal Scalability**: Linear scaling with executor count
3. **Resilience**: Automatic recovery from executor failures
4. **Resource Efficiency**: Database pool utilization stays below 85%
5. **Observability**: Complete visibility into executor health and performance
6. **Backward Compatibility**: Existing configurations continue to work

## Risks and Mitigations

### Risk 1: Increased Complexity
**Mitigation**: Phased implementation with thorough testing at each phase

### Risk 2: Database Pool Exhaustion
**Mitigation**: Semaphore-based connection limiting and backpressure

### Risk 3: Scaling Oscillation
**Mitigation**: Cooldown periods and hysteresis in scaling decisions

### Risk 4: Memory Growth
**Mitigation**: Executor lifecycle management and periodic restarts

## Related Work

- **TAS-32**: Step Result Coordination Processor (completed)
- **TAS-14**: Ruby Integration Testing (in progress)
- **TAS-35**: Coordinator Framework (follow-up)
- **TAS-36**: Concrete Executors (follow-up)
- **TAS-37**: Scaling and Monitoring (follow-up)
- **TAS-38**: Configuration and Testing (follow-up)

## References

- Current orchestration system: `src/orchestration/orchestration_system.rs`
- Circuit breaker: `src/resilience/circuit_breaker.rs`
- Current configuration: `src/orchestration/config.rs`
- pgmq integration: `src/messaging/pgmq_client.rs`
