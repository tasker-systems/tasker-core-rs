# TAS-97: Rust Worker API Alignment

**Parent**: [TAS-92](./README.md)
**Linear**: [TAS-97](https://linear.app/tasker-systems/issue/TAS-97)
**Branch**: `jcoletaylor/tas-97-rust-worker-api-alignment`
**Priority**: Medium (originally Low, but Medium for consistency)

## Objective

Minor adjustments to Rust worker APIs for cross-language alignment. Rust is already well-aligned; changes are primarily convenience methods and documentation.

## Summary of Changes

| Area | Current State | Target State | Effort |
|------|---------------|--------------|--------|
| Handler Signature | `call(&TaskSequenceStep)` | No change | None |
| Result Factories | Already aligned | No change | None |
| Error Fields | No `error_code` | Add helper method | Low |
| Registry API | Custom method names | Add helper methods | Low |
| Specialized Handlers | Examples only | Add patterns/docs | Low |
| Domain Events | `StepEventPublisher` exists | Verify alignment | Low |

## Key Discovery: error_code Already Exists!

**The `error_code` field is already present in the Rust codebase:**

```rust
// In StepExecutionMetadata (execution_types.rs:289)
pub error_code: Option<String>,

// In StepCompletedPayload (orchestration_messages.rs:224)
pub error_code: Option<String>,
```

Since `StepExecutionResult` contains `metadata: StepExecutionMetadata`, the error_code is already accessible via `result.metadata.error_code`. The only enhancement needed is convenience helper methods for ergonomics.

## Implementation Plan

### Phase 1: Error Code Convenience Helpers

**File to modify:** `tasker-shared/src/messaging/execution_types.rs`

**Add helper methods to `StepExecutionResult`:**
```rust
impl StepExecutionResult {
    /// Set error_code via builder pattern for ergonomic failure construction
    pub fn with_error_code(mut self, code: impl Into<String>) -> Self {
        self.metadata.error_code = Some(code.into());
        self
    }

    /// Get error_code from metadata (convenience accessor)
    pub fn error_code(&self) -> Option<&str> {
        self.metadata.error_code.as_deref()
    }
}
```

**Usage example:**
```rust
StepExecutionResult::failure(
    step_uuid,
    "Payment gateway timeout",
    "timeout",
    true, // retryable
)
.with_error_code("PAYMENT_GATEWAY_TIMEOUT")
```

**Note**: This is purely additive - no schema changes required since `error_code` already exists in `StepExecutionMetadata`.

### Phase 2: Registry Helper Methods

**File to modify:** `workers/rust/src/step_handlers/registry.rs`

**Add methods:**
```rust
impl RustStepHandlerRegistry {
    /// Check if a handler is registered (cross-language standard)
    pub fn is_registered(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// List all registered handler names (cross-language standard)
    pub fn list_handlers(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    // Existing methods remain:
    // - register_handler (alias: register)
    // - get_handler (alias: resolve)
}
```

### Phase 3: Error Type Constants

**File to create:** `tasker-shared/src/types/error_types.rs`

```rust
/// Recommended error_type values for cross-language consistency
pub mod error_types {
    pub const PERMANENT_ERROR: &str = "permanent_error";
    pub const RETRYABLE_ERROR: &str = "retryable_error";
    pub const VALIDATION_ERROR: &str = "validation_error";
    pub const TIMEOUT: &str = "timeout";
    pub const HANDLER_ERROR: &str = "handler_error";

    /// All valid error types
    pub const ALL: &[&str] = &[
        PERMANENT_ERROR,
        RETRYABLE_ERROR,
        VALIDATION_ERROR,
        TIMEOUT,
        HANDLER_ERROR,
    ];

    /// Check if an error type is one of the standard values
    pub fn is_standard(error_type: &str) -> bool {
        ALL.contains(&error_type)
    }
}
```

**Update:** `tasker-shared/src/lib.rs` to export the module.

### Phase 4: Batch Processing Field Standardization

**Files to review:** `workers/rust/src/step_handlers/batch_processing_*.rs`

**Ensure field names match:**
- `items_processed` (not `processed_count`)
- `items_succeeded`
- `items_failed`
- `start_cursor`, `end_cursor`, `batch_size`, `last_cursor`

**Example update:**
```rust
// In batch handler result construction
let result = HashMap::from([
    ("items_processed".to_string(), json!(count)),
    ("items_succeeded".to_string(), json!(succeeded)),
    ("items_failed".to_string(), json!(failed)),
    ("end_cursor".to_string(), json!(cursor)),
]);
```

### Phase 5: API Handler Pattern Documentation

**File to create:** `workers/rust/src/step_handlers/README.md`

**Content structure:**
```markdown
# Rust Step Handler Patterns

## API Handler Pattern

Recommended pattern for HTTP-based handlers using `reqwest`:

\`\`\`rust
use reqwest::Client;
use tasker_shared::messaging::StepExecutionResult;

pub struct ApiHandler {
    client: Client,
    base_url: String,
}

impl ApiHandler {
    pub async fn get(&self, path: &str) -> Result<Response, reqwest::Error> {
        self.client.get(format!("{}{}", self.base_url, path))
            .send()
            .await
    }

    pub async fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<Response, reqwest::Error> {
        self.client.post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
    }

    // Error classification based on status code
    fn classify_error(status: StatusCode) -> &'static str {
        match status.as_u16() {
            400..=499 => error_types::PERMANENT_ERROR,
            500..=599 => error_types::RETRYABLE_ERROR,
            _ => error_types::HANDLER_ERROR,
        }
    }
}
\`\`\`

## Decision Handler Pattern

For handlers that route to different steps based on conditions:

\`\`\`rust
pub struct DecisionHandler;

impl StepHandler for DecisionHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> StepExecutionResult {
        let condition = evaluate_condition(&step_data.inputs);

        let steps_to_activate = if condition {
            vec!["branch_a_step_1", "branch_a_step_2"]
        } else {
            vec!["branch_b_step_1"]
        };

        StepExecutionResult::success(
            HashMap::from([
                ("activated_steps".to_string(), json!(steps_to_activate)),
                ("routing_context".to_string(), json!({"condition": condition})),
            ]),
            None,
        )
    }
}
\`\`\`
```

### Phase 6: Verify StepEventPublisher Alignment

**File to review:** `tasker-worker/src/worker/step_event_publisher.rs`

**Verify `StepEventContext` has all fields:**
```rust
pub struct StepEventContext {
    pub task_uuid: String,
    pub step_uuid: String,
    pub step_name: String,
    pub namespace: String,
    pub correlation_id: String,
    pub result: Option<HashMap<String, Value>>,
    pub metadata: Option<HashMap<String, Value>>,
}
```

**Add missing fields if needed and document the trait.**

### Phase 7: Update Examples

**Files to update:**
- `workers/rust/src/step_handlers/batch_processing_*.rs`
- Create new example files if needed

**Ensure all examples:**
- Use standard error_type values
- Use standardized field names
- Demonstrate `with_error_code()` helper

## Files Summary

### New Files
| File | Purpose |
|------|---------|
| `tasker-shared/src/types/error_types.rs` | Error type constants |
| `workers/rust/src/step_handlers/README.md` | Pattern documentation |

### Modified Files
| File | Change Type |
|------|-------------|
| `tasker-shared/src/messaging/step_execution_result.rs` | Add helper method |
| `tasker-shared/src/lib.rs` | Export error_types |
| `workers/rust/src/step_handlers/registry.rs` | Add helper methods |
| `workers/rust/src/step_handlers/batch_processing_*.rs` | Verify/update field names |
| `tasker-worker/src/worker/step_event_publisher.rs` | Verify/document |

## Verification Checklist

- [x] `with_error_code()` helper method works
- [x] `error_code()` getter returns value from metadata
- [x] `with_error_type()` helper method works (bonus)
- [x] `error_type()` getter returns value from metadata (bonus)
- [x] Registry `is_registered()` method works
- [x] Registry `list_handlers()` method works
- [x] Error type constants defined and exported
- [x] `is_standard()` helper validates error types
- [x] `is_typically_retryable()` helper for default retry behavior
- [x] Batch processing examples use standard field names (CursorConfig already aligned)
- [x] API handler pattern documented
- [x] Decision handler pattern documented
- [x] Batch processing pattern documented
- [x] `StepEventPublisher` verified as already aligned
- [x] All unit tests pass
- [x] All integration tests pass
- [x] Clippy passes
- [ ] Documentation builds (deferred to TAS-98)

## Risk Assessment

**Low Risk**: Rust implementation is already well-aligned. Changes are:
- Adding convenience methods (non-breaking)
- Adding constants/documentation
- Verifying existing implementations

## Estimated Scope

- **New lines**: ~100 (error_types, README)
- **Modified lines**: ~50 (helper methods)
- **Test additions**: ~30
