# Rust Best Practices for Tasker Core

**Purpose**: Codify Rust-specific coding standards for the tasker-core project.

---

## Code Style

### Formatting
- Use `cargo fmt` (enforced via CI)
- Maximum line length: 100 characters (rustfmt default)
- Use trailing commas in multi-line constructs

### Naming Conventions
```rust
// Types: PascalCase
struct TaskRequestActor { ... }
enum StepState { ... }
trait StepHandler { ... }

// Functions/methods: snake_case
fn process_step_result() { ... }
async fn handle_message() { ... }

// Constants: SCREAMING_SNAKE_CASE
const DEFAULT_BATCH_SIZE: usize = 100;
static METRICS_PREFIX: &str = "tasker";

// Type parameters: single uppercase or descriptive PascalCase
fn process<T: Send>(item: T) { ... }
fn handle<Msg: Message>(msg: Msg) { ... }
```

### Module Organization
```rust
// lib.rs ordering
//! Crate-level documentation

// 1. Re-exports (pub use)
pub use types::{Task, Step};

// 2. Public modules
pub mod actors;
pub mod handlers;

// 3. Internal modules
mod internal;

// 4. Tests
#[cfg(test)]
mod tests;
```

---

## Error Handling

### Error Types
```rust
// Use thiserror for library errors
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Task not found: {0}")]
    NotFound(Uuid),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition { from: TaskState, to: TaskState },

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

// Result type alias per crate
pub type Result<T> = std::result::Result<T, TaskError>;
```

### Error Propagation
```rust
// Use ? operator for propagation
async fn get_task(id: Uuid) -> Result<Task> {
    let task = sqlx::query_as!(Task, "SELECT * FROM tasks WHERE id = $1", id)
        .fetch_optional(&pool)
        .await?
        .ok_or(TaskError::NotFound(id))?;
    Ok(task)
}

// Add context with .map_err() when needed
let config = load_config()
    .map_err(|e| ConfigError::LoadFailed { path: path.clone(), source: e })?;
```

### No Panics in Library Code
```rust
// BAD: Panic in library
fn get_handler(name: &str) -> &Handler {
    self.handlers.get(name).unwrap() // Panics if not found!
}

// GOOD: Return Option or Result
fn get_handler(&self, name: &str) -> Option<&Handler> {
    self.handlers.get(name)
}

// GOOD: Use expect only when invariant is guaranteed
fn get_handler(&self, name: &str) -> &Handler {
    self.handlers.get(name)
        .expect("handler was validated at registration time")
}
```

---

## Async Patterns

### Tokio Runtime
```rust
// Use #[tokio::main] for binaries
#[tokio::main]
async fn main() -> Result<()> {
    // ...
}

// Use #[tokio::test] for async tests
#[tokio::test]
async fn test_process_step() {
    // ...
}
```

### Channel Patterns (TAS-51)
```rust
// ALWAYS use bounded channels
let (tx, rx) = tokio::sync::mpsc::channel(config.channel_capacity);

// NEVER use unbounded
// let (tx, rx) = tokio::sync::mpsc::unbounded_channel(); // FORBIDDEN

// Configure via TOML
#[derive(Deserialize)]
struct ChannelConfig {
    capacity: usize,
}
```

### Actor Pattern
```rust
// Standard actor structure
pub struct MyActor {
    receiver: mpsc::Receiver<Message>,
    // ... state
}

impl MyActor {
    pub fn spawn(config: Config) -> ActorHandle {
        let (tx, rx) = mpsc::channel(config.channel_capacity);
        let actor = Self { receiver: rx, /* ... */ };

        let handle = tokio::spawn(async move {
            actor.run().await
        });

        ActorHandle { sender: tx, handle }
    }

    async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg).await;
        }
    }
}
```

---

## Database Patterns

### SQLx Usage
```rust
// Use query macros for compile-time checking
let task = sqlx::query_as!(
    Task,
    r#"SELECT id, state as "state: TaskState", created_at FROM tasks WHERE id = $1"#,
    id
)
.fetch_one(&pool)
.await?;

// Use transactions for multi-step operations
let mut tx = pool.begin().await?;
sqlx::query!("UPDATE tasks SET state = $1 WHERE id = $2", state, id)
    .execute(&mut *tx)
    .await?;
sqlx::query!("INSERT INTO task_transitions ...")
    .execute(&mut *tx)
    .await?;
tx.commit().await?;
```

### SQLx Cache
```bash
# After modifying queries, update cache:
cargo make sqlx-prepare

# Commit .sqlx/ directories
git add .sqlx/
```

---

## Documentation

### Module Documentation
```rust
//! # Task Processing
//!
//! This module handles task lifecycle management including:
//! - Task creation and initialization
//! - State transitions
//! - Result aggregation
//!
//! ## Example
//!
//! ```rust
//! use tasker_orchestration::TaskProcessor;
//!
//! let processor = TaskProcessor::new(config);
//! processor.start().await?;
//! ```

pub mod processor;
```

### Item Documentation
```rust
/// Processes a single workflow step.
///
/// This function handles the complete lifecycle of step execution,
/// including dependency resolution, handler invocation, and result
/// recording.
///
/// # Arguments
///
/// * `step` - The workflow step to process
/// * `context` - Execution context with task state
///
/// # Returns
///
/// Returns `Ok(StepResult)` on successful execution, or an error
/// if the step fails permanently.
///
/// # Errors
///
/// * `StepError::NotFound` - Step doesn't exist
/// * `StepError::InvalidState` - Step not in processable state
///
/// # Example
///
/// ```rust
/// let result = process_step(&step, &context).await?;
/// match result.outcome {
///     Outcome::Success => println!("Step completed"),
///     Outcome::Failure => println!("Step failed: {}", result.error),
/// }
/// ```
pub async fn process_step(step: &Step, context: &Context) -> Result<StepResult> {
    // ...
}
```

---

## Testing

### Test Organization
```rust
// Unit tests in same file
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transition_valid() {
        let result = TaskState::Pending.transition_to(TaskState::Initializing);
        assert!(result.is_ok());
    }
}

// Integration tests in tests/ directory
// tests/integration/task_lifecycle.rs
```

### Test Naming
```rust
#[test]
fn state_transition_from_pending_to_initializing_succeeds() { ... }

#[test]
fn state_transition_from_complete_to_pending_fails() { ... }

#[tokio::test]
async fn process_step_with_all_dependencies_met_executes_handler() { ... }
```

### Property-Based Testing
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn task_uuid_serialization_roundtrip(uuid in any::<Uuid>()) {
        let serialized = serde_json::to_string(&uuid).unwrap();
        let deserialized: Uuid = serde_json::from_str(&serialized).unwrap();
        assert_eq!(uuid, deserialized);
    }
}
```

---

## Lint Compliance (TAS-58)

### Use `#[expect]` Instead of `#[allow]`
```rust
// BAD: No reason given
#[allow(dead_code)]
fn unused_helper() { ... }

// GOOD: Reason documented
#[expect(dead_code, reason = "used by FFI bindings, not called from Rust")]
fn ffi_helper() { ... }

// GOOD: Used for future feature
#[expect(dead_code, reason = "TAS-35: RabbitMQ backend not yet implemented")]
fn rabbitmq_connect() { ... }
```

### Required Trait Implementations
```rust
// All public types must implement Debug
#[derive(Debug)]
pub struct TaskConfig {
    // ...
}

// Consider also implementing: Clone, Default where appropriate
#[derive(Debug, Clone, Default)]
pub struct StepConfig {
    // ...
}
```

---

## Performance Considerations

### Avoid Unnecessary Allocations
```rust
// BAD: Unnecessary String allocation
fn process(name: String) { ... }

// GOOD: Accept reference when not storing
fn process(name: &str) { ... }

// GOOD: Use Cow for conditional ownership
fn process(name: Cow<'_, str>) { ... }
```

### Use Iterators
```rust
// BAD: Collect then iterate
let names: Vec<_> = items.iter().map(|i| i.name.clone()).collect();
for name in names { ... }

// GOOD: Lazy iteration
for name in items.iter().map(|i| &i.name) { ... }
```

---

## Project-Specific Patterns

### Configuration Loading
```rust
// Use tasker-shared config utilities
use tasker_shared::config::ConfigLoader;

let config = ConfigLoader::new()
    .with_environment(env)
    .with_role(Role::Worker)
    .load()?;
```

### State Machine Transitions
```rust
// Use atomic SQL functions for state changes
let result = sqlx::query!(
    "SELECT claim_step_for_processing($1, $2) as claimed",
    step_id,
    processor_id
)
.fetch_one(&pool)
.await?;
```

### Event Publishing
```rust
// Use the event system for cross-component communication
let event = StepCompletedEvent {
    task_id,
    step_id,
    result: outcome,
};
event_publisher.publish(event).await?;
```

---

## References

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Tasker Core Tenets](../../principles/tasker-core-tenets.md)
- [MPSC Channel Guidelines](../../development/mpsc-channel-guidelines.md)
- [Development Patterns](../../development/development-patterns.md)
