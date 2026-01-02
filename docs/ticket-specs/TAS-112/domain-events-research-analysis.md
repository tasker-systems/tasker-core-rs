# Domain Events Cross-Language Research Analysis

**TAS-112 | Cross-Language Step Handler Ergonomics Analysis & Harmonization**

**Date**: 2025-12-30
**Scope**: TypeScript, Python, Ruby FFI integration for domain events
**Related Tickets**: TAS-65 (Domain Events), TAS-67 (FFI Dispatch), TAS-96 (Cross-Language Standard API)

---

## Executive Summary

This research document analyzes domain event implementations across all worker languages (TypeScript, Python, Ruby) and their FFI integration with the Rust core. The analysis reveals:

1. **Ruby** has the most mature implementation with full lifecycle hooks and dual FFI files
2. **TypeScript** was just completed to full feature parity with Ruby (TAS-112 work)
3. **Python** has core functionality but is missing lifecycle hooks and structured registries
4. **Critical Type Safety Issue**: Manual JSON construction in `bridge.rs` creates type drift risk

---

## Table of Contents

1. [Cross-Language Implementation Matrix](#1-cross-language-implementation-matrix)
2. [Type Safety Analysis](#2-type-safety-analysis)
3. [FFI Bridge Architecture Comparison](#3-ffi-bridge-architecture-comparison)
4. [Identified Gaps by Language](#4-identified-gaps-by-language)
5. [Priority Recommendations](#5-priority-recommendations)
6. [Action Items](#6-action-items)

---

## 1. Cross-Language Implementation Matrix

### Feature Comparison

| Feature | Ruby | TypeScript | Python | Rust Core |
|---------|------|------------|--------|-----------|
| **BasePublisher** | :white_check_mark: | :white_check_mark: | :white_check_mark: | N/A (handlers) |
| **Publisher Lifecycle Hooks** | :white_check_mark: | :white_check_mark: | :x: | N/A |
| - `before_publish()` | :white_check_mark: | :white_check_mark: | :x: | N/A |
| - `after_publish()` | :white_check_mark: | :white_check_mark: | :x: | N/A |
| - `on_publish_error()` | :white_check_mark: | :white_check_mark: | :x: | N/A |
| **transform_payload()** | :white_check_mark: | :white_check_mark: | :white_check_mark: | N/A |
| **should_publish()** | :white_check_mark: | :white_check_mark: | :white_check_mark: | N/A |
| **additional_metadata()** | :white_check_mark: | :white_check_mark: | :x: | N/A |
| **BaseSubscriber** | :white_check_mark: | :white_check_mark: | :white_check_mark: | :white_check_mark: |
| **Subscriber Lifecycle Hooks** | :white_check_mark: | :white_check_mark: | :x: | :white_check_mark: |
| - `before_handle()` | :white_check_mark: | :white_check_mark: | :x: | :white_check_mark: |
| - `after_handle()` | :white_check_mark: | :white_check_mark: | :x: | :white_check_mark: |
| - `on_handle_error()` | :white_check_mark: | :white_check_mark: | :x: | :white_check_mark: |
| **Pattern Matching** | :white_check_mark: | :white_check_mark: | :white_check_mark: | :white_check_mark: |
| **PublisherRegistry** | :white_check_mark: | :white_check_mark: | :warning: Partial | :white_check_mark: |
| **SubscriberRegistry** | :white_check_mark: | :white_check_mark: | :x: | :white_check_mark: |
| **Registry Freezing** | :white_check_mark: | :white_check_mark: | :x: | N/A |
| **InProcessEventPoller** | :white_check_mark: | :white_check_mark: | :white_check_mark: | N/A |
| **FFI Integration** | :white_check_mark: | :white_check_mark: | :white_check_mark: | N/A |
| **Durable Events (PGMQ)** | :white_check_mark: | :white_check_mark: | :white_check_mark: | :white_check_mark: |
| **Fast Events (Broadcast)** | :white_check_mark: | :white_check_mark: | :white_check_mark: | :white_check_mark: |

### Type System Comparison

| Type | Ruby | TypeScript | Python | Notes |
|------|------|------------|--------|-------|
| `StepEventContext` | :white_check_mark: | :white_check_mark: | :white_check_mark: | All aligned |
| `EventDeclaration` | Implicit | :white_check_mark: | :x: | Python lacks |
| `StepResult` | Implicit | :white_check_mark: | :x: | Python lacks |
| `PublishContext` | :white_check_mark: | :white_check_mark: | :x: | Python lacks |
| `DomainEvent` | :white_check_mark: | :white_check_mark: | :warning: Missing `executionResult` | Type mismatch |
| `DomainEventMetadata` | :white_check_mark: | :white_check_mark: | :white_check_mark: | All aligned |

---

## 2. Type Safety Analysis

### 2.1 Critical Issue: Manual JSON Construction

**Location**: `workers/typescript/src-rust/bridge.rs:409-423`

```rust
// PROBLEMATIC: Manual JSON construction
let json_value = json!({
    "eventId": event.event_id.to_string(),
    "eventName": event.event_name,
    "eventVersion": event.event_version,
    "metadata": {
        "taskUuid": event.metadata.task_uuid.to_string(),
        "stepUuid": event.metadata.step_uuid.map(|id| id.to_string()),
        "stepName": event.metadata.step_name,
        "namespace": event.metadata.namespace,
        "correlationId": event.metadata.correlation_id.to_string(),
        "firedAt": event.metadata.fired_at.to_rfc3339(),
        "firedBy": event.metadata.fired_by
    },
    "payload": event.payload
});
```

**Risk**: If the Rust `DomainEvent` structure changes, this manual JSON construction will silently produce incorrect output. Unlike step events which use `FfiStepEventDto` with `ts-rs` generation, domain events lack compile-time type safety.

**Comparison with Step Events** (correct pattern):
- Step events use `FfiStepEventDto` struct in `dto.rs`
- Types generated via `ts-rs` to `src/ffi/generated/`
- Compile-time validation ensures Rust and TypeScript stay in sync

### 2.2 Type Drift Points

| Location | Issue | Impact | Severity |
|----------|-------|--------|----------|
| `bridge.rs:409-423` | Manual JSON for domain events | Type drift risk | **High** |
| `types.ts:153-175` | `FfiDomainEvent` not ts-rs generated | No auto-sync | **High** |
| `domain-events.ts:1473` | `stepUuid ?? ''` loses null semantics | Data loss | **Medium** |
| `domain-events.ts:1488-1492` | `executionResult` synthesized | Missing data | **Medium** |
| Python `InProcessDomainEvent` | Missing `executionResult` field | Incomplete type | **Medium** |

### 2.3 Serialization Pattern Comparison

| Language | Bridge File | Domain Event Serialization | Type Safety |
|----------|-------------|---------------------------|-------------|
| **TypeScript** | `bridge.rs` | Manual `json!()` macro | :x: None |
| **Python** | `bridge.rs` + `observability.rs` | Manual `PyDict` construction | :x: None |
| **Ruby** | `in_process_event_ffi.rs` | Manual `RHash` construction | :x: None |

**Recommendation**: All bridges should use a shared `FfiDomainEventDto` struct with serde serialization, similar to `FfiStepEventDto`.

---

## 3. FFI Bridge Architecture Comparison

### 3.1 Ruby Implementation (Most Complete)

**Files**:
- `workers/ruby/ext/tasker_core/src/bridge.rs` - Core lifecycle
- `workers/ruby/ext/tasker_core/src/event_publisher_ffi.rs` - Durable publishing
- `workers/ruby/ext/tasker_core/src/in_process_event_ffi.rs` - Fast polling

**Architecture**:
```
Ruby Handler → FFI.publish_domain_event() → event_publisher_ffi.rs
                                           ↓
                                    DomainEventPublisher → PGMQ

Ruby Subscriber ← InProcessDomainEventPoller ← FFI.poll_in_process_events()
                                               ↓
                                        in_process_event_ffi.rs ← broadcast::Receiver
```

**Strengths**:
- Dedicated FFI files for publishing and polling
- Full `EventPublishRequest` struct with `serde_magnus` deserialization
- Thread-safe `WORKER_SYSTEM` mutex pattern
- Comprehensive error handling with Ruby exception types

### 3.2 TypeScript Implementation (Recently Completed)

**Files**:
- `workers/typescript/src-rust/bridge.rs` - Combined lifecycle, polling, and events

**Architecture**:
```
TypeScript Handler → Rust orchestration publishes (not direct FFI)

TypeScript Subscriber ← InProcessDomainEventPoller ← runtime.pollInProcessEvents()
                                                     ↓
                                              bridge.rs:poll_in_process_events()
                                                     ↓
                                              broadcast::Receiver
```

**Strengths**:
- Single bridge file (simpler but less modular)
- Runtime abstraction (`TaskerRuntime`) for multi-runtime support (Bun, Node, Deno)
- `ffiEventToDomainEvent()` transformation function

**Weaknesses**:
- No direct publishing FFI (relies on Rust orchestration)
- Manual JSON construction for domain events

### 3.3 Python Implementation

**Files**:
- `workers/python/src/bridge.rs` - Core lifecycle
- `workers/python/src/observability.rs` - Domain event polling

**Architecture**:
```
Python Handler → Rust orchestration publishes (not direct FFI)

Python Subscriber ← InProcessDomainEventPoller ← _poll_in_process_events()
                                                 ↓
                                          observability.rs:poll_in_process_events()
                                                 ↓
                                          broadcast::Receiver
```

**Strengths**:
- `pythonize` crate for safe serde-based serialization
- Threaded polling model (daemon thread)

**Weaknesses**:
- No direct publishing FFI
- Missing lifecycle hooks
- No structured registries

---

## 4. Identified Gaps by Language

### 4.1 TypeScript Gaps

| Gap | Description | Location | Priority |
|-----|-------------|----------|----------|
| **Manual JSON Serialization** | Domain events use `json!()` macro | `bridge.rs:409-423` | **High** |
| **FfiDomainEvent Not Generated** | Should use ts-rs like step events | `types.ts:153-175` | **High** |
| **executionResult Synthesized** | Original step result not preserved | `domain-events.ts:1488-1492` | **Medium** |
| **stepUuid Loses Null** | `?? ''` converts null to empty string | `domain-events.ts:1473` | **Low** |

### 4.2 Python Gaps

| Gap | Description | Location | Priority |
|-----|-------------|----------|----------|
| **Missing Lifecycle Hooks** | No before/after/error hooks | `domain_events.py` | **High** |
| **Missing EventDeclaration** | No type for event config | `domain_events.py` | **High** |
| **Missing StepResult** | No structured result type | `domain_events.py` | **High** |
| **Missing PublishContext** | No unified publish context | `domain_events.py` | **Medium** |
| **Missing SubscriberRegistry** | No centralized lifecycle management | `domain_events.py` | **Medium** |
| **executionResult Missing** | `InProcessDomainEvent` incomplete | `types.py:1012-1032` | **Medium** |
| **additional_metadata() Missing** | Can't add custom metadata | `domain_events.py` | **Medium** |

### 4.3 Ruby Gaps (Fewest)

| Gap | Description | Location | Priority |
|-----|-------------|----------|----------|
| **JSON String Payloads** | Complex payloads serialized as JSON strings | `in_process_event_ffi.rs:81` | **Low** |

---

## 5. Priority Recommendations

### 5.1 High Priority (Type Safety)

#### A. Create Shared FfiDomainEventDto

**What**: Define a Rust DTO struct for domain events similar to `FfiStepEventDto`

**Where**: `workers/typescript/src-rust/dto.rs` (and similar for Python/Ruby)

```rust
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../src/ffi/generated/"))]
pub struct FfiDomainEventDto {
    pub event_id: String,
    pub event_name: String,
    pub event_version: String,
    pub metadata: FfiDomainEventMetadataDto,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../src/ffi/generated/"))]
pub struct FfiDomainEventMetadataDto {
    pub task_uuid: String,
    pub step_uuid: Option<String>,
    pub step_name: Option<String>,
    pub namespace: String,
    pub correlation_id: String,
    pub fired_at: String,
    pub fired_by: Option<String>,
}
```

**Why**: Compile-time type safety, automatic TypeScript type generation, single source of truth

#### B. Update TypeScript Bridge to Use DTO

**What**: Replace manual JSON construction with DTO serialization

**Where**: `workers/typescript/src-rust/bridge.rs:409-423`

```rust
// Replace manual json!() with:
let dto = FfiDomainEventDto::from(&event);
let json_str = serde_json::to_string(&dto)?;
```

#### C. Add Python Lifecycle Hooks

**What**: Add `before_publish`, `after_publish`, `on_publish_error` to BasePublisher

**Where**: `workers/python/python/tasker_core/domain_events.py`

```python
class BasePublisher(ABC):
    # Existing methods...

    def before_publish(self, event_name: str, payload: dict, metadata: dict) -> bool:
        """Called before publishing. Return False to abort."""
        return True

    def after_publish(self, event_name: str, payload: dict, metadata: dict) -> None:
        """Called after successful publishing."""
        pass

    def on_publish_error(self, event_name: str, error: Exception, payload: dict) -> None:
        """Called when publishing fails."""
        pass
```

### 5.2 Medium Priority (Feature Parity)

#### D. Add Python Subscriber Lifecycle Hooks

**Where**: `workers/python/python/tasker_core/domain_events.py`

```python
class BaseSubscriber(ABC):
    # Existing methods...

    def before_handle(self, event: InProcessDomainEvent) -> bool:
        """Return False to skip handling."""
        return True

    def after_handle(self, event: InProcessDomainEvent) -> None:
        """Called after successful handling."""
        pass

    def on_handle_error(self, event: InProcessDomainEvent, error: Exception) -> None:
        """Called when handling fails."""
        pass
```

#### E. Add Python Type Definitions

**Where**: `workers/python/python/tasker_core/domain_events.py`

```python
@dataclass
class EventDeclaration:
    name: str
    condition: str | None = None  # 'success', 'failure', 'always'
    delivery_mode: str | None = None  # 'durable', 'fast', 'broadcast'
    publisher: str | None = None
    schema: dict[str, Any] | None = None

@dataclass
class StepResult:
    success: bool
    result: dict[str, Any] | None = None
    metadata: dict[str, Any] | None = None

@dataclass
class PublishContext:
    event_name: str
    step_result: StepResult
    event_declaration: EventDeclaration | None = None
    step_context: StepEventContext | None = None
```

#### F. Create Python SubscriberRegistry

**Where**: `workers/python/python/tasker_core/domain_events.py`

```python
class SubscriberRegistry:
    _instance: SubscriberRegistry | None = None
    _lock = threading.Lock()

    @classmethod
    def instance(cls) -> SubscriberRegistry:
        with cls._lock:
            if cls._instance is None:
                cls._instance = cls()
            return cls._instance

    def register(self, subscriber_class: type[BaseSubscriber]) -> None: ...
    def register_instance(self, subscriber: BaseSubscriber) -> None: ...
    def start_all(self, poller: InProcessDomainEventPoller) -> None: ...
    def stop_all(self) -> None: ...
    def stats(self) -> dict[str, Any]: ...
```

### 5.3 Low Priority (Cleanup)

#### G. Update Python InProcessDomainEvent Type

**Where**: `workers/python/python/tasker_core/types.py`

```python
class InProcessDomainEvent(BaseModel):
    event_id: UUID
    event_name: str
    event_version: str
    metadata: DomainEventMetadata
    payload: dict[str, Any]
    execution_result: StepResult  # ADD THIS
```

#### H. Document Null Handling Semantics

**Where**: Architecture documentation

Document the behavior of nullable fields:
- `stepUuid`: null for task-level events, populated for step-level events
- `stepName`: null for task-level events
- `firedBy`: null when publisher not specified

---

## 6. Action Items

### Immediate (Before TAS-112 Validation Gate 1)

- [ ] **HIGH**: Create `FfiDomainEventDto` in TypeScript bridge
- [ ] **HIGH**: Update `bridge.rs` to use DTO serialization
- [ ] **HIGH**: Add Python lifecycle hooks to `BasePublisher`
- [ ] **HIGH**: Add Python lifecycle hooks to `BaseSubscriber`
- [ ] **HIGH**: Add `additional_metadata()` to Python `BasePublisher`

### Near-Term (TAS-112 Phase 3)

- [ ] **MEDIUM**: Add Python type definitions (`EventDeclaration`, `StepResult`, `PublishContext`)
- [ ] **MEDIUM**: Create Python `SubscriberRegistry`
- [ ] **MEDIUM**: Update Python `InProcessDomainEvent` to include `executionResult`

### Future (Post TAS-112)

- [ ] **LOW**: Consider shared FFI domain event DTO across all languages
- [ ] **LOW**: Document nullable field semantics in architecture docs
- [ ] **LOW**: Add JSON schema validation support to Python

---

## Appendix A: File References

### TypeScript Worker
- `workers/typescript/src-rust/bridge.rs` - FFI bridge (409-423 manual JSON)
- `workers/typescript/src-rust/dto.rs` - DTO definitions
- `workers/typescript/src/ffi/types.ts` - FFI type definitions (153-175)
- `workers/typescript/src/handler/domain-events.ts` - Domain event infrastructure (1-1521)
- `workers/typescript/src/handler/index.ts` - Public exports

### Python Worker
- `workers/python/src/bridge.rs` - FFI bridge
- `workers/python/src/observability.rs` - Domain event polling (31-107)
- `workers/python/python/tasker_core/domain_events.py` - Domain event infrastructure
- `workers/python/python/tasker_core/types.py` - Type definitions (978-1032)

### Ruby Worker
- `workers/ruby/ext/tasker_core/src/bridge.rs` - FFI bridge (35-309)
- `workers/ruby/ext/tasker_core/src/event_publisher_ffi.rs` - Durable publishing (1-240)
- `workers/ruby/ext/tasker_core/src/in_process_event_ffi.rs` - Fast polling (1-320)
- `workers/ruby/lib/tasker_core/domain_events/base_publisher.rb` - Publisher base class
- `workers/ruby/lib/tasker_core/domain_events/base_subscriber.rb` - Subscriber base class
- `workers/ruby/lib/tasker_core/domain_events/publisher_registry.rb` - Publisher registry
- `workers/ruby/lib/tasker_core/domain_events/subscriber_registry.rb` - Subscriber registry
- `workers/ruby/lib/tasker_core/worker/in_process_domain_event_poller.rb` - Event poller

### Rust Core
- `tasker-shared/src/events/domain_events.rs` - Domain event types (1-348)
- `tasker-shared/src/events/registry.rs` - Event registry (1-300)
- `tasker-worker/src/worker/in_process_event_bus.rs` - In-process bus (1-380)
- `workers/rust/src/event_subscribers/` - Example subscribers

### Documentation
- `docs/architecture/domain-events.md` - Architecture overview
- `docs/ticket-specs/TAS-112/implementation-plan.md` - Implementation plan
- `docs/ticket-specs/TAS-112/domain-events.md` - Domain events specification

---

## Appendix B: Data Flow Diagrams

### Durable Event Publishing (PGMQ)

```
Step Handler Completes
       ↓
Rust Orchestration (StepCompletionProcessor)
       ↓
DomainEventPublisher.publish_event()
       ↓
┌─────────────────────────────────────┐
│  Custom Publisher (if specified)    │
│  - transform_payload()              │
│  - should_publish()                 │
│  - additional_metadata()            │
│  - before_publish() → after_publish │
└─────────────────────────────────────┘
       ↓
EventRouter (delivery_mode check)
       ↓
PGMQ: {namespace}_domain_events queue
       ↓
External Consumers (poll/notify)
```

### Fast Event Publishing (In-Process)

```
Step Handler Completes
       ↓
Rust Orchestration (StepCompletionProcessor)
       ↓
InProcessEventBus.publish()
       ↓
┌────────────────────────────────┐
│  broadcast::Sender             │
│  (tokio broadcast channel)     │
└────────────────────────────────┘
       ↓                    ↓
┌──────────────┐    ┌──────────────┐
│ Rust Handlers│    │ FFI Receivers│
│ EventRegistry│    │ Ruby/Python/ │
│              │    │ TypeScript   │
└──────────────┘    └──────────────┘
       ↓                    ↓
Pattern Match        poll_in_process_events()
       ↓                    ↓
EventHandler()       InProcessDomainEventPoller
                           ↓
                    BaseSubscriber.handle()
```

---

*Document generated as part of TAS-112 research. For questions, see implementation-plan.md.*
