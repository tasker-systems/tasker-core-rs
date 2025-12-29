# ADR: Handler Composition Pattern

**Status**: Accepted
**Date**: 2025-12
**Ticket**: [TAS-112](https://linear.app/tasker-systems/issue/TAS-112)

## Context

Cross-language step handler ergonomics research (TAS-112) revealed an architectural inconsistency:

- **Batchable handlers**: Already use **composition via mixins** (target pattern)
- **API handlers**: Use inheritance (subclass pattern)
- **Decision handlers**: Use inheritance (subclass pattern)

```
Current State:
✅ Batchable: class Handler(StepHandler, Batchable)  # Composition
❌ API: class Handler < APIHandler                    # Inheritance
❌ Decision: class Handler extends DecisionHandler    # Inheritance
```

**Guiding Principle** (Zen of Python): *"There should be one-- and preferably only one --obvious way to do it."*

## Decision

**Migrate all handler patterns to composition (mixins/traits)**, using batchable as the reference implementation.

**Target Architecture**:
```
All patterns use composition:
  Ruby:      include Base, include API, include Decision, include Batchable
  Python:    class Handler(StepHandler, API, Decision, Batchable)
  TypeScript: interface composition + mixins
  Rust:      trait composition (impl Base + API + Decision + Batchable)
```

**Benefits**:
- Single responsibility - each mixin handles one concern
- Flexible composition - handlers can mix capabilities as needed
- Easier testing - can test each capability independently
- Matches batchable pattern (already proven successful)

**Example Migration**:
```ruby
# Old pattern (deprecated)
class MyHandler < TaskerCore::StepHandler::API
  def call(context)
    api_success(data)
  end
end

# New pattern
class MyHandler < TaskerCore::StepHandler::Base
  include TaskerCore::StepHandler::Mixins::API

  def call(context)
    api_success(data)
  end
end
```

## Consequences

### Positive

- **Consistent architecture**: One pattern for all handler capabilities
- **Composable capabilities**: Mix API + Decision + Batchable as needed
- **Testable in isolation**: Each mixin can be tested independently
- **Matches proven pattern**: Batchable already validates approach
- **Cross-language alignment**: Same mental model in all languages

### Negative

- **Breaking change**: All existing handlers need migration
- **Learning curve**: Contributors must understand mixin pattern
- **Migration effort**: All examples and documentation need updates

### Neutral

- Pre-alpha status means breaking changes are acceptable
- Migration can be phased with deprecation warnings

## Related Decisions

### Ruby Result Unification

Ruby uses separate `Success`/`Error` classes while Python/TypeScript use unified result with `success` flag. Recommend unifying Ruby to match.

### Rust Handler Traits

Rust needs ergonomic traits for API, Decision, and Batchable capabilities to match other languages:

```rust
pub trait APICapable {
    fn api_success(&self, data: Value, status: u16) -> StepExecutionResult;
    fn api_failure(&self, message: &str, status: u16) -> StepExecutionResult;
}

pub trait DecisionCapable {
    fn decision_success(&self, step_names: Vec<String>) -> StepExecutionResult;
    fn skip_branches(&self, reason: &str) -> StepExecutionResult;
}
```

### FFI Boundary Types

Data structures crossing FFI boundaries must have identical serialization. Create explicit type mirrors in all languages:
- `DecisionPointOutcome`
- `BatchProcessingOutcome`
- `CursorConfig`

## Alternatives Considered

### Alternative 1: Keep Inheritance Pattern

Continue with subclass pattern for API and Decision.

**Rejected**: Inconsistent with batchable; makes multi-capability handlers awkward.

### Alternative 2: Migrate Batchable to Inheritance

Make batchable use inheritance to match others.

**Rejected**: Batchable composition is the better pattern; others should follow it.

### Alternative 3: Language-Specific Patterns

Let each language use its idiomatic pattern.

**Rejected**: Violates cross-language consistency principle; increases cognitive load.

## References

- [TAS-112 Specification](../ticket-specs/TAS-112/recommendations.md) - Full research synthesis
- [Composition Over Inheritance](../principles/composition-over-inheritance.md) - Principle documentation
- [Cross-Language Consistency](../principles/cross-language-consistency.md) - API philosophy
- [API Convergence Matrix](../workers/api-convergence-matrix.md) - Cross-language API reference
