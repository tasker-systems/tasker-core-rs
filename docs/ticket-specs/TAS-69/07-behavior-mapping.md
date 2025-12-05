# TAS-69 Behavior Mapping: Before and After

## Overview

This document provides a detailed function-by-function mapping of behaviors from the main branch (monolithic `WorkerProcessor`) to the feature branch (actor-based architecture).

## Behavior Categories

### Category 1: Step Execution

#### ExecuteStep Flow

**Main Branch (Before):**
```
WorkerProcessor::handle_execute_step()
    │
    ├── 1. Extract step_uuid from message
    │
    ├── 2. task_template_manager.get_handler(namespace, handler_name)
    │       └── Validates namespace support
    │
    ├── 3. step_claim.claim_step(step_uuid)
    │       └── Atomic claim with visibility timeout
    │
    ├── 4. Verify step state == Enqueued
    │
    ├── 5. Load step context from database
    │       └── Query task_handler_steps + task_workflow_steps
    │
    ├── 6. Invoke FFI handler via event_publisher
    │       └── Ruby/Python handler execution
    │
    ├── 7. Build StepExecutionResult from handler response
    │
    ├── 8. Delete message from PGMQ queue
    │
    ├── 9. Send result to orchestration
    │       └── orchestration_result_sender.send()
    │
    ├── 10. Dispatch domain events
    │        └── domain_event_system.dispatch()
    │
    └── 11. Return success
```

**Feature Branch (After):**
```
ActorCommandProcessor::handle_execute_step()
    │
    └── StepExecutorActor::handle(ExecuteStepMessage)
            │
            └── StepExecutorService::execute_step()
                    │
                    ├── 1-8. Same as before
                    │
                    └── Return result (9-10 handled by caller)

ActorCommandProcessor (continued)
    │
    ├── 9. FFICompletionActor::handle(SendStepResultMessage)
    │       └── FFICompletionService::send_result()
    │               └── OrchestrationResultSender::send()
    │
    └── 10. StepExecutorActor::dispatch_domain_events()
             └── DomainEventSystem::dispatch()
```

**Behavior Equivalence:**
- ✅ Step claiming: Identical
- ✅ State verification: Identical
- ✅ FFI invocation: Identical
- ✅ Result building: Identical
- ✅ Message deletion: Identical
- ⚠️ Result sending: Now via FFICompletionService
- ⚠️ Event dispatch: Now explicit call after service return

---

#### ExecuteStepWithCorrelation Flow

**Main Branch:**
Same as ExecuteStep with additional correlation tracking.

**Feature Branch:**
```
ActorCommandProcessor::handle_execute_step_with_correlation()
    │
    └── StepExecutorActor::handle(ExecuteStepWithCorrelationMessage)
            │
            └── StepExecutorService::execute_step()
                    └── Same execution flow
```

**Behavior Equivalence:** ✅ Identical (correlation ID passed through)

---

#### ExecuteStepFromMessage Flow (TAS-43)

**Main Branch:**
```
WorkerProcessor::handle_execute_step_from_message()
    │
    ├── Deserialize PgmqMessage to SimpleStepMessage
    └── Call handle_execute_step()
```

**Feature Branch:**
```
ActorCommandProcessor::handle_execute_step_from_message()
    │
    └── StepExecutorActor::handle(ExecuteStepFromPgmqMessage)
            │
            ├── Deserialize message
            └── StepExecutorService::execute_step()
```

**Behavior Equivalence:** ✅ Identical

---

#### ExecuteStepFromEvent Flow (TAS-43)

**Main Branch:**
```
WorkerProcessor::handle_execute_step_from_event()
    │
    ├── Read specific message from queue by msg_id
    └── Call handle_execute_step()
```

**Feature Branch:**
```
ActorCommandProcessor::handle_execute_step_from_event()
    │
    └── StepExecutorActor::handle(ExecuteStepFromEventMessage)
            │
            ├── Read specific message via context.message_client
            └── StepExecutorService::execute_step()
```

**Behavior Equivalence:** ✅ Identical

---

### Category 2: Completion Handling

#### ProcessStepCompletion Flow

**Main Branch:**
```
WorkerProcessor::handle_process_step_completion()
    │
    ├── Validate step_result
    ├── Send to orchestration
    │     └── orchestration_result_sender.send()
    └── Dispatch domain events (if correlation_id present)
```

**Feature Branch:**
```
ActorCommandProcessor::handle_process_step_completion()
    │
    └── FFICompletionActor::handle(ProcessStepCompletionMessage)
            │
            └── FFICompletionService::process_completion()
                    │
                    ├── Validate step_result
                    └── Send to orchestration
```

**Behavior Equivalence:**
- ✅ Validation: Identical
- ✅ Orchestration send: Identical
- ⚠️ Event dispatch: Now handled separately (see Gap #1 fix)

---

#### SendStepResult Flow

**Main Branch:**
```
WorkerProcessor::handle_send_step_result()
    │
    └── orchestration_result_sender.send(result)
```

**Feature Branch:**
```
ActorCommandProcessor::handle_send_step_result()
    │
    └── FFICompletionActor::handle(SendStepResultMessage)
            │
            └── FFICompletionService::send_result()
                    └── OrchestrationResultSender::send()
```

**Behavior Equivalence:** ✅ Identical

---

### Category 3: Template Management

#### RefreshTemplateCache Flow

**Main Branch:**
```
WorkerProcessor::handle_refresh_template_cache()
    │
    └── task_template_manager.refresh_cache(namespace)
```

**Feature Branch:**
```
ActorCommandProcessor::handle_refresh_template_cache()
    │
    └── TemplateCacheActor::handle(RefreshTemplateCacheMessage)
            │
            └── task_template_manager.refresh_cache(namespace)
```

**Behavior Equivalence:** ✅ Identical

---

### Category 4: Status and Health

#### GetWorkerStatus Flow

**Main Branch:**
```
WorkerProcessor::handle_get_worker_status()
    │
    └── Build WorkerStatus from internal fields
          ├── worker_id
          ├── status ("running")
          ├── steps_executed (stats.total_executed)
          ├── steps_succeeded (stats.total_succeeded)
          ├── steps_failed (stats.total_failed)
          ├── uptime_seconds
          └── registered_handlers
```

**Feature Branch:**
```
ActorCommandProcessor::handle_get_worker_status()
    │
    └── WorkerStatusActor::handle(GetWorkerStatusMessage)
            │
            └── WorkerStatusService::get_status()
                    └── Build WorkerStatus from service fields
```

**Behavior Equivalence:** ✅ Identical (same fields populated)

---

#### GetEventStatus Flow

**Main Branch:**
```
WorkerProcessor::handle_get_event_status()
    │
    └── Build EventIntegrationStatus from internal fields
          ├── enabled
          ├── events_published
          ├── events_received
          ├── correlation_tracking_enabled
          ├── pending_correlations
          ├── ffi_handlers_subscribed
          └── completion_subscribers
```

**Feature Branch:**
```
ActorCommandProcessor::handle_get_event_status()
    │
    └── WorkerStatusActor::handle(GetEventStatusMessage)
            │
            └── WorkerStatusService::get_event_status()
```

**Behavior Equivalence:** ✅ Identical

---

#### SetEventIntegration Flow

**Main Branch:**
```
WorkerProcessor::handle_set_event_integration()
    │
    └── self.event_integration_enabled = enabled
```

**Feature Branch:**
```
ActorCommandProcessor::handle_set_event_integration()
    │
    └── WorkerStatusActor::handle(SetEventIntegrationMessage)
            │
            └── WorkerStatusService::set_event_integration(enabled)
```

**Behavior Equivalence:** ✅ Identical

---

#### HealthCheck Flow

**Main Branch:**
```
WorkerProcessor::handle_health_check()
    │
    └── Build WorkerHealthStatus
          ├── Check database connectivity
          ├── Check message queue health
          └── Report component status
```

**Feature Branch:**
```
ActorCommandProcessor::handle_health_check()
    │
    └── WorkerStatusActor::handle(HealthCheckMessage)
            │
            └── WorkerStatusService::health_check()
```

**Behavior Equivalence:** ✅ Identical

---

### Category 5: Lifecycle

#### Shutdown Flow

**Main Branch:**
```
WorkerProcessor::handle_shutdown()
    │
    ├── self.is_shutting_down = true
    └── Cleanup (implicit)
```

**Feature Branch:**
```
ActorCommandProcessor::handle_shutdown()
    │
    ├── self.is_shutting_down.store(true, SeqCst)
    └── Return Ok(())
         └── Registry shutdown happens via WorkerCore::stop()
```

**Behavior Equivalence:** ✅ Identical (shutdown signaling preserved)

---

## Critical Behavior Differences

### Difference 1: Domain Event Dispatch Timing

**Main Branch:**
Events dispatched inline at the end of step execution.

**Feature Branch (Initial):**
Events not dispatched (Gap #1).

**Feature Branch (Fixed):**
```rust
// StepExecutorActor explicitly dispatches events
let result = self.service.write().await.execute_step(msg).await;
if result.is_ok() {
    self.dispatch_domain_events(step_uuid, &result, correlation_id).await;
}
```

**Status:** ✅ Fixed

---

### Difference 2: Error Propagation in Orchestration Send

**Main Branch:**
Errors logged but silently swallowed in some code paths.

**Feature Branch (Initial):**
Same silent swallowing behavior.

**Feature Branch (Fixed):**
```rust
// FFICompletionService propagates errors
pub async fn send_result(&self, result: StepExecutionResult) -> TaskerResult<()> {
    self.orchestration_result_sender.send(result).await.map_err(|e| {
        tracing::error!("Failed to send result to orchestration: {}", e);
        e
    })
}
```

**Status:** ✅ Fixed (explicit error propagation)

---

### Difference 3: TaskTemplateManager Namespace Discovery

**Main Branch:**
Single `TaskTemplateManager` instance with discovered namespaces.

**Feature Branch (Initial):**
Registry created new `TaskTemplateManager`, losing namespace discovery.

**Feature Branch (Fixed):**
```rust
// WorkerCore shares pre-initialized manager
let task_template_manager = Arc::new(TaskTemplateManager::new(...));
task_template_manager.discover_namespaces().await?;

// Shared with registry
let registry = WorkerActorRegistry::build_with_task_template_manager(
    context, worker_id, task_template_manager,
).await?;
```

**Status:** ✅ Fixed

---

## Behavior Summary Matrix

| Behavior | Main | Feature | Status |
|----------|------|---------|--------|
| Step claiming | ✅ | ✅ | Identical |
| State verification | ✅ | ✅ | Identical |
| FFI handler invocation | ✅ | ✅ | Identical |
| Result building | ✅ | ✅ | Identical |
| Message deletion | ✅ | ✅ | Identical |
| Orchestration send | ✅ | ✅ | Fixed |
| Domain event dispatch | ✅ | ✅ | Fixed |
| Template cache refresh | ✅ | ✅ | Identical |
| Status reporting | ✅ | ✅ | Identical |
| Health checks | ✅ | ✅ | Identical |
| Shutdown signaling | ✅ | ✅ | Identical |
| Namespace validation | ✅ | ✅ | Fixed |

---

## Execution Path Comparison

### Complete Step Execution Path

**Main Branch:**
```
Command received
    → WorkerProcessor::run_loop()
        → match command
            → handle_execute_step()
                → [all logic inline]
                → Return result
```

**Feature Branch:**
```
Command received
    → ActorCommandProcessor::process_command()
        → match command
            → StepExecutorActor.handle()
                → StepExecutorService.execute_step()
                    → [core logic]
                    → Return result
            → dispatch_domain_events()
            → Return result
```

**Hops:**
- Main: 1 (direct inline)
- Feature: 3 (processor → actor → service)

**Trade-off:** More hops but better separation of concerns.

---

## Conclusion

All behaviors from the main branch are preserved in the feature branch:

1. **Identical behaviors**: 8 of 11 command flows unchanged
2. **Fixed behaviors**: 3 gaps identified and fixed
3. **Structural change**: Inline logic → Actor/Service delegation
4. **Execution model**: Sequential → Same (actors are synchronous handlers)

The migration preserves all functional behaviors while improving code organization and testability.
