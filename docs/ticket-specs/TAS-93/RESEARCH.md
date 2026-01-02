# TAS-93 Research: Step Handler Resolution Analysis

**Date**: 2024-12-30
**Session**: Handler Router/Resolver Strategy Pattern Exploration

---

## Research Questions

1. How do current language workers resolve handler callables?
2. What patterns are already in use that diverge from the assumed model?
3. What constraints do core tenets impose on the solution?
4. What edge cases need to be supported?
5. How should concerns be separated for developer clarity?

---

## Current State Analysis

### The Implicit Contract

Today's handler resolution assumes a rigid mapping:

```
callable: "MyModule::MyHandler"
    ↓ (framework assumes)
Object.const_get("MyModule::MyHandler") → Class
    ↓ (framework assumes)
Class.new → instance
    ↓ (framework assumes)
instance.call(context) → result
```

This creates three implicit constraints:
1. Callable must be fully-qualified class name
2. Class must be instantiable with no/optional arguments
3. Class must have `call` method with expected signature

### Language-Specific Implementations

#### Ruby (`workers/ruby/lib/tasker_core/registry/step_handler_resolver.rb`)

```ruby
# Current pattern
klass = Object.const_get(callable)
instance = klass.new
instance.call(context)
```

- Uses Ruby's `const_get` for dynamic class lookup
- Assumes single `call` method entry point
- No support for method routing

#### Python (`workers/python/python/tasker_core/handler.py`)

```python
# Current pattern
handler_class = registry.get(handler_name)
instance = handler_class()
instance.call(context)
```

- Lookup by registered `handler_name` class attribute
- Same assumptions as Ruby

#### TypeScript (`workers/typescript/src/handler/registry.ts`)

```typescript
// Current pattern
const handlerClass = registry.get(name);
const instance = new handlerClass();
instance.call(context);
```

- Similar to Python pattern
- Class registered by name

#### Rust (`workers/rust/src/step_handlers/registry.rs`, lines 232-334)

```rust
// ALREADY DIFFERENT - explicit mapping
match name {
    "linear_step_1" => Some(Arc::new(LinearStep1Handler::new(config))),
    "process_payment" => Some(Arc::new(ProcessPaymentHandler::new(config))),
    // 50+ explicit mappings
}
```

**Key Insight**: Rust already implements explicit mapping. This is not a workaround for Rust's type system limitations - it's actually the more explicit, controlled pattern that other languages could benefit from.

---

## Edge Cases Identified

### 1. Method Routing

Developer wants one class to serve multiple steps with different methods:

```yaml
steps:
  - name: validate_order
    handler:
      callable: "OrderHandler.validate"  # HOW DO WE PARSE THIS?
  
  - name: process_order
    handler:
      callable: "OrderHandler.process"
```

**Problem**: Is `.` a module separator or method delimiter? Varies by language.

### 2. Factory Patterns

Handler instantiation depends on runtime configuration:

```yaml
handler:
  callable: "HandlerFactory.for_payment_type"
  initialization:
    gateway: stripe
```

**Problem**: Current model has no factory concept.

### 3. Parameterized Instantiation

Different handler variants based on initialization:

```yaml
handler:
  callable: "RetryableHandler"
  initialization:
    max_retries: 5
    circuit_breaker: payment_gateway
```

**Problem**: Initialization affects which handler variant, not just configuration.

### 4. Language Convention Conflicts

- Ruby: `Module::Class` uses `::`, methods use `.` or `#`
- Python: `module.Class` uses `.`, methods use `.`
- TypeScript: `namespace.Class` uses `.`, methods use `.`

**Problem**: Parsing conventions is error-prone.

---

## Constraint Analysis from Core Tenets

| Tenet | Constraint Imposed |
|-------|-------------------|
| #1 Defense in Depth | Resolution failures must not corrupt step state |
| #3 Composition over Inheritance | Resolvers should be composable, not hierarchical |
| #4 Cross-Language Consistency | Solution must work identically across all 4 languages |
| #8 Pre-Alpha Freedom | Can make breaking changes now if beneficial |

### Defense in Depth Implication

Resolution is a critical path operation. If it fails:
- Step must not be marked as executed
- Error must be traceable with correlation IDs
- Retry logic should not be affected by resolution failures

### Composition Implication

Bad: Resolver inheritance hierarchy
```
BaseResolver
  ├── ClassResolver
  │     └── MethodDispatchResolver
  └── FactoryResolver
```

Good: Resolver chain with pluggable resolvers
```
ResolverChain
  → ExplicitMappingResolver
  → CustomResolver
  → ClassConstantResolver
```

---

## Design Refinement: The Three Concerns Problem

### Initial Proposal (Had Issues)

The initial proposal conflated three concerns in one structure:

```yaml
handler:
  namespace: "PaymentProcessing"  # Location component
  class: "PaymentHandler"         # Location component
  method: "validate"              # Entry point
  resolver: "method_dispatch"     # Resolution strategy
```

**Problems identified**:
1. Which parts map 1:1 to code paths at runtime?
2. Which are informational/disambiguation for developer-space resolution?
3. Which control framework-level precedence?
4. How should a developer building their own resolver key their hashmap?

### The FQDN Value

The original `callable: "module.namespace.HandlerClass"` model had a key virtue: **it was like DNS - a single address string that uniquely identifies the target.**

Splitting it creates ambiguity:
- Is lookup key `"namespace::class"`? `"namespace.class"`? Just `"class"` within namespace?
- Different languages have different separator conventions
- Resolver must know how to assemble components

### Refined Model: Address + Entry Point + Hint

The solution cleanly separates three concerns:

| Concern | Field | Semantics | Who Owns It |
|---------|-------|-----------|-------------|
| **Address** | `callable` | The identifier resolvers use to find/create a handler | Resolver interprets |
| **Entry Point** | `method` | Which method to invoke on the resolved handler | Framework handles |
| **Resolution Strategy** | `resolver` | Which resolver should interpret the address | Framework routes |

**Key insight**: `callable` is like a URL - its format depends on who's parsing it.

- `https://google.com` → parsed by HTTP
- `s3://bucket/key` → parsed by S3
- `payments:stripe:refund` → parsed by custom PaymentResolver
- `Module::ClassName` → parsed by ClassConstantResolver

### Framework vs Resolver Responsibilities

**Resolvers only care about `callable`** - it's their lookup key:
```ruby
class MyCustomResolver < StepHandlerResolver
  def resolve(definition, config)
    # definition.callable is YOUR lookup key
    # Interpret it however makes sense for your resolver
    handler = @registry[definition.callable]
    
    # Return the handler instance
    # Framework handles method dispatch wrapping
    handler
  end
end
```

**Framework handles `method` dispatch**:
```ruby
class ResolverChain
  def resolve(definition, config)
    # 1. Find a resolver that can handle this definition
    handler = find_resolver_and_resolve(definition, config)
    
    # 2. Framework wraps for method dispatch if needed
    if definition.uses_method_dispatch?
      handler = MethodDispatchWrapper.new(handler, definition.method)
    end
    
    handler
  end
end
```

This separation means:
- **Resolvers have single responsibility**: address → instance
- **Method dispatch is orthogonal**: resolvers don't need to handle it
- **Custom resolvers are simple to write**: just implement lookup logic
- **Address format is resolver-defined**: your format, your rules

---

## Solution Exploration

### Option A: String Parsing (Rejected)

Parse callable strings based on conventions:
- `Module::Class` → Class resolution
- `Module::Class.method` → Method dispatch
- `Module::Class#method` → Ruby-style method

**Rejected because**:
- Ambiguous across languages
- Requires complex parsing logic
- Error-prone edge cases

### Option B: Fully Structured Definition (Rejected)

Split into explicit components:

```yaml
handler:
  namespace: "payments"
  class: "PaymentHandler"
  method: "validate"
```

**Rejected because**:
- Ambiguous how components combine for lookup
- Different languages use different separators
- Resolver must know assembly rules
- Loses the FQDN clarity

### Option C: Address + Entry Point + Hint (Selected)

Keep `callable` as single address string, add orthogonal fields:

```yaml
handler:
  callable: "payments::PaymentHandler"  # THE ADDRESS
  method: "validate"                     # THE ENTRY POINT
  resolver: "explicit_mapping"           # THE RESOLUTION HINT
```

**Selected because**:
- Preserves FQDN/DNS-like model
- Unambiguous - resolver receives exact string
- Clean separation of concerns
- Resolvers stay simple (just lookup)
- Framework handles method dispatch uniformly

---

## Resolver Chain Design

### Priority System

Lower priority = checked first. Allows custom resolvers to override defaults:

| Priority | Resolver | Address Format |
|----------|----------|----------------|
| 10 | ExplicitMappingResolver | Registered key strings |
| 20-99 | Custom Resolvers | Your defined patterns |
| 100 | ClassConstantResolver | `Module::ClassName` |

### Resolution Flow

```
HandlerDefinition
    │
    ├─→ Has resolver hint?
    │       │
    │      Yes → Direct lookup by resolver name
    │       │
    │      No ↓
    │
    └─→ ResolverChain.resolve()
            │
            ├─→ ExplicitMappingResolver.can_resolve?
            │       → Is callable a registered key?
            │
            ├─→ CustomResolvers.can_resolve?
            │       → Does callable match my pattern?
            │
            └─→ ClassConstantResolver.can_resolve?
                    → Does callable look like a class path?
```

### Error Handling

When no resolver succeeds:

```ruby
raise ResolutionError.new(
  callable: definition.callable,
  tried_resolvers: ["ExplicitMappingResolver", "PaymentResolver", "ClassConstantResolver"],
  message: "No resolver could handle callable 'payments:invalid:format'"
)
```

---

## Value Proposition Preservation

### Key Insight

Method dispatch doesn't change step definition - only resolution target.

Each step in the DAG still has:
- ✅ Unique name
- ✅ Clear dependencies
- ✅ Its own retry configuration
- ✅ Its own domain event declarations
- ✅ Its own timeout

The handler class is just a code organization convenience. Bounded responsibility remains at the **step level**, not the handler level.

### When Method Dispatch is Appropriate

✅ Closely related operations on the same entity:
```yaml
steps:
  - name: validate_order    → OrderWorkflowHandler.validate
  - name: process_order     → OrderWorkflowHandler.process
  - name: fulfill_order     → OrderWorkflowHandler.fulfill
```

❌ Unrelated concerns in same handler:
```yaml
# DON'T DO THIS
steps:
  - name: process_payment   → GodHandler.process_payment
  - name: send_email        → GodHandler.send_email
  - name: generate_report   → GodHandler.generate_report
```

---

## Risk Assessment

### Risk: Address Format Confusion

**Likelihood**: Medium
**Impact**: Medium

**Mitigations**:
1. Clear documentation of built-in resolver format expectations
2. Error messages show which resolvers were tried
3. Examples mapping format → resolver

### Risk: God Handler Anti-Pattern

**Likelihood**: Medium
**Impact**: High (violates bounded responsibility)

**Mitigations**:
1. Documentation emphasizing appropriate use cases
2. Linting warnings if handler has >5 step-like methods
3. Example patterns showing good vs. bad usage

### Risk: Debugging Complexity

**Likelihood**: Low
**Impact**: Medium

**Mitigations**:
1. Comprehensive resolution logging with correlation IDs
2. Each resolver logs can_resolve? result
3. Error messages list all resolvers tried

### Risk: Cross-Language Drift

**Likelihood**: Medium
**Impact**: High

**Mitigations**:
1. Shared interface definition
2. Cross-language test fixtures
3. API convergence matrix updates

---

## Decision Record

### Decision 1: No Opt-In Configuration Flags

**Context**: Should resolver features require configuration to enable?

**Decision**: No. Presence of code (resolver registration, method field usage) is the expression of intention.

**Rationale**: Configuration flags create environment drift risk. If dev/staging have flags enabled but production doesn't, CI passes while production fails.

### Decision 2: Runtime Validation Only

**Context**: Should we validate method existence at template load time?

**Decision**: No. Validation happens at runtime when resolution is attempted.

**Rationale**: Ruby's `define_method` and runtime metaprogramming make static analysis impractical. For cross-language consistency, all languages use runtime validation.

### Decision 3: Single Address Field (callable)

**Context**: Should we split callable into namespace/class/method components?

**Decision**: No. Keep `callable` as single string field.

**Rationale**: 
- Preserves FQDN/DNS-like model
- Avoids assembly ambiguity
- Resolver defines its own format
- Simpler mental model

### Decision 4: Framework Owns Method Dispatch

**Context**: Should resolvers handle method dispatch?

**Decision**: No. Resolvers return instances. Framework wraps for method dispatch.

**Rationale**:
- Single responsibility for resolvers
- Method dispatch is orthogonal to resolution
- Consistent behavior across all resolvers
- Simplifies custom resolver implementation

### Decision 5: Precedence-Based Fallback

**Context**: What if only callable is provided (no method field)?

**Decision**: Default to `.call` method for backward compatibility.

**Rationale**: Existing templates must continue to work without modification.

---

## Files Examined

### Ruby Worker
- `workers/ruby/lib/tasker_core/registry/handler_registry.rb`
- `workers/ruby/lib/tasker_core/registry/step_handler_resolver.rb`

### Python Worker
- `workers/python/python/tasker_core/handler.py`

### TypeScript Worker
- `workers/typescript/src/handler/registry.ts`

### Rust Worker
- `workers/rust/src/step_handlers/registry.rs` (lines 232-334 - explicit mapping)

### Shared
- `tasker-shared/src/models/core/task_template/mod.rs`
- `tasker-worker/src/worker/task_template_manager.rs`

### Documentation
- `docs/principles/tasker-core-tenets.md`
- `docs/principles/cross-language-consistency.md`
- `docs/principles/composition-over-inheritance.md`
- `docs/architecture/worker-event-systems.md`

---

## Next Steps

1. ✅ Create specification document
2. ✅ Update Linear ticket
3. ✅ Refine design based on concern separation analysis
4. ⏳ Create sub-tasks for each implementation phase
5. ⏳ Begin Phase 1 when ready (resolver interface & chain)

---

## Session Artifacts

- **Specification**: `./SPECIFICATION.md`
- **This Document**: `./RESEARCH.md`
