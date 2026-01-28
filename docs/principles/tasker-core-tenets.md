# Tasker Core Tenets

These 11 tenets guide all architectural and design decisions in Tasker Core. Each emerged from real implementation experience, root cause analyses, or architectural migrations.

---

## The 11 Tenets

### 1. Defense in Depth

**Multiple overlapping protection layers provide robust idempotency without single-point dependency.**

Protection comes from four independent layers:
- **Database-level atomicity**: Unique constraints, row locking, compare-and-swap
- **State machine guards**: Current state validation before transitions
- **Transaction boundaries**: All-or-nothing semantics
- **Application-level filtering**: State-based deduplication

Each layer catches what others might miss. No single layer is responsible for all protection.

**Origin**: [TAS-54](https://linear.app/tasker-systems/issue/TAS-54) - Processor UUID ownership was removed when analysis proved it provided redundant protection with harmful side effects (blocking recovery after crashes).

**Lesson**: Find the minimal set of protections that prevents corruption. Additional layers that prevent recovery are worse than none.

---

### 2. Event-Driven with Polling Fallback

**Real-time responsiveness via PostgreSQL LISTEN/NOTIFY, with polling as a reliability backstop.**

The system supports three deployment modes:
- **EventDrivenOnly**: Lowest latency, relies on pg_notify
- **PollingOnly**: Traditional polling, higher latency but simple
- **Hybrid** (recommended): Event-driven primary, polling fallback

Events can be missed (network issues, connection drops). Polling ensures eventual consistency.

**Origin**: [TAS-43](https://linear.app/tasker-systems/issue/TAS-43) - Event-driven task claiming was added for low-latency response while preserving reliability guarantees.

---

### 3. Composition Over Inheritance

**Mixins and traits for handler capabilities, not class hierarchies.**

Handler capabilities are composed via mixins:
```
Not: class Handler < API
But: class Handler < Base; include API, include Decision, include Batchable
```

This pattern enables:
- Selective capability inclusion
- Clear separation of concerns
- Easier testing of individual capabilities
- No diamond inheritance problems

**Origin**: [TAS-112](https://linear.app/tasker-systems/issue/TAS-112) - Analysis revealed Batchable handlers already used composition. This was identified as the target architecture for all handlers.

**See also**: [Composition Over Inheritance](./composition-over-inheritance.md)

---

### 4. Cross-Language Consistency

**Unified developer-facing APIs across Rust, Ruby, Python, and TypeScript.**

Consistent touchpoints include:
- Handler signatures: `call(context)` pattern
- Result factories: `success(data)` / `failure(error, retry_on)`
- Registry APIs: `register_handler(name, handler)`
- Specialized patterns: API, Decision, Batchable

Each language expresses these idiomatically while maintaining conceptual consistency.

**Origin**: [TAS-92](https://linear.app/tasker-systems/issue/TAS-92), [TAS-100](https://linear.app/tasker-systems/issue/TAS-100) - Cross-language API alignment established the "one obvious way" philosophy.

**See also**: [Cross-Language Consistency](./cross-language-consistency.md)

---

### 5. Actor-Based Decomposition

**Lightweight actors for lifecycle management and clear boundaries.**

Orchestration uses four core actors:
1. **TaskRequestActor**: Task initialization
2. **ResultProcessorActor**: Step result processing
3. **StepEnqueuerActor**: Batch step enqueueing
4. **TaskFinalizerActor**: Task completion

Worker uses five specialized actors:
1. **StepExecutorActor**: Step execution coordination
2. **FFICompletionActor**: FFI completion handling
3. **TemplateCacheActor**: Template cache management
4. **DomainEventActor**: Event dispatching
5. **WorkerStatusActor**: Status and health

Each actor handles specific message types, enabling testability and clear ownership.

**Origin**: [TAS-46](https://linear.app/tasker-systems/issue/TAS-46), [TAS-69](https://linear.app/tasker-systems/issue/TAS-69) - Actor pattern refactoring reduced monolithic processors from 1,575 LOC to ~150 LOC focused files.

---

### 6. State Machine Rigor

**Dual state machines (Task + Step) for atomic transitions with full audit trails.**

Task states (12): `Pending → Initializing → EnqueuingSteps → StepsInProcess → EvaluatingResults → Complete/Error`

Step states (8): `Pending → Enqueued → InProgress → Complete/Error`

All transitions are:
- Atomic (compare-and-swap at database level)
- Audited (full history in transitions table)
- Validated (state guards prevent invalid transitions)

**Origin**: [TAS-41](https://linear.app/tasker-systems/issue/TAS-41) - Enhanced state machines with richer task states were introduced for better workflow visibility.

---

### 7. Audit Before Enforce

**Track for observability, don't block for "ownership."**

Processor UUID is tracked in every transition for:
- Debugging (which instance processed which step)
- Audit trails (full history of processing)
- Metrics (load distribution analysis)

But not enforced for:
- Ownership claims (blocks recovery)
- Permission checks (redundant with state guards)

**Origin**: [TAS-54](https://linear.app/tasker-systems/issue/TAS-54) - Ownership enforcement removal proved that audit trails provide value without enforcement costs.

**Key insight**: When two actors receive identical messages, first succeeds atomically, second fails cleanly - no partial state, no corruption.

---

### 8. Pre-Alpha Freedom

**Break things early to get architecture right.**

In pre-alpha phase:
- Breaking changes are encouraged when architecture is fundamentally unsound
- No backward compatibility required for greenfield work
- Migration debt is cheaper than technical debt
- "Perfect" is the enemy of "architecturally sound"

This freedom enables:
- Rapid iteration on core patterns
- Learning from real implementation
- Correcting course before users depend on specifics

**Origin**: All major refactoring tickets (TAS-54, TAS-69, TAS-112) - Each made breaking changes that improved architecture fundamentally.

---

### 9. PostgreSQL as Foundation

**Database-level guarantees with flexible messaging (PGMQ default, RabbitMQ optional).**

PostgreSQL provides:
- **State storage**: Task and step state with transactional guarantees
- **Advisory locks**: Distributed coordination primitives
- **Atomic functions**: State transitions in single round-trip
- **Row-level locking**: Prevents concurrent modification

Messaging is provider-agnostic:
- **PGMQ** (default): Message queue built on PostgreSQL—single-dependency deployment
- **RabbitMQ** (optional): For high-throughput or existing broker infrastructure

The database is not just storage—it's the coordination layer. Message delivery is pluggable.

**Origin**: Core architecture decision - PostgreSQL's transactional guarantees eliminate entire classes of distributed systems problems. TAS-133 added messaging abstraction for deployment flexibility.

---

### 10. Bounded Resources

**All channels bounded, backpressure everywhere.**

Every MPSC channel is:
- **Bounded**: Fixed capacity, no unbounded memory growth
- **Configurable**: Sizes set via TOML configuration
- **Monitored**: Backpressure metrics exposed

Semaphores limit concurrent handler execution. Circuit breakers protect downstream services.

**Origin**: [TAS-51](https://linear.app/tasker-systems/issue/TAS-51) - Bounded MPSC channels were mandated after analysis of unbounded channel risks.

**Rule**: Never use `unbounded_channel()`. Always configure bounds via TOML.

---

### 11. Fail Loudly

**A system that lies is worse than one that fails. Errors are first-class citizens, not inconveniences to hide.**

When data is missing, malformed, or unexpected:
- **Return errors**, not fabricated defaults
- **Propagate failures** up the call stack
- **Make problems visible** immediately, not downstream
- **Trust nothing** that hasn't been validated

Silent defaults create phantom data—values that look valid but represent nothing real. A monitoring system that receives `0%` utilization cannot distinguish "system is idle" from "data was missing."

**What this means in practice:**

| Scenario | Wrong Approach | Right Approach |
|----------|----------------|----------------|
| gRPC response missing field | Return default value | Return `InvalidResponse` error |
| Config section absent | Use empty/zero defaults | Fail with clear message |
| Health check data missing | Fabricate "unknown" status | Error: "health data unavailable" |
| Optional vs Required | Treat all as optional | Distinguish explicitly in types |

**The trust equation:**

```
A client that returns fabricated data
  = A client that lies to you
  = Worse than a client that fails loudly
  = Debugging phantom bugs in production
```

**Origin**: [TAS-177](https://linear.app/tasker-systems/issue/TAS-177) - gRPC client refactoring revealed pervasive `unwrap_or_default()` patterns that silently fabricated response data. Analysis showed consumers could receive "valid-looking" responses containing entirely phantom data, breaking the trust contract between client and caller.

**Key insight**: When a gRPC server omits required fields, that's a protocol violation—not an opportunity to be "helpful" with defaults. The server is broken; pretending otherwise delays the fix and misleads operators.

**Rule**: Never use `unwrap_or_default()` or `unwrap_or_else(|| fabricated_value)` for required fields. Use `ok_or_else(|| ClientError::invalid_response(...))` instead.

---

## Meta-Principles

These overarching themes emerge from the tenets:

1. **Simplicity Over Elegance**: The minimal protection set that prevents corruption beats layered defense that prevents recovery

2. **Observation-Driven Design**: Let real behavior (parallel execution, edge cases) guide architecture

3. **Explicit Over Implicit**: Make boundaries, layers, and decisions visible in documentation and code

4. **Consistency Without Uniformity**: Align APIs while preserving language idioms

5. **Separation of Concerns**: Orchestration handles state and coordination; workers handle execution and domain events

6. **Errors Over Defaults**: When in doubt, fail with a clear error rather than proceeding with fabricated data

---

## Applying These Tenets

When making design decisions:

1. **Check against tenets**: Does this violate any of the 10 tenets?
2. **Find the precedent**: Has a similar decision been made before? (See ticket-specs)
3. **Document the trade-off**: What are you gaining and giving up?
4. **Consider recovery**: If this fails, how does the system recover?

When reviewing code:

1. **Bounded resources**: Are all channels bounded? All concurrency limited?
2. **State machine compliance**: Do transitions use atomic database operations?
3. **Language consistency**: Does the API align with other language workers?
4. **Composition pattern**: Are capabilities mixed in rather than inherited?
5. **Fail loudly**: Are missing/invalid data handled with errors, not silent defaults?
