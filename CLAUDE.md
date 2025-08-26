# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core** is a high-performance Rust implementation of workflow orchestration, designed to complement the existing Ruby on Rails **Tasker** engine at `/Users/petetaylor/projects/tasker-systems/tasker-engine/`.

**Architecture**: PostgreSQL message queue (pgmq) based system where Rust handles orchestration coordination and Ruby workers process steps autonomously through queue polling. Features comprehensive executor pool management, health monitoring, auto-scaling, and race condition elimination.

## Current Status (August 17, 2025)

### 🎉 TAS-37 COMPREHENSIVE ORCHESTRATION ENHANCEMENT COMPLETE (August 17, 2025)
- **Major Achievement**: Complete TAS-37 implementation including race condition elimination, dynamic executor pools, health monitoring, and shutdown-aware enhancements
- **Problem Solved**: Task finalization race conditions, lack of auto-scaling, false health alerts during shutdowns
- **Solution**: Atomic SQL-based finalization claiming, OrchestrationLoopCoordinator with operational state management
- **Result**: ✅ **Production Ready** - Comprehensive orchestration system with 645+ tests passing

### ✅ COMPLETED PHASES: Major Orchestration System Enhancements

#### TAS-37: Task Finalization Race Condition & Dynamic Orchestration (August 17, 2025)
**Comprehensive Solution**: See `docs/ticket-specs/TAS-37.md` and `docs/ticket-specs/TAS-37-supplemental.md`

**Core Problem Solved**: Multiple result processors attempting to finalize same task simultaneously
- **Race Condition Elimination**: Atomic `claim_task_for_finalization` SQL function
- **Dynamic Executor Pools**: Auto-scaling with threshold and rate-based algorithms
- **Health Monitoring**: Context-aware monitoring with operational state integration
- **Resource Validation**: Database connection and memory constraint validation
- **Shutdown-Aware Monitoring**: Eliminates false alerts during planned shutdowns

**Technical Achievements**:
- Zero "unclear state" logs through atomic finalization claiming
- 12+ comprehensive metrics for finalization observability
- OrchestrationLoopCoordinator with sophisticated state management
- SystemOperationalState enum (Normal, GracefulShutdown, Emergency, Stopped, Startup)
- Thread-safe operational state coordination between Rust and Ruby

#### TAS-34: Component-Based Configuration Architecture (August 15, 2025)
**Configuration Migration**: See `docs/ticket-specs/TAS-34.md` and `docs/ticket-specs/TAS-34-supplemental.md`

**Achievement**: Successfully replaced 630-line monolithic configuration with component-based system
- **Component Structure**: 10 focused TOML files (`auth`, `database`, `telemetry`, `engine`, `system`, `circuit_breakers`, `executor_pools`, `orchestration`, `pgmq`, `query_cache`)
- **Environment Overrides**: `config/tasker/environments/{env}/{component}.toml` for environment-specific customization
- **Validation**: Comprehensive configuration validation with proper error handling
- **Benefits**: Manageable file sizes, clear separation of concerns, environment-aware configuration

#### TAS-33: Circuit Breaker Integration (Completed)
**Resilience Enhancement**: See `docs/ticket-specs/TAS-33.md`

**Achievement**: Comprehensive circuit breaker implementation for database and messaging operations
- **UnifiedPgmqClient**: Seamless switching between standard and protected clients
- **ProtectedPgmqClient**: Circuit breaker protection for all PGMQ operations
- **Configuration-Driven**: Enable/disable circuit breakers via TOML configuration
- **Metrics Integration**: Complete observability for circuit breaker state transitions

#### TAS-32: Unified Configuration Manager (Completed)
**Configuration Foundation**: See `docs/ticket-specs/TAS-32.md` and `docs/ticket-specs/TAS-32-supplemental.md`

**Achievement**: Unified configuration loading with environment detection and validation
- **Environment Detection**: Automatic environment detection with manual override support
- **TOML-First**: Native TOML configuration with structured validation
- **Type Safety**: Strong typing throughout configuration system with comprehensive error handling
- **Component Integration**: Foundation for TAS-34 component-based architecture

#### TAS-31: Orchestration System Architecture (Completed)
**Core Orchestration**: See `docs/ticket-specs/TAS-31.md`

**Achievement**: Complete orchestration system with unified bootstrap and configuration
- **OrchestrationCore**: Single-source-of-truth bootstrap system eliminating multiple entry points
- **Component Integration**: Unified initialization of all orchestration components
- **Database Pool Management**: Sophisticated pool configuration with timeout and retry handling
- **Task Lifecycle**: Complete task initialization, step enqueueing, and result processing

### ✅ PREVIOUS PHASES: Foundation Work

#### Workflow Pattern Standardization (August 7, 2025)
**Pattern Unification**: All workflow examples use consistent patterns
- **Linear Workflow** ✅, **Mixed DAG Workflow** ✅, **Tree Workflow** ✅, **Diamond Workflow** ✅, **Order Fulfillment** ✅
- **Unified Pattern**: All handlers use `sequence.get_results('step_name')` and return `TaskerCore::Types::StepHandlerCallResult.success`
- **Developer Experience**: Consistent examples across all workflow patterns

#### Database Schema Foundation
- **UUID Migration Applied** ✅: Both `tasker_tasks` and `tasker_workflow_steps` have UUID columns for future simple message architecture
- **PGMQ Extension** ✅: PostgreSQL message queue extension enabled and configured
- **Finalization Claiming** ✅: Atomic SQL functions for race-condition-free task finalization

## Architecture Overview

### Core Orchestration System
- **OrchestrationCore**: Unified bootstrap system with consistent component initialization
- **OrchestrationLoopCoordinator**: Dynamic executor pool management with auto-scaling and health monitoring
- **Finalization System**: Race-condition-free task finalization with atomic SQL claiming
- **Circuit Breaker Integration**: Resilient messaging with configurable protection
- **Operational State Management**: Context-aware system state with shutdown coordination

### Queue Design Pattern
```
fulfillment_queue    - All fulfillment namespace steps
inventory_queue      - All inventory namespace steps
notifications_queue  - All notification namespace steps
orchestration_step_results - Step completion results processing
```

### Key Technical Patterns
- **PostgreSQL-Centric Architecture**: Shared database as API layer with pgmq for reliable messaging
- **Component-Based Configuration**: TOML configuration with environment overrides and validation
- **Atomic Operations**: SQL-based claiming prevents race conditions and ensures consistency
- **Operational State Awareness**: Health monitoring adapts behavior based on system state
- **Circuit Breaker Protection**: Configurable resilience for database and messaging operations

## Development Guidelines

### Code Quality Standards
- **Production-Ready Code**: All implementations complete with comprehensive error handling
- **Rust Best Practices**: Proper error propagation, thread-safe patterns, comprehensive documentation
- **Configuration-Driven**: All behavior configurable via TOML with environment-specific overrides
- **Comprehensive Testing**: Unit tests, integration tests, and end-to-end validation
- **Observability**: Detailed metrics and logging for production monitoring

### Current Working Branch
- **Branch**: `jcoletaylor/tas-37-task-finalizer-race-condition`
- **Status**: ✅ Complete - TAS-37 comprehensive orchestration enhancement implemented and tested

## Key File Locations

### TAS-37: Orchestration Enhancement
- **Core Components**:
  - `src/orchestration/core.rs` - Unified orchestration bootstrap with operational state
  - `src/orchestration/coordinator/` - Complete coordinator system (mod.rs, pool.rs, scaling.rs, monitor.rs)
  - `src/orchestration/coordinator/operational_state.rs` - System state management with transition validation
  - `src/orchestration/finalization_claimer.rs` - Race-condition-free task finalization
- **SQL Functions**: `migrations/20250818000001_add_finalization_claiming.sql` - Atomic finalization claiming
- **Ruby Integration**: `workers/ruby/ext/tasker_core/src/embedded_bridge.rs` - Enhanced FFI with shutdown coordination
- **Configuration**: `config/tasker/base/orchestration.toml` - Comprehensive orchestration configuration

### TAS-34: Component-Based Configuration
- **Component Files**: `config/tasker/base/{auth,database,telemetry,engine,system,circuit_breakers,executor_pools,orchestration,pgmq,query_cache}.toml`
- **Environment Overrides**: `config/tasker/environments/{test,development,production}/` - Environment-specific configuration
- **Core Implementation**: `src/config/component_loader.rs` - Component loading with hierarchical merging
- **Configuration Manager**: `src/config/mod.rs` - Unified configuration management

### TAS-33: Circuit Breaker Integration
- **Core Implementation**: `src/resilience/circuit_breaker.rs` - Circuit breaker implementation
- **Messaging Integration**: `src/messaging/protected_pgmq_client.rs` - Protected PGMQ client
- **Unified Client**: `src/messaging/unified_pgmq_client.rs` - Seamless client switching

### TAS-32: Unified Configuration
- **Configuration Manager**: `src/config/manager.rs` - Environment-aware configuration loading
- **TOML Integration**: `src/config/toml_loader.rs` - Native TOML configuration support
- **Type Definitions**: `src/config/types.rs` - Strong typing for all configuration

### TAS-31: Orchestration Foundation
- **Orchestration System**: `src/orchestration/orchestration_system.rs` - Main orchestration coordination
- **Task Initialization**: `src/orchestration/task_initializer.rs` - Task creation and setup
- **Result Processing**: `src/orchestration/step_result_processor.rs` - Step completion handling

### Database Schema
- **Core Tables**: Standard tasker tables with UUID columns for future architecture
- **PGMQ Extension**: PostgreSQL message queue extension enabled
- **Finalization Functions**: Atomic SQL functions for race-condition-free operations
- **Indexes**: Optimized indexes for UUID lookup and claim management

### Configuration Structure
- **Base Configuration**: `config/tasker/base/` - Component-based TOML configuration
- **Environment Overrides**: `config/tasker/environments/` - Environment-specific customization
- **Validation**: Comprehensive validation with descriptive error messages

## Development Commands

### Rust Core
```bash
# Core development (ALWAYS use --all-features for full consistency)
cargo build --all-features                         # Build project with all features
cargo test --all-features                          # Run tests with factory system and all features
cargo clippy --all-targets --all-features          # Lint code with all features
cargo fmt                                          # Format code

# Coverage and benchmarking
cargo llvm-cov --all-features                      # Generate coverage report
cargo bench --all-features                         # Run performance benchmarks

# Database operations
cargo sqlx migrate run                             # Run database migrations
cargo check --all-features                        # Fast compilation check
```

### Ruby Extension & Integration Tests
```bash
cd workers/ruby
bundle install                                     # Install Ruby dependencies
bundle exec rake compile                           # Compile Ruby extension

# Integration testing
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test TASKER_ENV=test bundle exec rspec spec/integration/ --format documentation

# Specific workflow tests
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test TASKER_ENV=test bundle exec rspec spec/integration/linear_workflow_integration_spec.rb --format documentation
```

### Database Setup
```bash
# From project root
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test cargo sqlx migrate run

# Test database connectivity
psql postgresql://tasker:tasker@localhost/tasker_rust_test -c "SELECT 1"
```

### Configuration Testing
```bash
# Test configuration loading
TASKER_ENV=test cargo run --example config_demo --all-features
TASKER_ENV=development cargo run --example config_demo --all-features
TASKER_ENV=production cargo run --example config_demo --all-features
```

## Implementation Status (August 17, 2025)

### ✅ COMPLETED: TAS-37 Comprehensive Orchestration Enhancement

#### Race Condition Elimination
- **Atomic Finalization**: `claim_task_for_finalization` SQL function ensures only one processor can finalize a task
- **Comprehensive Metrics**: 12+ metrics for finalization observability and debugging
- **Claim Management**: Timeout handling, claim extension, and proper cleanup on errors
- **Database Visibility**: Claims visible in database for debugging and operational monitoring

#### Dynamic Executor Pool Management
- **OrchestrationLoopCoordinator**: Complete coordinator system with subsystem management
- **Auto-Scaling Algorithms**: Threshold-based and rate-based scaling with configurable policies
- **Pool Management**: Dynamic pool creation, scaling, and lifecycle management
- **Resource Validation**: Database connection and memory constraint validation

#### Operational State Management (TAS-37 Supplemental)
- **SystemOperationalState**: Sophisticated state machine (Normal, GracefulShutdown, Emergency, Stopped, Startup)
- **Context-Aware Health Monitoring**: Health thresholds adapt based on operational state
- **Shutdown Coordination**: Ruby FFI integration with proper state transitions
- **False Alert Elimination**: No more health alerts during planned shutdown operations

### ✅ COMPLETED: TAS-34 Component-Based Configuration

#### Configuration Architecture Migration
- **Component Structure**: 10 focused TOML files replacing 630-line monolithic configuration
- **Environment Overrides**: Clean separation of base and environment-specific configuration
- **Hierarchical Merging**: Proper configuration merging with validation and error handling
- **Type Safety**: Strong typing throughout with comprehensive validation

### ✅ COMPLETED: TAS-33 Circuit Breaker Integration

#### Resilience Implementation
- **Circuit Breaker Pattern**: Comprehensive circuit breaker implementation for database and messaging
- **Unified Client**: Seamless switching between standard and protected PGMQ clients
- **Configuration Control**: Enable/disable circuit breakers via configuration
- **Metrics Integration**: Complete observability for circuit breaker operations

### ✅ COMPLETED: TAS-32 Unified Configuration Manager

#### Configuration Foundation
- **Environment Detection**: Automatic environment detection with manual override support
- **TOML-First**: Native TOML configuration with structured parsing and validation
- **Type Safety**: Strong typing throughout configuration system
- **Error Handling**: Comprehensive error handling with descriptive messages

### ✅ COMPLETED: TAS-31 Orchestration System Architecture

#### Core Orchestration
- **OrchestrationCore**: Unified bootstrap eliminating multiple entry points with different assumptions
- **Component Integration**: Consistent initialization of all orchestration components
- **Database Integration**: Sophisticated connection pool management with configuration
- **Task Lifecycle**: Complete task creation, step enqueueing, and result processing

### 🔄 AVAILABLE FOR FUTURE IMPLEMENTATION

#### TAS-35: Executor Pool Lifecycle Management (Available)
**Specification**: See `docs/ticket-specs/TAS-35.md`
- Detailed executor pool lifecycle management with graceful shutdown
- Worker thread coordination and resource cleanup
- Integration with TAS-37 coordinator system

#### TAS-36: Auto-Scaling Algorithm Enhancement (Available)
**Specification**: See `docs/ticket-specs/TAS-36.md`
- Advanced auto-scaling algorithms beyond threshold-based
- Predictive scaling based on workload patterns
- Resource optimization and cost management

#### TAS-39: Health Check Integration (Available)
**Specification**: See `docs/ticket-specs/TAS-39.md`
- Health check endpoint integration for load balancers
- Kubernetes readiness and liveness probe support
- Comprehensive system health reporting

## Testing Strategy

### Comprehensive Test Coverage (645+ Tests)
- **Unit Tests**: Each component thoroughly tested with comprehensive scenarios
- **Integration Tests**: End-to-end workflow validation with real database operations
- **Race Condition Tests**: Concurrent processor simulation for finalization claiming
- **Configuration Tests**: All configuration scenarios with environment overrides
- **Operational State Tests**: State transition validation and context-aware monitoring

### Key Test Suites
```bash
# Core orchestration tests
cargo test --all-features orchestration

# Configuration system tests
cargo test --all-features config

# Circuit breaker integration tests
cargo test --all-features circuit_breaker

# Ruby integration tests
TASKER_ENV=test bundle exec rspec spec/integration/ --format documentation

# Specific workflow validation
TASKER_ENV=test bundle exec rspec spec/integration/linear_workflow_integration_spec.rb
TASKER_ENV=test bundle exec rspec spec/integration/order_fulfillment_spec.rb
```

### Performance Testing
```bash
# Benchmark performance-critical operations
cargo bench --all-features

# Load testing with high concurrency
cargo test --release --all-features -- --nocapture
```

## Related Projects

- **tasker-engine/**: Production-ready Rails engine for workflow orchestration
- **tasker-blog/**: GitBook documentation with real-world engineering stories

## Key Documentation

### Ticket Specifications (Completed)
- **TAS-31**: `docs/ticket-specs/TAS-31.md` - Orchestration system architecture foundation
- **TAS-32**: `docs/ticket-specs/TAS-32.md` + `TAS-32-supplemental.md` - Unified configuration manager
- **TAS-33**: `docs/ticket-specs/TAS-33.md` - Circuit breaker integration for resilience
- **TAS-34**: `docs/ticket-specs/TAS-34.md` + `TAS-34-supplemental.md` - Component-based configuration
- **TAS-37**: `docs/ticket-specs/TAS-37.md` + `TAS-37-supplemental.md` - Race condition elimination and dynamic orchestration

### Ticket Specifications (Available for Implementation)
- **TAS-35**: `docs/ticket-specs/TAS-35.md` - Executor pool lifecycle management (detailed spec available)
- **TAS-36**: `docs/ticket-specs/TAS-36.md` - Auto-scaling algorithm enhancement (detailed spec available)
- **TAS-39**: `docs/ticket-specs/TAS-39.md` - Health check integration (detailed spec available)

### Technical Documentation
- **Configuration Guide**: Component-based TOML configuration with environment overrides
- **Circuit Breaker Guide**: Resilience patterns and configuration options
- **Orchestration Guide**: Complete orchestration system usage and configuration
- **Metrics Guide**: Comprehensive metrics collection and monitoring setup

## Success Metrics Achieved

### TAS-37: Orchestration Enhancement (Completed August 17, 2025)
- ✅ **Zero Race Conditions**: Atomic finalization claiming eliminates "unclear state" logs
- ✅ **Production Ready**: Comprehensive error handling, logging, and metrics collection
- ✅ **Operational Excellence**: Context-aware health monitoring prevents false alerts during shutdowns
- ✅ **Auto-Scaling**: Dynamic executor pools with threshold and rate-based algorithms
- ✅ **Thread Safety**: Proper concurrent programming patterns with Arc/RwLock
- ✅ **Comprehensive Observability**: 12+ metrics for finalization operations, coordinator health monitoring

### TAS-34: Component-Based Configuration (Completed August 15, 2025)
- ✅ **Configuration Migration**: Successfully replaced 630-line monolithic config with 10 focused components
- ✅ **Environment Support**: Working environment-specific configuration with proper merging
- ✅ **Type Safety**: Strong typing throughout with comprehensive validation
- ✅ **Code Quality**: All clippy warnings resolved, comprehensive error handling

### TAS-33: Circuit Breaker Integration (Completed)
- ✅ **Resilience**: Complete circuit breaker protection for database and messaging operations
- ✅ **Configuration Control**: Enable/disable circuit breakers via TOML configuration
- ✅ **Seamless Integration**: Unified client switching without breaking existing code
- ✅ **Observability**: Complete metrics for circuit breaker state and operations

### Overall System Quality
- ✅ **Production Ready**: All major components implemented with comprehensive testing
- ✅ **Comprehensive Testing**: 645+ tests covering all scenarios and edge cases
- ✅ **Code Quality**: Excellent (9.5/10) assessment from comprehensive code review
- ✅ **Documentation**: Thorough inline documentation and architectural guides
- ✅ **Observability**: Detailed metrics and logging for production monitoring

## Next Development Opportunities

The system is now production-ready with comprehensive orchestration capabilities. Future enhancements can build on the solid foundation:

1. **TAS-35 Executor Pool Lifecycle**: Enhanced pool management with graceful shutdown (specification ready)
2. **TAS-36 Auto-Scaling Enhancement**: Advanced algorithms beyond threshold-based scaling (specification ready)
3. **TAS-39 Health Check Integration**: Kubernetes and load balancer health check support (specification ready)
4. **Simple Message Architecture**: Future simplification of message structures using UUID-based approach
5. **Performance Optimization**: Benchmarking and optimization of critical paths

The current implementation provides a robust, scalable, and maintainable orchestration system ready for production deployment.
