# Handler Resolution Guide

**Last Updated**: 2026-01-08
**Audience**: Developers, Architects
**Status**: Active (TAS-93 Complete)
**Related Docs**: [Worker Event Systems](../architecture/worker-event-systems.md) | [API Convergence Matrix](../workers/api-convergence-matrix.md)

<- Back to [Guides](README.md)

---

## Overview

Handler resolution is the process of converting a **callable address** (a string in your YAML template) into an **executable handler instance** that can process workflow steps. TAS-93 introduces a flexible, extensible resolver chain pattern that works consistently across all language workers.

This guide covers:
- The mental model for handler resolution
- The common path for task templates
- Built-in resolvers and how they work
- Method dispatch for multi-method handlers
- Writing custom resolvers
- Cross-language considerations

---

## Mental Model

Handler resolution uses three key concepts:

```yaml
handler:
  callable: "PaymentProcessor"      # 1. Address: WHERE to find the handler
  method: "refund"                  # 2. Entry Point: WHICH method to invoke
  resolver: "explicit_mapping"      # 3. Resolution Hint: HOW to resolve
```

### 1. Address (callable)

The `callable` field is a **logical address** that identifies the handler. Think of it like a URL - it points to where the handler lives, but the format depends on your resolution strategy:

| Format | Example | Resolver |
|--------|---------|----------|
| Short name | `payment_processor` | ExplicitMappingResolver |
| Class path (Ruby) | `PaymentHandlers::ProcessPaymentHandler` | ClassConstantResolver |
| Module path (Python) | `payment_handlers.ProcessPaymentHandler` | ClassLookupResolver |
| Namespace path (TS) | `PaymentHandlers.ProcessPaymentHandler` | ClassLookupResolver |

### 2. Entry Point (method)

The `method` field specifies which method to invoke on the handler. This enables **multi-method handlers** - a single handler class that exposes multiple entry points:

```yaml
# Default: calls the `call` method
handler:
  callable: payment_processor

# Explicit method: calls the `refund` method instead
handler:
  callable: payment_processor
  method: refund
```

**When to use method dispatch:**
- Payment handlers with `charge`, `refund`, `void` methods
- Validation handlers with `validate_input`, `validate_output` methods
- CRUD handlers with `create`, `read`, `update`, `delete` methods

### 3. Resolution Hint (resolver)

The `resolver` field is an **optional optimization** that bypasses the resolver chain and goes directly to a specific resolver:

```yaml
# Let the chain figure it out (default)
handler:
  callable: payment_processor

# Skip directly to explicit mapping (faster, explicit)
handler:
  callable: payment_processor
  resolver: explicit_mapping
```

**When to use resolver hints:**
- Performance optimization for high-throughput steps
- Explicit documentation of resolution strategy
- Avoiding ambiguity when multiple resolvers could match

---

## The Common Path

For most templates, you don't need to think about resolution at all. The **default resolution flow** handles common cases automatically:

```yaml
# Most common pattern - just specify the callable
steps:
  - name: process_payment
    handler:
      callable: process_payment  # Resolved by ExplicitMappingResolver
      initialization:
        timeout_ms: 5000
```

**What happens under the hood:**

1. Worker receives step execution event
2. HandlerDispatchService extracts the `HandlerDefinition`
3. ResolverChain iterates through resolvers by priority
4. ExplicitMappingResolver (priority 10) finds the registered handler
5. Handler is invoked with `call()` method (default)

---

## Resolver Chain Architecture

The resolver chain is an ordered list of resolvers, each with a priority. Lower priority numbers are checked first:

```
┌─────────────────────────────────────────────────────────────────┐
│                      ResolverChain                               │
│                                                                  │
│  ┌──────────────────────┐  ┌──────────────────────┐            │
│  │ ExplicitMapping      │  │ ClassConstant        │            │
│  │ Priority: 10         │──│ Priority: 100        │──► ...     │
│  │                      │  │                      │            │
│  │ "process_payment" ──►│  │ "Handlers::Payment"──►           │
│  │  Handler instance    │  │  constantize()       │            │
│  └──────────────────────┘  └──────────────────────┘            │
└─────────────────────────────────────────────────────────────────┘
```

### Resolution Flow

```
HandlerDefinition
       │
       ▼
┌──────────────────┐
│ Has resolver     │──Yes──► Go directly to named resolver
│ hint?            │
└────────┬─────────┘
         │ No
         ▼
┌──────────────────┐
│ ExplicitMapping  │──can_resolve?──Yes──► Return handler
│ (priority 10)    │
└────────┬─────────┘
         │ No
         ▼
┌──────────────────┐
│ ClassConstant    │──can_resolve?──Yes──► Return handler
│ (priority 100)   │
└────────┬─────────┘
         │ No
         ▼
    ResolutionError
```

---

## Built-in Resolvers

### ExplicitMappingResolver (Priority 10)

The **primary resolver** for all workers. Handlers are registered with string keys at startup:

```rust
// Rust registration
registry.register("process_payment", Arc::new(ProcessPaymentHandler::new()));
```

```ruby
# Ruby registration
registry.register("process_payment", ProcessPaymentHandler)
```

```python
# Python registration
registry.register("process_payment", ProcessPaymentHandler)
```

```typescript
// TypeScript registration
registry.register("process_payment", ProcessPaymentHandler);
```

**When it resolves:** When the `callable` exactly matches a registered key.

**Best for:**
- Native Rust handlers (required - no runtime reflection)
- Performance-critical handlers
- Explicit, predictable resolution

### Class Lookup Resolvers (Priority 100)

**Dynamic language only** (Ruby, Python, TypeScript). Interprets the callable as a class path and instantiates it at runtime.

> **Naming Note**: Ruby uses `ClassConstantResolver` (Ruby terminology for classes). Python and TypeScript use `ClassLookupResolver`. The functionality is equivalent.

```yaml
# Ruby: Uses Object.const_get (ClassConstantResolver)
handler:
  callable: PaymentHandlers::ProcessPaymentHandler

# Python: Uses importlib (ClassLookupResolver)
handler:
  callable: payment_handlers.ProcessPaymentHandler

# TypeScript: Uses dynamic import (ClassLookupResolver)
handler:
  callable: PaymentHandlers.ProcessPaymentHandler
```

**When it resolves:** When the `callable` looks like a class/module path (contains `::`, `.`, or starts with uppercase).

**Best for:**
- Convention-over-configuration setups
- Handlers that don't need explicit registration
- Dynamic handler loading

**Not available in Rust:** Rust has no runtime reflection, so class lookup resolvers always return `None`. Use ExplicitMappingResolver instead.

---

## Method Dispatch

Method dispatch allows a single handler to expose multiple entry points. This is useful for handlers that perform related operations:

### Defining a Multi-Method Handler

```ruby
# Ruby
class PaymentHandler < TaskerCore::StepHandler::Base
  def call(context)
    # Default method - standard payment processing
  end

  def refund(context)
    # Refund-specific logic
  end

  def void(context)
    # Void-specific logic
  end
end
```

```python
# Python
class PaymentHandler(StepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        # Default method
        pass

    def refund(self, context: StepContext) -> StepHandlerResult:
        # Refund-specific logic
        pass
```

```typescript
// TypeScript
class PaymentHandler extends StepHandler {
  async call(context: StepContext): Promise<StepHandlerResult> {
    // Default method
  }

  async refund(context: StepContext): Promise<StepHandlerResult> {
    // Refund-specific logic
  }
}
```

```rust
// Rust - requires explicit method routing
impl RustStepHandler for PaymentHandler {
    async fn call(&self, step: &TaskSequenceStep) -> Result<StepExecutionResult> {
        // Default method
    }

    async fn invoke_method(&self, method: &str, step: &TaskSequenceStep) -> Result<StepExecutionResult> {
        match method {
            "refund" => self.refund(step).await,
            "void" => self.void(step).await,
            _ => self.call(step).await,
        }
    }
}
```

### Using Method Dispatch in Templates

```yaml
steps:
  - name: process_refund
    handler:
      callable: payment_handler
      method: refund  # Invokes refund() instead of call()
      initialization:
        reason_required: true
```

### How Method Dispatch Works

1. Resolver chain resolves the handler from `callable`
2. If `method` is specified and not "call", a `MethodDispatchWrapper` is applied
3. When invoked, the wrapper calls the specified method instead of `call()`

```
                    ┌───────────────────┐
HandlerDefinition ──│ ResolverChain     │── Handler
(method: "refund")  │                   │
                    └─────────┬─────────┘
                              │
                              ▼
                    ┌───────────────────┐
                    │ MethodDispatch    │
                    │ Wrapper           │
                    │                   │
                    │ inner.refund()    │
                    └───────────────────┘
```

---

## Writing Custom Resolvers

You can extend the resolver chain with custom resolution strategies for your domain.

### Rust Custom Resolver

```rust
use tasker_shared::registry::{StepHandlerResolver, ResolutionContext, ResolvedHandler};
use async_trait::async_trait;

#[derive(Debug)]
pub struct ServiceDiscoveryResolver {
    service_registry: Arc<ServiceRegistry>,
}

#[async_trait]
impl StepHandlerResolver for ServiceDiscoveryResolver {
    fn resolver_name(&self) -> &str {
        "service_discovery"
    }

    fn priority(&self) -> u32 {
        50  // Between explicit (10) and class constant (100)
    }

    fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
        // Resolve callables that start with "service://"
        definition.callable.starts_with("service://")
    }

    async fn resolve(
        &self,
        definition: &HandlerDefinition,
        context: &ResolutionContext,
    ) -> Result<Arc<dyn ResolvedHandler>, ResolutionError> {
        let service_name = definition.callable.strip_prefix("service://").unwrap();
        let handler = self.service_registry.lookup(service_name).await?;
        Ok(Arc::new(StepHandlerAsResolved::new(handler)))
    }
}
```

### Ruby Custom Resolver

```ruby
module TaskerCore
  module Registry
    class ServiceDiscoveryResolver < BaseResolver
      def resolver_name
        "service_discovery"
      end

      def priority
        50
      end

      def can_resolve?(definition)
        definition.callable.start_with?("service://")
      end

      def resolve(definition, context)
        service_name = definition.callable.delete_prefix("service://")
        handler_class = ServiceRegistry.lookup(service_name)
        handler_class.new
      end
    end
  end
end
```

### Python Custom Resolver

```python
from tasker_core.registry import BaseResolver, ResolutionError

class ServiceDiscoveryResolver(BaseResolver):
    def resolver_name(self) -> str:
        return "service_discovery"

    def priority(self) -> int:
        return 50

    def can_resolve(self, definition: HandlerDefinition) -> bool:
        return definition.callable.startswith("service://")

    async def resolve(
        self, definition: HandlerDefinition, context: ResolutionContext
    ) -> ResolvedHandler:
        service_name = definition.callable.removeprefix("service://")
        handler_class = self.service_registry.lookup(service_name)
        return handler_class()
```

### TypeScript Custom Resolver

```typescript
import { BaseResolver, HandlerDefinition, ResolutionContext } from './registry';

export class ServiceDiscoveryResolver extends BaseResolver {
  resolverName(): string {
    return 'service_discovery';
  }

  priority(): number {
    return 50;
  }

  canResolve(definition: HandlerDefinition): boolean {
    return definition.callable.startsWith('service://');
  }

  async resolve(
    definition: HandlerDefinition,
    context: ResolutionContext
  ): Promise<ResolvedHandler> {
    const serviceName = definition.callable.replace('service://', '');
    const HandlerClass = await this.serviceRegistry.lookup(serviceName);
    return new HandlerClass();
  }
}
```

### Registering Custom Resolvers

```rust
// Rust
let mut chain = ResolverChain::new();
chain.register(Arc::new(ExplicitMappingResolver::new()));
chain.register(Arc::new(ServiceDiscoveryResolver::new(service_registry)));
chain.register(Arc::new(ClassConstantResolver::new()));
```

```ruby
# Ruby
chain = TaskerCore::Registry::ResolverChain.new
chain.register(TaskerCore::Registry::ExplicitMappingResolver.new)
chain.register(ServiceDiscoveryResolver.new(service_registry))
chain.register(TaskerCore::Registry::ClassConstantResolver.new)
```

---

## Cross-Language Considerations

### Why Rust is Different

Rust has **no runtime reflection**, which affects handler resolution:

| Capability | Ruby/Python/TypeScript | Rust |
|------------|------------------------|------|
| Class Lookup Resolver | ✅ Works | ❌ Always returns None |
| Method dispatch | ✅ Native (`send`, `getattr`) | ⚠️ Requires `invoke_method` |
| Dynamic handler loading | ✅ `const_get`, `importlib` | ❌ Must pre-register |

**Best Practice for Rust:**
- Always use ExplicitMappingResolver with explicit registration
- Implement `invoke_method()` for multi-method handlers
- Use resolver hints (`resolver: explicit_mapping`) for clarity

### Method Dispatch by Language

| Language | Default Method | Dynamic Dispatch |
|----------|---------------|------------------|
| Ruby | `call` | `handler.public_send(method, context)` |
| Python | `call` | `getattr(handler, method)(context)` |
| TypeScript | `call` | `handler[method](context)` |
| Rust | `call` | `handler.invoke_method(method, step)` |

---

## Troubleshooting

### "Handler not found" Error

**Symptoms:** `ResolutionError: No resolver could resolve callable 'my_handler'`

**Causes:**
1. Handler not registered with ExplicitMappingResolver
2. Class path typo (for ClassConstantResolver)
3. Handler registered with different name than callable

**Solutions:**
```rust
// Verify registration
assert!(registry.is_registered("my_handler"));

// Check registered handlers
println!("{:?}", registry.list_handlers());
```

### Method Not Found

**Symptoms:** `MethodNotFound: Handler 'my_handler' does not respond to 'refund'`

**Causes:**
1. Method name typo in YAML template
2. Method not defined on handler class
3. Method is private (Ruby) or underscore-prefixed (Python)

**Solutions:**
```yaml
# Verify method name matches exactly
handler:
  callable: payment_handler
  method: refund  # Must match method name in handler
```

### Resolver Hint Ignored

**Symptoms:** Resolution works but seems slow, or wrong resolver is used

**Causes:**
1. Resolver hint name doesn't match any registered resolver
2. Resolver with that name returns `None` for this callable

**Solutions:**
```yaml
# Use exact resolver name
handler:
  callable: my_handler
  resolver: explicit_mapping  # Not "explicit" or "mapping"
```

---

## Best Practices

### 1. Prefer Explicit Registration

```yaml
# Good: Clear, predictable, works in all languages
handler:
  callable: process_payment

# Avoid: Relies on runtime class lookup, not portable to Rust
handler:
  callable: PaymentHandlers::ProcessPaymentHandler
```

### 2. Use Method Dispatch for Related Operations

```yaml
# Good: Single handler, multiple entry points
steps:
  - name: validate_input
    handler:
      callable: validator
      method: validate_input

  - name: validate_output
    handler:
      callable: validator
      method: validate_output

# Avoid: Separate handlers for closely related operations
steps:
  - name: validate_input
    handler:
      callable: input_validator
  - name: validate_output
    handler:
      callable: output_validator
```

### 3. Document Resolution Strategy

```yaml
# Good: Explicit about how resolution works
handler:
  callable: payment_processor
  resolver: explicit_mapping  # Self-documenting
  method: refund
  initialization:
    timeout_ms: 5000
```

### 4. Test Resolution in Isolation

```rust
#[test]
fn test_handler_resolution() {
    let chain = create_resolver_chain();
    let definition = HandlerDefinition::builder()
        .callable("process_payment")
        .build();

    assert!(chain.can_resolve(&definition));
}
```

---

## Summary

| Concept | Purpose | Default |
|---------|---------|---------|
| `callable` | Handler address | Required |
| `method` | Entry point method | `"call"` |
| `resolver` | Resolution strategy hint | Chain iteration |
| ExplicitMappingResolver | Registered handlers | Priority 10 |
| ClassConstantResolver / ClassLookupResolver | Dynamic class lookup | Priority 100 |
| MethodDispatchWrapper | Multi-method support | Applied when `method` != `"call"` |

The resolver chain provides a flexible, extensible system for handler resolution that works consistently across all language workers while respecting each language's capabilities.
