# Tasker Core Navigation Guide for Claude Sessions

## Purpose

This guide helps Claude sessions efficiently navigate Tasker Core documentation. Read this when starting work on the project to understand where information lives and what patterns to recognize.

---

## Trigger → Document Mapping

| If the conversation involves... | First consult... | Then if deeper context needed... |
|--------------------------------|------------------|----------------------------------|
| Handler implementation patterns | `docs/workers/patterns-and-practices.md` | Language-specific: `docs/workers/{lang}.md` |
| Why something was designed this way | `docs/principles/tasker-core-tenets.md` | Specific ticket-spec if referenced |
| State transitions, task/step lifecycle | `docs/architecture/states-and-lifecycles.md` | |
| Cross-language API consistency | `docs/principles/cross-language-consistency.md` | `docs/workers/api-convergence-matrix.md` |
| Retry, backoff, error handling | `docs/guides/retry-semantics.md` | `docs/architecture/circuit-breakers.md` |
| Batch processing, cursors | `docs/guides/batch-processing.md` | |
| Checkpoint yielding, progress persistence | `docs/guides/batch-processing.md#checkpoint-yielding-tas-125` | `docs/workers/patterns-and-practices.md#checkpoint-yielding` |
| Something is stuck/failing | `docs/guides/dlq-system.md` | |
| Historical context, "why did we..." | `docs/CHRONOLOGY.md` | Specific ticket-spec if referenced |
| Core philosophy, design values | `docs/principles/tasker-core-tenets.md` | |
| Defense layers, idempotency | `docs/principles/defense-in-depth.md` | `docs/architecture/idempotency-and-atomicity.md` |
| FFI, language bindings, safety | `docs/reference/ffi-telemetry-pattern.md` | `docs/development/ffi-callback-safety.md` |
| Configuration, TOML structure | `docs/guides/configuration-management.md` | |
| Event systems, PGMQ, notifications | `docs/architecture/events-and-commands.md` | `docs/architecture/domain-events.md` |
| Actor patterns, service decomposition | `docs/architecture/actors.md` | `docs/architecture/worker-actors.md` |
| Concurrency, channels, backpressure | `docs/architecture/backpressure-architecture.md` | `docs/development/mpsc-channel-guidelines.md` |
| Handler composition patterns | `docs/principles/composition-over-inheritance.md` | |
| Crate structure, module organization | `docs/architecture/crate-architecture.md` | Crate-specific `AGENTS.md` files |

---

## Anti-Patterns: Don't Search For These

Use training knowledge directly for:
- General Rust/Ruby/Python/TypeScript language questions
- PostgreSQL syntax and standard operations
- Basic git, cargo, bundler, npm, bun commands
- HTTP status codes, REST conventions
- Standard library usage
- Tokio async patterns (general)
- SQLx usage patterns (general)

Only search Tasker-specific documentation for Tasker-specific concepts.

---

## Documentation Layers

Understanding the hierarchy helps choose where to look:

### 1. Principles (`docs/principles/`)
**The "why"** - Core values and design philosophy

Read when:
- Design decisions are questioned
- Evaluating alternatives
- Understanding trade-offs

Key documents:
- `tasker-core-tenets.md` - The 10 foundational tenets
- `defense-in-depth.md` - Protection layer model
- `cross-language-consistency.md` - Multi-language API philosophy
- `composition-over-inheritance.md` - Handler composition patterns

### 2. Architecture (`docs/architecture/`)
**The "what"** - System structure and patterns

Read when:
- Understanding how components interact
- Debugging flow issues
- Adding new capabilities

Key documents:
- `actors.md` - Actor pattern and orchestration actors
- `states-and-lifecycles.md` - State machine specifications
- `events-and-commands.md` - Event system architecture
- `backpressure-architecture.md` - Bounded resources and flow control

### 3. Guides (`docs/guides/`)
**The "how"** - Practical implementation guidance

Read when:
- Implementing features
- Following workflows
- Troubleshooting issues

Key documents:
- `quick-start.md` - Getting started
- `batch-processing.md` - Batch workflow patterns
- `conditional-workflows.md` - Decision point patterns
- `retry-semantics.md` - Error handling and retries

### 4. Workers (`docs/workers/`)
**Language-specific handler development**

Read when:
- Writing handlers in a specific language
- Understanding FFI boundaries
- Aligning APIs across languages

Key documents:
- `patterns-and-practices.md` - Common handler patterns
- `api-convergence-matrix.md` - Cross-language API reference
- `{ruby,python,typescript,rust}.md` - Language-specific details

### 5. Development (`docs/development/`)
**Developer tooling and patterns**

Read when:
- Setting up development environment
- Understanding build system
- Following coding standards

Key documents:
- `tooling.md` - cargo-make and build system
- `mpsc-channel-guidelines.md` - Channel usage rules
- `ffi-callback-safety.md` - FFI safety patterns

### 6. Operations (`docs/operations/`)
**Production operation guidance**

Read when:
- Tuning performance
- Monitoring system health
- Debugging production issues

Key documents:
- `mpsc-channel-tuning.md` - Channel configuration
- `backpressure-monitoring.md` - Monitoring backpressure

---

## Quick Context Loading

When starting a session with limited project context:

1. **Already in context**: `CLAUDE.md` (root) - commands and essential structure
2. **First read if design questions**: `docs/principles/tasker-core-tenets.md` - the 10 core tenets
3. **If architecture questions**: `docs/architecture/crate-architecture.md`
4. **Then**: Navigate based on user's specific question using trigger table above

For deep-dive sessions, also consider:
- `docs/CHRONOLOGY.md` for historical evolution
- Relevant `docs/ticket-specs/TAS-*/` for specific features

---

## Crate-Level Documentation

Each major crate has its own `AGENTS.md` with detailed module organization:

| Crate | AGENTS.md Location | Focus |
|-------|-------------------|-------|
| tasker-orchestration | `tasker-orchestration/AGENTS.md` | Actor pattern, services, state machines |
| tasker-worker | `tasker-worker/AGENTS.md` | Handler dispatch, FFI, completions |

---

## Linear Ticket Patterns

- Active tickets reference `docs/ticket-specs/TAS-XXX/` for detailed specs
- Key insights are extracted to `docs/principles/` and `docs/CHRONOLOGY.md`
- Don't read full ticket-specs directories unless specifically asked
- Ticket-specs are historical records; principles docs are the living reference

---

## Project Knowledge Search Tips

When using search tools:
- Search for concepts, not file names
- Use Tasker-specific terminology:
  - "step handler" (not "handler class")
  - "decision point" (not "conditional")
  - "batch cursor" (not "pagination")
  - "checkpoint yield" (not "save progress")
  - "domain event" (not "message")
  - "orchestration actor" (not "processor")
- If results seem thin, try related terms from the tenets or architecture docs
- Remember: the search finds content, but this guide tells you *where* to start

---

## Common Investigation Patterns

### "Why doesn't X work?"

1. Check `docs/architecture/states-and-lifecycles.md` for valid state transitions
2. Check `docs/guides/dlq-system.md` if task appears stuck
3. Check `docs/guides/retry-semantics.md` for error handling behavior

### "How do I implement Y?"

1. Check `docs/workers/patterns-and-practices.md` for patterns
2. Check `docs/workers/{language}.md` for language specifics
3. Check `docs/workers/example-handlers.md` for code examples

### "Why was Z designed this way?"

1. Check `docs/principles/tasker-core-tenets.md` for relevant tenet
2. Check `docs/CHRONOLOGY.md` for historical context
3. Check `docs/ticket-specs/TAS-*/` if specific ticket referenced

### "How do I add a new capability?"

1. Check `docs/principles/composition-over-inheritance.md` for pattern
2. Check `docs/workers/api-convergence-matrix.md` for API alignment
3. Check `docs/principles/cross-language-consistency.md` for multi-language considerations

### "How do I checkpoint long-running batch work?"

1. Check `docs/guides/batch-processing.md#checkpoint-yielding-tas-125` for full guide
2. Check `docs/workers/patterns-and-practices.md#checkpoint-yielding` for code patterns
3. Ensure handler uses `checkpoint_yield()` (or `checkpointYield()` in TypeScript)
4. Use `BatchWorkerContext` accessors to resume from checkpoint

---

## Related Documentation

- [Documentation Hub](./README.md) - Full documentation index
- [CLAUDE.md](../CLAUDE.md) - Project commands and structure
- [Tasker Core Tenets](./principles/tasker-core-tenets.md) - Core design principles
