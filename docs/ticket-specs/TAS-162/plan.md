# TAS-162: Hot Path Logging Optimization Plan

**Branch:** `jcoletaylor/tas-162-hot-path-logging-optimization`
**Date:** 2026-01-22
**Status:** Implementation Complete
**Linear:** [TAS-162](https://linear.app/tasker-systems/issue/TAS-162/hot-path-logging-optimization)

---

## Background

From TAS-161 profiling (see `docs/ticket-specs/TAS-71/profiling-report.md`), logging overhead accounts for **~5.15% of CPU time**:

| Component | % of Total Time |
|-----------|-----------------|
| `tracing_subscriber` | 2.70% |
| Stdout `write` syscall | 2.45% |
| `Instrumented` futures | 1.05% |
| `fmt::write` | 0.46% |

The profiled hot-path components:

| Component | % of Total | Samples |
|-----------|------------|---------|
| `command_processor_actor` | 4.31% | 4,856 |
| `result_processor` | 4.03% | 4,542 |
| `step_enqueuer` | 2.49% | 2,808 |
| `task_finalizer` | 2.27% | 2,563 |

**Target:** ~2-3% CPU reduction

---

## Code Review Findings

### 1. step_enqueuer.rs (HIGHEST IMPACT)

**Location:** `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs`

**Current State:** Most verbose logging in hot path - **15+ `info!()` calls** per step enqueue operation:
- `info!()` at lines 160, 345, 365, 495, 510, 538, 565, 573 (per-step logging)
- Multiple `debug!()` calls in filtering loops (lines 183, 216, 281, 422, 444, 475, 608)

**Observations:**
- Already has `enable_detailed_logging` config flag (line 280) but only guards 1 debug! call
- Emits `info!()` for every individual step enqueue - O(n) logging per task
- Redundant logging: logs same step_uuid in multiple info! calls on success path

**Opportunities:**
1. Consolidate per-step `info!()` calls into summary logging
2. Expand `enable_detailed_logging` guard to cover more debug/info calls
3. Use `debug!()` instead of `info!()` for per-step details

### 2. command_processor_actor.rs (HIGH IMPACT)

**Location:** `tasker-orchestration/src/actors/command_processor_actor.rs`

**Current State:**
- `info!()` at creation (line 125)
- `debug!()` on every command variant (lines 283, 500, 548, 689, 834)
- `info!()` on successful step result processing (line 712, 727)
- Heavy `warn!()`/`error!()` logging for error paths (appropriate)

**Observations:**
- Debug logging on every message receive is expensive in hot path
- Multiple sequential debug! calls could be consolidated

**Opportunities:**
1. Add `tracing::enabled!(Level::DEBUG)` guards for expensive debug calls
2. Use `#[instrument(skip_all)]` to reduce span field overhead
3. Consider metrics instead of logging for per-message tracking

### 3. result_processing/service.rs (MEDIUM IMPACT)

**Location:** `tasker-orchestration/src/orchestration/lifecycle/result_processing/service.rs`

**Current State:**
- Uses `event!(Level::INFO, ...)` at start/end of every result processing (lines 102, 109, 112, 129, 137, 139)
- Uses `#[instrument(...)]` with field extraction on every call

**Observations:**
- `event!()` macro creates span events even when not actively traced
- `#[instrument]` creates spans with field serialization overhead

**Opportunities:**
1. Replace `event!()` with guarded `info!()` calls
2. Use `#[instrument(skip_all)]` and only log essential identifiers
3. Consider sampling for high-frequency success events

### 4. task_finalization/service.rs (MEDIUM IMPACT)

**Location:** `tasker-orchestration/src/orchestration/lifecycle/task_finalization/service.rs`

**Current State:**
- `debug!()` at lines 90, 105, 170, 179, 192, 206, 220, 226, 232
- `event!()` at lines 74, 78, 152 for tracing events
- `#[instrument(...)]` with correlation_id recording

**Observations:**
- Every finalization decision branch has separate debug! call
- Pattern matches emit separate debug! for each ExecutionStatus variant

**Opportunities:**
1. Consolidate decision-branch logging into single debug! after match
2. Add `tracing::enabled!` guards for debug-level logging
3. Use `#[instrument(skip_all)]` for simpler span overhead

---

## Implementation Plan

### Phase 1: Logging Guard Macros

**Goal:** Create reusable macros that guard both I/O AND string interpolation/allocation

**Why macros?** Even with `tracing::enabled!()` guards, argument evaluation happens before the check:
```rust
// Problem: expensive_format() runs even if DEBUG is disabled
if tracing::enabled!(Level::DEBUG) {
    debug!("Context: {}", expensive_format());
}
```

**Proposed macros** (add to existing `tasker-shared/src/logging.rs` which already has `log_task!`, `log_step!`, etc.):

```rust
/// Block-based conditional logging - guards entire code block
/// Prevents both I/O and any string formatting/allocation inside block
macro_rules! maybe_log {
    ($config:expr, $level:expr, { $($body:tt)* }) => {
        if $config.enable_detailed_logging && ::tracing::enabled!($level) {
            $($body)*
        }
    };
}

/// Config-gated tracing macro - ergonomic single-line variant
macro_rules! detail_log {
    ($config:expr, $level:ident, $($arg:tt)*) => {
        if $config.enable_detailed_logging {
            ::tracing::$level!($($arg)*);
        }
    };
}
```

**Usage patterns:**

```rust
// Block-based: for multiple statements or expensive computations
maybe_log!(self.config, Level::DEBUG, {
    let context = build_expensive_debug_context();
    debug!(step_uuid = %uuid, context = %context, "Processing step");
});

// Single-line: for simple log statements
detail_log!(self.config, debug, step_uuid = %uuid, "Processing step");
```

### Phase 2: Expand Configuration Pattern

**Goal:** Ensure all hot-path modules have access to `enable_detailed_logging`

1. **Verify config availability** in each target module
   - `StepEnqueuer` already has `StepEnqueuerConfig.enable_detailed_logging`
   - Add similar config to `CommandProcessorConfig` if needed
   - May need to thread config through actor constructors

2. **Apply macros** to per-item logging (per-step, per-message)

### Phase 2: Logging Consolidation

**Goal:** Reduce O(n) logging to O(1) summary logging

1. **step_enqueuer.rs**: Consolidate per-step info! into batch summary
   ```rust
   // Before: 4 info!() calls per step
   info!("Preparing to enqueue step");
   info!("Created step message");
   info!("Successfully marked step as enqueued");
   info!("Step enqueueing complete");

   // After: 1 debug!() per step (guarded), 1 info!() summary
   if self.config.enable_detailed_logging {
       debug!(step_uuid = %uuid, "Enqueued step");
   }
   // ... at end of batch
   info!(steps_enqueued = count, duration_ms = dur, "Batch complete");
   ```

2. **command_processor_actor.rs**: Consolidate handler debug logging
   - Single entry debug! with command type
   - Single exit debug! with result status

### Phase 3: Instrument Optimization

**Goal:** Reduce `#[instrument]` macro overhead

1. **Use `skip_all` for frequently-called functions**
   ```rust
   // Before
   #[instrument(skip(self), fields(task_uuid = %task_uuid))]

   // After
   #[instrument(skip_all, fields(task_uuid = %task_uuid))]
   ```

2. **Remove instrument from inner helper functions**
   - Keep instrument on public API entry points
   - Remove from internal implementation functions

### Phase 4: Reduce Redundant Logging

**Goal:** Eliminate logging that duplicates span information

**Note:** `event!(Level::INFO, "x")` and `info!("x")` are equivalent - `info!()` expands to `event!()`. The overhead is identical. The optimization is about *whether* to log, not *which macro*.

1. **Remove redundant span-entry/exit events**
   ```rust
   // Before: #[instrument] already creates span, event!() is redundant
   #[instrument(skip(self), fields(task_uuid = %task_uuid))]
   pub async fn finalize_task(&self, task_uuid: Uuid) -> Result<...> {
       event!(Level::INFO, "task.finalization_started");  // Redundant!
       // ...
       event!(Level::INFO, "task.finalization_completed"); // Redundant!
   }

   // After: Let the span handle entry/exit, log only meaningful state changes
   #[instrument(skip_all, fields(task_uuid = %task_uuid))]
   pub async fn finalize_task(&self, task_uuid: Uuid) -> Result<...> {
       // ... actual work, only log on errors or state changes
   }
   ```

2. **Consider event sampling** for high-frequency success paths (future work)
   - Emit every Nth success event
   - Always emit failures/errors

---

## Logging Level Audit Results

**Methodology:** Identified `info!` calls that are per-item/per-step/per-message (should be `debug!`) vs system-level events (keep as `info!`).

**Total: 32 `info!` → `debug!` changes across 7 files**

### step_enqueuer.rs (6 changes)
| Lines | Current | Recommendation | Reasoning |
|-------|---------|----------------|-----------|
| 435-440 | `info!` | → `debug!` | Per-step retry transition inside loop |
| 495-502 | `info!` | → `debug!` | Per-step "Preparing to enqueue" inside loop |
| 510-515 | `info!` | → `debug!` | Per-step "Created step message" inside loop |
| 538-542 | `info!` | → `debug!` | Per-step "Successfully marked as enqueued" inside loop |
| 565-571 | `info!` | → `debug!` | Per-step "Sent to pgmq" inside loop |
| 573-581 | `info!` | → `debug!` | Per-step "Enqueueing complete" inside loop |

**Keep as info!:** Lines 160-165 (batch start), 345-350 (task state transition), 365-373 (batch summary)

### command_processor_actor.rs (3 changes)
| Lines | Current | Recommendation | Reasoning |
|-------|---------|----------------|-----------|
| 322-332 | `info!` | → `debug!` | Per-command in process_command dispatcher |
| 712-717 | `info!` | → `debug!` | Per-message result processing succeeded |
| 727-731 | `info!` | → `debug!` | Per-message acknowledgment |

**Keep as info!:** Lines 125-129 (actor creation), 927-937 (batch summary)

### result_processing/service.rs (4 changes)
| Lines | Current | Recommendation | Reasoning |
|-------|---------|----------------|-----------|
| 102 | `event!(Level::INFO, ...)` | → `debug!` | Per-step processing started |
| 110 | `event!(Level::INFO, ...)` | → `debug!` | Per-step processing completed |
| 129 | `event!(Level::INFO, ...)` | → `debug!` | Per-step execution result started |
| 137 | `event!(Level::INFO, ...)` | → `debug!` | Per-step execution result completed |

### result_processing/message_handler.rs (10 changes)
| Lines | Reasoning |
|-------|-----------|
| 101-108 | Per-step "Starting step result notification processing" |
| 161-166 | Per-step "Processing completed successfully" |
| 205-212 | Per-step execution result completion |
| 271-276 | Per-step result completion |
| 306-313 | Per-step "Processing step result for orchestration" |
| 446-452 | Per-decision-point "Processing decision point outcome" |
| 463-467 | Per-decision-point completion |
| 555-559 | Per-batch "determined no batches needed" |
| 566-572 | Per-batch "Processing batch worker creation" |
| 585-589 | Per-batch completion |

### result_processing/state_transition_handler.rs (7 changes)
| Lines | Reasoning |
|-------|-----------|
| 82-88 | Per-step "Processing orchestration state transition" |
| 115-120 | Per-step "Successfully transitioned step" |
| 196-200 | Per-step "Transitioning to WaitingForRetry" |
| 205-209 | Per-step "Transitioning to Error state" |
| 224-230 | Per-step "Error marked as non-retryable" |
| 237-243 | Per-step "Step has exceeded retry limit" |
| 246-252 | Per-step "Step is retryable with attempts remaining" |

### result_processing/task_coordinator.rs (1 change)
| Lines | Reasoning |
|-------|-----------|
| 223-230 | Per-step "Task finalization completed successfully" (coordination detail) |

### task_finalization/state_handlers.rs (1 change)
| Lines | Reasoning |
|-------|-----------|
| 218-222 | Per-task "Handling waiting state by delegating" |

---

## Files to Modify

| File | Priority | Changes |
|------|----------|---------|
| `tasker-shared/src/logging.rs` | HIGH | Add `maybe_log!` and `detail_log!` macros to existing logging module |
| `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs` | HIGH | 6× info!→debug!, apply macros, consolidate |
| `tasker-orchestration/src/actors/command_processor_actor.rs` | HIGH | 3× info!→debug!, apply macros |
| `tasker-orchestration/src/orchestration/lifecycle/result_processing/message_handler.rs` | HIGH | 10× info!→debug! |
| `tasker-orchestration/src/orchestration/lifecycle/result_processing/state_transition_handler.rs` | MEDIUM | 7× info!→debug! |
| `tasker-orchestration/src/orchestration/lifecycle/result_processing/service.rs` | MEDIUM | 4× event!→debug!, skip_all on instrument |
| `tasker-orchestration/src/orchestration/lifecycle/task_finalization/service.rs` | MEDIUM | skip_all on instrument |
| `tasker-orchestration/src/orchestration/lifecycle/task_finalization/state_handlers.rs` | MEDIUM | 1× info!→debug! |
| `tasker-orchestration/src/orchestration/lifecycle/result_processing/task_coordinator.rs` | LOW | 1× info!→debug! |
| `tasker-orchestration/src/actors/result_processor_actor.rs` | LOW | skip_all on instrument |
| `tasker-orchestration/src/actors/task_finalizer_actor.rs` | LOW | skip_all on instrument |

---

## Acceptance Criteria

- [ ] Audit completed for all identified hot-path modules
- [ ] 32× `info!` → `debug!` changes applied (per-item logging demoted)
- [ ] `maybe_log!` and `detail_log!` macros added to `tasker-shared/src/logging.rs`
- [ ] Config-driven `enable_detailed_logging` guards applied to remaining debug! calls
- [ ] Per-step/per-message logging consolidated to summary logging where beneficial
- [ ] `#[instrument(skip_all)]` used for frequently-called helpers
- [ ] Benchmark before/after with profiling (`cargo make bp && cargo make rso`)
- [ ] All existing tests pass
- [ ] Document logging level guidelines in `docs/development/`

---

## Verification Plan

### Before Changes
```bash
# Build with profiling symbols
cargo make bp

# Run benchmark baseline
cargo make bench-e2e

# Profile orchestration server
cargo make rso  # Let run during bench, Ctrl-C to collect
```

### After Changes
```bash
# Same profiling workflow
cargo make bp
cargo make bench-e2e
cargo make rso

# Compare tracing_subscriber % and stdout write % to baseline
cargo make pat  # Analyze tasker-specific functions
```

**Success Criteria:** Combined logging overhead reduced from ~5.15% to <3%

---

## Out of Scope

- Async/buffered logging infrastructure (larger architectural change)
- Log level filtering at subscriber level (requires runtime config changes)
- Removal of error/warn logging (these should always emit)

---

## References

- Profiling Report: `docs/ticket-specs/TAS-71/profiling-report.md`
- Optimization Tickets: `docs/ticket-specs/TAS-71/optimization-tickets.md`
- Related: TAS-161 (Profiling Infrastructure)
