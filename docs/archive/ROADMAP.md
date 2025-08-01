# Tasker Core Rust - Development Roadmap

## Foundation Status ✅
- **Multi-workspace architecture**: Main core + Ruby extension
- **Ruby FFI bindings**: Magnus integration with method registration
- **Build system**: Working Ruby gem compilation with rb_sys
- **Git hooks**: Multi-workspace validation with shared utilities

## Phase 1: Testing Infrastructure 🧪

### 1.1 Tasker Core Integration Tests
- [ ] Complex workflow integration tests
  - [ ] Linear workflows (A→B→C→D)
  - [ ] Diamond patterns (A→(B,C)→D)
  - [ ] Parallel merge patterns
  - [ ] Tree and mixed DAG patterns
- [ ] SQL function integration tests
  - [ ] Step readiness with complex dependencies
  - [ ] Task execution context queries
  - [ ] System health monitoring
  - [ ] Analytics and metrics
- [ ] Orchestration coordinator tests
  - [ ] Concurrent step execution
  - [ ] State machine transitions
  - [ ] Error handling and recovery
  - [ ] Backoff and retry logic

### 1.2 Ruby Binding Tests
- [ ] Basic Ruby client functionality tests
  - [ ] Task handler lifecycle
  - [ ] Step handler execution
  - [ ] Context serialization/deserialization
  - [ ] Error translation
- [ ] Performance function tests
  - [ ] get_task_execution_context
  - [ ] discover_viable_steps
  - [ ] batch_update_step_states
  - [ ] analyze_dependencies

### 1.3 Ruby Gem RSpec Tests
- [ ] Gem integration tests
  - [ ] Loading and initialization
  - [ ] Module structure verification
  - [ ] Error hierarchy validation
  - [ ] Performance benchmarks

## Phase 2: Configuration & Assumptions 🔧

### 2.1 Configuration Management
- [ ] Environment-based configuration
  - [ ] Database connection pooling settings
  - [ ] Timeout configurations
  - [ ] Retry policies
  - [ ] Concurrency limits
- [ ] YAML configuration loading
  - [ ] Task handler configurations
  - [ ] Step handler settings
  - [ ] System event definitions
- [ ] Configuration validation
  - [ ] Type checking
  - [ ] Required fields
  - [ ] Default values

### 2.2 Assumption Documentation
- [ ] Document all hardcoded values
- [ ] Create configuration options for:
  - [ ] Maximum retry attempts
  - [ ] Backoff strategies
  - [ ] Queue priorities
  - [ ] State transition rules

## Phase 3: Developer Operations API 🛠️

### 3.1 Core CRUD Operations
- [ ] Task management operations
  - [ ] Reset attempt counts
  - [ ] Cancel task and remaining steps
  - [ ] Force complete/fail tasks
  - [ ] Update task metadata
- [ ] Step management operations
  - [ ] Update step results
  - [ ] Mark steps as resolved
  - [ ] Reset step state
  - [ ] Override dependencies
- [ ] Safety validations
  - [ ] State machine constraints
  - [ ] Audit trail generation
  - [ ] Permission checking hooks

### 3.2 Ruby Binding Exposure
- [ ] Wrap core operations in Ruby API
- [ ] Add Rails-friendly interfaces
- [ ] REST endpoint helpers
- [ ] GraphQL mutation support

## Phase 4: Event Publishing Integration 📡

### 4.1 Core Event System
- [ ] Complete event publisher implementation
  - [ ] Event buffering
  - [ ] Batch publishing
  - [ ] Error handling
  - [ ] Metrics collection
- [ ] Event type definitions
  - [ ] Task lifecycle events
  - [ ] Step execution events
  - [ ] System health events
  - [ ] Error/warning events

### 4.2 Ruby Dry Events Integration
- [ ] Bridge Rust events to dry-events
- [ ] Follow Rails engine patterns from `/Users/petetaylor/projects/tasker-systems/tasker-engine/lib/tasker/events/`
  - [ ] Event publisher adapter
  - [ ] Event subscriber registration
  - [ ] Event filtering/routing
- [ ] Maintain event compatibility
  - [ ] Same event names
  - [ ] Compatible payloads
  - [ ] Consistent timestamps

## Phase 5: Structured Logging 📝

### 5.1 Core Logging Infrastructure
- [ ] Structured logging implementation
  - [ ] JSON formatter
  - [ ] Field standardization
  - [ ] Log levels
  - [ ] Performance optimization
- [ ] Correlation ID generation
  - [ ] UUID v7 for sortability
  - [ ] Thread-local storage
  - [ ] Propagation across boundaries

### 5.2 Ruby Logging Integration
- [ ] Follow Rails engine patterns from `/Users/petetaylor/projects/tasker-systems/tasker-engine/lib/tasker/logging/`
  - [ ] Logger adapter
  - [ ] Formatter compatibility
  - [ ] Rails.logger integration
- [ ] Correlation ID flow
  - [ ] Ruby → Rust propagation
  - [ ] Rust → Ruby propagation
  - [ ] HTTP header support

## Phase 6: Production Readiness 🚀

### 6.1 Performance Optimization
- [ ] Benchmark critical paths
- [ ] Optimize SQL queries
- [ ] Connection pool tuning
- [ ] Memory usage analysis

### 6.2 Documentation
- [ ] API documentation
- [ ] Integration guides
- [ ] Migration playbook
- [ ] Troubleshooting guide

### 6.3 Monitoring & Observability
- [ ] OpenTelemetry integration
- [ ] Metrics exporters
- [ ] Health check endpoints
- [ ] Dashboard templates

## Priority Order

1. **Testing Infrastructure** - Ensure correctness and prevent regressions
2. **Configuration Management** - Remove hardcoded assumptions
3. **Event Publishing** - Enable real-time monitoring
4. **Structured Logging** - Improve debugging and observability
5. **Developer Operations** - Support production operations
6. **Production Readiness** - Final polish and optimization

## Next Steps

Start with Phase 1.1: Tasker Core Integration Tests
- Focus on complex workflow patterns
- Ensure SQL functions work correctly
- Validate orchestration coordinator behavior