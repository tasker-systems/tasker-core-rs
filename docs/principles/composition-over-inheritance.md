# Composition Over Inheritance

**Last Updated**: 2026-01-01
**Related Tickets**: TAS-112 (Cross-Language Handler Harmonization)

This document describes Tasker Core's approach to handler composition using mixins and traits rather than class hierarchies.

## The Core Principle

```
Not: class Handler < API
But: class Handler < Base; include API, include Decision, include Batchable
```

Handlers gain capabilities by mixing in modules, not by inheriting from specialized base classes.

---

## Why Composition?

### The Problem with Inheritance

Deep inheritance hierarchies create problems:

```ruby
# BAD: Inheritance-based capabilities
class APIDecisionBatchableHandler < APIDecisionHandler < APIHandler < BaseHandler
  # Which methods came from where?
  # How do I override just one behavior?
  # What if I need Batchable but not API?
end
```

| Problem | Description |
|---------|-------------|
| Diamond problem | Multiple paths to same ancestor |
| Tight coupling | Can't change base without affecting all children |
| Inflexible | Can't pick-and-choose capabilities |
| Hard to test | Must test entire hierarchy |
| Opaque behavior | Method origin unclear |

### The Composition Solution

Mixins provide selective capabilities:

```ruby
# GOOD: Composition-based capabilities
class MyHandler < TaskerCore::StepHandler::Base
  include TaskerCore::StepHandler::APICapable
  include TaskerCore::StepHandler::DecisionCapable

  def call(context)
    # Has API methods (get, post, put, delete)
    # Has Decision methods (decision_success, decision_no_branches)
    # Does NOT have Batchable methods (didn't include it)
  end
end
```

| Benefit | Description |
|---------|-------------|
| Selective inclusion | Only the capabilities you need |
| Clear origin | Module name indicates where methods come from |
| Independent testing | Test each mixin in isolation |
| Flexible combination | Any combination of capabilities |
| Flat structure | No deep hierarchies to navigate |

---

## The TAS-112 Discovery

Analysis of Batchable handlers revealed they already used the composition pattern:

```ruby
# Batchable was the TARGET architecture all along
class BatchHandler < Base
  include BatchableCapable  # Already doing it right!

  def call(context)
    batch_ctx = get_batch_context(context)
    # ...process batch...
    batch_worker_complete(processed_count: count, result_data: data)
  end
end
```

TAS-112 recommended migrating API and Decision handlers to match this pattern.

---

## Capability Modules

### Available Capabilities

| Capability | Module (Ruby) | Methods Provided |
|------------|---------------|------------------|
| API | `APICapable` | `get`, `post`, `put`, `delete` |
| Decision | `DecisionCapable` | `decision_success`, `decision_no_branches` |
| Batchable | `BatchableCapable` | `get_batch_context`, `batch_worker_complete`, `handle_no_op_worker` |

### Rust Traits

```rust
// Rust uses traits for the same pattern
pub trait APICapable {
    async fn get(&self, path: &str, params: Option<Params>) -> Response;
    async fn post(&self, path: &str, data: Option<Value>) -> Response;
    async fn put(&self, path: &str, data: Option<Value>) -> Response;
    async fn delete(&self, path: &str, params: Option<Params>) -> Response;
}

pub trait DecisionCapable {
    fn decision_success(&self, steps: Vec<String>, result: Value) -> StepExecutionResult;
    fn decision_no_branches(&self, result: Value) -> StepExecutionResult;
}

pub trait BatchableCapable {
    fn get_batch_context(&self, context: &StepContext) -> BatchContext;
    fn batch_worker_complete(&self, count: usize, data: Value) -> StepExecutionResult;
}
```

### Python Mixins

```python
# Python uses multiple inheritance (mixins)
from tasker_core.step_handler import StepHandler
from tasker_core.step_handler.mixins import APIMixin, DecisionMixin

class MyHandler(StepHandler, APIMixin, DecisionMixin):
    def call(self, context: StepContext) -> StepHandlerResult:
        # Has both API and Decision methods
        response = self.get("/api/endpoint")
        return self.decision_success(["next_step"], response)
```

### TypeScript Mixins (TAS-112)

```typescript
// TypeScript uses mixin functions applied in constructor
import { StepHandler } from 'tasker-worker-ts';
import { applyAPI, applyDecision, APICapable, DecisionCapable } from 'tasker-worker-ts';

class MyHandler extends StepHandler implements APICapable, DecisionCapable {
  constructor() {
    super();
    applyAPI(this);       // Adds get/post/put/delete methods
    applyDecision(this);  // Adds decisionSuccess/skipBranches methods
  }

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Has both API and Decision methods
    const response = await this.get('/api/endpoint');
    return this.decisionSuccess(['next_step'], response.body);
  }
}
```

---

## Separation of Concerns

### What Orchestration Owns

The orchestration layer handles:
- Domain event publishing (after results committed)
- Decision point step creation (from DecisionPointOutcome)
- Batch worker creation (from BatchProcessingOutcome)
- State machine transitions

### What Workers Own

Workers handle:
- Decision logic (returns `DecisionPointOutcome`)
- Batch analysis (returns `BatchProcessingOutcome`)
- Handler execution (returns `StepHandlerResult`)
- Custom publishers/subscribers (fast path events)

### The Boundary

```
┌─────────────────────────────────────────────────────────────────┐
│                        Worker Space                              │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ Handler.call(context)                                       ││
│  │   - Executes business logic                                 ││
│  │   - Uses API/Decision/Batchable capabilities               ││
│  │   - Returns StepHandlerResult with outcome                  ││
│  └─────────────────────────────────────────────────────────────┘│
│                              ↓ Result (with outcome)             │
├─────────────────────────────────────────────────────────────────┤
│                    Orchestration Space                           │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ Process result                                              ││
│  │   - Commit state transition                                 ││
│  │   - If DecisionPointOutcome: create decision steps          ││
│  │   - If BatchProcessingOutcome: create batch workers         ││
│  │   - Publish domain events                                   ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

---

## FFI Boundary Types

Outcomes crossing the FFI boundary need explicit types:

### DecisionPointOutcome

```rust
// Rust definition
pub enum DecisionPointOutcome {
    ActivateSteps { step_names: Vec<String> },
    NoBranches,
}

// Serialized (all languages)
{
    "type": "ActivateSteps",
    "step_names": ["branch_a", "branch_b"]
}
```

### BatchProcessingOutcome

```rust
// Rust definition
pub enum BatchProcessingOutcome {
    Continue { cursor: CursorConfig },
    Complete,
    NoOp,
}

// Serialized (all languages)
{
    "type": "Continue",
    "cursor": {
        "position": "offset_123",
        "batch_size": 100
    }
}
```

---

## Migration Path (TAS-112)

### Cross-Language Migration Examples

#### Ruby

Before (inheritance):
```ruby
class MyAPIHandler < TaskerCore::APIHandler
  def call(context)
    # ...
  end
end
```

After (composition):
```ruby
class MyAPIHandler < TaskerCore::StepHandler::Base
  include TaskerCore::StepHandler::Mixins::API

  def call(context)
    # Same implementation, different structure
  end
end
```

#### Python

Before (inheritance):
```python
class MyAPIHandler(APIHandler):
    def call(self, context):
        # ...
```

After (composition):
```python
from tasker_core.step_handler import StepHandler
from tasker_core.step_handler.mixins import APIMixin

class MyAPIHandler(StepHandler, APIMixin):
    def call(self, context):
        # Same implementation, different structure
```

#### TypeScript

Before (inheritance):
```typescript
class MyAPIHandler extends APIHandler {
  async call(context: StepContext): Promise<StepHandlerResult> {
    // ...
  }
}
```

After (composition):
```typescript
import { StepHandler } from 'tasker-worker-ts';
import { applyAPI, APICapable } from 'tasker-worker-ts';

class MyAPIHandler extends StepHandler implements APICapable {
  constructor() {
    super();
    applyAPI(this);
  }

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Same implementation, different structure
  }
}
```

#### Rust

Rust already used the composition pattern via traits:
```rust
// Rust has always used traits (composition)
impl StepHandler for MyHandler { ... }
impl APICapable for MyHandler { ... }
impl DecisionCapable for MyHandler { ... }
```

### Breaking Changes Implemented (TAS-112)

The migration to composition involved breaking changes:
1. Base class changes across all languages
2. Module/mixin includes required
3. Ruby cursor indexing changed from 1-indexed to 0-indexed

All breaking changes were accumulated and released together in TAS-112.

---

## Anti-Patterns

### Don't: Inherit from Multiple Specialized Classes

```ruby
# BAD: Ruby doesn't support multiple inheritance like this
class MyHandler < APIHandler, DecisionHandler  # Syntax error!
```

### Don't: Reimplement Mixin Methods

```ruby
# BAD: Overriding mixin methods defeats the purpose
class MyHandler < Base
  include APICapable

  def get(path, params: {})
    # Custom implementation - now you own this forever
  end
end
```

### Don't: Mix Concerns

```ruby
# BAD: Handler doing orchestration's job
class MyHandler < Base
  include DecisionCapable

  def call(context)
    # Don't create steps directly!
    create_workflow_step("next_step")  # Orchestration does this!

    # Do return the outcome
    decision_success(steps: ["next_step"], result_data: {})
  end
end
```

---

## Testing Composition

### Test Mixins in Isolation

```ruby
# Test the mixin itself
RSpec.describe TaskerCore::StepHandler::APICapable do
  let(:handler) { Class.new { include TaskerCore::StepHandler::APICapable }.new }

  it "provides get method" do
    expect(handler).to respond_to(:get)
  end
end
```

### Test Handler with Stubs

```ruby
# Test handler behavior, stub mixin methods
RSpec.describe MyHandler do
  let(:handler) { MyHandler.new }

  it "calls API and makes decision" do
    allow(handler).to receive(:get).and_return({ status: 200 })

    result = handler.call(context)

    expect(result.decision_point_outcome.type).to eq("ActivateSteps")
  end
end
```

---

## Related Documentation

- [Tasker Core Tenets](./tasker-core-tenets.md) - Tenet #3: Composition Over Inheritance
- [Cross-Language Consistency](./cross-language-consistency.md) - How composition works across languages
- [Patterns and Practices](../worker-crates/patterns-and-practices.md) - Handler patterns
- [TAS-112 Specification](../ticket-specs/TAS-112/) - Original analysis
