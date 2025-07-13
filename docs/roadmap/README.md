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

### 🎯 Current Focus: Testing Foundation
**Phase**: Complete critical placeholders, then comprehensive integration tests

**Why**: Before building more features, we need to ensure the foundation works correctly with:
- Complex workflow execution (Diamond, Parallel, Tree patterns)
- SQL functions working with real dependencies
- State machine transitions functioning properly
- Event publishing flowing correctly

## Development Phases

### Phase 1: Critical Foundation (Current - Week 1)
**Goal**: Fix critical placeholders and validate core functionality works

#### Week 1: Integration Tests & Placeholder Resolution
- [ ] **Complex Workflow Integration Tests**
  - Linear workflows (A→B→C→D) 
  - Diamond patterns (A→(B,C)→D)
  - Parallel merge patterns
  - Tree and mixed DAG patterns
- [ ] **SQL Function Integration Tests**
  - Step readiness with complex dependencies
  - Task execution context queries
  - System health monitoring
  - Analytics and metrics
- [ ] **Ruby Binding Basic Tests**
  - Module loading and initialization
  - Error hierarchy validation
  - Basic handler instantiation
  - Context serialization

**Success Criteria**: All integration tests pass, no critical placeholders remain

### Phase 2: Event Publishing & Configuration (Week 2)
**Goal**: Complete event system and remove hardcoded values

#### Week 2: Event System & Configuration
- [ ] **Event Publishing Core**
  - Complete EventPublisher implementation
  - Add event buffering and batching  
  - Create event type registry
  - Test event flow end-to-end
- [ ] **Ruby Event Bridge**
  - Implement FFI callback registration
  - Bridge to dry-events system
  - Test event propagation Ruby→Rust→Ruby
  - Ensure Rails engine compatibility
- [ ] **Configuration Management**
  - Extract all hardcoded values
  - Create config/tasker-core.yaml
  - Add environment variable overrides
  - Document all configuration options

**Success Criteria**: Events flow properly, zero hardcoded configuration values

### Phase 3: Developer Operations & Logging (Week 3)
**Goal**: Enable production operations and observability

#### Week 3: Operations & Observability
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

### Phase 4: Production Readiness (Week 4+)
**Goal**: Performance optimization and production deployment

#### Beyond Week 3: Polish & Optimization
- [ ] **Performance Optimization**
- [ ] **Documentation Completion**
- [ ] **Monitoring & Observability**
- [ ] **Final Production Testing**

## Critical Dependencies Resolution

### High Priority Placeholders (Must Fix)

Based on comprehensive TODO and placeholder analysis:

#### 1. State Machine Integration
**Files**: `src/client/{step_handler.rs, task_handler.rs}`
**Issue**: Client handlers can't transition states - core workflow execution incomplete
**Impact**: Workflow execution fundamentally broken

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

**Last Updated**: 2025-01-13  
**Next Review**: After Phase 1 completion  
**Status**: Active Development - Phase 1 (Testing Foundation)