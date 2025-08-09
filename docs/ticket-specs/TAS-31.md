# TAS-31: Production Resilience & Performance Optimization

**Linear Ticket**: https://linear.app/tasker-systems/issue/TAS-31/production-resilience-and-performance-optimization  
**GitHub Branch**: `jcoletaylor/tas-31-production-resilience-performance-optimization`  
**Created**: August 8, 2025  
**Estimated Duration**: 2 weeks  

## 🎯 Strategic Objective

Build production resilience and performance optimization on top of our successful pgmq architecture pivot (TAS-14). This ticket combines circuit breaker implementation for fault isolation with FFI memory optimization to prepare the system for production scale deployment.

## 📊 Context & Background

### Post-TAS-14 System State
- ✅ **pgmq Architecture**: Successfully implemented with PostgreSQL-backed reliability
- ✅ **Simple Message Processing**: UUID-based messages working across all workflow patterns
- ✅ **Integration Tests**: Linear, Mixed DAG, Tree, Diamond, and Order Fulfillment workflows all passing
- ✅ **Unified Logging**: Cross-language consistency between Rust and Ruby

### Strategic Focus Areas
With the architecture now stable, we can focus on:
1. **Operational Resilience**: Prevent cascade failures under production load
2. **Performance Optimization**: Reduce memory overhead and improve efficiency

## 📋 Implementation Plan

### Week 1: Circuit Breaker Implementation

**Objective**: Implement fault isolation patterns to prevent cascade failures

#### Key Components

**1. Core Circuit Breaker Module** (`src/resilience/circuit_breaker.rs`)
- Configurable failure thresholds and timeout patterns
- State management: Closed → Open → Half-Open transitions
- Metrics collection and monitoring integration
- Thread-safe implementation with atomic state tracking

**2. Integration Points**
- **Queue Operations**: Circuit breakers around pgmq read/write operations (HIGH PRIORITY)
- **Step Handler Execution**: Timeouts and failure thresholds for Ruby FFI calls
- **External API Calls**: Circuit breakers in Ruby step handlers for third-party integrations
- **Namespace Isolation**: Failed namespace doesn't affect other queues

**Note**: Database circuit breakers removed based on SQLx best practices research - SQLx connection pools already provide optimal resilience.

**3. Configuration Integration**
- Extend existing `TaskerConfig` with circuit breaker settings
- Environment-specific thresholds (development vs production)
- Runtime configuration updates without restart

#### Technical Specifications

```rust
// Core circuit breaker structure
pub struct CircuitBreaker {
    state: AtomicCircuitState,
    failure_threshold: usize,
    timeout: Duration,
    success_threshold: usize,
    metrics: Arc<Mutex<CircuitMetrics>>,
}

pub enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, reject all calls
    HalfOpen, // Testing recovery
}

// Usage patterns
impl CircuitBreaker {
    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where F: Future<Output = Result<T, E>>
    
    pub fn metrics(&self) -> CircuitMetrics
    pub fn force_open(&self)
    pub fn force_closed(&self)
}
```

#### Files to Create/Modify

**New Files**:
- `src/resilience/mod.rs` - Resilience module definition
- `src/resilience/circuit_breaker.rs` - Core implementation
- `src/resilience/metrics.rs` - Metrics collection
- `src/resilience/config.rs` - Configuration structures

**Modified Files**:
- `src/config/mod.rs` - Add circuit breaker configuration
- `src/orchestration/orchestration_system.rs` - Integrate circuit breakers
- `src/messaging/pgmq_client.rs` - Add circuit breaker protection
- `bindings/ruby/ext/tasker_core/src/lib.rs` - FFI call protection

#### Success Criteria

- ✅ Failed namespace isolation: Other namespaces continue processing
- ✅ Database failure containment: Connection issues don't crash orchestration  
- ✅ Circuit breaker state transitions: Proper Open/Half-Open/Closed behavior
- ✅ Load testing validation: System degrades gracefully under failure scenarios
- ✅ Monitoring endpoints expose circuit breaker status

### Week 2: FFI Memory Optimization

**Objective**: Reduce memory allocations at the Rust-Ruby boundary by 60-80%

#### Key Optimizations

**1. String Return Optimization**
- Convert getter methods to return `&str` instead of `String.clone()`
- Use `Cow<'_, str>` for flexible ownership patterns
- Implement `AsRef<str>` traits for zero-copy access
- Audit all FFI string methods for optimization opportunities

**2. String Interning System**
- Cache commonly repeated strings (namespaces, task names, versions)
- LRU eviction with configurable cache limits
- Integration with message creation paths
- Thread-safe shared cache across FFI calls

**3. Memory Profiling Integration**
- Add benchmarks to measure allocation improvements
- Memory usage tracking in development mode
- Performance regression detection

#### Technical Specifications

```rust
// String optimization patterns
impl OrchestrationHandleInfo {
    // Before: Unnecessary clones
    pub fn handle_type(&self) -> String {
        self.handle_type.clone()
    }
    
    // After: Zero-copy access
    pub fn handle_type(&self) -> &str {
        &self.handle_type
    }
    
    // Flexible ownership with Cow
    pub fn namespace(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.namespace)
    }
}

// String interning system
pub struct StringCache {
    cache: Arc<RwLock<LruCache<String, Arc<str>>>>,
    max_size: usize,
}

impl StringCache {
    pub fn intern(&self, s: &str) -> Arc<str>
    pub fn clear(&self)
    pub fn size(&self) -> usize
}
```

#### Files to Create/Modify

**New Files**:
- `src/utils/string_cache.rs` - String interning implementation
- `benches/memory_benchmarks.rs` - Memory allocation benchmarks

**Modified Files**:
- `bindings/ruby/ext/tasker_core/src/types.rs` - FFI type optimizations
- `bindings/ruby/ext/tasker_core/src/context.rs` - Context handling improvements
- `bindings/ruby/ext/tasker_core/src/orchestration_manager.rs` - String optimization
- `src/messaging/message.rs` - Message creation optimization

#### Success Criteria

- ✅ **String Allocation Reduction**: 60-80% fewer allocations at FFI boundary
- ✅ **Memory Usage Improvement**: Measurable reduction in peak memory usage
- ✅ **Performance Benchmarks**: Faster FFI call processing  
- ✅ **API Compatibility**: Zero breaking changes to existing Ruby interfaces
- ✅ **Cache Management**: Controlled memory usage with LRU eviction

## 🔧 Technical Implementation Details

### Circuit Breaker Integration Points

**1. PGMQ Operations** (`src/messaging/pgmq_client.rs`)
```rust
impl PgmqClient {
    pub async fn read_messages_with_circuit_breaker(&self, queue: &str) -> Result<Vec<Message>> {
        self.circuit_breaker
            .call(|| self.read_messages_internal(queue))
            .await
    }
}
```

**2. FFI Boundary** (`bindings/ruby/ext/tasker_core/src/lib.rs`)
```rust
#[magnus::function]
pub fn initialize_task_with_protection(request_json: String) -> Result<String, Error> {
    CIRCUIT_BREAKER
        .call(|| initialize_task_internal(request_json))
        .await
        .map_err(|e| Error::new(exception::standard_error(), e.to_string()))
}
```

**3. Ruby API Integration** (step handlers)
```ruby
# API handlers automatically classify errors for circuit breaker integration
class MyApiHandler < TaskerCore::StepHandler::Api
  def process(task, sequence, step)
    # Makes HTTP request with automatic error classification
    response = get('/api/endpoint')
    
    # Returns StepHandlerCallResult with metadata for backoff calculation
    TaskerCore::Types::StepHandlerCallResult.success(
      result: response.body,
      metadata: {
        operation: 'api_call',
        response_time_ms: 150,
        http_status: response.status
      }
    )
  rescue TaskerCore::RetryableError => e
    # RetryableError includes retry_after metadata for backoff calculator
    raise e
  end
end
```

### Memory Optimization Strategy

**Phase 1**: Audit Current Allocations
- Profile existing FFI calls to identify allocation hotspots
- Measure baseline memory usage in typical workflows
- Identify most frequently allocated strings

**Phase 2**: Implement Zero-Copy Patterns
- Convert string getters to return references
- Use `Cow<'_, str>` for conditional ownership
- Implement string interning for repeated values

**Phase 3**: Validation & Benchmarking
- Memory allocation benchmarks before/after
- Integration tests to ensure no behavioral changes
- Performance regression testing

## 🏗️ Implementation Strategy (Updated Based on System Architect Analysis)

### Phase 1: Database Circuit Breaker Removal ✅

**Completed based on SQLx best practices research:**
- Removed database circuit breaker from YAML configuration
- Updated documentation to focus on queue operations only
- Simplified circuit breaker scope to high-value integration points

**Rationale**: SQLx connection pools already provide optimal resilience patterns, and additional circuit breaking would be counterproductive.

### Phase 2: High-Value PGMQ Circuit Breaker Integration

**Priority Integration Points (based on system architect analysis):**

1. **OrchestrationLoop** (`src/orchestration/orchestration_loop.rs`)
   - Replace `PgmqClient` with `ProtectedPgmqClient` for task claiming operations
   - Critical path: Claims ready tasks with priority fairness

2. **StepResultProcessor** (`src/orchestration/step_result_processor.rs`)
   - Protect `read_messages()` calls from `orchestration_step_results` queue
   - High frequency: Processes worker results continuously

3. **TaskRequestProcessor** (`src/orchestration/task_request_processor.rs`)
   - Protect `read_messages()` calls from `orchestration_task_requests` queue
   - Entry point: External API → orchestration pipeline

4. **StepEnqueuer** (`src/orchestration/step_enqueuer.rs`)
   - Protect `send_message()` calls to namespace-specific queues
   - High volume: Individual step enqueueing with dependency resolution

**Implementation Pattern:**
```rust
// Before: Direct client usage
let pgmq_client = Arc::new(PgmqClient::new_with_pool(pool.clone()).await);

// After: Protected client with circuit breaker
let pgmq_client = Arc::new(ProtectedPgmqClient::new_with_yaml_config(
    database_url, 
    &config.circuit_breakers
).await?);
```

### Phase 3: Ruby Integration and API Circuit Breaking

**Ruby Queue Worker Protection** (`bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`):
- Integrate circuit breaker protection for high-frequency polling operations
- Protect result publishing to orchestration queue

**API Circuit Breaker Integration** (`bindings/ruby/lib/tasker_core/step_handler/api.rb`):
- Leverage existing error classification system (RetryableError/PermanentError)
- Integrate with backoff calculator for Retry-After header support
- Add circuit breaker protection around Faraday HTTP calls

**Backoff Calculator Integration** (`src/orchestration/backoff_calculator.rs`):
- Enhance `BackoffContext` to accept Ruby metadata from `StepHandlerCallResult`
- Support implicit circuit breaker retries based on API response patterns
- Pipeline HTTP status codes and retry-after headers from Ruby to Rust

**Integration Flow:**
```
Ruby API Handler → StepHandlerCallResult.metadata → 
Queue Message → StepResultProcessor → 
BackoffCalculator.calculate_and_apply_backoff() → 
Circuit Breaker State Updates
```

## 📊 Success Metrics & Validation

### Circuit Breaker Effectiveness

**Functional Tests**:
- Simulate database connection failures → Verify orchestration continues with circuit open
- Simulate queue failures → Verify namespace isolation works
- Test circuit breaker state transitions under load

**Load Testing**:
- 1000 concurrent tasks with 10% failure rate → Verify graceful degradation
- Database unavailable for 30 seconds → Verify circuit opens and recovers
- Mixed failure scenarios → Verify independent circuit behavior

**Monitoring**:
- Circuit breaker state exposed via health endpoints
- Metrics integration with existing logging system
- Alerting on circuit breaker state changes

### Memory Optimization Impact

**Benchmarking**:
- FFI call allocation tracking before/after optimization
- Long-running process memory usage patterns
- Garbage collection pressure in Ruby processes

**Performance Tests**:
- FFI call latency improvements
- Message creation throughput
- Memory usage under sustained load

**Validation**:
- All existing integration tests pass
- No behavioral changes in workflows
- Ruby API remains fully compatible

## 🎯 Risk Analysis & Mitigation

### Week 1 Risks (Circuit Breakers)

**Risk**: Circuit breakers introduce call overhead
- **Mitigation**: Benchmark overhead, optimize hot paths, use zero-cost abstractions where possible

**Risk**: Complex state management bugs
- **Mitigation**: Comprehensive unit tests, well-established circuit breaker patterns, atomic state operations

**Risk**: Configuration complexity
- **Mitigation**: Sensible defaults, clear documentation, runtime validation

### Week 2 Risks (Memory Optimization)

**Risk**: Lifetime management complexity
- **Mitigation**: Start with simple cases, use `Cow<'_, str>` for flexibility, comprehensive testing

**Risk**: Breaking Ruby API compatibility
- **Mitigation**: Thorough integration testing, maintain existing method signatures, gradual rollout

**Risk**: Cache memory bloat
- **Mitigation**: LRU eviction, configurable cache limits, monitoring cache size

## 📈 Expected Outcomes

### Operational Benefits
- **Fault Tolerance**: System continues operating during partial failures
- **Graceful Degradation**: Failed components don't cause system-wide outages
- **Namespace Isolation**: Queue failures isolated to specific workflows
- **Production Monitoring**: Circuit breaker status in health endpoints

### Performance Benefits
- **Memory Efficiency**: 60-80% reduction in FFI string allocations  
- **Reduced GC Pressure**: Less garbage collection in Ruby processes
- **Faster FFI Calls**: Zero-copy string access where possible
- **Sustainable Performance**: Better long-running process characteristics

### Foundation for Scale
- **High Availability**: Circuit breaker patterns enable robust production deployment
- **Resource Efficiency**: Memory optimizations support higher throughput
- **Monitoring Integration**: Observability for production operations
- **Configuration Flexibility**: Environment-specific tuning capabilities

## 🚀 Getting Started

### Prerequisites
- TAS-14 (pgmq architecture) must be completed and stable
- All existing integration tests passing
- Development environment with Ruby extension compilation working

### Development Workflow
1. **Week 1**: Focus on circuit breaker implementation and integration
2. **Week 2**: Focus on FFI memory optimization and benchmarking  
3. **Continuous**: Run integration tests after each major change
4. **End of Week 1**: Validate circuit breaker effectiveness with load testing
5. **End of Week 2**: Validate memory improvements with benchmarks

### Testing Strategy
- **Unit Tests**: Circuit breaker logic, string optimization functions
- **Integration Tests**: Full workflow execution with circuit breakers active
- **Load Tests**: Failure scenario validation and recovery testing
- **Benchmarks**: Memory allocation tracking and performance measurement
- **Regression Tests**: Ensure no behavioral changes to existing functionality

This ticket positions the system for production deployment by combining fault tolerance and performance characteristics needed for reliable, high-scale workflow orchestration.