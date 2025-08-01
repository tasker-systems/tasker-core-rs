# TODO Analysis and Action Plan

This document categorizes all TODO items found in the codebase and provides an action plan for addressing them.

## Executive Summary

Found **87 TODO items** across the codebase. Most fall into three categories:
1. **Missing Core Functionality** (35 items) - Essential features not yet implemented
2. **Intentionally Delayed Future State** (42 items) - Planned enhancements and optimizations  
3. **Unnecessary/Low Priority** (10 items) - Can be removed or deprioritized

## Category Breakdown

### 1. Missing Core Functionality (High Priority) - 35 items

These TODOs represent core functionality that's essential for the system to work properly.

#### A. FFI Interfaces (3 items)
- `src/ffi/c_api.rs` - C-compatible ABI implementation
- `src/ffi/python.rs` - Python FFI bindings using pyo3  
- `src/ffi/ruby.rs` - Ruby FFI bindings using magnus

**Action**: Implement based on integration requirements

#### B. Core Event System (2 items)
- `src/events/lifecycle_events.rs` - Comprehensive workflow lifecycle coverage
- `src/events/subscriber.rs` - Event subscription management

**Action**: Remove empty stubs per consolidation plan, functionality exists elsewhere

#### C. Core Orchestration Components (4 items)
- `src/orchestration/backoff_calculator.rs` - Exponential backoff and retry timing
- `src/orchestration/task_finalizer.rs` - Task completion and state management
- Step execution error class extraction (2 instances)

**Action**: Implement immediately - critical for orchestration

#### D. Model Method Implementations (26 items)
Essential method implementations in `workflow_step.rs` and `task.rs`:

**WorkflowStep Critical Methods**:
- State transition queries (completed, failed, pending, by_current_state)
- Readiness checking integration with StepReadinessStatus
- Dependency analysis and retry logic
- DAG relationship setup and traversal
- Template-based step creation

**Task Critical Methods**:
- State transition queries (by_current_state, active status)
- TaskRequest integration and conversion
- State machine integration
- Step lookup and dependency graph analysis

**Action**: Prioritize based on usage in orchestration layer

### 2. Intentionally Delayed Future State (Medium Priority) - 42 items

These represent planned enhancements and optimizations that can be implemented incrementally.

#### A. Performance Optimizations (15 items)
- Complex state machine transition queries 
- Metadata-based filtering and calculations
- Memoization clearing for reloaded instances
- Association preloading strategies

#### B. Advanced Analytics (8 items)
- Retry attempt calculations from transitions
- Average execution time calculations  
- Time-based completion and failure filtering
- Task completion statistics

#### C. Configuration and Template System (6 items)
- Template-based step creation and configuration
- Default option generation based on named task configuration
- Validation system integration

#### D. Enhanced Event Integration (5 items)
- EventPublisher integration for step execution events
- Actual FFI bridge implementation for cross-language events
- Type validation for event payloads

#### E. Orchestration Enhancements (8 items)
- System configuration integration for discovery/batch settings
- Actual task name resolution instead of placeholder formatting
- Processing mode determination (sequential vs parallel)
- Failed steps tracking during execution

### 3. Unnecessary/Low Priority (Low Priority) - 10 items

#### A. Property Testing Issues (3 items)
- `tests/property_based_tests.rs` - Async issues with property testing

**Action**: Fix or remove ignored tests

#### B. Test Data Dependencies (6 items)
- Insights tests waiting for factory system (already implemented)
- API integration workflow step states application

**Action**: Update tests to use existing factory system

#### C. Development/Placeholder Comments (1 item)
- SQL AST builder refactoring comment

**Action**: Address during performance optimization phase

## Implementation Roadmap

### Phase 1: Core Model Functionality (Weeks 1-2)
**Foundation Priority - Essential model layer implementation**

1. **WorkflowStep Implementation**:
   - Complete state transition queries (completed, failed, pending, by_current_state)
   - Implement dependency analysis methods
   - Add retry eligibility checking
   - Basic readiness checking methods

2. **Task Implementation**:
   - Complete state transition queries (by_current_state, active status)
   - Add basic TaskRequest integration
   - Implement step lookup methods
   - Step and task state machine integration

3. **DAG and Dependency Logic**:
   - Implement step relationship setup
   - Add basic template-based step creation
   - Foundation for dependency analysis

### Phase 2: Critical Infrastructure (Weeks 3-4)
**Orchestration Priority - Building on model foundation**

1. **Core Orchestration Components**:
   - Implement `BackoffCalculator` for retry timing (integrates with model retry logic)
   - Implement `TaskFinalizer` for completion handling (uses model state transitions)
   - Add error class extraction for step execution

2. **Integration and Coordination**:
   - Orchestration layer integration with completed model functionality
   - State transition coordination between models and orchestration
   - Event integration for model state changes

3. **Clean Up Empty Stubs**:
   - Remove `events/lifecycle_events.rs` and `events/subscriber.rs`
   - Update module references

### Phase 3: Advanced Features (Weeks 5-8)
**Medium Priority - Enhanced functionality**

1. **Performance Optimizations**:
   - Implement complex state transition queries
   - Add metadata-based filtering
   - Optimize readiness checking performance

2. **Analytics and Insights**:
   - Implement retry attempt calculations
   - Add execution time tracking
   - Complete task completion statistics

3. **Enhanced Configuration**:
   - Full template system implementation
   - Advanced default option generation
   - Validation system integration

### Phase 4: Integration and Polish (Weeks 9-12)
**Lower Priority - Integration features**

1. **FFI Implementation** (as needed):
   - Implement based on actual integration requirements
   - Start with most critical language binding

2. **Advanced Event Integration**:
   - Complete EventPublisher integration
   - Implement FFI bridge for events
   - Add event payload type validation

3. **Test and Documentation**:
   - Fix property testing issues
   - Update insight tests to use factories
   - Complete API documentation

## Success Metrics

- [ ] All critical orchestration functionality works end-to-end
- [ ] State transition queries return accurate results
- [ ] Retry and backoff logic functions correctly
- [ ] Task and step lifecycle management is complete
- [ ] Performance targets are met for readiness checking
- [ ] All tests pass without ignored items

## Dependencies and Blockers

1. **TaskRequest Model**: Many TODOs depend on this not-yet-modeled concept
   - **Action**: Define TaskRequest architecture before implementing dependent features

2. **Template System**: Step creation depends on template definitions
   - **Action**: Complete orchestration config system first

3. **State Machine Integration**: Many methods need state machine completion
   - **Action**: Ensure state machine layer is fully functional

## Risk Assessment

**High Risk**:
- Core orchestration components (BackoffCalculator, TaskFinalizer) - system won't work without these

**Medium Risk**:
- Model state transition queries - impacts readiness checking accuracy

**Low Risk**:
- Analytics and performance optimizations - can be added incrementally
- FFI bindings - only needed when integrating with other languages