# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of the core workflow orchestration engine, designed to complement the existing Ruby on Rails **Tasker** engine found at `/Users/petetaylor/projects/tasker-systems/tasker-engine/`. This project leverages Rust's memory safety, fearless parallelism, and performance characteristics to handle computationally intensive workflow orchestration, dependency resolution, and state management operations.

**Architecture**: Delegation-based pattern where Rust implements the complete orchestration core that frameworks (Rails, Python, Node.js) extend through FFI integration with `process()` and `process_results()` hooks.

## Development Roadmap - Source of Truth

**ALWAYS REFER TO**: `docs/roadmap/README.md` as the authoritative source for:
- Current development phase and priorities
- Weekly milestones and success criteria
- Critical placeholder analysis and resolution strategy
- Implementation guidelines and code quality standards

**Current Status**: Phase 1 (Testing Foundation) - Week 1
- **Goal**: Fix critical placeholders through comprehensive integration tests
- **Focus**: Complex workflow tests, SQL function validation, Ruby binding tests
- **Rule**: No new placeholder/stub code - all implementations must be complete

## Code Design Principles

- **No More Placeholders**: All new code must be implemented to completion
- **Testing-Driven**: Use failing tests to expose and fix existing placeholders
- **Sequential Progress**: Complete current phase before proceeding to next
- Use TODO for intentionally delayed future work, but not to sidestep a better pattern

## Testing Guidelines

- As much as possible, tests should go in our tests/ directory, excluding doctests
- Use sqlx::test for tests requiring database access
- Use #[test] on its own for pure unit tests
- **Integration Tests Priority**: Complex workflows reveal system boundaries and force placeholder completion

## MCP Server Integration

**Essential Development Tools**: This project uses Model Context Protocol (MCP) servers to enhance development workflow and capabilities.

### Configured MCP Servers
- **PostgreSQL MCP** (`crystaldba/postgres-mcp`): Database operations, performance analysis, and migration management
- **GitHub Official MCP** (`github/github-mcp-server`): Repository operations, PR management, and CI/CD integration  
- **Cargo Package MCP** (`artmann/package-registry-mcp`): Rust dependency management and security analysis
- **Docker MCP** (`docker/mcp-servers`): Containerized testing and deployment automation
- **Rust Documentation MCP** (`Govcraft/rust-docs-mcp-server`): Real-time Rust best practices and API guidance
- **Context7 MCP** (SSE): Enhanced development context and intelligence
- **Tasker MCP** (Added July 2025): We now have access to all of the mcp servers in .mcp.json and should use them

**Configuration**: See `docs/MCP_TOOLS.md` for detailed setup, capabilities, and integration patterns.

**Benefits**: Enhanced database development, automated dependency management, streamlined CI/CD workflows, and real-time Rust guidance.

## Current Development Context (January 2025)

### Foundation Complete ✅
- **Multi-workspace Architecture**: Main core + Ruby extension workspaces
- **Ruby FFI Integration**: Magnus-based bindings with proper build system  
- **State Machine Framework**: Complete implementation with event publishing
- **Factory System**: Comprehensive test data generation for complex workflows
- **Orchestration Coordinator**: Core structure with async processing
- **Database Layer**: Models, scopes, and SQL functions implemented
- **Git Infrastructure**: Multi-workspace validation hooks and build artifacts management

### Critical Gaps (Current Focus) 🚧
Based on comprehensive placeholder analysis, these must be fixed before meaningful testing:

1. **State Machine Integration** - Client handlers can't transition states (workflow execution broken)
2. **Event Publishing System** - All publishing stubbed as no-ops (no monitoring possible)
3. **Ruby Step Delegation** - Placeholder step execution (can't run Ruby handlers from Rust)
4. **Configuration Hardcoding** - Prevents production deployment with environment-specific settings

### Implementation Status

#### ✅ Implemented
- **State Machine System**: TaskStateMachine and StepStateMachine with event publishing
- **Factory System**: Complex workflow patterns (Linear, Diamond, Parallel, Tree, Mixed DAG)
- **Ruby Bindings Foundation**: Handler base classes, task initialization, database integration
- **TaskHandlerRegistry**: Complete handler lookup with Ruby class names and YAML templates
- **SQL Function Integration**: High-performance operations leveraging existing tested functions
- **Orchestration Components**: BackoffCalculator, TaskFinalizer, TaskEnqueuer, Error Classification

#### 🚧 Critical Missing (Phase 1 Focus)
- **Client Handler State Integration**: State transitions in `src/client/{step_handler.rs, task_handler.rs}`
- **Event Publishing Core**: Complete implementation in `src/orchestration/event_publisher.rs`
- **Ruby Framework Integration**: Step delegation in `bindings/ruby/ext/tasker_core/src/handlers.rs`
- **Queue Integration**: Task enqueuing in `src/orchestration/task_enqueuer.rs`

## Recent Accomplishments (January 2025)

### State Machine Integration (COMPLETED)
- **Complete State Machine System**: Implemented all 7 planned phases with TaskStateMachine and StepStateMachine
- **Event-Driven Architecture**: Built EventPublisher with broadcast channels for real-time state transition notifications
- **Orchestration Integration**: Seamlessly integrated with OrchestrationCoordinator for unified workflow management
- **Database Persistence**: Full StateTransitionRepository with SQLx integration and idempotent operations
- **Guard System**: Comprehensive guard traits for dependency validation and completion checking
- **Testing Framework**: 103 integration tests + 42 doc tests with comprehensive coverage
- **Event Publishing Fix**: Resolved broadcast channel issue when no subscribers are present
- **Coverage Migration**: Migrated from cargo-tarpaulin to cargo-llvm-cov for superior LLVM-based coverage analysis

### SQL Scopes Implementation
- Created comprehensive Rails-like SQL scope system in `src/scopes.rs`
- Implemented `TaskScope`, `WorkflowStepScope`, and `TaskTransitionScope` with chainable query builders
- Added support for time-based queries, state filtering, and complex JOIN operations
- Achieved 11/12 test coverage with sophisticated deduplication for SQL JOINs

### Factory System for Complex Workflows
- Built comprehensive factory system in `tests/factories/` inspired by Rails patterns
- Implemented complex workflow patterns: Linear, Diamond, Parallel Merge, Tree, and Mixed DAG
- Created API integration workflow factories with multi-step dependencies
- Added dummy task workflows for orchestration testing
- Implemented find-or-create patterns for idempotent test data creation
- Added batch generation capabilities with controlled pattern distributions

### Configuration Architecture
- Separated configuration (`src/config.rs`) from constants (`src/constants.rs`)
- Created hierarchical configuration structure with nested components
- Maintained clean separation between runtime settings and immutable values

### Orchestration Layer Implementation
- **Phase 1 & 2 Complete**: Foundation model layer and critical infrastructure components implemented
- **BackoffCalculator**: Exponential backoff with jitter, server-requested delays, and context-aware retry strategies
- **TaskFinalizer**: Context-driven task completion with state machine integration and intelligent reenqueue logic
- **TaskEnqueuer**: Framework-agnostic task delegation supporting Rust, FFI, WASM, and JNI targets
- **Error Classification System**: Centralized error categorization with 10 error types and actionable remediation
- **Delegation Architecture**: Rust orchestrates decisions while frameworks handle execution and queue management
- **SQL Function Integration**: Boundary-defined reuse with existing tested SQL functions for step readiness and execution context
- **Production Ready**: Comprehensive test coverage with working demo examples and framework compatibility

### Ruby FFI Integration Progress
- **Multi-Workspace Setup**: Main core + Ruby extension with proper build isolation
- **Magnus Integration**: Complete Ruby FFI bindings with method registration
- **Build System**: Working Ruby gem compilation with rb_sys
- **Handler Foundation**: Ruby base classes (BaseTaskHandler, BaseStepHandler) with proper hooks
- **Context Serialization**: Type conversion between Rust and Ruby with proper error handling
- **Database Integration**: Task and WorkflowStep creation from Ruby with state machine setup

## Architecture Patterns Established

### Delegation-Based Architecture
- **Rust Core**: Handles orchestration, state management, dependency resolution, performance-critical operations
- **Framework Integration**: Rails/Python/Node.js handle business logic execution through FFI
- **SQL Functions**: Provide high-performance intelligence for step readiness and system health
- **Event System**: Real-time workflow monitoring and Rails integration through dry-events bridge

### Performance Targets
- **10-100x faster** dependency resolution vs PostgreSQL functions
- **<1ms FFI overhead** per orchestration call
- **>10k events/sec** cross-language event processing
- **<10% penalty** vs native Ruby execution for delegation

### Ruby Integration Workflow
1. **Task Discovery**: Rails → TaskHandlerRegistry → Ruby class name + YAML template
2. **Task Initialization**: Ruby → Rust task creation with DAG setup → Database persistence
3. **Task Execution**: Rails job → Rust orchestration → Concurrent step processing
4. **Step Processing**: Rust coordination → Ruby step handlers (concurrent) → Result collection
5. **Task Finalization**: Completion analysis → Backoff calculation → Re-enqueuing with delay
6. **Event Publishing**: Rust events → FFI bridge → Ruby dry-events → Rails job queue

## Quality Standards

### Code Quality Requirements
- **No Placeholder Code**: All new implementations must be complete
- **Test-Driven Development**: Write failing tests that expose placeholders, then implement
- **SQLx Integration**: All database tests use `#[sqlx::test]` with automatic isolation
- **Type Safety**: Full compile-time verification with proper error handling
- **Documentation**: Working doctests with realistic examples

### Development Workflow
- **Multi-Workspace Validation**: Git hooks validate both main core and Ruby extension
- **Continuous Integration**: Comprehensive CI/CD with quality gates, security auditing, and performance testing
- **Memory Safety**: All FFI boundaries properly managed with Ruby GC integration
- **Configuration Management**: All hardcoded values extracted to YAML with environment overrides

## Related Projects Context

### Multi-Project Ecosystem
- **tasker-engine/**: Production-ready Rails engine for workflow orchestration
- **tasker-core-rs/**: High-performance Rust core for performance-critical operations  
- **tasker-blog/**: GitBook documentation with real-world engineering stories

### Integration Context
- **Database Schema**: Shared PostgreSQL schema between Rails and Rust
- **Configuration Compatibility**: YAML-based configuration matching Rails patterns
- **Event Compatibility**: Event format aligned with Rails dry-events and Statesman
- **Migration Strategy**: Zero-disruption integration with feature flag rollout

## Current Working Context

- **Main Branch**: `orchestration`
- **Development Phase**: Phase 1 (Testing Foundation) - Week 1
- **Priority**: Integration tests for complex workflows to expose and fix critical placeholders
- **Success Criteria**: All integration tests pass, no critical placeholders remain
- **Next Phase**: Configuration management and event publishing completion

## Key File Locations

### Roadmap and Planning
- **Primary Source**: `docs/roadmap/README.md` (ALWAYS REFERENCE THIS)
- **Critical Placeholders**: `docs/roadmap/critical-placeholders.md`
- **Ruby Integration**: `docs/roadmap/ruby-integration.md`
- **Testing Strategy**: `docs/roadmap/integration-tests.md`
- **Configuration Plan**: `docs/roadmap/configuration.md`

### Core Implementation
- **Models**: `src/models/core/` (Task, WorkflowStep, etc.)
- **Orchestration**: `src/orchestration/` (WorkflowCoordinator, TaskFinalizer, etc.)
- **State Machines**: `src/state_machine/` (TaskStateMachine, StepStateMachine)
- **Ruby Bindings**: `bindings/ruby/ext/tasker_core/src/` (FFI integration)
- **Test Factories**: `tests/factories/` (Complex workflow patterns)

### Configuration and Infrastructure
- **Database Schema**: `db/structure.sql`
- **Migrations**: `migrations/`
- **Configuration**: `config/` (YAML configuration files)
- **Git Hooks**: `.githooks/` (Multi-workspace validation)

---

**Remember**: Always consult `docs/roadmap/README.md` for current priorities and development context. The roadmap is the single source of truth for development planning and progress tracking.