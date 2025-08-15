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

## Resource Management and Configuration Architecture

### Critical Resource Constraint Issues

**PROBLEM IDENTIFIED**: Current configuration allows executor pools to exceed system resource limits:

- **Production Environment**: Executor pools can spawn 73 executors (10+20+15+20+8) but database pool only has 50 connections
- **Test Environment**: Could spawn 10 executors but only 25 database connections
- **Database Constraint**: Primary bottleneck is database connection pool exhaustion

**IMPACT**: Database pool exhaustion, connection timeouts, system instability under load.

### Understanding Tokio's Threading Model

**Why CPU cores don't limit async executors:**

```rust
// TRADITIONAL THREADING (1:1 model)
// 1 executor = 1 system thread = 1 core needed
for i in 0..num_cores {
    std::thread::spawn(|| {
        // This blocks a system thread
        process_tasks();
    });
}

// TOKIO ASYNC (M:N model) 
// Many executors = few system threads = efficient core usage
for i in 0..1000 { // Can spawn thousands!
    tokio::spawn(async {
        // This is an async task that yields at .await
        process_tasks().await;
    });
}
// Tokio scheduler maps all 1000 async tasks to ~8 system threads
```

**Key Insight**: Our executors are async tasks that spend most of their time waiting (polling queues, waiting for database responses). When they hit an `.await`, they yield control and Tokio can run other tasks on the same system thread. This is why we can run many more executors than CPU cores.

### Proposed Resource Management Architecture

#### 1. System Resource Detection

```rust
pub struct SystemResourceLimits {
    pub database_max_connections: usize,
    pub available_memory_mb: usize,
    pub cpu_cores: usize, // Informational only - not used for limiting executors
}

impl SystemResourceLimits {
    pub fn detect(db_pool: &PgPool) -> Self {
        let database_max_connections = db_pool.size() as usize;
        let available_memory_mb = Self::get_available_memory();
        let cpu_cores = num_cpus::get(); // For monitoring only
        
        Self {
            database_max_connections,
            available_memory_mb,
            cpu_cores, // Not used for executor limiting!
        }
    }
    
    /// Calculate maximum safe executors based on real constraints
    pub fn max_safe_executors(&self) -> usize {
        // Primary constraint: database connections (85% utilization for safety)
        let db_based_limit = (self.database_max_connections as f64 * 0.85) as usize;
        
        // Secondary constraint: memory (rough estimate: 50MB per executor)
        let memory_based_limit = self.available_memory_mb / 50;
        
        // Return the most restrictive limit
        db_based_limit.min(memory_based_limit)
        
        // NOTE: CPU cores are NOT used for limiting! Tokio's M:N threading
        // model allows many async executors to run efficiently on few system threads.
    }
}
```

#### 2. Resource Budget Allocation

```rust
pub struct ResourceBudget {
    pub total_executor_limit: usize,
    pub allocated_executors: HashMap<ExecutorType, usize>,
    pub reserved_connections: usize, // For system overhead
}

impl ResourceBudget {
    pub fn calculate(limits: &SystemResourceLimits, config: &ExecutorPoolsConfig) -> Result<Self> {
        let reserved_connections = 5; // System overhead
        let available_for_executors = limits.database_max_connections.saturating_sub(reserved_connections);
        
        // Calculate total requested executors
        let total_requested: usize = config.iter()
            .map(|(_, pool_config)| pool_config.max_executors)
            .sum();
        
        if total_requested > available_for_executors {
            return Err(TaskerError::ConfigurationError(
                format!("Executor pool configuration requests {} executors but only {} database connections available", 
                        total_requested, available_for_executors)
            ));
        }
        
        Ok(Self {
            total_executor_limit: available_for_executors,
            allocated_executors: HashMap::new(),
            reserved_connections,
        })
    }
}
```

#### 3. Configuration Validation

```rust
impl OrchestrationLoopCoordinator {
    pub async fn new_with_resource_validation(config: CoordinatorConfig, db_pool: PgPool) -> Result<Self> {
        // Detect system resources
        let system_limits = SystemResourceLimits::detect(&db_pool);
        
        // Validate configuration against resources
        let resource_budget = ResourceBudget::calculate(&system_limits, &config.executor_pools)?;
        
        info!(
            cpu_cores = system_limits.cpu_cores,
            max_db_connections = system_limits.database_max_connections,
            max_total_executors = resource_budget.total_executor_limit,
            "System resources detected and validated"
        );
        
        Ok(Self::with_resource_budget(config, db_pool, resource_budget).await?)
    }
}
```

### Configuration Decomposition Architecture

#### Problem: Monolithic Configuration File

Current `config/tasker-config.yaml` has grown to **630 lines** with mixed concerns, making it difficult to:
- Understand component relationships
- Manage environment-specific overrides  
- Separate Ruby worker configuration from Rust orchestration
- Validate cross-component resource constraints

#### Proposed Component-Based Structure

```
config/
├── tasker/
│   ├── database.yaml           # Database connection and pooling
│   ├── orchestration.yaml      # Core orchestration settings
│   ├── executor_pools.yaml     # Executor pool configurations
│   ├── circuit_breakers.yaml   # Resilience settings
│   ├── pgmq.yaml              # Message queue settings
│   └── telemetry.yaml         # Monitoring and logging
├── tasker/environments/
│   ├── test/
│   │   ├── database.yaml       # Test-specific overrides
│   │   ├── executor_pools.yaml # Smaller pools for tests
│   │   └── circuit_breakers.yaml
│   ├── development/
│   │   └── database.yaml
│   └── production/
│       ├── database.yaml       # Production scale settings
│       ├── executor_pools.yaml # High-throughput pools
│       └── telemetry.yaml      # Production monitoring
└── shared/
    └── queue_names.yaml        # Shared between Rust/Ruby
```

#### Component File Examples

**config/tasker/database.yaml:**
```yaml
# Database configuration component
database:
  adapter: postgresql
  encoding: unicode
  pool:
    max_connections: 25
    min_connections: 5
    acquire_timeout_seconds: 30
    idle_timeout_seconds: 300
    max_lifetime_seconds: 3600
  variables:
    statement_timeout: 5000
  checkout_timeout: 10
  reaping_frequency: 10
```

**config/tasker/executor_pools.yaml:**
```yaml
# Executor pool configuration component
executor_pools:
  coordinator:
    auto_scaling_enabled: true
    target_utilization: 0.75
    resource_validation_enabled: true
    max_db_pool_usage: 0.85
  
  # Pool definitions with resource awareness
  task_request_processor:
    min_executors: 1
    max_executors: 5
    resource_weight: 1.0  # Lower priority for scaling
  
  step_result_processor:
    min_executors: 2
    max_executors: 10
    resource_weight: 2.0  # Higher priority for scaling
```

**config/tasker/environments/production/executor_pools.yaml:**
```yaml
# Production overrides with resource validation
executor_pools:
  coordinator:
    target_utilization: 0.70  # More conservative in production
    scaling_cooldown_seconds: 120
  
  task_request_processor:
    max_executors: 8  # Higher for production
  
  step_result_processor:
    max_executors: 15  # Higher for production
    
# CONSTRAINT VALIDATION:
# Total max executors: 8 + 15 + ... = must not exceed system limits
```

#### Configuration Loader Updates

```rust
pub struct ComponentConfigLoader {
    base_path: PathBuf,
    environment: String,
}

impl ComponentConfigLoader {
    /// Load configuration from component files with environment overrides
    pub async fn load_config(&self) -> Result<TaskerConfig> {
        // 1. Load base component configurations
        let mut config = self.load_base_components().await?;
        
        // 2. Apply environment overrides
        self.apply_environment_overrides(&mut config).await?;
        
        // 3. Validate cross-component constraints
        self.validate_resource_constraints(&config)?;
        
        // 4. Detect and support legacy monolithic config for backward compatibility
        if !self.has_component_structure().await {
            warn!("Using legacy monolithic config - consider migrating to component structure");
            return self.load_legacy_config().await;
        }
        
        Ok(config)
    }
    
    fn validate_resource_constraints(&self, config: &TaskerConfig) -> Result<()> {
        let total_max_executors: usize = config.executor_pools.values()
            .map(|pool| pool.max_executors)
            .sum();
            
        let db_connections = config.database.pool.max_connections;
        let available_for_executors = (db_connections as f64 * 0.85) as usize;
        
        if total_max_executors > available_for_executors {
            return Err(TaskerError::ConfigurationError(format!(
                "Resource constraint violation: {} max executors requested but only {} database connections available",
                total_max_executors, available_for_executors
            )));
        }
        
        Ok(())
    }
}
```

### Ruby/Rust Configuration Separation

#### Problem: Deployment Coupling

Currently Ruby workers and Rust orchestration share the same configuration directory, creating deployment coupling. Ruby gems deployed in Rails applications can't depend on the full Rust repository configuration structure.

#### Proposed Solution: Independent Configuration with Shared Elements

**Rust Configuration (unchanged):**
```
config/tasker/           # Full Rust configuration
├── database.yaml
├── executor_pools.yaml  # Rust-only
├── orchestration.yaml   # Rust-only
└── shared/
    └── queue_names.yaml # Shared template
```

**Ruby Configuration (new location):**
```
bindings/ruby/config/tasker/
├── database.yaml        # Copied/templated from Rust
├── pgmq.yaml           # Ruby worker queue settings
├── circuit_breakers.yaml # Ruby worker resilience
└── queue_names.yaml    # Copied from shared template
```

#### Configuration Synchronization Tool

```bash
# Tool to sync shared configuration between Rust and Ruby
./scripts/sync-shared-config.sh
```

```bash
#!/bin/bash
# Sync shared configuration elements from Rust to Ruby
RUST_CONFIG_DIR="config/tasker"
RUBY_CONFIG_DIR="bindings/ruby/config/tasker"

# Create Ruby config directory
mkdir -p "$RUBY_CONFIG_DIR"

# Copy shared elements
cp "$RUST_CONFIG_DIR/shared/queue_names.yaml" "$RUBY_CONFIG_DIR/"

# Template database configuration (remove Rust-specific elements)
sed 's/executor_pools:.*//' "$RUST_CONFIG_DIR/database.yaml" > "$RUBY_CONFIG_DIR/database.yaml"

echo "Shared configuration synchronized"
```

### Implementation Plan

#### ✅ Phase 0: Unified Orchestration Architecture (Foundation) - COMPLETED
**Timeline**: ~~2-3 days~~ **COMPLETED August 15, 2025**
**Goal**: Replace multiple bootstrapping strategies with single OrchestrationLoopCoordinator path ✅

**IMPLEMENTATION COMPLETED:**

All orchestration paths now use the unified `OrchestrationLoopCoordinator` architecture:

1. ✅ **`bootstrap.rs`** - All bootstrap methods delegate to `OrchestrationLoopCoordinator::start()`
   - `bootstrap_embedded()` uses unified `bootstrap()` method
   - `bootstrap_standalone()` and `bootstrap_testing()` use coordinator
   - Single entry point achieved

2. ✅ **`orchestration_system.rs`** - `start()` method creates and delegates to coordinator
   ```rust
   // Line 257: Unified coordinator delegation
   coordinator.start().await?;
   ```

3. ✅ **`embedded_bridge.rs`** - Uses coordinator-based bootstrap
   ```rust
   // Line 253: Uses unified bootstrap
   OrchestrationBootstrap::bootstrap_embedded(namespaces).await
   // Line 266: "bootstrapped successfully using OrchestrationLoopCoordinator"
   ```

**UNIFIED ARCHITECTURE ACHIEVED:**
```rust
// ALL PATHS NOW USE: Single unified approach
ALL PATHS -> OrchestrationLoopCoordinator::start()
  ├── bootstrap.rs -> OrchestrationLoopCoordinator
  ├── orchestration_system.rs -> OrchestrationLoopCoordinator  
  └── embedded_bridge.rs -> OrchestrationLoopCoordinator
```

**PRODUCTION READY FEATURES:**
- 🎯 **Single Entry Point**: All deployment modes use `OrchestrationLoopCoordinator`
- 🎯 **Unified Lifecycle**: Consistent start/stop/status across all modes
- 🎯 **Resource Management**: Integrated resource validation (Phase 1 completion)
- 🎯 **Health Monitoring**: Built-in health checks and metrics
- 🎯 **Dynamic Scaling**: Auto-scaling executor pools based on load

#### ✅ Phase 1: Resource Constraint Validation (COMPLETED)
**Timeline**: ~~1-2 days~~ **COMPLETED August 15, 2025**
**Goal**: Prevent database pool exhaustion in current system ✅

**COMPLETED TASKS:**
1. ✅ **SystemResourceLimits::detect()** - Implemented in `src/orchestration/coordinator/resource_limits.rs`
2. ✅ **Startup validation** - Integrated in `OrchestrationLoopCoordinator::start()`
3. ✅ **Fail fast validation** - Configuration validation with immediate error
4. ✅ **Memory & CPU detection** - Real system resource detection using sysinfo crate
5. ✅ **Pool statistics** - Real database pool monitoring instead of placeholders
6. ✅ **Queue metrics** - Real pgmq metrics instead of placeholder implementations

**IMPLEMENTATION COMPLETED:**
```rust
// WORKING IMPLEMENTATION in src/orchestration/coordinator/resource_limits.rs
impl SystemResourceLimits {
    pub async fn detect(database_pool: &PgPool) -> Result<Self> {
        // Real implementation with:
        // - Actual database connection detection
        // - Memory detection via sysinfo crate  
        // - CPU core detection
        // - Resource utilization calculations
        // - Comprehensive validation warnings
    }
}

impl OrchestrationLoopCoordinator {
    pub async fn start(&self) -> Result<()> {
        // PHASE 1: Resource Constraint Validation (TAS-34)
        let resource_validator = ResourceValidator::new(
            self.orchestration_core.database_pool(),
            self.config_manager.clone(),
        ).await?;

        let validation_result = resource_validator.validate_and_fail_fast().await?;
        
        // Fail fast if configuration exceeds resource limits
        // Proceeds only if validation passes
    }
}
```

**PRODUCTION READY FEATURES:**
- 🎯 **Database Pool Protection**: Prevents 38 executors from overwhelming 25-connection pool
- 🎯 **Memory Detection**: Real memory usage tracking (16,384 MB total, ~700 MB available detected)
- 🎯 **CPU Detection**: Real CPU core count detection (12 cores detected)
- 🎯 **Resource Warnings**: Comprehensive warnings for memory pressure, pool utilization
- 🎯 **Fail-Fast Protection**: System refuses to start with unsafe configurations
- 🎯 **Production Metrics**: Real queue metrics and pool statistics implementations

#### Phase 2: Component Configuration Decomposition 
**Timeline**: 3-5 days
**Goal**: Break monolithic config into manageable components

1. Create component YAML files from existing monolithic config
2. Implement `ComponentConfigLoader` with backward compatibility  
3. Add environment override support
4. Migrate existing configuration while maintaining legacy support
5. Update documentation and examples

#### Phase 3: Ruby/Rust Configuration Separation
**Timeline**: 2-3 days  
**Goal**: Enable independent deployment of Ruby and Rust components

1. Create Ruby-specific configuration directory structure
2. Implement configuration sync tooling
3. Update Ruby gem to read from local config directory
4. Add deployment documentation for configuration management
5. Test configuration independence

#### Phase 4: Dynamic Resource-Aware Scaling Integration
**Timeline**: 3-4 days
**Goal**: Integrate resource awareness into auto-scaling decisions

1. Add resource budget tracking to `OrchestrationLoopCoordinator`
2. Update scaling algorithms to respect resource constraints  
3. Add resource utilization monitoring
4. Implement resource-based backpressure
5. Add comprehensive resource monitoring and alerting

### Resource Monitoring and Alerting

#### New Metrics

**Resource Utilization:**
- `orchestration_db_connections_used_ratio`
- `orchestration_cpu_utilization_per_core`
- `orchestration_executor_efficiency_ratio`
- `orchestration_resource_constraint_violations_total`

**Configuration Health:**
- `orchestration_config_validation_status`
- `orchestration_component_config_load_time_seconds`
- `orchestration_shared_config_sync_age_seconds`

#### Health Check Enhancements

```rust
GET /health/orchestration/resources
{
  "status": "healthy",
  "system_limits": {
    "cpu_cores": 8,
    "database_max_connections": 50,
    "max_total_executors": 42
  },
  "current_utilization": {
    "active_executors": 28,
    "database_connections_used": 35,
    "cpu_utilization": 0.65
  },
  "resource_budget": {
    "allocated_executors": 28,
    "remaining_capacity": 14,
    "constraint_violations": 0
  }
}
```

### Backward Compatibility Strategy

1. **Configuration Format**: Support both monolithic and component-based configs
2. **Migration Path**: Provide tooling to convert existing configs
3. **Deprecation Timeline**: Warn about monolithic usage, remove after 2 versions
4. **Testing**: Comprehensive tests for both configuration formats

### Success Metrics

1. **Resource Safety**: Zero database pool exhaustion incidents
2. **Configuration Manageability**: <100 lines per component config file  
3. **Deployment Independence**: Ruby gem deployment without Rust repository
4. **Scaling Efficiency**: Resource utilization stays within defined thresholds
5. **Backward Compatibility**: Existing configurations continue working

## Related Work

- **TAS-32**: Step Result Coordination Processor (completed)

## References

- Current orchestration system: `src/orchestration/orchestration_system.rs`
- Circuit breaker: `src/resilience/circuit_breaker.rs`
- Current configuration: `src/orchestration/config.rs`
- pgmq integration: `src/messaging/pgmq_client.rs`
