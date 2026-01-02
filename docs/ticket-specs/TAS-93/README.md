# TAS-93: Step Handler Router/Resolver Strategy Pattern

**Linear**: [TAS-93](https://linear.app/tasker-systems/issue/TAS-93/expose-a-developer-facing-step-handler-router-resolver)
**Status**: Planning Complete
**Priority**: Low (foundational, no breaking changes)

---

## Quick Links

| Document | Purpose |
|----------|---------|
| [SPECIFICATION.md](./SPECIFICATION.md) | Full technical specification with implementation phases |
| [RESEARCH.md](./RESEARCH.md) | Analysis findings, design refinement, and decision records |

---

## One-Line Summary

Expose a developer-facing strategy pattern for step handler resolution with clean separation between address lookup (resolver's job) and method dispatch (framework's job).

---

## The Mental Model

### Three Concerns, Cleanly Separated

| Concern | Field | Who Handles It |
|---------|-------|----------------|
| **Address** | `callable` | Resolver interprets (like a URL) |
| **Entry Point** | `method` | Framework wraps |
| **Resolution Hint** | `resolver` | Framework routes |

### The Address Model

Think of `callable` like a URL - its format depends on who parses it:

| Address Format | Resolver | Interpretation |
|----------------|----------|----------------|
| `Module::ClassName` | ClassConstantResolver | Ruby constant lookup |
| `my_handler_key` | ExplicitMappingResolver | Registered key |
| `payments:stripe:refund` | YourCustomResolver | Your domain logic |

---

## Schema

```yaml
handler:
  # THE ADDRESS - resolver's lookup key
  callable: "payments::PaymentHandler"
  
  # THE ENTRY POINT - framework handles dispatch
  method: "validate"  # Optional, defaults to "call"
  
  # THE RESOLUTION HINT - skip chain, use this resolver
  resolver: "explicit_mapping"  # Optional
  
  initialization:
    timeout_ms: 30000
```

---

## Key Decisions

1. **Single `callable` field** - preserves FQDN/DNS-like model, no assembly ambiguity
2. **Framework owns method dispatch** - resolvers just return instances
3. **No opt-in flags** - code presence = intention
4. **Runtime validation** - Ruby metaprogramming prevents static analysis
5. **Backward compatible** - existing templates work unchanged

---

## Implementation Phases

| Phase | Deliverable | Est. |
|-------|-------------|------|
| 1 | Resolver interface, chain, method dispatch wrapper | 3-4d |
| 2 | ExplicitMappingResolver (Ruby/Python/TS) | 1-2d |
| 3 | Custom resolver extension point | 1-2d |
| 4 | Documentation with mental model | 1d |

**Total**: 6-9 days

---

## For Custom Resolver Builders

Your resolver's job is simple:

```ruby
class MyResolver < StepHandlerResolver
  def can_resolve?(definition)
    # Does definition.callable match your format?
    definition.callable.start_with?('my_prefix:')
  end
  
  def resolve(definition, config)
    # definition.callable is YOUR lookup key
    # Return handler instance (framework handles method dispatch)
    @registry[definition.callable]
  end
end
```

---

## Related Tickets

- TAS-108: Breaking API changes (may benefit from resolver pattern)
- TAS-109: Cross-language ergonomics (resolver chain must align)
