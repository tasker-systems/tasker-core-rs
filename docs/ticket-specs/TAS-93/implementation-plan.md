# TAS-93: Implementation Plan

**Branch**: `jcoletaylor/tas-93-expose-a-developer-facing-step-handler-router-resolver`
**Status**: In Progress
**Created**: 2025-01-07

---

## Overview

This document outlines the phase-by-phase implementation approach for TAS-93, including validation gates, unit test requirements, E2E test strategy, and final review criteria.

### Implementation Philosophy

- **Test-first within each phase**: Unit tests are written alongside implementation
- **Backward compatibility**: Every phase maintains existing template compatibility
- **Incremental delivery**: Each phase produces working, tested code
- **Cross-language alignment**: Rust implementation first, then Ruby/Python/TypeScript

---

## Phase 1: Core Types and Resolver Interface (Rust)

**Estimated Effort**: 3-4 days

### 1.1 Extend HandlerDefinition

**Location**: `tasker-shared/src/models/core/task_template/mod.rs`

Add new optional fields to `HandlerDefinition`:

```rust
pub struct HandlerDefinition {
    pub callable: String,

    // NEW: Entry point method (defaults to "call")
    #[serde(default)]
    pub method: Option<String>,

    // NEW: Resolution strategy hint (bypass chain)
    #[serde(default)]
    pub resolver: Option<String>,

    pub initialization: HashMap<String, Value>,
}

impl HandlerDefinition {
    pub fn effective_method(&self) -> &str {
        self.method.as_deref().unwrap_or("call")
    }

    pub fn uses_method_dispatch(&self) -> bool {
        self.method.is_some() && self.method.as_deref() != Some("call")
    }
}
```

**Unit Tests** (`tasker-shared/src/models/core/task_template/mod.rs` or dedicated test module):

```rust
#[cfg(test)]
mod handler_definition_tests {
    #[test]
    fn test_effective_method_defaults_to_call() { ... }

    #[test]
    fn test_effective_method_returns_specified_method() { ... }

    #[test]
    fn test_uses_method_dispatch_false_when_none() { ... }

    #[test]
    fn test_uses_method_dispatch_false_when_call() { ... }

    #[test]
    fn test_uses_method_dispatch_true_for_other_methods() { ... }

    #[test]
    fn test_serde_backward_compatible_without_new_fields() { ... }

    #[test]
    fn test_serde_with_method_field() { ... }

    #[test]
    fn test_serde_with_resolver_field() { ... }
}
```

### 1.2 Define StepHandlerResolver Trait

**Location**: `tasker-shared/src/registry/step_handler_resolver.rs` (new file)

```rust
use async_trait::async_trait;
use std::sync::Arc;

/// Strategy trait for resolving callable addresses to handler instances.
#[async_trait]
pub trait StepHandlerResolver: Send + Sync + std::fmt::Debug {
    /// Resolve a callable address to a handler instance.
    async fn resolve(
        &self,
        definition: &HandlerDefinition,
        config: &StepHandlerConfig,
    ) -> Option<Arc<dyn StepHandler>>;

    /// Check if this resolver can handle the given callable format.
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool;

    /// Resolver name for logging and metrics.
    fn resolver_name(&self) -> &str;

    /// Priority in the resolver chain (lower = checked first).
    fn priority(&self) -> u32 { 100 }
}
```

**Unit Tests** (`tasker-shared/src/registry/step_handler_resolver.rs`):

```rust
#[cfg(test)]
mod resolver_trait_tests {
    // Test implementations for trait contract validation

    #[test]
    fn test_default_priority_is_100() { ... }

    #[tokio::test]
    async fn test_mock_resolver_can_resolve() { ... }

    #[tokio::test]
    async fn test_mock_resolver_returns_none_when_cannot_resolve() { ... }
}
```

### 1.3 Implement ResolverChain

**Location**: `tasker-shared/src/registry/resolver_chain.rs` (new file)

```rust
pub struct ResolverChain {
    resolvers: Vec<Arc<dyn StepHandlerResolver>>,
}

impl ResolverChain {
    pub fn new() -> Self { ... }

    pub fn register(&mut self, resolver: Arc<dyn StepHandlerResolver>) { ... }

    pub async fn resolve(
        &self,
        definition: &HandlerDefinition,
        config: &StepHandlerConfig,
    ) -> Result<Arc<dyn StepHandler>, ResolutionError> { ... }

    fn sorted_resolvers(&self) -> Vec<&Arc<dyn StepHandlerResolver>> { ... }
}
```

**Unit Tests** (`tasker-shared/src/registry/resolver_chain.rs`):

```rust
#[cfg(test)]
mod resolver_chain_tests {
    #[test]
    fn test_resolvers_sorted_by_priority() { ... }

    #[tokio::test]
    async fn test_chain_tries_resolvers_in_order() { ... }

    #[tokio::test]
    async fn test_chain_returns_first_successful_resolution() { ... }

    #[tokio::test]
    async fn test_chain_skips_resolvers_that_cannot_resolve() { ... }

    #[tokio::test]
    async fn test_chain_uses_resolver_hint_directly() { ... }

    #[tokio::test]
    async fn test_chain_error_includes_tried_resolvers() { ... }

    #[tokio::test]
    async fn test_empty_chain_returns_error() { ... }
}
```

### 1.4 Implement MethodDispatchWrapper

**Location**: `tasker-shared/src/registry/method_dispatch_wrapper.rs` (new file)

```rust
/// Wrapper that redirects .call() to a specified method.
pub struct MethodDispatchWrapper {
    inner: Arc<dyn StepHandler>,
    method_name: String,
}

impl MethodDispatchWrapper {
    pub fn new(handler: Arc<dyn StepHandler>, method: &str) -> Self { ... }
}

#[async_trait]
impl StepHandler for MethodDispatchWrapper {
    async fn call(&self, step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult> {
        // Invoke the specified method on inner handler
        // For Rust, this means the handler must support dynamic dispatch
        // Implementation will use a trait method like `invoke_method(name, step)`
    }
}
```

**Unit Tests**:

```rust
#[cfg(test)]
mod method_dispatch_wrapper_tests {
    #[tokio::test]
    async fn test_wrapper_invokes_specified_method() { ... }

    #[tokio::test]
    async fn test_wrapper_preserves_handler_name() { ... }

    #[test]
    fn test_wrapper_stores_method_name() { ... }
}
```

### Phase 1 Validation Gate

| Criterion | Validation |
|-----------|------------|
| HandlerDefinition extended | `cargo test --package tasker-shared handler_definition` |
| Resolver trait compiles | `cargo check --package tasker-shared` |
| ResolverChain works | `cargo test --package tasker-shared resolver_chain` |
| MethodDispatchWrapper works | `cargo test --package tasker-shared method_dispatch` |
| Backward compatibility | Existing YAML fixtures load without errors |
| No clippy warnings | `cargo clippy --package tasker-shared --all-targets` |

**Gate Command**:
```bash
cargo test --package tasker-shared --all-features && \
cargo clippy --package tasker-shared --all-targets --all-features -- -D warnings
```

---

## Phase 2: Built-in Resolvers (Rust)

**Estimated Effort**: 1-2 days

### 2.1 ExplicitMappingResolver

**Location**: `tasker-shared/src/registry/resolvers/explicit_mapping.rs` (new file)

Mirrors the existing Rust registry pattern:

```rust
pub struct ExplicitMappingResolver {
    mappings: RwLock<HashMap<String, HandlerFactory>>,
}

impl ExplicitMappingResolver {
    pub fn new() -> Self { ... }

    pub fn register<F>(&self, key: &str, factory: F)
    where F: Fn(&StepHandlerConfig) -> Arc<dyn StepHandler> + Send + Sync + 'static { ... }

    pub fn register_instance(&self, key: &str, handler: Arc<dyn StepHandler>) { ... }
}

impl StepHandlerResolver for ExplicitMappingResolver {
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
        self.mappings.read().unwrap().contains_key(&definition.callable)
    }

    async fn resolve(...) -> Option<Arc<dyn StepHandler>> { ... }

    fn priority(&self) -> u32 { 10 } // Checked first
}
```

**Unit Tests**:

```rust
#[cfg(test)]
mod explicit_mapping_resolver_tests {
    #[test]
    fn test_can_resolve_registered_key() { ... }

    #[test]
    fn test_cannot_resolve_unregistered_key() { ... }

    #[tokio::test]
    async fn test_resolve_returns_registered_handler() { ... }

    #[tokio::test]
    async fn test_resolve_with_factory() { ... }

    #[test]
    fn test_priority_is_10() { ... }

    #[test]
    fn test_register_overwrites_existing() { ... }
}
```

### 2.2 ClassConstantResolver (Rust - No-Op Fallback)

**Location**: `tasker-shared/src/registry/resolvers/class_constant.rs` (new file)

For Rust, this is a no-op since Rust doesn't have runtime class lookup. It serves as a documentation placeholder and can log warnings:

```rust
pub struct ClassConstantResolver;

impl StepHandlerResolver for ClassConstantResolver {
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
        // In Rust, we can't do runtime class lookup
        // Log a warning if callable looks like a class path
        false
    }

    async fn resolve(...) -> Option<Arc<dyn StepHandler>> {
        None // Never resolves - Rust requires explicit mapping
    }

    fn priority(&self) -> u32 { 100 } // Last in chain
}
```

**Unit Tests**:

```rust
#[cfg(test)]
mod class_constant_resolver_tests {
    #[test]
    fn test_can_resolve_always_false_in_rust() { ... }

    #[tokio::test]
    async fn test_resolve_always_returns_none() { ... }

    #[test]
    fn test_priority_is_100() { ... }
}
```

### Phase 2 Validation Gate

| Criterion | Validation |
|-----------|------------|
| ExplicitMappingResolver works | `cargo test explicit_mapping` |
| ClassConstantResolver compiles | `cargo test class_constant` |
| Resolvers integrate with chain | `cargo test resolver_chain` |
| All existing tests pass | `cargo test --all-features` |

**Gate Command**:
```bash
cargo test --package tasker-shared --all-features && \
cargo test --package tasker-worker --all-features
```

---

## Phase 3: Worker Integration (Rust)

**Estimated Effort**: 2 days

### Architectural Decision: Adapter Pattern (2025-01-07)

**Context**: During implementation, we analyzed how to integrate the new `ResolverChain`
infrastructure with the existing `HandlerDispatchService`. The original plan suggested
modifying `HandlerDispatchService` directly to use `ResolverChain`.

**Challenge**: There's a trait mismatch between the systems:
- `HandlerDispatchService` uses `StepHandlerRegistry` → returns `Arc<dyn StepHandler>`
- `ResolverChain.resolve()` → returns `Arc<dyn ResolvedHandler>`

**Decision**: Use the **Adapter Pattern** instead of direct modification.

**Rationale**:
1. `HandlerDispatchService` is well-tested, production-ready code (TAS-67, TAS-75)
2. Follows composition-over-inheritance principle from core tenets
3. Provides cohesion without tight coupling
4. Mild indirection will be mostly compiled out (zero-cost abstractions)
5. Enables gradual migration via `HybridStepHandlerRegistry`

**Implementation**:
- Create `ResolverChainRegistry` implementing `StepHandlerRegistry`
- Create trait bridge adapters: `StepHandlerAsResolved`, `ExecutableHandler`
- Leave `HandlerDispatchService` unchanged
- Update `RustStepHandlerRegistryAdapter` to use `ExplicitMappingResolver` internally

**New Files**:
- `tasker-worker/src/worker/handlers/resolver_integration.rs`

---

### 3.1 Create Resolver Integration Module

**Location**: `tasker-worker/src/worker/handlers/resolver_integration.rs` (new file)

This module provides trait bridge adapters:

```rust
/// Adapter: StepHandler → ResolvedHandler
pub struct StepHandlerAsResolved {
    inner: Arc<dyn StepHandler>,
    supported_methods: Vec<String>,
}

/// Adapter: ResolvedHandler → StepHandler (executable)
pub struct ExecutableHandler {
    handler: Arc<dyn ResolvedHandler>,
    dispatch_wrapper: Option<MethodDispatchWrapper>,
    executor: Arc<dyn HandlerExecutor>,
}

/// StepHandlerRegistry implementation using ResolverChain
pub struct ResolverChainRegistry {
    chain: Arc<ResolverChain>,
    default_executor: Arc<dyn HandlerExecutor>,
}

/// Hybrid registry for gradual migration
pub struct HybridStepHandlerRegistry<L: StepHandlerRegistry> {
    resolver_registry: ResolverChainRegistry,
    legacy_registry: Arc<L>,
}
```

**Unit Tests** (`tasker-worker/src/worker/handlers/resolver_integration.rs`):

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_step_handler_as_resolved() { ... }

    #[test]
    fn test_executable_handler_from_step_handler() { ... }

    #[test]
    fn test_resolver_chain_registry_handler_available() { ... }

    #[test]
    fn test_hybrid_registry_handler_available() { ... }

    #[test]
    fn test_hybrid_registry_registered_handlers() { ... }
}
```

### 3.2 Integrate ResolverChain into HandlerDispatchService (REVISED)

**Location**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

**NO CHANGES REQUIRED** - The dispatch service continues to use `StepHandlerRegistry` trait.
The new `ResolverChainRegistry` implements this trait, providing resolver chain functionality
without modifying the dispatch service.

```rust
pub struct HandlerDispatchService {
    resolver_chain: Arc<ResolverChain>,
    // ... existing fields
}

impl HandlerDispatchService {
    pub fn new(resolver_chain: Arc<ResolverChain>, ...) -> Self { ... }

    async fn resolve_handler(&self, step: &TaskSequenceStep) -> TaskerResult<Arc<dyn StepHandler>> {
        let definition = step.handler_definition()?;
        let config = self.build_config(step);

        self.resolver_chain.resolve(&definition, &config).await
            .map_err(|e| TaskerError::ResolutionError(e.to_string()))
    }
}
```

**Unit Tests** (`tasker-worker/src/worker/handlers/dispatch_service.rs`):

```rust
#[cfg(test)]
mod dispatch_service_resolver_tests {
    #[tokio::test]
    async fn test_dispatch_uses_resolver_chain() { ... }

    #[tokio::test]
    async fn test_dispatch_applies_method_wrapper() { ... }

    #[tokio::test]
    async fn test_dispatch_error_on_unresolvable() { ... }
}
```

### 3.2 Update RustStepHandlerRegistry to use ExplicitMappingResolver

**Location**: `workers/rust/src/step_handlers/registry.rs`

Refactor to use the new resolver pattern:

```rust
impl RustStepHandlerRegistry {
    pub fn new() -> Self {
        let explicit_resolver = Arc::new(ExplicitMappingResolver::new());

        // Register all handlers
        explicit_resolver.register("linear_step_1", |config| {
            Arc::new(LinearStep1Handler::new(config.clone()))
        });
        // ... remaining handlers

        let mut chain = ResolverChain::new();
        chain.register(explicit_resolver);

        Self { chain }
    }
}
```

**Unit Tests** (update existing tests in `workers/rust/src/step_handlers/registry.rs`):

```rust
#[cfg(test)]
mod registry_resolver_tests {
    #[tokio::test]
    async fn test_registry_resolves_via_chain() { ... }

    #[test]
    fn test_registry_uses_explicit_mapping() { ... }

    #[tokio::test]
    async fn test_registry_backward_compatible() { ... }
}
```

### Phase 3 Validation Gate

| Criterion | Validation |
|-----------|------------|
| DispatchService uses resolver | `cargo test --package tasker-worker dispatch` |
| RustRegistry uses resolver | `cargo test --package tasker-worker-rust registry` |
| All worker tests pass | `cargo test --package tasker-worker --all-features` |
| All rust worker tests pass | `cargo test --package tasker-worker-rust --all-features` |

**Gate Command**:
```bash
cargo test --package tasker-worker --all-features && \
cargo test --package tasker-worker-rust --all-features
```

---

## Phase 4: FFI Language Workers

**Estimated Effort**: 3-4 days

### 4.1 Ruby Worker Integration

**Location**: `workers/ruby/lib/tasker_core/`

Create resolver infrastructure:

```
workers/ruby/lib/tasker_core/registry/
├── step_handler_resolver.rb       # Base class
├── resolver_chain.rb              # Chain implementation
├── resolvers/
│   ├── explicit_mapping_resolver.rb
│   └── class_constant_resolver.rb
└── method_dispatch_wrapper.rb
```

**Ruby Unit Tests** (`workers/ruby/spec/registry/`):

```ruby
# spec/registry/resolver_chain_spec.rb
RSpec.describe TaskerCore::Registry::ResolverChain do
  describe '#resolve' do
    it 'tries resolvers in priority order' do ... end
    it 'returns first successful resolution' do ... end
    it 'uses resolver hint when provided' do ... end
    it 'raises ResolutionError when no resolver succeeds' do ... end
  end
end

# spec/registry/explicit_mapping_resolver_spec.rb
RSpec.describe TaskerCore::Registry::ExplicitMappingResolver do
  describe '#can_resolve?' do
    it 'returns true for registered keys' do ... end
    it 'returns false for unregistered keys' do ... end
  end

  describe '#resolve' do
    it 'returns registered handler instance' do ... end
    it 'calls factory with config' do ... end
  end
end

# spec/registry/class_constant_resolver_spec.rb
RSpec.describe TaskerCore::Registry::ClassConstantResolver do
  describe '#can_resolve?' do
    it 'returns true for class-like callables' do ... end
    it 'returns false for simple string keys' do ... end
  end

  describe '#resolve' do
    it 'instantiates class from callable string' do ... end
    it 'returns nil for undefined classes' do ... end
  end
end

# spec/registry/method_dispatch_wrapper_spec.rb
RSpec.describe TaskerCore::Registry::MethodDispatchWrapper do
  it 'invokes specified method instead of call' do ... end
  it 'preserves handler identity' do ... end
end
```

### 4.2 Python Worker Integration

**Location**: `workers/python/python/tasker_core/`

Create resolver infrastructure:

```
workers/python/python/tasker_core/registry/
├── __init__.py
├── resolver.py                    # Base class
├── resolver_chain.py              # Chain implementation
├── resolvers/
│   ├── __init__.py
│   ├── explicit_mapping.py
│   └── class_lookup.py
└── method_dispatch_wrapper.py
```

**Python Unit Tests** (`workers/python/tests/`):

```python
# tests/test_resolver_chain.py
class TestResolverChain:
    def test_tries_resolvers_in_priority_order(self): ...
    def test_returns_first_successful_resolution(self): ...
    def test_uses_resolver_hint_when_provided(self): ...
    def test_raises_resolution_error_when_no_resolver_succeeds(self): ...

# tests/test_explicit_mapping_resolver.py
class TestExplicitMappingResolver:
    def test_can_resolve_registered_key(self): ...
    def test_cannot_resolve_unregistered_key(self): ...
    def test_resolve_returns_registered_handler(self): ...

# tests/test_method_dispatch_wrapper.py
class TestMethodDispatchWrapper:
    def test_invokes_specified_method(self): ...
    def test_preserves_handler_identity(self): ...
```

### 4.3 TypeScript Worker Integration

**Location**: `workers/typescript/src/`

Create resolver infrastructure:

```
workers/typescript/src/registry/
├── index.ts
├── resolver.ts                    # Interface
├── resolver-chain.ts              # Chain implementation
├── resolvers/
│   ├── index.ts
│   ├── explicit-mapping.ts
│   └── class-lookup.ts
└── method-dispatch-wrapper.ts
```

**TypeScript Unit Tests** (`workers/typescript/tests/`):

```typescript
// tests/resolver-chain.test.ts
describe('ResolverChain', () => {
  it('tries resolvers in priority order', async () => { ... });
  it('returns first successful resolution', async () => { ... });
  it('uses resolver hint when provided', async () => { ... });
  it('throws ResolutionError when no resolver succeeds', async () => { ... });
});

// tests/explicit-mapping-resolver.test.ts
describe('ExplicitMappingResolver', () => {
  it('canResolve returns true for registered keys', () => { ... });
  it('canResolve returns false for unregistered keys', () => { ... });
  it('resolve returns registered handler', async () => { ... });
});
```

### Phase 4 Validation Gate

| Criterion | Validation |
|-----------|------------|
| Ruby resolver tests pass | `cd workers/ruby && bundle exec rspec spec/registry/` |
| Python resolver tests pass | `cd workers/python && pytest tests/test_resolver*.py` |
| TypeScript resolver tests pass | `cd workers/typescript && bun test resolver` |
| FFI integration works | Run integration tests for each language |

**Gate Command**:
```bash
cd workers/ruby && bundle exec rspec spec/registry/ && \
cd ../python && pytest tests/test_resolver*.py && \
cd ../typescript && bun test resolver
```

---

## Phase 5: E2E Tests

**Estimated Effort**: 2 days

### 5.1 New E2E Test Fixtures

Create test fixtures that exercise resolver patterns:

**Location**: `tests/fixtures/task_templates/`

```yaml
# tests/fixtures/task_templates/rust/resolver_method_dispatch.yaml
name: resolver_method_dispatch_test
namespace_name: resolver_tests
version: 1.0.0
description: Tests method dispatch via resolver pattern

task_handler:
  callable: resolver_tests
  initialization: {}

steps:
  - name: validate_with_method
    description: Tests explicit method dispatch
    handler:
      callable: validation_handler
      method: validate_input  # NEW: Method dispatch
      initialization: {}
    dependencies: []

  - name: process_with_method
    description: Tests different method on same handler class
    handler:
      callable: validation_handler
      method: process_result  # Different method, same handler
      initialization: {}
    dependencies: [validate_with_method]

# tests/fixtures/task_templates/rust/resolver_explicit_mapping.yaml
name: resolver_explicit_mapping_test
namespace_name: resolver_tests
version: 1.0.0
description: Tests explicit mapping resolver hint

steps:
  - name: mapped_handler_step
    description: Uses explicit resolver hint
    handler:
      callable: my_custom_key  # Not a class path
      resolver: explicit_mapping  # NEW: Resolver hint
      initialization: {}
    dependencies: []
```

**Similar fixtures for**:
- `tests/fixtures/task_templates/ruby/resolver_*.yaml`
- `tests/fixtures/task_templates/python/resolver_*.yaml`
- `tests/fixtures/task_templates/typescript/resolver_*.yaml`

### 5.2 E2E Test Implementation

**Location**: `tests/e2e/rust/resolver_tests.rs` (new file)

```rust
//! E2E tests for step handler resolver patterns

use crate::common::{IntegrationTestManager, IntegrationTestUtils};

#[tokio::test]
async fn test_resolver_method_dispatch() {
    let manager = IntegrationTestManager::setup().await.unwrap();

    let request = IntegrationTestUtils::create_task_request(
        "resolver_method_dispatch_test",
        "resolver_tests",
        serde_json::json!({"input": "test_data"}),
    );

    let task = manager.create_task(request).await.unwrap();
    let result = IntegrationTestUtils::wait_for_task_completion(&manager, task.id).await;

    assert!(result.is_ok());
    // Verify both methods were invoked correctly
}

#[tokio::test]
async fn test_resolver_explicit_mapping() {
    let manager = IntegrationTestManager::setup().await.unwrap();

    let request = IntegrationTestUtils::create_task_request(
        "resolver_explicit_mapping_test",
        "resolver_tests",
        serde_json::json!({}),
    );

    let task = manager.create_task(request).await.unwrap();
    let result = IntegrationTestUtils::wait_for_task_completion(&manager, task.id).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_resolver_fallback_chain() {
    // Test that chain falls back when first resolver cannot resolve
}

#[tokio::test]
async fn test_resolver_error_includes_tried_resolvers() {
    // Test error message includes which resolvers were attempted
}

#[tokio::test]
async fn test_backward_compatible_templates() {
    // Ensure existing templates (no method/resolver fields) still work
    let manager = IntegrationTestManager::setup().await.unwrap();

    // Use existing linear_workflow template
    let request = IntegrationTestUtils::create_task_request(
        "linear_workflow_handler",
        "linear_workflow",
        serde_json::json!({}),
    );

    let task = manager.create_task(request).await.unwrap();
    let result = IntegrationTestUtils::wait_for_task_completion(&manager, task.id).await;

    assert!(result.is_ok());
}
```

**Similar E2E tests for**:
- `tests/e2e/ruby/resolver_tests.rs`
- `tests/e2e/python/resolver_tests.rs`
- `tests/e2e/typescript/resolver_tests.rs`

### 5.3 Handler Implementations for E2E Tests

**Location**: `workers/rust/src/step_handlers/resolver_tests.rs` (new file)

```rust
//! Test handlers for resolver E2E tests

pub struct MultiMethodHandler {
    config: StepHandlerConfig,
}

impl MultiMethodHandler {
    pub fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    pub async fn validate_input(&self, step: &TaskSequenceStep) -> anyhow::Result<StepExecutionResult> {
        // Validation logic
    }

    pub async fn process_result(&self, step: &TaskSequenceStep) -> anyhow::Result<StepExecutionResult> {
        // Processing logic
    }
}

impl RustStepHandler for MultiMethodHandler {
    fn name(&self) -> &str { "validation_handler" }

    async fn call(&self, step: &TaskSequenceStep) -> anyhow::Result<StepExecutionResult> {
        // Default call method
    }

    // NEW: Method dispatch support
    async fn invoke_method(&self, method: &str, step: &TaskSequenceStep) -> anyhow::Result<StepExecutionResult> {
        match method {
            "validate_input" => self.validate_input(step).await,
            "process_result" => self.process_result(step).await,
            "call" | _ => self.call(step).await,
        }
    }
}
```

### Phase 5 Validation Gate

| Criterion | Validation |
|-----------|------------|
| Resolver E2E fixtures valid | YAML syntax check |
| Rust E2E tests pass | `cargo test --test resolver_tests` |
| Ruby E2E tests pass | Integration with Ruby worker |
| Python E2E tests pass | Integration with Python worker |
| TypeScript E2E tests pass | Integration with TypeScript worker |
| Backward compatibility verified | Existing E2E tests still pass |

**Gate Command**:
```bash
# Run all E2E tests including new resolver tests
DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test" \
cargo nextest run --profile default -E 'test(/resolver/) | test(/e2e/)'
```

---

## Phase 6: Documentation and Code Review

**Estimated Effort**: 1-2 days

### 6.1 Documentation Updates

| Document | Updates |
|----------|---------|
| `docs/guides/handler-resolution.md` | NEW: Complete guide with mental model |
| `docs/architecture/worker-event-systems.md` | Update dispatch flow diagrams |
| `docs/development/development-patterns.md` | Add resolver extension patterns |
| `docs/reference/api-convergence-matrix.md` | Add resolver API alignment |

### 6.2 Handler Resolution Guide Structure

```markdown
# Handler Resolution Guide

## Mental Model
- Address (callable) = URL-like identifier
- Entry Point (method) = Which method to invoke
- Resolution Hint (resolver) = Bypass chain

## Built-in Resolvers
- ExplicitMappingResolver (priority 10)
- ClassConstantResolver (priority 100)

## Writing Custom Resolvers
- Implement StepHandlerResolver trait
- Register with ResolverChain
- Define your callable format

## Examples
- Method dispatch patterns
- Explicit mapping patterns
- Custom domain resolvers

## Cross-Language Considerations
- Rust: Compile-time, explicit mapping only
- Ruby: Runtime class lookup supported
- Python: Module import lookup supported
- TypeScript: Dynamic import lookup supported
```

### 6.3 Code Review Checklist

**Architecture Review**:
- [ ] Clean separation of concerns (resolver vs framework)
- [ ] No circular dependencies introduced
- [ ] Proper error handling with context
- [ ] Logging with correlation IDs

**API Review**:
- [ ] Consistent naming across languages
- [ ] Backward compatible schema changes
- [ ] Clear extension points for custom resolvers

**Testing Review**:
- [ ] Unit test coverage for all new code
- [ ] E2E tests cover key scenarios
- [ ] Edge cases handled (empty chain, no match, etc.)

**Documentation Review**:
- [ ] Mental model clearly explained
- [ ] Examples for each language
- [ ] Migration guide if needed

### Phase 6 Validation Gate

| Criterion | Validation |
|-----------|------------|
| Documentation complete | All listed docs exist and are accurate |
| No lint warnings | `cargo clippy --all-targets --all-features` |
| All tests pass | `cargo nextest run --all-features` |
| Code review approved | PR review passed |

**Final Gate Command**:
```bash
# Full validation suite
cargo make check && \
cargo make test && \
cargo doc --all-features --no-deps
```

---

## Summary Timeline

| Phase | Description | Est. Days | Cumulative |
|-------|-------------|-----------|------------|
| 1 | Core Types & Resolver Interface | 3-4 | 3-4 |
| 2 | Built-in Resolvers | 1-2 | 4-6 |
| 3 | Worker Integration | 2 | 6-8 |
| 4 | FFI Language Workers | 3-4 | 9-12 |
| 5 | E2E Tests | 2 | 11-14 |
| 6 | Documentation & Review | 1-2 | 12-16 |

**Total Estimated Effort**: 12-16 days

---

## Risk Mitigation

### Risk: Breaking Existing Templates
**Mitigation**:
- All new fields are optional with sensible defaults
- Run full E2E suite before each phase completion
- Explicit backward compatibility test in Phase 5

### Risk: Cross-Language Drift
**Mitigation**:
- Define interface in Rust first (source of truth)
- API convergence matrix updated in Phase 6
- Cross-language E2E tests validate behavior parity

### Risk: Performance Regression
**Mitigation**:
- ResolverChain is O(n) where n is small (2-5 resolvers)
- ExplicitMappingResolver is O(1) hash lookup
- Benchmark before/after if concerns arise

---

## Appendix: File Change Summary

### New Files

```
tasker-shared/src/registry/
├── mod.rs (update)
├── step_handler_resolver.rs (new)
├── resolver_chain.rs (new)
├── method_dispatch_wrapper.rs (new)
└── resolvers/
    ├── mod.rs (new)
    ├── explicit_mapping.rs (new)
    └── class_constant.rs (new)

workers/ruby/lib/tasker_core/registry/
├── step_handler_resolver.rb (new)
├── resolver_chain.rb (new)
├── method_dispatch_wrapper.rb (new)
└── resolvers/ (new directory)

workers/python/python/tasker_core/registry/ (new directory)

workers/typescript/src/registry/ (new directory)

tests/e2e/rust/resolver_tests.rs (new)
tests/fixtures/task_templates/*/resolver_*.yaml (new)

docs/guides/handler-resolution.md (new)
```

### Modified Files

```
tasker-shared/src/models/core/task_template/mod.rs
tasker-worker/src/worker/handlers/dispatch_service.rs
workers/rust/src/step_handlers/registry.rs
workers/rust/src/step_handlers/mod.rs
workers/typescript/src-rust/dto.rs (HandlerDefinitionDto)
docs/architecture/worker-event-systems.md
docs/reference/api-convergence-matrix.md
```
