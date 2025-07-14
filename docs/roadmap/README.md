# Tasker Core Rust - Development Roadmap

**Source of Truth for Development Progress and Planning**

## Project Status Overview

### ✅ Foundation Complete
- **Multi-workspace Architecture**: Main core + Ruby extension workspaces
- **Ruby FFI Integration**: Magnus-based bindings with proper build system
- **State Machine Framework**: Complete implementation with event publishing
- **Factory System**: Comprehensive test data generation for complex workflows
- **Orchestration Coordinator**: Core structure with async processing
- **Database Layer**: Models, scopes, and SQL functions implemented
- **Git Infrastructure**: Multi-workspace validation hooks and build artifacts management

### 🚨 CRITICAL: Ruby FFI Mitigation Required (January 2025)
**URGENT**: Prior session accidentally destroyed working Ruby FFI implementation before git commit
**STATUS**: Ruby bindings currently contain incorrect re-implementations instead of proper FFI bridges
**IMPACT**: Complete breakdown of Ruby-Rust integration - bindings don't use core logic

#### Critical Issues Discovered
1. **Handler Re-implementation**: `bindings/ruby/ext/tasker_core/src/handlers/` incorrectly re-implements `src/orchestration/step_handler.rs` and `src/orchestration/task_handler.rs` logic
2. **Event System Duplication**: `bindings/ruby/ext/tasker_core/src/events/` completely reimplements `src/events/` instead of bridging to it
3. **Architecture Violations**: Ruby bindings create new database connections and runtime instances per call instead of using singleton patterns
4. **Lost Working Implementation**: Previous working FFI bridge was lost in prior session before git commit

#### Recovery Strategy
- **Phase 3 PAUSED**: Enhanced event integration postponed until FFI foundation is restored
- **NEW PRIORITY**: Ruby FFI Mitigation Plan (see `docs/roadmap/ruby-ffi-mitigation-plan.md`)
- **APPROACH**: Remove incorrect implementations, create proper FFI bridges that delegate to core logic
- **TIMELINE**: 1-2 sessions to restore working Ruby integration

### 🎯 Current Focus: Ruby FFI Mitigation
**Phase**: Emergency recovery of Ruby FFI integration
**Goal**: Restore proper delegation architecture where Ruby bindings use core Rust logic

## Development Phases

### Phase 0: Ruby FFI Mitigation (🚨 URGENT - Current Priority)
**Goal**: Restore proper delegation architecture for Ruby FFI integration
**Timeline**: 1-2 sessions
**Status**: CRITICAL - Must complete before any other Ruby work

#### Session 1: Remove Incorrect Implementations
- [ ] **Delete Incorrect Handler Files**
  - Remove `bindings/ruby/ext/tasker_core/src/handlers/base_step_handler.rs`
  - Remove `bindings/ruby/ext/tasker_core/src/handlers/base_task_handler.rs`
- [ ] **Delete Incorrect Event Files**
  - Remove `bindings/ruby/ext/tasker_core/src/events/bridge.rs`
  - Remove `bindings/ruby/ext/tasker_core/src/events/publisher.rs`
  - Remove `bindings/ruby/ext/tasker_core/src/events/subscriber.rs`
- [ ] **Create Global Resource Management**
  - Implement singleton pattern for shared orchestration resources
  - Create proper FFI bridges that delegate to core logic
- [ ] **Update Module Structure**
  - Remove incorrect module exports
  - Update build system for new FFI bridge structure

#### Session 2: Integration Testing & Validation
- [ ] **Basic FFI Tests**
  - Test FFI bridges delegate to core logic
  - Verify singleton pattern works correctly
  - Validate resource reuse across multiple calls
- [ ] **Performance Validation**
  - Measure FFI overhead <1ms per call
  - Test database connection reuse
  - Validate memory usage stability
- [ ] **Commit Working Implementation**
  - Comprehensive testing of restored functionality
  - Git commit to prevent future loss

**Success Criteria**: Ruby bindings use core logic through proper FFI bridges, no reimplementation

### Phase 1: Critical Foundation (✅ COMPLETED - Week 1)
**Goal**: Fix critical placeholders and validate core functionality works

#### Week 1: Integration Tests & Placeholder Resolution
- [x] **Complex Workflow Integration Tests**
  - Linear workflows (A→B→C→D) ✅
  - Diamond patterns (A→(B,C)→D) ✅
  - Parallel merge patterns ✅
  - Tree and mixed DAG patterns ✅
- [x] **SQL Function Integration Tests**
  - Step readiness with complex dependencies ✅
  - Task execution context queries ✅
  - System health monitoring ✅
  - Analytics and metrics ✅
- [x] **TaskInitializer Implementation** ✅
  - Extracted from Ruby bindings with transaction safety
  - StateManager integration (eliminated code duplication)
  - Comprehensive test coverage (8 integration tests)
  - Integrated into complex workflow tests
- [ ] **Ruby Binding Basic Tests** (Postponed to Phase 2)
  - Module loading and initialization
  - Error hierarchy validation
  - Basic handler instantiation
  - Context serialization

**Success Criteria**: ✅ All critical integration tests pass, TaskInitializer fully implemented

### Phase 2: Event Publishing & Configuration (✅ COMPLETED - Week 2)
**Goal**: Complete event system and remove hardcoded values

#### Week 2: Event System & Configuration
- [x] **Event Publishing Core** ✅
  - Complete EventPublisher implementation
  - Add event buffering and batching
  - Create event type registry
  - Test event flow end-to-end
- [x] **Ruby Event Bridge** ✅
  - Implement FFI callback registration
  - Bridge to dry-events system
  - Test event propagation Ruby→Rust→Ruby
  - Ensure Rails engine compatibility
- [x] **Configuration Management** ✅
  - Extract all hardcoded values
  - Create config/tasker-core.yaml
  - Add environment variable overrides
  - Document all configuration options

**Success Criteria**: ✅ Events flow properly, zero hardcoded configuration values

### Phase 3: Enhanced Event Integration (🎯 CURRENT - Week 3)
**Goal**: Deep Rails dry-events integration with bidirectional event flow

#### Week 3: Rails Event System Integration
- [ ] **FFI Publishing Bridge** ⭐ HIGH PRIORITY
  - Enable Rust to publish directly to Rails dry-events Publisher singleton
  - Create payload serialization with Rails-compatible structure
  - Map Rust event types to Rails event constants
  - Integration testing with existing Rails subscribers
- [ ] **Custom Event Registration Bridge**
  - Register Rust custom events in Rails CustomRegistry
  - Bridge handler metadata (fired_by, descriptions)
  - Enable event catalog integration
- [ ] **BaseSubscriber Compatibility Layer**
  - Rust events compatible with Rails BaseSubscriber pattern
  - Automatic method routing for Rust-originating events
  - Safe payload access pattern compliance

**Success Criteria**: Rust events appear natively in Rails, existing Rails subscribers work seamlessly

### Phase 4: Developer Operations & Logging (Week 4)
**Goal**: Enable production operations and observability

#### Week 4: Operations & Observability
- [ ] **CRUD Operations**
  - Task reset attempts, cancel operations
  - Step result updates, mark resolved
  - State validation and audit trails
- [ ] **Structured Logging**
  - StructuredLogger with JSON formatting
  - CorrelationId generation (UUID v7)
  - Thread-local correlation storage
  - Rails.logger integration
- [ ] **Ruby Integration**
  - Expose CRUD operations via Magnus
  - Correlation ID flow Ruby↔Rust
  - REST endpoint helpers
  - Production monitoring hooks

**Success Criteria**: Full operational capabilities, structured logging operational

### Phase 5: Production Readiness (Week 5+)
**Goal**: Performance optimization and production deployment

#### Beyond Week 4: Polish & Optimization
- [ ] **Performance Optimization**
- [ ] **Documentation Completion**
- [ ] **Monitoring & Observability**
- [ ] **Final Production Testing**

## Critical Dependencies Resolution

### High Priority Placeholders (Must Fix)

Based on comprehensive TODO and placeholder analysis:

#### 1. State Machine Integration ✅ FIXED
**Files**: `src/client/{step_handler.rs, task_handler.rs}`
**Issue**: Client handlers can't transition states - core workflow execution incomplete
**Resolution**: TaskInitializer now properly integrates with StateManager, creating initial state transitions and initializing state machines
**Impact**: Workflow execution now functional

#### 2. Event Publishing System
**File**: `src/orchestration/task_finalizer.rs` (lines 703, 715, 730, 740, 750, 761)
**Issue**: All event publishing is no-ops
**Impact**: No monitoring, Rails integration impossible

#### 3. Ruby Framework Integration
**File**: `bindings/ruby/ext/tasker_core/src/handlers.rs`
**Issue**: Step delegation placeholder (line 1223)
**Impact**: Can't execute Ruby step handlers from Rust orchestration

#### 4. Configuration Hardcoding
**Files**: Multiple locations with hardcoded timeouts, limits, paths
**Issue**: Production deployment impossible without configuration management
**Impact**: Cannot tune for different environments

### Medium Priority Implementation Gaps

#### 5. SQL Function Edge Cases
**Files**: Various model files with complex query TODOs
**Issue**: Advanced queries not implemented
**Impact**: Limits analytics and complex dependency resolution

#### 6. Queue Integration
**File**: `src/orchestration/task_enqueuer.rs:393`
**Issue**: No actual queue system integration
**Impact**: Tasks can't be enqueued in production

## Architecture Principles

### Code Quality Standards
**"No More Placeholders"**: All new code must be implemented to completion
- No TODO stubs in production paths
- No placeholder return values
- Break work into discrete, completable sections
- Always implement with tests

### Testing-Driven Development
**"Fix Placeholders Through Tests"**: Use comprehensive tests to force completion
- Integration tests reveal missing implementations
- Tests validate actual functionality vs placeholders
- Test failures guide implementation priorities

### Sequential Progress
**"Follow the Plan"**: Work through roadmap systematically
- Complete Phase 1 before Phase 2
- Address critical dependencies first
- Validate each phase before proceeding
- Use success criteria to measure completion

## Key Technical Decisions

### Multi-Workspace Strategy
- **Main workspace**: Core Rust orchestration + future Python/WASM bindings
- **Ruby extension workspace**: Standalone for rb_sys compatibility
- **Benefits**: Proper isolation, build system compatibility, clean separation

### Performance Strategy
- **Core value**: 10-100x performance improvements through optimized SQL functions and Rust orchestration
- **Not about**: Ruby binding micro-optimizations (those are admin utilities)
- **Focus**: Dependency resolution, state management, concurrent processing

### Event Architecture
- **Pattern**: Rust EventPublisher → FFI bridge → Ruby dry-events
- **Benefits**: Real-time monitoring, Rails integration, production observability
- **Implementation**: Buffered, batched, with proper error handling

### Configuration Management
- **Pattern**: YAML configuration with environment overrides
- **Scope**: All timeouts, limits, paths, database settings
- **Goal**: Zero hardcoded values in production code

## Development Guidelines

### Implementation Approach
1. **Start with tests** - Write failing tests that reveal placeholders
2. **Implement completely** - No stubs, no partial implementations
3. **Validate immediately** - Run tests after each implementation
4. **Document decisions** - Update roadmap with learnings

### Success Metrics
- **Week 1**: All integration tests pass, complex workflows execute correctly
- **Week 2**: Events flow end-to-end, all configuration externalized
- **Week 3**: Full CRUD operations, structured logging operational
- **Production**: Zero placeholder code, comprehensive observability

## Related Documentation

### Current Phase Documents
- [Event System Analysis](./EVENT_SYSTEM.md) - Rails event system analysis and integration roadmap
- [Integration Test Strategy](./integration-tests.md) - Testing approach and patterns
- [Critical Placeholders](./critical-placeholders.md) - Placeholder analysis and priorities
- [Configuration Management](./configuration.md) - Configuration strategy and implementation

### Architecture Documents
- [Ruby Integration](./ruby-integration.md) - Ruby FFI binding architecture and workflow
- [State Machine Strategy](../STATE_MACHINE_STRATEGY.md) - State management approach
- [Orchestration Analysis](../ORCHESTRATION_ANALYSIS.md) - Performance and coordination design

### Reference Documentation
- [Testing Strategy](../TESTING_STRATEGY.md) - Overall testing approach
- [MCP Tools](../MCP_TOOLS.md) - Development tooling and integration
- [Schema Analysis](../SCHEMA_ANALYSIS.md) - Database design and relationships

### Archive
- [Historical Plans](./archive/) - Previous planning documents and deprecated strategies

---

**Last Updated**: 2025-01-14
**Next Review**: After Phase 3 completion
**Status**: Active Development - Phase 3 (Enhanced Event Integration)
