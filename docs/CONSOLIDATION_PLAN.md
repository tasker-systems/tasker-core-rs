# Module Consolidation Plan

This document outlines the findings from our architectural review of overlapping modules and provides a clear consolidation strategy.

## Executive Summary

After comprehensive analysis, most modules serve distinct purposes and should remain separate. The primary consolidation opportunity is in the configuration systems, where we have two overlapping implementations that should be unified.

## Analysis Findings

### 1. Registry Systems: orchestration/registry.rs vs registry/ namespace

**Finding**: These serve **different purposes** and should remain separate.

- **`orchestration/registry.rs`**: 
  - Specialized for orchestration task handlers
  - Dual-path support (Rust direct references + FFI stringified references)
  - Thread-safe with RwLock
  - Event integration for registration notifications
  
- **`registry/` namespace**: 
  - General-purpose system registries
  - Contains HandlerFactory, PluginRegistry, SubscriberRegistry
  - Broader scope beyond just task handlers

**Decision**: ✅ **Keep separate** - No consolidation needed

### 2. Event Publishers: orchestration/event_publisher.rs vs events/ namespace

**Finding**: The orchestration event publisher is more advanced and serves a specific purpose.

- **`orchestration/event_publisher.rs`**:
  - Sophisticated async event publishing system
  - FFI bridge support for cross-language events
  - Correlation IDs for distributed tracing
  - Event batching and backpressure handling
  - Broadcast channels with subscriber management
  
- **`events/` namespace**:
  - Basic infrastructure for system-wide events
  - Contains empty `lifecycle_events.rs` stub (TODO placeholder)
  - General publisher and subscriber interfaces

**Decision**: 
- ✅ Keep `orchestration/event_publisher.rs` for orchestration-specific events
- 🗑️ Remove empty `events/lifecycle_events.rs` stub
- ✅ Maintain `events/` for general system event infrastructure

### 3. Configuration Systems: orchestration/config.rs vs config.rs

**Finding**: Significant functional overlap with different approaches - prime candidate for consolidation.

- **`orchestration/config.rs`** (681 lines):
  - Advanced YAML-driven configuration system
  - Task and step template definitions
  - Environment-specific overlays
  - Schema validation
  - Mirrors Rails engine configuration structure
  
- **`config.rs`** (255 lines):
  - Simple runtime configuration
  - Environment variable loading
  - Basic validation
  - Duplicates many settings already in orchestration config

**Decision**: 🔧 **Consolidate** - Merge `config.rs` functionality into `orchestration/config.rs`

### 4. Events and States: Multiple Overlapping Definitions

**Finding**: Event constants and state definitions are scattered across multiple files with some duplication.

| File | Purpose | Overlap Issue |
|------|---------|---------------|
| `constants.rs` | System constants and events | Contains comprehensive event constants + re-exports state types |
| `orchestration/system_events.rs` | Rails compatibility layer | Duplicates event constants in different namespace |
| `state_machine/events.rs` | State machine event enums | Typed events (TaskEvent, StepEvent) - properly scoped |
| `state_machine/states.rs` | State definitions | TaskState, WorkflowStepState enums - canonical source |

**Decision**: 
- ✅ `constants.rs` remains the source of truth for event string constants
- ✅ `orchestration/system_events.rs` provides Rails compatibility layer (keep as-is)
- ✅ `state_machine/` files are properly focused on state machine types
- 🔧 Remove state type re-exports from `constants.rs` (use direct imports from `state_machine::states`)

## Implementation Plan

### Phase 1: High Priority - Configuration Consolidation

1. **Merge `config.rs` into `orchestration/config.rs`**:
   - Move `TaskerConfig` struct from `config.rs` to orchestration config
   - Integrate environment variable loading with YAML configuration
   - Ensure backward compatibility for existing code
   - Update all imports throughout the codebase

2. **Update documentation**:
   - Document the unified configuration approach
   - Provide migration guide for any breaking changes

### Phase 2: Medium Priority - Cleanup

1. **Remove empty stubs**:
   - Delete `events/lifecycle_events.rs` (empty TODO file)
   - Update `events/mod.rs` to remove the module reference

2. **Clean up state type references**:
   - Remove re-exports from `constants.rs`
   - Update all code to import directly from `state_machine::states`

### Phase 3: Low Priority - Documentation

1. **Document the architecture**:
   - Create clear documentation explaining the separation of concerns
   - Add module-level documentation explaining when to use each registry type
   - Document the event system architecture

## Benefits of This Plan

1. **Reduced Complexity**: Single configuration system instead of two
2. **Better Maintainability**: Clear separation of concerns between modules
3. **Improved Developer Experience**: Less confusion about which module to use
4. **Type Safety**: Proper use of Rust's type system for events and states

## Non-Goals

This consolidation plan explicitly does **not**:
- Merge orchestration-specific components with general-purpose ones
- Remove the Rails compatibility layer (needed for FFI)
- Change the public API significantly

## Success Metrics

- All tests continue to pass after consolidation
- No performance regression
- Reduced lines of code through deduplication
- Clearer module boundaries