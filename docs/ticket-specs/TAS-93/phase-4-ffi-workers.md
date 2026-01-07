# TAS-93 Phase 4: FFI Worker Resolvers (Ruby, Python, TypeScript)

**Status**: Phase 4a (Ruby) Complete | Phase 4b-c (Python, TypeScript) Planned
**Created**: 2025-01-07
**Updated**: 2026-01-07
**Depends On**: Phase 3 (Rust adapter pattern complete)

---

## Overview

Phase 4 implements the resolver chain pattern for dynamic languages (Ruby, Python, TypeScript). Unlike Rust, these languages have runtime reflection capabilities that enable **inferential resolution** - the ability to try multiple resolution strategies and infer handler targets from callable formats.

### Key Differences from Rust

| Aspect | Rust | Ruby/Python/TypeScript |
|--------|------|------------------------|
| Method dispatch | Compile-time only | Runtime reflection |
| Class lookup | Explicit mapping required | Can infer from string |
| Adapter pattern | Required (execution bridge) | Not required |
| Resolution | Exact match or fail | Inferential with fallbacks |

### Goals

1. **Inferential Resolution**: Try multiple strategies before failing
2. **Developer-Friendly Extension**: Simple `RegistryResolver` base class for custom resolvers
3. **Idiomatic Implementations**: Leverage each language's strengths
4. **Cross-Language Consistency**: Same mental model, different implementations

---

## Phase 4a: Ruby Implementation (COMPLETE)

### What Was Built

The Ruby implementation introduces a complete resolver chain infrastructure integrated with the existing `HandlerRegistry`. Key components:

1. **ResolverChain**: Priority-ordered chain that orchestrates resolution
2. **BaseResolver**: Abstract base class defining the resolver contract
3. **ExplicitMappingResolver** (priority 10): For explicitly registered handlers
4. **ClassConstantResolver** (priority 100): Infers classes from callable strings
5. **RegistryResolver**: Developer-friendly DSL for custom resolvers
6. **MethodDispatchWrapper**: Redirects `.call()` to specified method
7. **FFI Integration**: HandlerWrapper passes `method`/`resolver` fields from Rust

### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     RUBY RESOLVER ARCHITECTURE                       │
└─────────────────────────────────────────────────────────────────────┘

                        ┌─────────────────────┐
                        │   StepSubscriber    │
                        │   (subscriber.rb)   │
                        └──────────┬──────────┘
                                   │
                   step_data.step_definition.handler
                                   │
                                   ▼
                        ┌─────────────────────┐
                        │  HandlerRegistry    │
                        │     (Singleton)     │
                        └──────────┬──────────┘
                                   │
                   normalize_to_definition(handler_spec)
                                   │
                                   ▼
                        ┌─────────────────────┐
                        │  HandlerDefinition  │
                        │  (Dry::Struct)      │
                        │  - callable         │
                        │  - handler_method   │
                        │  - resolver         │
                        │  - initialization   │
                        └──────────┬──────────┘
                                   │
                                   ▼
                        ┌─────────────────────┐
                        │   ResolverChain     │
                        │   - resolve()       │
                        │   - can_resolve?()  │
                        └──────────┬──────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          │                        │                        │
          ▼                        ▼                        ▼
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ ExplicitMapping  │    │ CustomResolver   │    │  ClassConstant   │
│   (priority 10)  │    │  (priority 50)   │    │  (priority 100)  │
└────────┬─────────┘    └────────┬─────────┘    └────────┬─────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                                 ▼
                        ┌─────────────────────┐
                        │ MethodDispatchWrapper│
                        │ (if handler_method)  │
                        └──────────┬──────────┘
                                   │
                                   ▼
                        ┌─────────────────────┐
                        │   Handler Instance   │
                        │   ready for .call()  │
                        └─────────────────────┘
```

### File Structure (Actual)

```
workers/ruby/lib/tasker_core/
├── registry.rb                              # Entry point, loads all registry files
├── registry/
│   ├── handler_registry.rb                  # Main registry (singleton, uses ResolverChain)
│   ├── resolver_chain.rb                    # Chain orchestration
│   └── resolvers/
│       ├── base_resolver.rb                 # Abstract base class (contract)
│       ├── registry_resolver.rb             # Developer-friendly DSL base class
│       ├── explicit_mapping_resolver.rb     # Priority 10 - explicit registration
│       ├── class_constant_resolver.rb       # Priority 100 - class inference
│       └── method_dispatch_wrapper.rb       # .call() → .method() redirection
├── models.rb                                # HandlerWrapper (FFI data wrapper)
├── types/
│   └── task_template.rb                     # HandlerDefinition (Dry::Struct)
└── subscriber.rb                            # Step execution subscriber
```

### Data Flow: FFI to Handler Resolution

```
1. Rust serializes TaskSequenceStep via serde_magnus
   - HandlerDefinition includes: callable, method, resolver, initialization

2. Ruby receives step_data with nested structure
   - step_data[:step_definition][:handler] = { callable:, method:, resolver:, ... }

3. StepSubscriber calls:
   handler = @handler_registry.resolve_handler(step_data.step_definition.handler)

4. HandlerRegistry.resolve_handler converts HandlerWrapper → HandlerDefinition:
   - HandlerWrapper.callable      → HandlerDefinition.callable
   - HandlerWrapper.handler_method → HandlerDefinition.handler_method
   - HandlerWrapper.resolver      → HandlerDefinition.resolver
   - HandlerWrapper.initialization → HandlerDefinition.initialization

5. ResolverChain.resolve(definition, config):
   - If resolver hint → use ONLY that resolver
   - Otherwise → try resolvers in priority order

6. If uses_method_dispatch? → wrap with MethodDispatchWrapper

7. Return handler ready for .call(context)
```

---

## Contract: Respecting `method` and `resolver` Fields

The `HandlerDefinition` schema includes two critical fields that **must be respected**:

### The `resolver` Field (Resolution Hint)

If `resolver` is specified, **bypass the chain entirely** and go directly to that resolver:

```yaml
handler:
  callable: "my_custom_key"
  resolver: "explicit_mapping"  # MUST use ExplicitMappingResolver directly
```

**Actual implementation** (from `resolver_chain.rb`):
```ruby
def resolve(definition, config = {})
  # Contract: If resolver hint is present, use ONLY that resolver
  return resolve_with_hint(definition, config) if definition.has_resolver_hint?

  # Otherwise: Try inferential chain resolution
  resolve_with_chain(definition, config)
end

def resolve_with_hint(definition, config)
  resolver_name = definition.resolver
  resolver = @resolvers_by_name[resolver_name]

  unless resolver
    raise ResolverNotFoundError, "Unknown resolver: '#{resolver_name}'"
  end

  handler = resolver.resolve(definition, config)
  return nil unless handler

  wrap_for_method_dispatch(handler, definition)
end
```

### The `method` Field (Entry Point)

If `method` is specified (and is not "call"), wrap the handler for method dispatch:

```yaml
handler:
  callable: "OrderWorkflowHandler"
  method: "validate"  # MUST invoke .validate(), not .call()
```

**Actual implementation** (from `resolver_chain.rb`):
```ruby
def wrap_for_method_dispatch(handler, definition)
  # No wrapping needed if using default .call method
  return handler unless definition.uses_method_dispatch?

  effective_method = definition.effective_method.to_sym

  unless handler.respond_to?(effective_method)
    @logger.warn("Handler #{handler.class} doesn't respond to ##{effective_method}")
    return nil
  end

  Resolvers::MethodDispatchWrapper.new(handler, effective_method)
end
```

### Helper Methods (from `HandlerDefinition`)

These are implemented in `types/task_template.rb`:

```ruby
class HandlerDefinition < Dry::Struct
  attribute :callable, Types::Strict::String
  attribute :initialization, Types::Hash.default({}.freeze)
  attribute :handler_method, Types::String.optional.default(nil)
  attribute :resolver, Types::String.optional.default(nil)

  # Returns the method to invoke (defaults to "call")
  def effective_method
    handler_method.presence || 'call'
  end

  # Returns true if method dispatch is needed (method set and not "call")
  def uses_method_dispatch?
    handler_method.present? && handler_method != 'call'
  end

  # Returns true if a resolver hint is provided
  def has_resolver_hint?
    resolver.present?
  end
end
```

---

## Core Concept: Inferential Resolution

Dynamic languages can "try things" at runtime that Rust cannot:

```ruby
# Ruby can try to constantize a string and see if it works
def infer_handler(callable)
  # Does "PaymentHandler" exist as a class? Let's find out!
  Object.const_get(callable).new
rescue NameError
  nil  # Nope, try something else
end
```

This enables a resolution chain that **progressively loosens constraints**:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    INFERENTIAL RESOLUTION FLOW                       │
└─────────────────────────────────────────────────────────────────────┘

callable: "Payments::RefundHandler"

Step 1: ExplicitMappingResolver (Priority 10)
  → Is "Payments::RefundHandler" a registered key?
  → NO → continue

Step 2: Custom Resolvers (Priority 20-99)
  → Does any custom resolver claim this callable?
  → NO → continue

Step 3: ClassConstantResolver (Priority 100)
  → Does it look like a class path? (starts with capital, has ::)
  → YES → Try Object.const_get("Payments::RefundHandler")
  → SUCCESS! Return instance

───────────────────────────────────────────────────────────────────────

callable: "process_payment"

Step 1: ExplicitMappingResolver
  → Is "process_payment" registered?
  → YES → Return registered handler
  → DONE

───────────────────────────────────────────────────────────────────────

callable: "payments:stripe:refund"

Step 1: ExplicitMappingResolver
  → Registered? NO

Step 2: PaymentResolver (custom, priority 20)
  → Pattern /^payments:(\w+):(\w+)$/ matches!
  → Custom logic creates StripeRefundHandler
  → SUCCESS!
```

---

## HandlerRegistry Integration

The `HandlerRegistry` (singleton) owns a `ResolverChain` instance and provides backward-compatible APIs:

```ruby
class HandlerRegistry
  include Singleton

  def initialize
    @handlers = {}  # Legacy compatibility hash
    @resolver_chain = ResolverChain.default
    bootstrap_handlers!
  end

  # Main resolution method - accepts String, HandlerDefinition, or HandlerWrapper
  def resolve_handler(handler_spec)
    definition = normalize_to_definition(handler_spec)
    handler = @resolver_chain.resolve(definition)
    return handler if handler

    # Fallback: try legacy @handlers hash
    handler_class = @handlers[definition.callable]
    return nil unless handler_class

    instantiate_handler(handler_class, definition)
  end

  # Register handler (goes to both chain and legacy hash)
  def register_handler(class_name, handler_class)
    @resolver_chain.register(class_name, handler_class)
    @handlers[class_name] = handler_class
  end

  # TAS-93: Add custom resolver to the chain
  def add_resolver(resolver)
    @resolver_chain.add_resolver(resolver)
  end

  private

  # Normalize any handler spec to HandlerDefinition
  def normalize_to_definition(handler_spec)
    case handler_spec
    when String
      TaskerCore::Types::HandlerDefinition.new(callable: handler_spec)
    when TaskerCore::Types::HandlerDefinition
      handler_spec
    when TaskerCore::Models::HandlerWrapper
      # FFI wrapper - now includes handler_method and resolver!
      TaskerCore::Types::HandlerDefinition.new(
        callable: handler_spec.callable,
        initialization: handler_spec.initialization.to_h,
        handler_method: handler_spec.handler_method,
        resolver: handler_spec.resolver
      )
    else
      # Duck typing fallback
      # ...
    end
  end
end
```

---

## FFI Integration: HandlerWrapper

The `HandlerWrapper` class in `models.rb` wraps handler data from Rust FFI:

```ruby
# models.rb
class HandlerWrapper
  attr_reader :callable, :initialization, :handler_method, :resolver

  def initialize(handler_data)
    @callable = handler_data[:callable]
    @initialization = (handler_data[:initialization] || {}).with_indifferent_access
    # TAS-93: Method dispatch - note Rust field is 'method' but we use 'handler_method'
    @handler_method = handler_data[:method]
    # TAS-93: Resolver hint for direct resolver routing
    @resolver = handler_data[:resolver]
  end
end
```

The Rust `HandlerDefinition` (in `tasker-shared/src/models/core/task_template/mod.rs`) serializes automatically via `serde_magnus`:

```rust
pub struct HandlerDefinition {
    pub callable: String,
    #[serde(default)]
    pub method: Option<String>,     // → Ruby :method → handler_method
    #[serde(default)]
    pub resolver: Option<String>,   // → Ruby :resolver
    #[serde(default)]
    pub initialization: HashMap<String, Value>,
}
```

---

## Subscriber Integration

The `StepExecutionSubscriber` in `subscriber.rb` was updated to pass the full handler object:

```ruby
# subscriber.rb - call() method
def call(event)
  step_data = event.payload[:task_sequence_step]

  # TAS-93: Resolve step handler from registry using full handler definition
  # This enables method dispatch (handler_method) and resolver hints (resolver)
  handler = @handler_registry.resolve_handler(step_data.step_definition.handler)

  unless handler
    raise Errors::ConfigurationError,
          "No handler found for #{step_data.step_definition.handler.callable}"
  end

  # Execute handler with unified context
  context = TaskerCore::Types::StepContext.new(step_data)
  result = handler.call(context)  # May actually call .method() via wrapper
  # ...
end
```

---

## Built-in Resolvers (Actual Implementation)

### 1. ExplicitMappingResolver (Priority 10)

```ruby
class ExplicitMappingResolver < BaseResolver
  def initialize(name = 'explicit_mapping')
    @name = name
    @handlers = {}
    @mutex = Mutex.new
  end

  def name
    @name
  end

  def priority
    10
  end

  def can_resolve?(definition, _config = {})
    @handlers.key?(definition.callable)
  end

  def resolve(definition, config = {})
    entry = @handlers[definition.callable]
    return nil unless entry

    instantiate_handler(entry, config)
  end

  def register(key, handler_or_class)
    @mutex.synchronize { @handlers[key.to_s] = handler_or_class }
  end

  def registered_callables
    @handlers.keys
  end

  private

  def instantiate_handler(entry, config)
    case entry
    when Class
      instantiate_class(entry, config)
    when Proc
      entry.call(config)
    else
      entry  # Already an instance
    end
  end
end
```

### 2. ClassConstantResolver (Priority 100)

```ruby
class ClassConstantResolver < BaseResolver
  # Matches Ruby class-like patterns (e.g., "Payments::RefundHandler")
  CLASS_PATTERN = /\A[A-Z][A-Za-z0-9_]*(::[A-Z][A-Za-z0-9_]*)*\z/

  def name
    'class_constant'
  end

  def priority
    100
  end

  def can_resolve?(definition, _config = {})
    definition.callable.match?(CLASS_PATTERN)
  end

  def resolve(definition, config = {})
    klass = constantize(definition.callable)
    return nil unless klass

    instantiate_handler(klass, config)
  end

  private

  def constantize(string)
    names = string.split('::')
    names.reduce(Object) { |constant, name| constant.const_get(name, false) }
  rescue NameError
    nil
  end
end
```

---

## MethodDispatchWrapper (Actual Implementation)

```ruby
class MethodDispatchWrapper
  attr_reader :handler, :target_method

  def initialize(handler, target_method)
    @handler = handler
    @target_method = target_method.to_sym

    unless @handler.respond_to?(@target_method)
      raise ArgumentError,
            "Handler #{@handler.class} does not respond to '#{@target_method}'"
    end
  end

  # Redirect .call() to the target method
  def call(context)
    @handler.public_send(@target_method, context)
  end

  # Delegate respond_to? to inner handler (except for :call)
  def respond_to_missing?(method_name, include_private = false)
    @handler.respond_to?(method_name, include_private) || super
  end

  # Delegate other methods to inner handler
  def method_missing(method_name, ...)
    if @handler.respond_to?(method_name)
      @handler.public_send(method_name, ...)
    else
      super
    end
  end

  # Get the wrapped handler (for testing/debugging)
  def unwrap
    @handler
  end
end
```

---

## Developer-Facing API: `RegistryResolver`

The `RegistryResolver` provides a DSL for creating custom resolvers:

```ruby
class RegistryResolver < BaseResolver
  class << self
    attr_reader :pattern_value, :prefix_value, :priority_val

    # DSL: Declare regex pattern this resolver handles
    def handles_pattern(regex)
      @pattern_value = regex
    end

    # DSL: Declare string prefix this resolver handles
    def handles_prefix(prefix_string)
      @prefix_value = prefix_string
    end

    # DSL: Set priority (lower = checked first)
    def priority_value(value)
      @priority_val = value
    end
  end

  def name
    self.class.name&.demodulize&.underscore || 'custom_resolver'
  end

  def priority
    self.class.priority_val || 50
  end

  def can_resolve?(definition, _config = {})
    callable = definition.callable

    if self.class.pattern_value
      callable.match?(self.class.pattern_value)
    elsif self.class.prefix_value
      callable.start_with?(self.class.prefix_value)
    else
      false
    end
  end

  # Subclasses should implement resolve_handler for cleaner override
  def resolve(definition, config = {})
    resolve_handler(definition, config)
  end

  # Override this in subclasses
  def resolve_handler(definition, config)
    raise NotImplementedError, "#{self.class}#resolve_handler must be implemented"
  end
end
```

**Example custom resolver:**

```ruby
class PaymentResolver < TaskerCore::Registry::Resolvers::RegistryResolver
  handles_pattern(/^payments:(?<provider>\w+):(?<action>\w+)$/)
  priority_value 20

  def resolve_handler(definition, _config)
    match = definition.callable.match(self.class.pattern_value)
    provider = match[:provider]
    action = match[:action]

    handler_class = PaymentHandlers.for(provider: provider, action: action)
    handler_class&.new
  end
end
```

---

## Test Coverage (Phase 4a)

### Handler Registry Tests (`spec/registry/handler_registry_spec.rb`)

29 tests covering:

```ruby
TaskerCore::Registry::HandlerRegistry
  #resolve_handler with string callable
    resolves handler by class name string
    returns nil for unknown handler
    handler responds to call
  #resolve_handler with HandlerDefinition
    without method dispatch
      resolves handler from definition
      handler is not wrapped
    with method dispatch
      wraps handler for method dispatch
      call() invokes the specified method
      unwrap returns the underlying handler
    with initialization config
      passes config to handler
  #resolve_handler with HandlerWrapper (FFI)
    resolves from HandlerWrapper
    passes initialization from wrapper
    with TAS-93 method dispatch from FFI
      wraps handler for method dispatch when method is specified
      invokes the specified method via call()
      exposes handler_method attribute
    with TAS-93 resolver hint from FFI
      exposes resolver attribute
  #register_handler
    registers in both resolver chain and legacy hash
  #handler_available?
    returns true for registered handlers
    returns false for unknown handlers
  #registered_handlers
    includes handlers from both chain and legacy hash
    returns sorted unique list
  #add_resolver
    adds custom resolver to the chain
    custom resolver can resolve handlers
  cross-language aliases
    register is alias for register_handler
    resolve is alias for resolve_handler
    is_registered is alias for handler_available?
    list_handlers is alias for registered_handlers
  resolver chain integration
    explicit registration takes priority over class constant
    class constant resolution works for unregistered classes
  thread safety
    concurrent registration is safe
```

### Validation Commands

```bash
# Run all registry tests
cd workers/ruby
TASKER_ENV=test bundle exec rspec spec/registry/ --format documentation

# Run full Ruby spec suite
TASKER_ENV=test bundle exec rspec spec/ --format progress
# Result: 460 examples, 0 failures
```

---

## Removed Code

### Dead Code Cleanup

The following files were **removed** as they referenced non-existent classes and were not part of the actual dispatch path:

- `workers/ruby/lib/tasker_core/registry/step_handler_resolver.rb` - Referenced `TaskTemplateRegistry` which doesn't exist
- `workers/ruby/spec/registry/step_handler_resolver_integration_spec.rb` - Tests for removed code

The actual production dispatch path uses:
- `HandlerRegistry` (singleton) → `ResolverChain` → Built-in/Custom Resolvers → `MethodDispatchWrapper`

---

## Phase 4b: Python Implementation (PLANNED)

Same architecture, Pythonic idioms:

```
workers/python/python/tasker_core/registry/
├── __init__.py
├── base_resolver.py           # ABC base class
├── registry_resolver.py       # Developer-friendly DSL
├── resolver_chain.py          # Chain orchestration
├── method_dispatch_wrapper.py # Method dispatch
├── errors.py                  # Error types
└── resolvers/
    ├── __init__.py
    ├── explicit_mapping.py
    └── class_lookup.py
```

Key differences:
- Use `importlib` for class lookup
- Use `@dataclass` or `pydantic` for HandlerDefinition
- Use `abc.ABC` and `@abstractmethod`

## Phase 4c: TypeScript Implementation (PLANNED)

Same architecture, TypeScript idioms:

```
workers/typescript/src/registry/
├── index.ts
├── base-resolver.ts           # Interface
├── registry-resolver.ts       # Developer-friendly base class
├── resolver-chain.ts          # Chain orchestration
├── method-dispatch-wrapper.ts # Method dispatch
├── errors.ts                  # Error types
└── resolvers/
    ├── index.ts
    ├── explicit-mapping.ts
    └── class-lookup.ts
```

Key differences:
- Use interfaces instead of abstract classes
- Dynamic import for class lookup
- Async resolution support

---

## Summary

Phase 4a (Ruby) delivers:

1. **`ResolverChain`**: Priority-ordered chain with `resolver` hint bypass
2. **`BaseResolver`**: Contract for all resolvers
3. **`RegistryResolver`**: Developer-friendly DSL for custom resolvers
4. **Built-in Resolvers**: ExplicitMapping (priority 10), ClassConstant (priority 100)
5. **`MethodDispatchWrapper`**: Framework handles `handler_method` field uniformly
6. **`HandlerRegistry` Integration**: Uses ResolverChain for all resolution
7. **FFI Integration**: `HandlerWrapper` passes `method`/`resolver` from Rust

The key insight is that dynamic languages don't need Rust's adapter pattern - they can resolve and dispatch at runtime. The resolver chain provides structure and extensibility without the bridging complexity.
