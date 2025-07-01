# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of the core workflow orchestration engine, designed to complement the existing Ruby on Rails **Tasker** engine found at `/Users/petetaylor/projects/tasker/`. This project leverages Rust's memory safety, fearless parallelism, and performance characteristics to handle computationally intensive workflow orchestration, dependency resolution, and state management operations.

## Architecture Context

This Rust implementation is based on the production-ready Rails Tasker engine, which provides:

- **Complex Workflow Orchestration**: DAG-based task and workflow step management
- **Intelligent State Management**: Task and step state machines with retry logic
- **High-Performance Dependency Resolution**: Currently implemented as PostgreSQL functions
- **Event-Driven Architecture**: 56+ lifecycle events with pub/sub patterns
- **Production Observability**: OpenTelemetry, structured logging, and health monitoring

## Core Components to Implement

### 1. Orchestration Engine
- **Coordinator**: Main orchestration system initialization and monitoring
- **StepExecutor**: Concurrent step execution with performance monitoring  
- **TaskFinalizer**: Task completion and state management
- **ViableStepDiscovery**: High-performance step readiness calculation
- **BackoffCalculator**: Exponential backoff and retry timing

### 2. State Management
- **TaskStateMachine**: Thread-safe task lifecycle management
- **StepStateMachine**: Individual step state transitions
- Atomic state transitions with idempotent operations

### 3. Database Layer
- **Core Entities**: Tasks, WorkflowSteps, NamedTasks, WorkflowStepEdges
- **Dependency Graph Analysis**: Efficient DAG traversal algorithms
- **Step Readiness Calculation**: Replace PostgreSQL functions with Rust
- **State Persistence**: Transaction-safe state management

### 4. Event System
- **Event Publisher**: High-throughput event publishing
- **Event Routing**: Efficient event distribution to subscribers
- **Lifecycle Events**: Comprehensive workflow lifecycle coverage

### 5. Registry Systems
- **HandlerFactory**: Thread-safe task handler registration
- **PluginRegistry**: Dynamic plugin discovery and loading
- **SubscriberRegistry**: Event subscriber management

## Performance Targets

The Rust implementation should provide significant performance improvements in:

- **Dependency Resolution**: 10-100x faster than current PostgreSQL functions
- **Concurrent Step Execution**: Memory-safe parallelism with better resource utilization
- **State Transitions**: Lock-free concurrent operations where possible
- **Event Processing**: High-throughput event publishing and routing

## FFI Integration Strategy

This core will be exposed to multiple dynamic languages:

- **Ruby**: Using `magnus` gem for Rails integration
- **Python**: Using `PyO3` for Python bindings  
- **JavaScript/TypeScript**: Using Node.js FFI or Deno FFI
- **Shared Library**: C-compatible ABI for maximum interoperability

## Database Schema Modeling

Core entities to model based on `/Users/petetaylor/projects/tasker/spec/dummy/db/structure.sql`:

- `tasker_tasks` - Main task instances with JSONB context
- `tasker_workflow_steps` - Individual step instances with retry state
- `tasker_named_tasks` - Task templates with versioning
- `tasker_named_steps` - Step definitions and metadata
- `tasker_workflow_step_edges` - Dependency relationships (DAG)
- `tasker_task_namespaces` - Organizational hierarchy
- State transition tables for comprehensive audit trails

## Development Commands

Standard Rust development workflow:
- `cargo build` - Build the project
- `cargo test` - Run test suite
- `cargo run` - Execute main binary
- `cargo bench` - Run performance benchmarks
- `cargo clippy` - Lint checking
- `cargo fmt` - Code formatting

## Project Structure

The project will follow standard Rust conventions with additional organization for:
- Core engine (`src/orchestration/`)
- State machines (`src/state_machine/`)
- Database models (`src/models/`)
- Event system (`src/events/`)
- FFI bindings (`src/ffi/`)
- Registry systems (`src/registry/`)

## Safety and Reliability

This implementation prioritizes:
- **Memory Safety**: Eliminate memory leaks in long-running processes
- **Type Safety**: Prevent runtime errors in critical workflow logic
- **Concurrency Safety**: Data race elimination in multi-threaded operations
- **Error Handling**: Explicit error types with exhaustive pattern matching
- **Battle-tested Logic**: Port proven patterns from production Rails engine

## Integration Notes

This Rust core is designed to work alongside, not replace, the Rails Tasker engine. The Rails engine provides the web interface, developer ergonomics, and rich domain-specific language, while this Rust core handles the performance and safety-critical workflow orchestration logic.