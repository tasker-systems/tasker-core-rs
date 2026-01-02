# TAS-93: Step Handler Router/Resolver Strategy Pattern

**Status**: Planning Complete
**Created**: 2024-12-30
**Author**: Pete Taylor / Claude Analysis Session
**Linear**: [TAS-93](https://linear.app/tasker-systems/issue/TAS-93)

---

## Executive Summary

Expose a developer-facing strategy pattern for step handler registration and resolution, enabling flexible mapping from task template `callable` definitions to runtime handler execution. This addresses the current tight coupling between callable strings and class-instantiation-with-call-method assumptions while preserving Tasker's core value proposition of bounded, idempotent steps.

---

## Problem Statement

### Current Constraint

Today, the task template's `handler.callable` field enforces an implicit contract:

```yaml
handler:
  callable: "PaymentProcessing::ProcessPaymentHandler"
```

The framework assumes:
1. The callable is a fully-qualified class name
2. The class can be instantiated with no/optional arguments
3. The class has a `call(context)` method

### Where This Breaks Down

1. **Method Routing**: Developers cannot route to specific methods on a class
2. **Factory Patterns**: No support for parameterized handler instantiation based on runtime config
3. **Language Mismatches**: Rust's compiled nature already requires explicit mapping (see `workers/rust/src/step_handlers/registry.rs` lines 232-334)
4. **Custom Resolution**: No extension point for domain-specific handler lookup strategies

### Existing Patterns Already Diverge

**Rust** (explicit mapping - already the target pattern):
```rust
match name {
    "linear_step_1" => Some(Arc::new(LinearStep1Handler::new(config))),
    "process_payment" => Some(Arc::new(ProcessPaymentHandler::new(config))),
    // 50+ explicit mappings
}
```

**Ruby/Python/TypeScript** (implicit class resolution):
```ruby
Object.const_get(callable).new
```

The Rust pattern is actually more controlled and explicit - we should enable similar flexibility in other languages.

---

## Core Conceptual Model

### The Three Concerns

The handler definition conflates three distinct concerns that must be cleanly separated:

| Concern | Field | Semantics | Who Owns It |
|---------|-------|-----------|-------------|
| **Address** | `callable` | The identifier resolvers use to find/create a handler | Resolver interprets |
| **Entry Point** | `method` | Which method to invoke on the resolved handler | Framework handles |
| **Resolution Strategy** | `resolver` | Which resolver should interpret the address | Framework routes |

### The Address Model (callable)

Think of `callable` like a URL or DNS name - it's an **address** that identifies a handler. Different resolvers interpret addresses differently:

| Address Format | Resolver | Interpretation |
|----------------|----------|----------------|
| `Module::ClassName` | ClassConstantResolver | Ruby constant lookup |
| `module.ClassName` | ClassLookupResolver | Python/TS import path |
| `my_handler_key` | ExplicitMappingResolver | Registered key lookup |
| `payments:stripe:refund` | YourCustomResolver | Your domain logic |

**The format of `callable` depends on which resolver interprets it** - just as URL format depends on protocol (http vs s3 vs ftp).

### Why Not Split namespace/class/method?

We considered a fully structured schema:

```yaml
# REJECTED - creates ambiguity
handler:
  namespace: "payments"
  class: "PaymentHandler"
  method: "validate"
```

**Problems with splitting:**
- Is the lookup key `"payments::PaymentHandler"`? `"payments.PaymentHandler"`? Just `"PaymentHandler"` within namespace `"payments"`?
- Resolver must know how to assemble the parts
- Different languages have different separator conventions

**Single `callable` is unambiguous:**
```yaml
# ACCEPTED - resolver receives exact string
handler:
  callable: "payments::PaymentHandler"
  method: "validate"
```

The resolver receives exactly this string. No assembly required, no format guessing.

### Framework vs Resolver Responsibilities

```
┌─────────────────────────────────────────────────────────────────────┐
│                         FRAMEWORK LAYER                              │
│                                                                      │
│  1. Route to resolver (using `resolver` hint or chain)              │
│  2. Wrap handler for method dispatch (using `method` field)         │
│  3. Invoke handler with context                                      │
│  4. Handle result/error                                              │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ definition.callable
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         RESOLVER LAYER                               │
│                                                                      │
│  1. Receive `callable` string as lookup key                         │
│  2. Interpret it according to resolver's own format/convention      │
│  3. Find or create handler instance                                  │
│  4. Return handler instance (or nil if can't resolve)               │
│                                                                      │
│  Resolvers DO NOT handle method dispatch - just return instances    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Design Decisions

### Decision 1: No Configuration Flags for Opt-In

**Decision**: The presence of custom resolver code is the expression of intention.

**Rationale**: Configuration flags create environment drift risk - if development/staging have flags enabled but production doesn't, CI/UAT would pass while production breaks. Committed code that went through PR review is intentional.

### Decision 2: Runtime Validation

**Decision**: Resolution failures are runtime errors, not static analysis/load-time validation.

**Rationale**: Ruby's `define_method` and runtime metaprogramming make static analysis impractical. While Python/TypeScript can leverage type hints for IDE support, enforcement must be runtime for cross-language consistency.

### Decision 3: Precedence-Based Fallback

**Decision**: If only a class name is provided (no method specified), default to `.call` method.

**Rationale**: Backward compatibility with existing templates. The resolution chain tries specific patterns first, falls back to `.call` convention.

### Decision 4: Single Address Field (callable)

**Decision**: Keep `callable` as a single string field. Do not split into `namespace`/`class`/`method` components.

**Rationale**: 
- Preserves the FQDN/DNS-like model where the string IS the address
- Avoids ambiguity about how components should be assembled
- Each resolver defines its own format expectations
- Simpler mental model for developers

### Decision 5: Framework Owns Method Dispatch

**Decision**: Resolvers return handler instances. The framework wraps for method dispatch.

**Rationale**:
- Resolvers have a single responsibility: address → instance
- Method dispatch is orthogonal to resolution strategy
- Simplifies custom resolver implementation
- Consistent behavior across all resolvers

---

## Proposed Architecture

### Handler Definition Schema

```yaml
handler:
  # THE ADDRESS (required)
  # The identifier the resolver uses to find/create a handler.
  # Format depends on which resolver interprets it.
  callable: "payments::order::PaymentHandler"
  
  # THE ENTRY POINT (optional, defaults to "call")
  # Which method to invoke on the resolved handler.
  # Framework handles this - resolvers don't need to.
  method: "validate"
  
  # THE RESOLUTION HINT (optional)
  # If set, skip the resolver chain and use this resolver directly.
  # If unset, the chain tries each resolver in priority order.
  resolver: "explicit_mapping"
  
  # INITIALIZATION (optional)
  # Passed to the resolver, which may use it for instantiation.
  initialization:
    timeout_ms: 30000
```

### Resolver Interface

The resolver interface is intentionally simple - resolvers only need to map `callable` to handler instances:

#### Rust

```rust
// tasker-shared/src/registry/step_handler_resolver.rs

/// Strategy trait for resolving callable addresses to handler instances.
/// 
/// Resolvers interpret the `callable` field as a lookup key and return
/// handler instances. The framework handles method dispatch wrapping.
#[async_trait]
pub trait StepHandlerResolver: Send + Sync {
    /// Resolve a callable address to a handler instance.
    /// 
    /// The `definition.callable` is YOUR lookup key - interpret it however
    /// makes sense for your resolver. Return the handler instance, or None
    /// if this resolver cannot handle the given callable.
    /// 
    /// You do NOT need to handle method dispatch - the framework wraps
    /// the returned handler if `definition.method` is set.
    async fn resolve(
        &self,
        definition: &HandlerDefinition,
        config: &StepHandlerConfig,
    ) -> Option<Arc<dyn StepHandler>>;
    
    /// Check if this resolver can handle the given callable format.
    /// 
    /// This is a quick check before attempting resolution. Return true
    /// if the callable format matches what this resolver expects.
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool;
    
    /// Resolver name for logging and metrics.
    fn resolver_name(&self) -> &str;
    
    /// Priority in the resolver chain (lower = checked first).
    fn priority(&self) -> u32 {
        100 // Default priority
    }
}
```

#### Ruby

```ruby
# workers/ruby/lib/tasker_core/registry/step_handler_resolver.rb

module TaskerCore
  module Registry
    # Base class for step handler resolvers.
    #
    # Resolvers interpret the `callable` field as a lookup key and return
    # handler instances. The framework handles method dispatch wrapping.
    class StepHandlerResolver
      # Resolve a callable address to a handler instance.
      #
      # The `definition.callable` is YOUR lookup key - interpret it however
      # makes sense for your resolver. Return the handler instance, or nil
      # if this resolver cannot handle the given callable.
      #
      # You do NOT need to handle method dispatch - the framework wraps
      # the returned handler if `definition.method` is set.
      #
      # @param definition [HandlerDefinition] The handler definition
      # @param config [Hash] Handler configuration
      # @return [Object, nil] Handler instance or nil
      def resolve(definition, config = {})
        raise NotImplementedError, "#{self.class} must implement #resolve"
      end
      
      # Check if this resolver can handle the given callable format.
      #
      # @param definition [HandlerDefinition] The handler definition
      # @return [Boolean]
      def can_resolve?(definition)
        raise NotImplementedError, "#{self.class} must implement #can_resolve?"
      end
      
      # Resolver name for logging.
      # @return [String]
      def resolver_name
        self.class.name
      end
      
      # Priority in the resolver chain (lower = checked first).
      # @return [Integer]
      def priority
        100
      end
    end
  end
end
```

#### Python

```python
# workers/python/python/tasker_core/registry/resolver.py

from abc import ABC, abstractmethod
from typing import Optional
from ..types import HandlerDefinition, StepHandler, StepHandlerConfig

class StepHandlerResolver(ABC):
    """Base class for step handler resolvers.
    
    Resolvers interpret the `callable` field as a lookup key and return
    handler instances. The framework handles method dispatch wrapping.
    """
    
    @abstractmethod
    def resolve(
        self,
        definition: HandlerDefinition,
        config: StepHandlerConfig,
    ) -> Optional[StepHandler]:
        """Resolve a callable address to a handler instance.
        
        The `definition.callable` is YOUR lookup key - interpret it however
        makes sense for your resolver. Return the handler instance, or None
        if this resolver cannot handle the given callable.
        
        You do NOT need to handle method dispatch - the framework wraps
        the returned handler if `definition.method` is set.
        """
        pass
    
    @abstractmethod
    def can_resolve(self, definition: HandlerDefinition) -> bool:
        """Check if this resolver can handle the given callable format."""
        pass
    
    @property
    def resolver_name(self) -> str:
        """Resolver name for logging."""
        return self.__class__.__name__
    
    @property
    def priority(self) -> int:
        """Priority in the resolver chain (lower = checked first)."""
        return 100
```

#### TypeScript

```typescript
// workers/typescript/src/handler/resolver.ts

import { HandlerDefinition, StepHandler, StepHandlerConfig } from '../types';

/**
 * Base interface for step handler resolvers.
 * 
 * Resolvers interpret the `callable` field as a lookup key and return
 * handler instances. The framework handles method dispatch wrapping.
 */
export interface StepHandlerResolver {
  /**
   * Resolve a callable address to a handler instance.
   * 
   * The `definition.callable` is YOUR lookup key - interpret it however
   * makes sense for your resolver. Return the handler instance, or null
   * if this resolver cannot handle the given callable.
   * 
   * You do NOT need to handle method dispatch - the framework wraps
   * the returned handler if `definition.method` is set.
   */
  resolve(
    definition: HandlerDefinition,
    config: StepHandlerConfig,
  ): Promise<StepHandler | null>;
  
  /**
   * Check if this resolver can handle the given callable format.
   */
  canResolve(definition: HandlerDefinition): boolean;
  
  /**
   * Resolver name for logging.
   */
  readonly resolverName: string;
  
  /**
   * Priority in the resolver chain (lower = checked first).
   */
  readonly priority: number;
}
```

### Resolver Chain

The framework's resolver chain handles routing and method dispatch:

```ruby
# Framework implementation (simplified)
class ResolverChain
  def resolve(definition, config)
    # 1. If resolver hint provided, use it directly
    if definition.resolver.present?
      resolver = find_resolver_by_name(definition.resolver)
      handler = resolver.resolve(definition, config)
      raise ResolutionError, "Resolver '#{definition.resolver}' could not resolve '#{definition.callable}'" unless handler
    else
      # 2. Otherwise, try resolvers in priority order
      handler = try_resolver_chain(definition, config)
    end
    
    # 3. Framework handles method dispatch wrapping
    if definition.uses_method_dispatch?
      handler = MethodDispatchWrapper.new(handler, definition.effective_method)
    end
    
    handler
  end
  
  private
  
  def try_resolver_chain(definition, config)
    tried = []
    
    sorted_resolvers.each do |resolver|
      next unless resolver.can_resolve?(definition)
      
      tried << resolver.resolver_name
      result = resolver.resolve(definition, config)
      
      if result
        log_resolution(definition, resolver, :success)
        return result
      end
    end
    
    raise ResolutionError.new(
      callable: definition.callable,
      tried_resolvers: tried,
      message: "No resolver could handle callable '#{definition.callable}'"
    )
  end
end
```

### Built-in Resolvers

#### 1. ClassConstantResolver (Priority 100 - Default)

Interprets `callable` as a fully-qualified class path:

```ruby
class ClassConstantResolver < StepHandlerResolver
  # Expects: "Module::ClassName" or "module.ClassName"
  
  def can_resolve?(definition)
    # Can always try as fallback - checks if it looks like a class path
    definition.callable.match?(/^[A-Z]/) || definition.callable.include?('::')
  end
  
  def resolve(definition, config)
    klass = Object.const_get(definition.callable)
    klass.new(config: config)
  rescue NameError => e
    logger.debug("ClassConstantResolver: #{definition.callable} not found")
    nil
  end
  
  def priority
    100
  end
end
```

#### 2. ExplicitMappingResolver (Priority 10)

Interprets `callable` as a registered key:

```ruby
class ExplicitMappingResolver < StepHandlerResolver
  # Expects: Any string that was registered
  
  def initialize
    @mappings = {}
  end
  
  def register(key, handler_factory)
    @mappings[key.to_s] = handler_factory
  end
  
  def can_resolve?(definition)
    @mappings.key?(definition.callable)
  end
  
  def resolve(definition, config)
    factory = @mappings[definition.callable]
    return nil unless factory
    
    factory.call(config)
  end
  
  def priority
    10  # Checked first
  end
end
```

### Custom Resolver Example

A developer building a domain-specific resolver:

```ruby
# Custom resolver for payment handlers with format "payments:provider:action"
class PaymentHandlerResolver < TaskerCore::Registry::StepHandlerResolver
  PATTERN = /^payments:(\w+):(\w+)$/
  
  def can_resolve?(definition)
    definition.callable.match?(PATTERN)
  end
  
  def resolve(definition, config)
    match = definition.callable.match(PATTERN)
    return nil unless match
    
    provider, action = match.captures
    
    # Your domain logic to find/create the handler
    handler_class = PaymentHandlers.for(provider: provider, action: action)
    handler_class.new(config: config)
  end
  
  def resolver_name
    "PaymentHandlerResolver"
  end
  
  def priority
    20  # After explicit mapping, before class constant
  end
end

# Registration
TaskerCore::Registry::ResolverChain.register_resolver(
  PaymentHandlerResolver.new
)
```

Usage in template:
```yaml
handler:
  callable: "payments:stripe:refund"
  method: "execute"
```

---

## Data Model Changes

### HandlerDefinition

```rust
/// Handler definition with address, entry point, and resolution hint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
pub struct HandlerDefinition {
    /// The address - a lookup key interpreted by resolvers.
    /// Format depends on which resolver handles it (like URL schemes).
    #[validate(length(min = 1, max = 500))]
    pub callable: String,

    /// The entry point - which method to invoke (defaults to "call").
    /// Framework handles this; resolvers just return instances.
    #[serde(default)]
    pub method: Option<String>,
    
    /// Resolution hint - bypass chain and use this resolver directly.
    #[serde(default)]
    pub resolver: Option<String>,

    /// Initialization parameters passed to resolver/handler.
    #[serde(default)]
    #[builder(default)]
    pub initialization: HashMap<String, Value>,
}

impl HandlerDefinition {
    /// Get the effective method name (defaults to "call").
    pub fn effective_method(&self) -> &str {
        self.method.as_deref().unwrap_or("call")
    }
    
    /// Check if this definition uses method dispatch.
    pub fn uses_method_dispatch(&self) -> bool {
        self.method.is_some() && self.method.as_deref() != Some("call")
    }
}
```

---

## Implementation Phases

### Phase 1: Foundation (Resolver Interface & Chain)

**Deliverables**:
1. Define `StepHandlerResolver` trait/interface in all languages
2. Implement `ResolverChain` with priority-ordered resolution
3. Implement `MethodDispatchWrapper` in framework layer
4. Migrate existing resolution logic to `ClassConstantResolver`
5. Add `method` and `resolver` fields to `HandlerDefinition`
6. Add resolution logging with correlation IDs

**Files to create/modify**:
- `tasker-shared/src/registry/step_handler_resolver.rs` (new)
- `tasker-shared/src/registry/resolver_chain.rs` (new)
- `tasker-shared/src/registry/method_dispatch_wrapper.rs` (new)
- `tasker-shared/src/registry/resolvers/class_constant.rs` (new)
- `tasker-shared/src/models/core/task_template/mod.rs` (extend HandlerDefinition)
- Corresponding files in Ruby/Python/TypeScript workers

**Breaking Changes**: None (pure refactoring with new optional fields)

**Estimated Effort**: 3-4 days

### Phase 2: Explicit Mapping Resolver

**Deliverables**:
1. Implement `ExplicitMappingResolver` in Ruby/Python/TypeScript
2. Align with existing Rust pattern
3. Add registration API across all languages
4. Document the address format expectations

**Breaking Changes**: None (additive)

**Estimated Effort**: 1-2 days

### Phase 3: Custom Resolver Extension Point

**Deliverables**:
1. Enable custom resolver registration in chain
2. Add priority-based ordering with custom resolvers
3. Document extension patterns with examples
4. Create example domain-specific resolver

**Breaking Changes**: None (additive)

**Estimated Effort**: 1-2 days

### Phase 4: Documentation & Examples

**Deliverables**:
1. Create `docs/guides/handler-resolution.md` with mental model
2. Update `docs/workers/patterns-and-practices.md`
3. Add resolver examples for each language
4. Update API convergence matrix
5. Add ADR for resolver pattern decision

**Estimated Effort**: 1 day

---

## Cross-Language Alignment Matrix

| Concept | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| Resolver trait | `StepHandlerResolver` | `StepHandlerResolver` | `StepHandlerResolver` | `StepHandlerResolver` |
| Chain | `ResolverChain` | `ResolverChain` | `ResolverChain` | `ResolverChain` |
| Method wrapper | `MethodDispatchWrapper` | `MethodDispatchWrapper` | `MethodDispatchWrapper` | `MethodDispatchWrapper` |
| Default resolver | `ExplicitMappingResolver`¹ | `ClassConstantResolver` | `ClassLookupResolver` | `ClassLookupResolver` |
| Registration | `register_resolver()` | `register_resolver()` | `register_resolver()` | `registerResolver()` |

¹ Rust already uses explicit mapping due to compiled nature.

---

## Testing Strategy

### Unit Tests

```ruby
RSpec.describe TaskerCore::Registry::ResolverChain do
  describe 'method dispatch wrapping' do
    it 'wraps handler when method is specified' do
      definition = HandlerDefinition.new(
        callable: 'OrderHandler',
        method: 'validate'
      )
      
      # Resolver returns raw handler
      handler = resolver_chain.resolve(definition, {})
      
      # Framework wrapped it - calling .call invokes .validate
      result = handler.call(mock_context)
      expect(result.result[:method_invoked]).to eq('validate')
    end
    
    it 'does not wrap when method is nil or "call"' do
      definition = HandlerDefinition.new(callable: 'OrderHandler')
      
      handler = resolver_chain.resolve(definition, {})
      
      # No wrapping - .call invokes .call
      expect(handler).to be_a(OrderHandler)
    end
  end
end

RSpec.describe 'Custom Resolver' do
  it 'receives callable as lookup key' do
    custom_resolver = Class.new(StepHandlerResolver) do
      def can_resolve?(definition)
        definition.callable.start_with?('custom:')
      end
      
      def resolve(definition, config)
        # We receive the full callable string
        expect(definition.callable).to eq('custom:my_handler')
        MockHandler.new
      end
    end.new
    
    ResolverChain.register_resolver(custom_resolver)
    
    definition = HandlerDefinition.new(callable: 'custom:my_handler')
    handler = ResolverChain.resolve(definition, {})
    
    expect(handler).to be_a(MockHandler)
  end
end
```

---

## Risk Analysis

### Risk 1: Address Format Confusion

**Risk**: Developers unsure what format to use for `callable`.

**Mitigation**:
- Clear documentation of built-in resolver expectations
- Error messages include "tried resolvers" list
- Examples showing format → resolver mapping

### Risk 2: God Handler Anti-Pattern

**Risk**: Method dispatch enables handlers with many methods.

**Mitigation**:
- Documentation emphasizing appropriate use (related operations)
- Linting warnings for handlers with >5 step methods
- Examples showing good vs. bad patterns

### Risk 3: Resolution Debugging

**Risk**: Complex resolution chains hard to debug.

**Mitigation**:
- Comprehensive logging with correlation IDs
- Each resolver logs can_resolve? result
- Error includes full resolution attempt trace

---

## Acceptance Criteria

### Phase 1 Complete When:
- [ ] `StepHandlerResolver` trait/interface exists in all 4 languages
- [ ] `ResolverChain` implements priority-ordered resolution
- [ ] `MethodDispatchWrapper` handles method field in framework layer
- [ ] Existing templates work unchanged (backward compatibility)
- [ ] Resolution logging includes resolver name and correlation ID
- [ ] `HandlerDefinition` supports `method` and `resolver` fields

### Phase 2 Complete When:
- [ ] `ExplicitMappingResolver` works in Ruby/Python/TypeScript
- [ ] Registration API is consistent across languages

### Phase 3 Complete When:
- [ ] Custom resolvers can be registered
- [ ] Priority ordering works with custom resolvers
- [ ] Example custom resolver documented

### Phase 4 Complete When:
- [ ] `docs/guides/handler-resolution.md` created with mental model
- [ ] Address format documentation clear
- [ ] API convergence matrix updated

---

## Example: Complete Use Case

### Template

```yaml
name: order_processing
namespace_name: fulfillment
version: "1.0.0"

steps:
  - name: validate_order
    handler:
      callable: "OrderWorkflowHandler"
      method: "validate"
    
  - name: process_payment
    handler:
      callable: "payments:stripe:charge"  # Custom resolver format
      method: "execute"
    dependencies: [validate_order]
    
  - name: fulfill_order
    handler:
      callable: "fulfill_order"  # Explicit mapping key
      resolver: "explicit_mapping"
    dependencies: [process_payment]
```

### Resolution Flow

```
Step: validate_order
  callable: "OrderWorkflowHandler"
  method: "validate"
  
  → ResolverChain tries:
    1. ExplicitMappingResolver: can_resolve? = false (not registered)
    2. ClassConstantResolver: can_resolve? = true (looks like class)
       → resolve() returns OrderWorkflowHandler instance
  → Framework wraps with MethodDispatchWrapper("validate")
  → Execution calls wrapper.call(context) → handler.validate(context)

Step: process_payment
  callable: "payments:stripe:charge"
  method: "execute"
  
  → ResolverChain tries:
    1. ExplicitMappingResolver: can_resolve? = false
    2. PaymentHandlerResolver: can_resolve? = true (matches pattern)
       → resolve() returns StripeChargeHandler instance
  → Framework wraps with MethodDispatchWrapper("execute")
  → Execution calls wrapper.call(context) → handler.execute(context)

Step: fulfill_order
  callable: "fulfill_order"
  resolver: "explicit_mapping"
  
  → Framework routes directly to ExplicitMappingResolver (hint provided)
  → resolve() returns registered FulfillOrderHandler instance
  → No method dispatch (defaults to "call")
  → Execution calls handler.call(context)
```

---

## Related Documentation

- [Tasker Core Tenets](../../principles/tasker-core-tenets.md)
- [Cross-Language Consistency](../../principles/cross-language-consistency.md)
- [Composition Over Inheritance](../../principles/composition-over-inheritance.md)
- [Worker Event Systems](../../architecture/worker-event-systems.md)

---

## Appendix A: HandlerDefinition JSON Schema

```yaml
type: object
required:
  - callable
properties:
  callable:
    type: string
    minLength: 1
    maxLength: 500
    description: |
      The address - a lookup key interpreted by resolvers.
      Format depends on which resolver handles it:
      - ClassConstantResolver: "Module::ClassName"
      - ExplicitMappingResolver: registered key string
      - Custom resolvers: your defined format
  
  method:
    type: string
    default: null
    description: |
      The entry point - which method to invoke on the resolved handler.
      Defaults to "call". Framework handles dispatch wrapping.
  
  resolver:
    type: string
    default: null
    description: |
      Resolution hint - bypass the chain and use this resolver directly.
      If unset, the chain tries resolvers in priority order.
  
  initialization:
    type: object
    additionalProperties: true
    default: {}
    description: |
      Initialization parameters passed to resolver/handler constructor.
```

---

## Appendix B: Resolution Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     STEP HANDLER RESOLUTION FLOW                         │
└─────────────────────────────────────────────────────────────────────────┘

Template YAML
    │
    │ Parse
    ▼
┌───────────────────┐
│ HandlerDefinition │
│  - callable       │  ← THE ADDRESS (resolver's lookup key)
│  - method?        │  ← THE ENTRY POINT (framework handles)
│  - resolver?      │  ← THE RESOLUTION HINT (framework routes)
│  - initialization │
└─────────┬─────────┘
          │
          │ Has resolver hint?
          │
    ┌─────┴─────┐
    │           │
   Yes          No
    │           │
    ▼           ▼
┌─────────┐  ┌──────────────────────────────────────────────┐
│ Direct  │  │              ResolverChain                   │
│ Lookup  │  │                                              │
│         │  │  Each resolver receives definition.callable  │
└────┬────┘  │  and interprets it according to its format   │
     │       │                                              │
     │       │  Priority 10: ExplicitMappingResolver        │
     │       │    Format: registered key strings            │
     │       │                                              │
     │       │  Priority 20-99: Custom Resolvers            │
     │       │    Format: your defined patterns             │
     │       │                                              │
     │       │  Priority 100: ClassConstantResolver         │
     │       │    Format: "Module::ClassName"               │
     │       │                                              │
     │       └──────────────────────────────────────────────┘
     │           │
     │           │ First successful resolve
     │           │
     └─────┬─────┘
           │
           ▼
    ┌─────────────┐
    │   Handler   │  ← Raw instance from resolver
    │  Instance   │
    └──────┬──────┘
           │
           │ definition.method set?
           │
     ┌─────┴─────┐
     │           │
    Yes          No
     │           │
     ▼           │
┌────────────┐   │
│MethodDispatch  │
│  Wrapper   │   │
└─────┬──────┘   │
      │          │
      └────┬─────┘
           │
           ▼
    ┌─────────────────┐
    │ .call(context)  │  → invokes .method(context) if wrapped
    └─────────────────┘
           │
           ▼
    ┌─────────────────┐
    │ StepHandlerResult│
    └─────────────────┘
```
