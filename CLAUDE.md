# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of the core workflow orchestration engine, designed to complement the existing Ruby on Rails **Tasker** engine found at `/Users/petetaylor/projects/tasker/`. This project leverages Rust's memory safety, fearless parallelism, and performance characteristics to handle computationally intensive workflow orchestration, dependency resolution, and state management operations.

**Architecture**: Step handler foundation where Rust implements the complete step handler base class that frameworks (Rails, Python, Node.js) extend through subclassing with `process()` and `process_results()` hooks.

## Comprehensive Migration Scope

### All Rails Models Must Be Migrated
**Location**: `/Users/petetaylor/projects/tasker/app/models/tasker/`

**Complete Model List** (18 models + subdirectories):
- `task.rb` (16KB, 425 lines) - Core task model with complex scopes
- `workflow_step.rb` (17KB, 462 lines) - Step execution and state management
- `workflow_step_transition.rb` (15KB, 435 lines) - Step state transitions
- `task_transition.rb` (7.3KB, 236 lines) - Task state transitions
- `task_diagram.rb` (10KB, 333 lines) - Workflow visualization
- `workflow_step_edge.rb` (3.3KB, 95 lines) - DAG relationships
- `named_task.rb` (3.5KB, 122 lines) - Task templates
- `named_tasks_named_step.rb` (2.7KB, 83 lines) - Task-step relationships
- `named_step.rb` (1.4KB, 42 lines) - Step definitions
- `step_dag_relationship.rb` (1.9KB, 66 lines) - DAG structure
- `dependent_system.rb` (738B, 27 lines) - External system references
- `dependent_system_object_map.rb` (2.7KB, 65 lines) - System mappings
- `step_readiness_status.rb` (2.1KB, 60 lines) - Step readiness tracking
- `task_namespace.rb` (1.1KB, 42 lines) - Organizational hierarchy
- `task_annotation.rb` (1.1KB, 37 lines) - Task metadata
- `annotation_type.rb` (669B, 27 lines) - Annotation categories
- `task_execution_context.rb` (967B, 30 lines) - Execution metadata
- `application_record.rb` (2.8KB, 71 lines) - Base model patterns
- `diagram/` subdirectory - Additional diagram models

**Critical Requirement**: All ActiveRecord scopes, validations, associations, and business logic methods must be fully migrated to Rust with equivalent functionality.

### Core Tasker Logic Migration
**Location**: `/Users/petetaylor/projects/tasker/lib/tasker/`

**Essential Components** (the heart of the Tasker system):
- `constants.rb` (16KB, 418 lines) - **CRITICAL** - System constants and enums
- `configuration.rb` (12KB, 326 lines) - **CRITICAL** - Configuration management
- `cache_strategy.rb` (17KB, 470 lines) - Caching and performance optimization
- `task_builder.rb` (15KB, 433 lines) - Task construction and validation
- `handler_factory.rb` (12KB, 323 lines) - Step handler creation and management
- `state_machine.rb` (2.8KB, 84 lines) - State machine foundations
- `orchestration.rb` (1.9KB, 46 lines) - Core orchestration logic
- `task_handler.rb` (1.3KB, 44 lines) - Task handler base
- `events.rb` (1.2KB, 38 lines) - Event system foundation
- `registry.rb` (684B, 23 lines) - Component registry
- `types.rb` (1.9KB, 65 lines) - Type definitions
- `functions.rb` (351B, 13 lines) - SQL function wrappers
- `errors.rb` (3.1KB, 91 lines) - Error handling
- `telemetry.rb` (2.6KB, 60 lines) - Observability and metrics
- `authorization.rb` (2.3KB, 76 lines) - Security and permissions
- `cache_capabilities.rb` (4.6KB, 132 lines) - Cache management
- `identity_strategy.rb` (1.3KB, 39 lines) - Identity and hashing

**Subdirectories** (comprehensive logic):
- `state_machine/` - Complete state machine implementations
- `orchestration/` - Core orchestration algorithms
- `step_handler/` - Step handler foundation and implementations
- `task_handler/` - Task handler implementations
- `events/` - Full lifecycle event system and pub/sub model
- `registry/` - Component registration and discovery
- `functions/` - SQL function replacements
- `types/` - Type system and validations
- `telemetry/` - Observability and monitoring
- `authorization/` - Security and access control
- `authentication/` - Authentication systems
- `health/` - Health checks and diagnostics
- `logging/` - Structured logging
- `concerns/` - Shared behaviors and mixins
- `analysis/` - Workflow analysis and optimization
- `constants/` - Detailed constant definitions

## Architecture Context

This Rust implementation is based on the production-ready Rails Tasker engine, which provides:

- **Complex Workflow Orchestration**: DAG-based task and workflow step management
- **Intelligent State Management**: Task and step state machines with retry logic
- **High-Performance Dependency Resolution**: Currently implemented as PostgreSQL functions
- **Event-Driven Architecture**: 56+ lifecycle events with pub/sub patterns
- **Production Observability**: OpenTelemetry, structured logging, and health monitoring
- **Step Handler Foundation**: Base class that frameworks extend for business logic

## Core Components to Implement

### 1. Step Handler Foundation (Primary)
- **StepHandlerFoundation**: Complete step handler base class with lifecycle management
- **Handle Method**: Full `handle()` implementation with pre/post processing
- **Framework Hooks**: `process()` and `process_results()` extension points
- **Queue Abstraction**: Dependency injection for Sidekiq, Celery, Bull
- **Multi-Language FFI**: Ruby, Python, Node.js subclassing support

### 2. Complete Model Layer
- **All 18+ Rails Models**: Full migration with ActiveRecord scope equivalents
- **Complex Associations**: Maintain all model relationships and validations
- **Business Logic**: All model methods and computed properties
- **Scopes and Queries**: High-performance equivalents of all ActiveRecord scopes

### 3. Orchestration Engine
- **Coordinator**: Main orchestration system from `orchestration/`
- **ViableStepDiscovery**: High-performance step readiness calculation
- **TaskFinalizer**: Task completion and state management
- **BackoffCalculator**: Exponential backoff and retry timing from `state_machine/`

### 4. Configuration and Constants
- **Constants System**: Complete migration of `constants.rb` (16KB)
- **Configuration Management**: Full `configuration.rb` (12KB) implementation
- **Cache Strategy**: Advanced caching from `cache_strategy.rb` (17KB)
- **Type System**: Type definitions and validations from `types/`

### 5. Event System and Pub/Sub
- **Lifecycle Events**: 56+ event types from `events/` directory
- **Event Publisher**: High-throughput event publishing
- **Subscriber Registry**: Pub/sub model for event subscribers
- **Event Routing**: Efficient event distribution

### 6. State Management
- **TaskStateMachine**: Complete state machine from `state_machine/`
- **StepStateMachine**: Individual step state transitions
- **Transition Tracking**: Full audit trail with WorkflowStepTransition equivalent
- **Atomic Operations**: Transaction-safe state management

### 7. Registry and Factory Systems
- **HandlerFactory**: Complete migration of `handler_factory.rb` (12KB)
- **Registry System**: Component registration from `registry/`
- **Task Builder**: Task construction from `task_builder.rb` (15KB)
- **Plugin System**: Dynamic plugin discovery and loading

### 8. Observability and Telemetry
- **Telemetry System**: Complete `telemetry.rb` migration
- **Health Monitoring**: Health check system from `health/`
- **Structured Logging**: Logging system from `logging/`
- **Performance Metrics**: Cache and performance tracking

## Performance Targets

The Rust implementation should provide significant performance improvements in:

- **Step Handler Lifecycle**: 10-100x faster than Ruby step handler execution
- **Dependency Resolution**: 10-100x faster than current PostgreSQL functions
- **State Transitions**: Sub-millisecond atomic state changes
- **Event Processing**: >10k events/sec publishing and routing
- **Model Queries**: High-performance equivalents of complex ActiveRecord scopes
- **Cache Operations**: Efficient cache strategy implementation

## FFI Integration Strategy

This core will be exposed to multiple dynamic languages:

- **Ruby**: Using `magnus` gem for Rails integration with step handler subclassing
- **Python**: Using `PyO3` for Python bindings with FastAPI integration
- **Node.js**: Using N-API for JavaScript step handler subclassing
- **C API**: C-compatible ABI for maximum interoperability

## Database Schema Modeling

Core entities to model based on `/Users/petetaylor/projects/tasker/spec/dummy/db/structure.sql`:

**Primary Tables**:
- `tasker_tasks` - Main task instances with JSONB context
- `tasker_workflow_steps` - Individual step instances with retry state
- `tasker_named_tasks` - Task templates with versioning
- `tasker_named_steps` - Step definitions and metadata
- `tasker_workflow_step_edges` - Dependency relationships (DAG)
- `tasker_task_namespaces` - Organizational hierarchy

**Supporting Tables**:
- `tasker_workflow_step_transitions` - Step state change audit trail
- `tasker_task_transitions` - Task state change audit trail
- `tasker_task_annotations` - Task metadata and annotations
- `tasker_annotation_types` - Annotation categorization
- `tasker_dependent_systems` - External system references
- `tasker_dependent_system_object_maps` - System object mappings
- `tasker_step_dag_relationships` - DAG structure representation
- `tasker_step_readiness_statuses` - Step readiness tracking
- `tasker_task_execution_contexts` - Execution metadata
- `tasker_task_diagrams` - Workflow visualization data

## Development Commands

Standard Rust development workflow:
- `cargo build` - Build the project
- `cargo test` - Run test suite including model and scope tests
- `cargo run` - Execute main binary
- `cargo bench` - Run performance benchmarks
- `cargo clippy` - Lint checking
- `cargo fmt` - Code formatting

## Project Structure

The project follows standard Rust conventions with comprehensive organization:
- Core step handler foundation (`src/step_handler/`)
- Complete model layer (`src/models/`)
- Orchestration engine (`src/orchestration/`)
- State machines (`src/state_machine/`)
- Event system and pub/sub (`src/events/`)
- Registry and factory systems (`src/registry/`)
- Configuration and constants (`src/config/`)
- FFI bindings (`src/ffi/`)
- Queue abstraction (`src/queue/`)
- Telemetry and observability (`src/telemetry/`)

## Safety and Reliability

This implementation prioritizes:
- **Memory Safety**: Eliminate memory leaks in long-running processes
- **Type Safety**: Prevent runtime errors in critical workflow logic
- **Concurrency Safety**: Data race elimination in multi-threaded operations
- **Error Handling**: Explicit error types with exhaustive pattern matching
- **Battle-tested Logic**: Port proven patterns from production Rails engine
- **Universal Foundation**: Consistent behavior across Rails, Python, Node.js

## Integration Notes

This Rust core serves as the foundational step handler that frameworks extend. The Rails engine provides the web interface and developer ergonomics, while this Rust core handles all performance and safety-critical workflow orchestration logic. The same foundation works across Rails, Python FastAPI, and Node.js Express applications.

## Project Scope
- Project will implement a comprehensive high-performance Rust core for the Tasker workflow orchestration engine
- Complete migration of all Rails models, logic, and business rules
- Provide a universal step handler foundation for multiple language frameworks
- Achieve significant performance improvements over existing Ruby implementation