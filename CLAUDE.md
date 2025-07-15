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

**Current Status**: Phase 3 (Ruby Integration Testing) - Week 3 - TaskConfigFinder Complete
- **Goal**: Complete validation of Ruby-Rust integration and prepare for production use
- **Focus**: End-to-end Ruby step handler testing, database integration, performance validation
- **Recent Achievement**: ✅ TaskConfigFinder implementation complete with registry and file system fallback
- **Rule**: Ensure Ruby step handlers work seamlessly with Rust orchestration core

## Recent Major Achievement: TaskConfigFinder Implementation

### ✅ TaskConfigFinder Complete (January 2025)
**Status**: ✅ **COMPLETE** - Centralized task configuration discovery with registry integration
**Impact**: Eliminated hardcoded configuration paths and enabled Ruby handler configuration registration

#### Key Features Implemented
- **Registry-First Search**: Checks TaskHandlerRegistry for registered configurations before file system
- **File System Fallback**: Searches multiple paths with versioned and default naming patterns
- **Path Resolution**: Uses configurable `task_config_directory` from `tasker-config.yaml`
- **Type Conversion**: Seamless conversion between config and model TaskTemplate types
- **Ruby Integration**: Enables Ruby handlers to register configurations directly in registry

#### Technical Implementation
```rust
// Search Strategy: Registry → File System
pub async fn find_task_template(
    &self,
    namespace: &str,
    name: &str,
    version: &str,
) -> OrchestrationResult<TaskTemplate>

// Path Patterns:
// 1. <config_dir>/tasks/{namespace}/{name}/{version}.(yml|yaml)
// 2. <config_dir>/tasks/{name}/{version}.(yml|yaml)
// 3. <config_dir>/tasks/{name}.(yml|yaml)
```

#### Integration Points
- **StepExecutor**: Now uses TaskConfigFinder instead of hardcoded paths
- **WorkflowCoordinator**: Creates and injects TaskConfigFinder into StepExecutor
- **TaskHandlerRegistry**: Enhanced with TaskTemplate storage and retrieval
- **Ruby Handlers**: Can register configurations directly in registry

#### Test Coverage
- ✅ **553 total tests passing** (92 unit + 86 integration + 199 comprehensive + 63 doctests + more)
- ✅ **TaskConfigFinder demo** working with registry and file system examples
- ✅ **Ruby bindings** still compile correctly after changes
- ✅ **All git hooks** passing including doctest compilation

## Code Design Principles

- **No More Placeholders**: All new code must be implemented to completion [[memory:3255552]]
- **Testing-Driven**: Use failing tests to expose and fix existing placeholders
- **Sequential Progress**: Complete current phase before proceeding to next
- **Proper Integration**: All code must delegate properly to Rust core system [[memory:3255552]]
- Use TODO for intentionally delayed future work, but not to sidestep a better pattern

## Testing Guidelines

- As much as possible, tests should go in our tests/ directory, excluding doctests
- Use sqlx::test for tests requiring database access
- Use #[test] on its own for pure unit tests
- **Integration Tests Priority**: Complex workflows reveal system boundaries and force placeholder completion
- **Doctest Excellence**: Maintain high doctest success rate with working examples

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

### 🚨 CRITICAL: Ruby FFI Mitigation Crisis
**URGENT PRIORITY**: Prior session accidentally destroyed working Ruby FFI implementation before git commit
**CURRENT STATUS**: Ruby bindings contain incorrect re-implementations instead of proper FFI bridges
**IMPACT**: Complete breakdown of Ruby-Rust integration - bindings don't use core logic

#### What Happened
- Previous session had working Ruby FFI bridges that properly delegated to core Rust logic
- Session ended before committing working implementation to git
- Subsequent session accidentally overwrote working code with incorrect re-implementations
- Current bindings violate delegation architecture by reimplementing core logic

#### Critical Issues in Current Codebase
1. **Handler Violations**: `bindings/ruby/ext/tasker_core/src/handlers/base_step_handler.rs` and `base_task_handler.rs` incorrectly re-implement logic from `src/orchestration/step_handler.rs` and `src/orchestration/task_handler.rs`
2. **Event System Duplication**: `bindings/ruby/ext/tasker_core/src/events/` completely reimplements `src/events/publisher.rs` and `src/events/types.rs` instead of bridging to them
3. **Architecture Violations**: Ruby bindings create new database connections, runtime instances, and orchestration components per FFI call instead of using singleton patterns
4. **Performance Impact**: Each FFI call recreates expensive resources, violating the delegation architecture's performance goals

#### Recovery Strategy
- **Phase 3 PAUSED**: Enhanced event integration postponed until FFI foundation is restored
- **NEW PRIORITY**: Ruby FFI Mitigation Plan (documented in `docs/roadmap/ruby-ffi-mitigation-plan.md`)
- **APPROACH**: Remove incorrect implementations, create proper FFI bridges that delegate to core logic
- **TIMELINE**: 1-2 sessions to restore working Ruby integration before resuming Phase 3

### Testing-Driven Development Success 🎯
**Approach**: Using comprehensive integration tests to systematically expose and fix critical placeholders
**Philosophy**: Test failures are documentation - they show us exactly what needs to be implemented
**Results**: Methodical identification and resolution of system-breaking issues

### Critical Placeholders Fixed Through Testing ✅
1. **SQL Schema Alignment** - Fixed `error_steps` vs `failed_steps` column mismatch in TaskExecutionContext
2. **Type System Integrity** - Fixed BigDecimal to f64 conversion in TaskFinalizer
3. **SQL Type Compatibility** - Fixed `named_step_id` i64 vs i32 mismatch across all components
4. **Database Function Integration** - Verified get_task_execution_context SQL function alignment

### Phase 2 Completion Success ✅
**Event Publishing & Configuration**: All critical components implemented and working
- **FFI Event Bridge**: Rust→Ruby event forwarding with callback registration
- **Configuration Management**: All hardcoded values extracted to YAML
- **External Callback System**: Cross-language event handler registry functional
- **Integration Tests**: Events flow end-to-end with proper error handling

### Foundation Complete ✅
- **Multi-workspace Architecture**: Main core + Ruby extension workspaces
- **Ruby FFI Integration**: Magnus-based bindings with proper build system
- **State Machine Framework**: Complete implementation with event publishing
- **Factory System**: Comprehensive test data generation for complex workflows
- **Orchestration Coordinator**: Core structure with async processing
- **Database Layer**: Models, scopes, and SQL functions implemented
- **Git Infrastructure**: Multi-workspace validation hooks and build artifacts management

### Integration Test Infrastructure ✅
- **MockFrameworkIntegration**: Complete test framework integration for orchestration testing
- **Real Task Creation**: Factory system properly creates tasks with workflow steps
- **Orchestration Flow**: End-to-end test successfully reaches step state validation
- **Systematic Discovery**: Each test run exposes the next critical placeholder requiring implementation

### Implementation Status

#### ✅ Implemented
- **State Machine System**: TaskStateMachine and StepStateMachine with event publishing
- **Factory System**: Complex workflow patterns (Linear, Diamond, Parallel, Tree, Mixed DAG)
- **Ruby Bindings Foundation**: Handler base classes, task initialization, database integration
- **TaskHandlerRegistry**: Complete handler lookup with Ruby class names and YAML templates
- **SQL Function Integration**: High-performance operations leveraging existing tested functions
- **Orchestration Components**: BackoffCalculator, TaskFinalizer, TaskEnqueuer, Error Classification

#### 🎯 Phase 3 Focus (Enhanced Event Integration)
- **FFI Publishing Bridge**: Enable Rust to publish directly to Rails dry-events Publisher singleton
- **Payload Compatibility**: Create Rails-compatible event payload structures
- **Event Type Mapping**: Map Rust event names to Rails event constants
- **BaseSubscriber Integration**: Rust events compatible with Rails subscription patterns
- **Custom Event Registration**: Bridge Rust custom events to Rails CustomRegistry

## Recent Achievements (January 2025)

### ✅ Ruby FFI Integration Complete
- **Step Handler Architecture**: `RubyStepHandler` properly implements Rust `StepHandler` trait
- **Task Configuration Flow**: Step handlers resolved through task templates, not class names
- **Previous Step Results**: Dependencies loaded using `WorkflowStep::get_dependencies()`
- **Magnus Integration**: TypedData objects properly cloned and converted
- **Compilation Success**: All trait bounds and missing functions resolved
- **Test Coverage**: 95+ Rust orchestration tests passing, Ruby extension compiles cleanly

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

### Event System Unification (January 2025)
- **Unified EventPublisher**: Combined three separate event implementations into single cohesive system in `src/events/`
- **Dual API Support**: Simple (name + context) and structured (typed events) APIs for different use cases
- **FFI Bridge Foundation**: Event system ready for cross-language integration with Rails dry-events
- **Type Conversion System**: Seamless conversion between orchestration and events types
- **Comprehensive Testing**: 15 tests covering all event publishing scenarios with proper error handling

### TaskHandlerRegistry Unification (January 2025) - ✅ COMPLETED
- **Problem Identified**: Ruby bindings had duplicate TaskHandlerRegistry implementation creating new instances on every FFI call, losing all registered handlers
- **Root Cause**: 296 lines of duplicate implementation in `bindings/ruby/ext/tasker_core/src/handlers.rs` instead of using core registry + singleton pattern violation
- **Impact**: Complete breakdown of Ruby-Rust integration workflow as handler lookup failed consistently
- **Solution Implemented**:
  - Removed duplicate TaskHandlerRegistry implementation from Ruby bindings
  - Created unified architecture using `tasker_core::registry::TaskHandlerRegistry` as single source of truth
  - Implemented proper singleton pattern with `OnceLock<TaskHandlerRegistry>` for FFI operations
  - Created `TaskHandlerRegistryWrapper` for Rails compatibility with YAML file lookup
  - Updated all FFI wrapper functions to use unified singleton implementation
- **Results**:
  - Reduced from 296 lines of duplicate code to 76 lines of wrapper code
  - Registry state now maintained across FFI calls
  - Access to advanced core features: event publishing, dual-path support, validation, namespaces
  - Thread-safe concurrent access with proper memory management
  - Backward compatibility with Rails YAML-based handler discovery maintained
- **Status**: ✅ CRITICAL ISSUE RESOLVED - Ruby-Rust integration workflow now functional

### Phase 2: Event Publishing & Configuration (COMPLETED - January 2025)
- **FFI Event Bridge Implementation**: Created `event_bridge_register` and `event_bridge_unregister` FFI functions with global RUBY_EVENT_CALLBACK registry
- **External Callback System**: Implemented `register_external_event_callback` in EventPublisher for cross-language event forwarding
- **Configuration Management**: Extracted hardcoded values from task_handler.rs and workflow_coordinator.rs to YAML configuration
- **Integration Testing**: Created comprehensive event bridge tests verifying Rust→Ruby event flow
- **Rails Compatibility**: Updated Ruby events.rb to use actual FFI functions instead of placeholders
- **Compilation Fixes**: Resolved all import path and API compatibility issues across the test suite
- **Status**: ✅ PHASE 2 COMPLETED - Events flow properly, zero hardcoded configuration values

### Rails Engine Event System Analysis (January 2025)
- **Architecture Analysis**: Comprehensive review of Rails engine event system including Publisher singleton, BaseSubscriber pattern, and dry-events integration
- **Pattern Documentation**: Analyzed declarative subscriptions, automatic method routing, dual event types (system + custom), and observability strategy
- **Integration Opportunities**: Identified specific enhancements for Rust-Ruby bridge including FFI publishing, payload compatibility, and event type mapping
- **Implementation Roadmap**: Created detailed Phase 3 plan for enhanced event integration with specific technical specifications
- **Documentation**: Created comprehensive EVENT_SYSTEM.md with gap analysis and implementation roadmap
- **Status**: ✅ ANALYSIS COMPLETED - Ready for Phase 3 enhanced integration implementation

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
- **Development Phase**: Phase 0 (Ruby FFI Mitigation) - URGENT PRIORITY
- **Current Status**: Ruby FFI integration completely broken, requires immediate recovery
- **Recent Crisis**:
  - Working Ruby FFI implementation was lost in prior session before git commit
  - Current bindings contain incorrect re-implementations instead of proper FFI bridges
  - Architecture violations prevent Ruby-Rust integration from functioning
- **Current Priority**: Execute Ruby FFI Mitigation Plan (see `docs/roadmap/ruby-ffi-mitigation-plan.md`)
- **Next Priorities**:
  1. Remove incorrect handler and event implementations
  2. Create proper FFI bridges that delegate to core logic
  3. Implement singleton pattern for shared resources
  4. Validate performance targets and commit working implementation
- **Success Criteria**: Ruby bindings use core logic through proper FFI bridges, no duplicate implementations
- **Post-Recovery**: Resume Phase 3 enhanced event integration once FFI foundation is restored

## Key File Locations

### Roadmap and Planning
- **Primary Source**: `docs/roadmap/README.md` (ALWAYS REFERENCE THIS)
- **URGENT**: `docs/roadmap/ruby-ffi-mitigation-plan.md` (Ruby FFI recovery plan)
- **Event System Analysis**: `docs/roadmap/EVENT_SYSTEM.md` (Rails integration roadmap)
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

## Latest Session Summary (January 2025)

### What We Accomplished
1. **Comprehensive Rails Event System Analysis**: Detailed review of Publisher singleton, BaseSubscriber pattern, dry-events integration, and observability strategy
2. **EVENT_SYSTEM.md Documentation**: Created complete analysis document with gap analysis and Phase 3 implementation roadmap
3. **Roadmap Updates**: Updated docs/roadmap/README.md to reflect Phase 2 completion and Phase 3 priorities
4. **CLAUDE.md Updates**: Comprehensive updates to reflect current Phase 3 status and enhanced event integration focus
5. **Implementation Roadmap**: Detailed technical specifications for FFI publishing bridge, payload compatibility, and event type mapping

### Current Phase 3 Priorities (Starting Next Session)
1. **FFI Publishing Bridge**: Implement `rust_publish_to_rails` FFI function to enable Rust publishing directly to Rails dry-events Publisher singleton
2. **Payload Compatibility**: Create Rails-compatible event payload structures with automatic timestamp enhancement
3. **Event Type Mapping**: Map Rust event names to Rails constants for seamless integration

### Key Files Created/Modified This Session
- `docs/roadmap/EVENT_SYSTEM.md`: Complete Rails event system analysis and integration roadmap (NEW)
- `docs/roadmap/README.md`: Updated to reflect Phase 2 completion and Phase 3 focus
- `CLAUDE.md`: Comprehensive updates for Phase 3 status and priorities

---

**Remember**: Always consult `docs/roadmap/README.md` for current priorities and development context. The roadmap is the single source of truth for development planning and progress tracking.
