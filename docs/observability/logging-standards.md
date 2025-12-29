# Tasker-Core Logging Standards

**Version**: 1.0
**Last Updated**: 2025-10-07
**Status**: Active
**Related**: TAS-29 Phase 1.5 - Observability Standardization

---

## Table of Contents

1. [Philosophy](#philosophy)
2. [Log Levels](#log-levels)
3. [Structured Fields](#structured-fields)
4. [Message Style](#message-style)
5. [Instrument Macro](#instrument-macro)
6. [Error Handling](#error-handling)
7. [Examples](#examples)
8. [Enforcement](#enforcement)

---

## Philosophy

**Principles**:
- **Production-First**: Logs must be parseable, searchable, and professional
- **Correlation-Driven**: All operations include correlation_id for distributed tracing (TAS-29)
- **Structured**: Fields over string interpolation for aggregation and querying
- **Concise**: Clear, actionable messages without noise
- **Consistent**: Predictable patterns across all code

**Anti-Patterns to Avoid**:
- ❌ Emojis (🚀✅❌) - Breaks log parsers, unprofessional
- ❌ All-caps prefixes ("BOOTSTRAP:", "CORE:") - Redundant with module paths
- ❌ Ticket references ("TAS-40", "JIRA-123") - Internal, meaningless externally
- ❌ String interpolation - Use structured fields instead
- ❌ Verbose messages - Be concise, let fields provide detail

---

## Log Levels

### ERROR - Unrecoverable Failures

**When to Use**:
- Database connection permanently lost
- Critical system component failure
- Unrecoverable state machine violation
- Data corruption detected
- Message queue unavailable

**Characteristics**:
- Requires immediate human intervention
- Service degradation or outage
- Cannot automatically recover
- Should trigger alerts/pages

**Example**:
```rust
error!(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    error = %e,
    "Failed to claim task for finalization: database unavailable"
);
```

### WARN - Degraded Operation

**When to Use**:
- Retryable failures after exhausting retries
- Circuit breaker opened (degraded mode)
- Fallback behavior activated
- Rate limiting engaged
- Configuration issues (non-fatal)
- Unexpected but handled conditions

**Characteristics**:
- Service continues but degraded
- Automatic recovery possible
- Should be monitored for patterns
- May indicate upstream problems

**Example**:
```rust
warn!(
    correlation_id = %correlation_id,
    step_uuid = %step_uuid,
    retry_count = attempts,
    max_retries = max_attempts,
    next_retry_at = ?next_retry,
    "Step execution failed after max retries, will not retry further"
);
```

### INFO - Lifecycle Events

**When to Use**:
- System startup/shutdown
- Task created/completed/failed
- Step enqueued/completed
- State transitions (task/step)
- Configuration loaded
- Significant business events

**Characteristics**:
- Normal operation milestones
- Useful for understanding flow
- Production-ready verbosity
- Default log level in production

**Example**:
```rust
info!(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    steps_enqueued = count,
    duration_ms = elapsed.as_millis(),
    "Task initialization complete"
);
```

### DEBUG - Detailed Diagnostics

**When to Use**:
- Discovery query results
- Queue depth checks
- Dependency analysis details
- Configuration value dumps
- State machine transition details
- Detailed operation flow

**Characteristics**:
- Troubleshooting information
- Not shown in production (usually)
- Safe to be verbose
- Helps understand "why"

**Example**:
```rust
debug!(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    viable_steps = steps.len(),
    pending_steps = pending.len(),
    blocked_steps = blocked.len(),
    "Step readiness analysis complete"
);
```

### TRACE - Very Verbose

**When to Use**:
- Function entry/exit in hot paths
- Loop iteration details
- Deep parameter inspection
- Performance profiling hooks

**Characteristics**:
- Extremely verbose
- Usually disabled even in dev
- Performance impact acceptable
- Use sparingly

**Example**:
```rust
trace!(
    correlation_id = %correlation_id,
    iteration = i,
    "Polling loop iteration"
);
```

---

## Structured Fields

### Required Fields (Context-Dependent)

**Always Include**:
```rust
correlation_id = %correlation_id,  // TAS-29: ALWAYS when available
```

**When Task Context Available**:
```rust
correlation_id = %correlation_id,
task_uuid = %task_uuid,
namespace = %namespace,
```

**When Step Context Available**:
```rust
correlation_id = %correlation_id,
task_uuid = %task_uuid,
step_uuid = %step_uuid,
namespace = %namespace,
```

**For Operations**:
```rust
correlation_id = %correlation_id,
// ... entity IDs ...
operation = "step_enqueue",        // Operation identifier
duration_ms = elapsed.as_millis(), // Timing for operations
```

**For Errors**:
```rust
correlation_id = %correlation_id,
// ... entity IDs ...
error = %e,                        // Error Display
error_type = %type_name::<E>(),   // Optional: Error type
```

### Field Ordering (MANDATORY)

**Standard Order**:
1. **correlation_id** (always first - TAS-29)
2. **Entity IDs** (task_uuid, step_uuid, namespace)
3. **Operation/Action** (operation, state, status)
4. **Measurements** (duration_ms, count, size)
5. **Error Info** (error, error_type, context)
6. **Other Context** (additional fields)

**Example**:
```rust
info!(
    // 1. Correlation ID (ALWAYS FIRST)
    correlation_id = %correlation_id,

    // 2. Entity IDs
    task_uuid = %task_uuid,
    step_uuid = %step_uuid,
    namespace = %namespace,

    // 3. Operation
    operation = "step_transition",
    from_state = %old_state,
    to_state = %new_state,

    // 4. Measurements
    duration_ms = elapsed.as_millis(),

    // 5. No errors (success case)

    // 6. Other context
    processor_id = %processor_uuid,

    "Step state transition complete"
);
```

### Field Formatting

**Use Display Formatting (`%`)**:
```rust
// ✅ CORRECT: Let tracing handle formatting
correlation_id = %correlation_id,
task_uuid = %task_uuid,
error = %e,
```

**Avoid Manual Conversion**:
```rust
// ❌ WRONG: Manual to_string()
task_uuid = task_uuid.to_string(),

// ❌ WRONG: Debug formatting for production types
task_uuid = ?task_uuid,  // Use ? only for Debug types
```

**Field Naming**:
```rust
// ✅ Standard names
duration_ms          // Not elapsed_ms, time_ms
error                // Not err, error_message
step_uuid            // Not workflow_step_uuid (be consistent)
retry_count          // Not attempts, retries
max_retries          // Not max_attempts
```

---

## Message Style

### Guidelines

**DO**:
- ✅ Be concise and actionable
- ✅ Use present tense for states: "Step enqueued"
- ✅ Use past tense for events: "Task completed"
- ✅ Start with the subject: "Task completed" not "Successfully completed task"
- ✅ Focus on WHAT happened (fields show HOW)

**DON'T**:
- ❌ Use emojis: "🚀 Starting..." → "Starting orchestration system"
- ❌ Use all-caps prefixes: "BOOTSTRAP: Starting..." → "Starting orchestration bootstrap"
- ❌ Include ticket numbers: "TAS-40: Processing..." → "Processing command"
- ❌ Be redundant: "Successfully enqueued step successfully" → "Step enqueued"
- ❌ Include technical jargon: "Atomic CAS transition succeeded" → "State transition complete"
- ❌ Be verbose: Keep messages under 10 words ideally

### Before/After Examples

**Lifecycle Events**:
```rust
// ❌ BEFORE
info!("🚀 BOOTSTRAP: Starting unified orchestration system bootstrap");

// ✅ AFTER
info!("Starting orchestration system bootstrap");
```

**Operation Completion**:
```rust
// ❌ BEFORE
info!("✅ STEP_ENQUEUER: Successfully marked step {} as enqueued (TAS-32)", step_uuid);

// ✅ AFTER
info!(
    correlation_id = %correlation_id,
    step_uuid = %step_uuid,
    "Step marked as enqueued"
);
```

**Error Handling**:
```rust
// ❌ BEFORE
error!("❌ ORCHESTRATION_LOOP: Failed to process task {}: {}", task_uuid, e);

// ✅ AFTER
error!(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    error = %e,
    "Task processing failed"
);
```

**Shutdown**:
```rust
// ❌ BEFORE
info!("🛑 Shutdown signal received, initiating graceful shutdown...");

// ✅ AFTER
info!("Shutdown signal received, initiating graceful shutdown");
```

---

## Instrument Macro

### When to Use

Use `#[instrument]` for:
- Function-level spans in hot paths
- Automatic correlation ID tracking
- Operations that should appear in traces
- Functions with significant duration

**Benefits**:
- Automatic span creation
- Automatic timing
- Better OpenTelemetry integration (Phase 2)
- Cleaner code

### Example

```rust
use tracing::instrument;

#[instrument(skip(self), fields(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    namespace = %namespace
))]
pub async fn process_task(
    &self,
    correlation_id: Uuid,
    task_uuid: Uuid,
    namespace: String,
) -> Result<TaskResult> {
    // Span automatically created with fields above
    info!("Starting task processing");

    // ... implementation ...

    info!(
        duration_ms = start.elapsed().as_millis(),
        "Task processing complete"
    );

    Ok(result)
}
```

### Skip Parameters

**Always skip**:
- `self` (redundant)
- Large structures (use specific fields instead)
- Sensitive data (passwords, tokens, PII)

```rust
#[instrument(
    skip(self, context),  // Skip large context
    fields(
        correlation_id = %correlation_id,
        task_uuid = %context.task_uuid,  // Extract specific fields
    )
)]
```

---

## Error Handling

### Error Context

**Always include**:
```rust
error!(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    error = %e,              // Error Display (user-friendly)
    error_type = %type_name::<E>(),  // Optional: For classification
    "Operation failed"
);
```

### Error Propagation

```rust
// ✅ Log and return for caller to handle
debug!(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    error = %e,
    "Step discovery query failed, will retry"
);
return Err(e);

// ❌ Don't log at every level (causes noise)
// Instead: Log once at appropriate level where handled
```

### Error Classification

```rust
match result {
    Err(e) if e.is_retryable() => {
        warn!(
            correlation_id = %correlation_id,
            error = %e,
            retry_count = attempts,
            "Operation failed, will retry"
        );
    }
    Err(e) => {
        error!(
            correlation_id = %correlation_id,
            error = %e,
            "Operation failed permanently"
        );
    }
    Ok(result) => {
        info!(
            correlation_id = %correlation_id,
            duration_ms = elapsed.as_millis(),
            "Operation completed successfully"
        );
    }
}
```

---

## Examples

### Complete Examples by Scenario

#### Task Initialization

```rust
#[instrument(skip(self), fields(
    correlation_id = %task_request.correlation_id,
    task_name = %task_request.name,
    namespace = %task_request.namespace
))]
pub async fn create_task_from_request(
    &self,
    task_request: TaskRequest,
) -> Result<TaskInitializationResult> {
    let correlation_id = task_request.correlation_id;
    let start = Instant::now();

    info!("Starting task initialization");

    // Create task
    let task = self.create_task(&task_request).await?;

    debug!(
        task_uuid = %task.task_uuid,
        template_uuid = %task.named_task_uuid,
        "Task created in database"
    );

    // Discover steps
    let steps = self.discover_initial_steps(task.task_uuid).await?;

    info!(
        correlation_id = %correlation_id,
        task_uuid = %task.task_uuid,
        step_count = steps.len(),
        duration_ms = start.elapsed().as_millis(),
        "Task initialization complete"
    );

    Ok(TaskInitializationResult {
        task_uuid: task.task_uuid,
        step_count: steps.len(),
    })
}
```

#### Step Enqueueing

```rust
pub async fn enqueue_step(
    &self,
    correlation_id: Uuid,
    task_uuid: Uuid,
    step: &ViableStep,
) -> Result<()> {
    let start = Instant::now();

    debug!(
        correlation_id = %correlation_id,
        task_uuid = %task_uuid,
        step_uuid = %step.step_uuid,
        step_name = %step.name,
        queue = %step.queue_name,
        "Enqueueing step"
    );

    let message = self.create_message(correlation_id, task_uuid, step)?;

    self.pgmq_client
        .send(&step.queue_name, &message)
        .await?;

    info!(
        correlation_id = %correlation_id,
        task_uuid = %task_uuid,
        step_uuid = %step.step_uuid,
        queue = %step.queue_name,
        duration_ms = start.elapsed().as_millis(),
        "Step enqueued"
    );

    Ok(())
}
```

#### Error Handling

```rust
match self.process_step_result(result).await {
    Ok(()) => {
        info!(
            correlation_id = %result.correlation_id,
            task_uuid = %result.task_uuid,
            step_uuid = %result.step_uuid,
            duration_ms = elapsed.as_millis(),
            "Step result processed"
        );
    }
    Err(e) if e.is_retryable() => {
        warn!(
            correlation_id = %result.correlation_id,
            task_uuid = %result.task_uuid,
            step_uuid = %result.step_uuid,
            error = %e,
            retry_count = result.attempts,
            "Step result processing failed, will retry"
        );
        return Err(e);
    }
    Err(e) => {
        error!(
            correlation_id = %result.correlation_id,
            task_uuid = %result.task_uuid,
            step_uuid = %result.step_uuid,
            error = %e,
            "Step result processing failed permanently"
        );
        return Err(e);
    }
}
```

#### Bootstrap/Shutdown

```rust
pub async fn bootstrap() -> Result<OrchestrationSystemHandle> {
    info!("Starting orchestration system bootstrap");

    let config = ConfigManager::load()?;
    debug!(environment = %config.environment, "Configuration loaded");

    let context = SystemContext::from_config(config).await?;
    info!(processor_uuid = %context.processor_uuid, "System context initialized");

    let core = OrchestrationCore::new(context).await?;
    info!("Orchestration core initialized");

    // ... more initialization ...

    info!(
        processor_uuid = %core.processor_uuid,
        namespaces = ?core.supported_namespaces,
        "Orchestration system bootstrap complete"
    );

    Ok(handle)
}

pub async fn shutdown(&mut self) -> Result<()> {
    info!("Initiating graceful shutdown");

    if let Some(coordinator) = &self.event_coordinator {
        coordinator.stop().await?;
        debug!("Event coordinator stopped");
    }

    info!("Orchestration system shutdown complete");
    Ok(())
}
```

---

## Enforcement

### Code Review Checklist

Before merging, verify:
- [ ] No emojis in log messages
- [ ] No all-caps component prefixes
- [ ] No TAS-XX references in runtime logs
- [ ] correlation_id present in all task/step operations
- [ ] Structured fields follow standard ordering
- [ ] Messages are concise and actionable
- [ ] Appropriate log levels used
- [ ] Error context is complete

### CI Checks

**Recommended lints** (future):
```bash
# Check for emojis
! grep -r '[🔧✅🚀❌⚠️📊🔍🎉🛡️⏱️📝🏗️🎯🔄💡📦🧪🌉🔌⏳🛑]' src/

# Check for all-caps prefixes
! grep -rE '(info|debug|warn|error)!\(".*[A-Z_]{3,}:' src/

# Check for TAS-XX in logs (allow in comments)
! grep -rE '(info|debug|warn|error)!.*TAS-[0-9]' src/
```

### Pre-commit Hook

Add to `.git/hooks/pre-commit`:
```bash
#!/bin/bash
./scripts/audit-logging.sh --check || {
    echo "❌ Logging standards violation detected"
    echo "Run: ./scripts/audit-logging.sh for details"
    exit 1
}
```

---

## Migration Guide

### For Existing Code

1. **Remove emojis**: Use find/replace
2. **Remove all-caps prefixes**: Simple cleanup
3. **Add correlation_id**: Extract from context
4. **Reorder fields**: correlation_id first
5. **Shorten messages**: Remove redundancy
6. **Verify log levels**: Lifecycle = INFO, diagnostics = DEBUG

### For New Code

1. **Always include correlation_id** when context available
2. **Use `#[instrument]`** for significant functions
3. **Follow field ordering**: correlation_id, IDs, operation, measurements, errors
4. **Keep messages concise**: Under 10 words
5. **Choose appropriate level**: ERROR (fatal), WARN (degraded), INFO (lifecycle), DEBUG (diagnostic)

---

## FAQ

**Q: Should I use `info!` or `debug!` for step enqueueing?**
A: `info!` - It's a significant lifecycle event even if frequent.

**Q: When should I add `duration_ms`?**
A: For any operation that:
- Calls external systems (DB, queue)
- Is in the hot path
- Takes >10ms typically
- Needs performance monitoring

**Q: Can I use emojis in error messages?**
A: No. Never use emojis in any log message. They break parsers and are unprofessional.

**Q: Should correlation_id really always be first?**
A: Yes. This enables easy correlation across all logs. It's the #1 most important field for distributed tracing.

**Q: What about TAS-XX in module docs?**
A: Acceptable in module-level documentation for architectural context. Remove from runtime logs and inline comments.

**Q: Can I include stack traces in logs?**
A: Use `error = %e` which includes the error chain. Only add explicit backtrace for truly exceptional cases.

---

## References

- [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)
- [Tracing Crate Documentation](https://docs.rs/tracing)
- [OpenTelemetry Semantic Conventions](https://opentelemetry.io/docs/specs/semconv/)

---

**Document End**

*This is a living document. Propose changes via PR with rationale.*
