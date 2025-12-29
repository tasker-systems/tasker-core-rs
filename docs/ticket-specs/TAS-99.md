# Documentation Audit and Restructuring

# TAS-99: Documentation Audit and Restructuring

## Executive Summary

Restructure `docs/` from flat hierarchy to cognitively-organized structure, establish explicit principles from implicit patterns, extract historical chronology from ticket-specs, and create a navigation guide for synthetic minds (Claude sessions) to efficiently traverse the knowledge space.

**Status**: Pre-alpha greenfield project - documentation restructuring encouraged to establish correct patterns early.

---

## Problem Statement

### 1\. Structural Sprawl

The `docs/` directory has grown organically with 24 top-level `.md` files without clear grouping. While the `docs/README.md` hub is excellent, the underlying structure doesn't match the logical categories it presents.

### 2\. Hidden Institutional Knowledge

The `docs/ticket-specs/` directory contains \~40 directories with 150+ files of incredibly detailed specifications, RCAs, phase summaries, and implementation details. This represents a **goldmine of implicit principles and hard-won lessons** that is essentially "write-only" - captured but never surfaced for ongoing use.

Key examples of buried knowledge:

* [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery): "Processor UUID ownership was redundant protection with harmful side effects" → Auto-recovery principle
* [TAS-100](https://linear.app/tasker-systems/issue/TAS-100/bun-and-nodejs-typescript-worker): FFI vs WASM analysis proving FFI pragmatically superior → Architecture decision
* [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization): "Batchable already uses mixin pattern - this is the TARGET architecture" → Composition principle
* **TAS-67/69**: Actor pattern enables testability and clear boundaries → Actor decomposition principle

### 3\. Implicit Principles Not Documented

Core tenets that drive the system exist only implicitly across ticket specs and code. The `docs/principles/` directory exists but contains only the Zen of Python - no Tasker-specific principles.

### 4\. ticket-specs Growing Unwieldy

Without a management strategy, ticket-specs will continue to accumulate. Many contain 10-20+ files per ticket with detailed phase breakdowns that served their purpose during implementation but now obscure the key insights.

### 5\. No Navigation Guide for Synthetic Minds

Both biological and synthetic minds face the same fundamental constraint: limited working memory/context window. Humans build tacit "when to reach for what" knowledge through experience. Each Claude session starts fresh - it knows *how* to search but not *what patterns to recognize* that should trigger a search, or *where* to look first.

The root `CLAUDE.md` is optimized for "get started quickly" rather than "navigate the knowledge space effectively."

---

## Current Documentation Landscape

### Structure Analysis

```
docs/
├── README.md                    # Excellent hub, but links to flat hierarchy
├── 24 top-level .md files       # Core docs, good content, flat organization
├── architecture-decisions/      # 3 ADRs - underutilized pattern
├── benchmarks/                  # 11 files - well organized
├── development/                 # 5 files - developer tooling
├── observability/               # 11 files - good depth
├── operations/                  # 2 files - sparse but useful
├── principles/                  # 1 file (Zen of Python only)
├── testing/                     # 3 files - testing patterns
├── ticket-specs/                # ~40 directories, ~150+ files - MASSIVE
└── worker-crates/               # 9 files - worker-specific docs
```

### Strengths

1. **Comprehensive Hub (docs/README.md)**: Excellent role-based navigation, topic indexes, "finding documentation by use case" sections
2. **ticket-specs/ Detail**: Incredibly thorough specifications with RCAs, phase summaries, implementation details
3. **Emergent Patterns**: Good subdirectory structure for specialized topics (observability, benchmarks, worker-crates)

### Weaknesses

1. **Flat Top-Level**: 24 .md files at root level without clear grouping
2. **Hidden Historical Knowledge**: Lessons learned buried in ticket-specs, not surfaced
3. **Underutilized ADRs**: Only 3 architecture decisions, despite many more implicit in ticket-specs
4. **Empty/Sparse Sections**: `archive/` referenced but doesn't exist, `operations/` has only 2 files
5. **Principles Nascent**: Only contains Zen of Python - no Tasker-specific principles
6. **ticket-specs Growing**: Will become unwieldy over time without management
7. **No Claude Navigation**: Each session rediscovers navigation patterns

---

## Proposed Organizational Structure

Reorganize documents by **cognitive function** (reference vs. guide vs. principle):

```
docs/
├── README.md                         # Documentation hub (keep, enhance)
├── CLAUDE-GUIDE.md                   # NEW: Navigation guide for synthetic minds
├── CHRONOLOGY.md                     # NEW: Historical development timeline
│
├── principles/                       # Core values and standards
│   ├── README.md                     # Principles index
│   ├── tasker-core-tenets.md         # Core architectural tenets (10 tenets)
│   ├── cross-language-consistency.md # Language-agnostic API design
│   ├── composition-over-inheritance.md
│   ├── defense-in-depth.md           # Idempotency layers
│   ├── zen-of-python-PEP-20.md       # Reference (keep)
│   └── rust-guidelines.md            # Microsoft/Rust API guidelines
│
├── architecture/                     # Architectural reference docs
│   ├── README.md                     # Architecture index
│   ├── crate-architecture.md         # (moved from root)
│   ├── actors.md                     # (moved from root)
│   ├── worker-actors.md              # (moved from root)
│   ├── states-and-lifecycles.md      # (moved from root)
│   ├── events-and-commands.md        # (moved from root)
│   ├── domain-events.md              # (moved from root)
│   ├── idempotency-and-atomicity.md  # (moved from root)
│   ├── backpressure-architecture.md  # (moved from root)
│   ├── circuit-breakers.md           # (moved from root)
│   └── deployment-patterns.md        # (moved from root)
│
├── decisions/                        # Architecture Decision Records
│   ├── README.md                     # ADR index with template
│   ├── TAS-46-actor-pattern.md       # Extracted from ticket-specs
│   ├── TAS-51-bounded-mpsc.md        # (moved from architecture-decisions/)
│   ├── TAS-54-ownership-removal.md   # Extracted from ticket-specs
│   ├── TAS-57-backoff-consolidation.md
│   ├── TAS-67-dual-event-system.md   # Extracted
│   ├── TAS-69-worker-decomposition.md # Extracted
│   ├── TAS-92-api-alignment.md       # Extracted
│   ├── TAS-100-ffi-over-wasm.md      # Extracted
│   ├── TAS-112-composition-pattern.md # Extracted
│   └── rca-parallel-execution-timing-bugs.md
│
├── guides/                           # How-to guides
│   ├── README.md                     # Guides index
│   ├── quick-start.md                # (moved from root)
│   ├── use-cases-and-patterns.md     # (moved from root)
│   ├── conditional-workflows.md      # (moved from root)
│   ├── batch-processing.md           # (moved from root)
│   ├── dlq-system.md                 # (moved from root)
│   ├── retry-semantics.md            # (moved from root)
│   └── configuration-management.md   # (moved from root)
│
├── workers/                          # Worker documentation (renamed from worker-crates/)
│   ├── README.md
│   ├── api-convergence-matrix.md
│   ├── patterns-and-practices.md
│   ├── example-handlers.md
│   ├── memory-management.md
│   ├── rust.md
│   ├── ruby.md
│   ├── python.md
│   └── typescript.md
│
├── development/                      # Developer reference (keep structure)
├── operations/                       # Operational guides (expand)
├── observability/                    # (keep current structure)
├── benchmarks/                       # (keep current structure)
├── testing/                          # (keep current structure)
│
├── reference/                        # Technical reference
│   ├── README.md
│   ├── table-management.md           # (moved from root)
│   ├── task-and-step-readiness.md    # (moved from root)
│   ├── sccache-configuration.md      # (moved from root)
│   ├── library-deployment-patterns.md
│   └── ffi-telemetry-pattern.md
│
└── ticket-specs/                     # Historical specs (with archival strategy)
    ├── README.md                     # NEW: Index with status per ticket
    └── [archived ticket directories]
```

---

## [CLAUDE-GUIDE.md](http://CLAUDE-GUIDE.md) Specification

### Purpose

Create a navigation skill for synthetic minds (Claude sessions) that provides:

1. **Trigger patterns** - "When you see X, consult Y"
2. **Layered depth** - Quick reference → deep dive paths
3. **Anti-patterns** - What NOT to search for (use training knowledge instead)
4. **Quick context loading** - Minimal reads to get oriented

### Rationale

Both biological and synthetic minds face the same constraint: limited working memory. The solutions are similar:

| Human | Claude |
| -- | -- |
| Can't hold entire codebase in mind | Can't hold entire codebase in context |
| Uses docs, notes, IDE navigation | Uses project knowledge search, file tools |
| Learns "where to look" over time | Needs to be told "where to look" per session |
| Benefits from consistent organization | Benefits from consistent organization |

Key difference: humans build tacit "when to reach for what" knowledge through experience. Each Claude session starts fresh. Good documentation extends cognition for *minds*, regardless of substrate.

### Proposed Structure

```markdown
# Tasker Core Navigation Guide for Claude Sessions

## Purpose

This guide helps Claude sessions efficiently navigate Tasker Core documentation.
Read this when starting work on the project to understand where information lives.

## Trigger → Document Mapping

| If the conversation involves... | First consult... | Then if deeper context needed... |
|--------------------------------|------------------|----------------------------------|
| Handler implementation patterns | `docs/workers/patterns-and-practices.md` | Language-specific: `docs/workers/{lang}.md` |
| Why something was designed this way | `docs/decisions/` index | Specific ADR for the feature |
| State transitions, task/step lifecycle | `docs/architecture/states-and-lifecycles.md` | |
| Cross-language API consistency | `docs/principles/cross-language-consistency.md` | `docs/workers/api-convergence-matrix.md` |
| Retry, backoff, error handling | `docs/guides/retry-semantics.md` | `docs/architecture/circuit-breakers.md` |
| Batch processing, cursors | `docs/guides/batch-processing.md` | |
| Something is stuck/failing | `docs/guides/dlq-system.md` | |
| Historical context, "why did we..." | `docs/CHRONOLOGY.md` | Specific ticket-spec if referenced |
| Core philosophy, design values | `docs/principles/tasker-core-tenets.md` | |
| FFI, language bindings, safety | `docs/reference/ffi-telemetry-pattern.md` | `docs/development/ffi-callback-safety.md` |
| Configuration, TOML structure | `docs/guides/configuration-management.md` | |
| Event systems, PGMQ, notifications | `docs/architecture/events-and-commands.md` | `docs/architecture/domain-events.md` |
| Actor patterns, service decomposition | `docs/architecture/actors.md` | `docs/architecture/worker-actors.md` |
| Concurrency, idempotency, atomicity | `docs/architecture/idempotency-and-atomicity.md` | `docs/principles/defense-in-depth.md` |

## Anti-Patterns: Don't Search For These

Use training knowledge directly for:
- General Rust/Ruby/Python/TypeScript language questions
- PostgreSQL syntax and standard operations
- Basic git, cargo, bundler, npm commands
- HTTP status codes, REST conventions
- Standard library usage

Only search Tasker-specific documentation for Tasker-specific concepts.

## Documentation Layers

Understanding the hierarchy helps choose where to look:

1. **Principles** (`docs/principles/`) - The "why" - core values and design philosophy
   - Read when: design decisions are questioned, evaluating alternatives
   
2. **Architecture** (`docs/architecture/`) - The "what" - system structure and patterns
   - Read when: understanding how components interact, debugging flow issues
   
3. **Guides** (`docs/guides/`) - The "how" - practical implementation guidance
   - Read when: implementing features, following workflows
   
4. **Decisions** (`docs/decisions/`) - The "why this over that" - ADRs with context
   - Read when: understanding trade-offs, revisiting past choices
   
5. **Workers** (`docs/workers/`) - Language-specific handler development
   - Read when: writing handlers, understanding FFI boundaries
   
6. **Reference** (`docs/reference/`) - The "details" - precise specifications
   - Read when: need exact behavior, edge cases, configuration options

## Quick Context Loading

When starting a session with limited project context:

1. **Already in context**: `CLAUDE.md` (root) - commands and essential structure
2. **First read**: `docs/principles/tasker-core-tenets.md` - the 10 core tenets
3. **If architecture questions**: `docs/architecture/crate-architecture.md`
4. **Then**: Navigate based on user's specific question using trigger table above

For deep-dive sessions, also consider:
- `docs/CHRONOLOGY.md` for historical evolution
- Relevant `docs/decisions/TAS-*.md` for specific features

## Linear Ticket Patterns

- Active tickets reference `docs/ticket-specs/TAS-XXX/` for detailed specs
- Key insights are extracted to `docs/decisions/` and `docs/CHRONOLOGY.md`
- Don't read full ticket-specs directories unless specifically asked
- Use the `ticket-specs/README.md` index to find status and key documents

## Project Knowledge Search Tips

When using `project_knowledge_search`:
- Search for concepts, not file names
- Use Tasker-specific terminology (e.g., "step handler", "decision point", "batch cursor")
- If results seem thin, try related terms from the tenets or architecture docs
- Remember: the search finds content, but this guide tells you *where* to start
```

### Integration Points

1. **Root **[**CLAUDE.md**](http://CLAUDE.md): Add brief reference to `docs/CLAUDE-GUIDE.md` for documentation navigation
2. **docs/README.md**: Add note that `CLAUDE-GUIDE.md` exists for AI-assisted sessions
3. **Project Instructions** (if using [Claude.ai](http://Claude.ai) Projects): Consider adding "Consult `docs/CLAUDE-GUIDE.md` for navigation patterns"

---

## Proposed Principles (docs/principles/)

### Core Tenets to Extract

Based on analysis of Linear tickets [TAS-41](https://linear.app/tasker-systems/issue/TAS-41/worker-rust-domain-handler) through [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization) and existing documentation:

| \# | Tenet | Origin | Description |
| -- | -- | -- | -- |
| 1 | **Defense in Depth** | [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery) | Multiple overlapping protection layers (DB atomicity, state guards, app logic, PGMQ semantics) |
| 2 | **Event-Driven with Polling Fallback** | [TAS-43](https://linear.app/tasker-systems/issue/TAS-43/event-driven-task-claiming-with-pg-notify) | Real-time via PostgreSQL LISTEN/NOTIFY, polling for reliability |
| 3 | **Composition Over Inheritance** | [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization) | Mixins/traits for handler composition, not class hierarchies |
| 4 | **Cross-Language Consistency** | [TAS-92](https://linear.app/tasker-systems/issue/TAS-92/consistent-developer-space-apis-and-ergonomics-parent), [TAS-100](https://linear.app/tasker-systems/issue/TAS-100/bun-and-nodejs-typescript-worker) | Unified APIs across Rust, Ruby, Python, TypeScript |
| 5 | **Actor-Based Decomposition** | [TAS-46](https://linear.app/tasker-systems/issue/TAS-46/actors-and-services), [TAS-69](https://linear.app/tasker-systems/issue/TAS-69/worker-command-actor-service-refactor) | Lightweight actors for lifecycle management |
| 6 | **State Machine Rigor** | [TAS-41](https://linear.app/tasker-systems/issue/TAS-41/worker-rust-domain-handler) | Dual state machines (Task + Step) for atomic transitions |
| 7 | **Audit Before Enforce** | [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery) | Track for observability, don't block for "ownership" |
| 8 | **Pre-Alpha Freedom** | All | Break things early to get architecture right |
| 9 | **PostgreSQL as Foundation** | Architecture | PGMQ, atomic SQL functions, database-level guarantees |
| 10 | **Bounded Resources** | [TAS-51](https://linear.app/tasker-systems/issue/TAS-51/bounded-and-unbounded-mpsc-channels) | All channels bounded, backpressure everywhere |

### Principles Documents to Create

1. `tasker-core-tenets.md`: The 10 core tenets with explanations and examples
2. `cross-language-consistency.md`: The "one API" philosophy - `call(context)`, `success()`/`failure()`, registry APIs
3. `composition-over-inheritance.md`: The mixin pattern, why inheritance is harmful for handlers
4. `defense-in-depth.md`: The four-layer protection model from [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery)

---

## Historical Chronology (docs/CHRONOLOGY.md)

### Key Development Moments to Capture

| Date | Ticket | Category | Insight |
| -- | -- | -- | -- |
| 2025-08 | [TAS-28](https://linear.app/tasker-systems/issue/TAS-28/tasker-core-web) | Foundation | Axum web API established |
| 2025-08 | [TAS-29](https://linear.app/tasker-systems/issue/TAS-29/comprehensive-observability-and-benchmarking-modernization) | Observability | OpenTelemetry + correlation IDs + benchmarking |
| 2025-08 | [TAS-37](https://linear.app/tasker-systems/issue/TAS-37/taskfinalizer-race-condition) | Bug Fix | Task finalizer race condition eliminated |
| 2025-10 | [TAS-41](https://linear.app/tasker-systems/issue/TAS-41/worker-rust-domain-handler) | Architecture | Enhanced state machines with richer task states |
| 2025-10 | [TAS-43](https://linear.app/tasker-systems/issue/TAS-43/event-driven-task-claiming-with-pg-notify) | Architecture | Event-driven task claiming via pg_notify |
| 2025-10 | [TAS-46](https://linear.app/tasker-systems/issue/TAS-46/actors-and-services) | Architecture | Actor-based lifecycle components introduced |
| 2025-10 | [TAS-49](https://linear.app/tasker-systems/issue/TAS-49/comprehensive-task-dead-letter-queue-dlq-and-lifecycle-management) | Feature | Dead Letter Queue system |
| 2025-10 | [TAS-51](https://linear.app/tasker-systems/issue/TAS-51/bounded-and-unbounded-mpsc-channels) | Resilience | Bounded MPSC channels everywhere |
| 2025-10 | [TAS-53](https://linear.app/tasker-systems/issue/TAS-53/dynamic-workflows-and-decisionpoint-workflow-steps) | Feature | Dynamic workflows and decision points |
| 2025-10 | [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery) | **Breakthrough** | Ownership enforcement removed - tasks auto-recover |
| 2025-11 | [TAS-59](https://linear.app/tasker-systems/issue/TAS-59/batch-processing-implementation-plan) | Feature | Batch processing with cursor workers |
| 2025-11 | [TAS-65](https://linear.app/tasker-systems/issue/TAS-65/distributed-event-system-architecture) | Architecture | Distributed event system (durable/fast/broadcast) |
| 2025-12 | [TAS-67](https://linear.app/tasker-systems/issue/TAS-67/rust-worker-dual-event-system) | Architecture | Rust worker dual-event system |
| 2025-12 | [TAS-69](https://linear.app/tasker-systems/issue/TAS-69/worker-command-actor-service-refactor) | Refactor | Worker actor-service decomposition |
| 2025-12 | [TAS-72](https://linear.app/tasker-systems/issue/TAS-72/pyo3-python-worker-foundations) | Workers | Python worker via PyO3 |
| 2025-12 | [TAS-75](https://linear.app/tasker-systems/issue/TAS-75/backpressure-consistency) | Resilience | Backpressure and circuit breakers unified |
| 2025-12 | [TAS-92](https://linear.app/tasker-systems/issue/TAS-92/consistent-developer-space-apis-and-ergonomics-parent) | API | Cross-language API alignment |
| 2025-12 | [TAS-100](https://linear.app/tasker-systems/issue/TAS-100/bun-and-nodejs-typescript-worker) | Workers | TypeScript worker via FFI (FFI > WASM decision) |
| 2025-12 | [TAS-111](https://linear.app/tasker-systems/issue/TAS-111/makefiletoml-enhancements) | Tooling | cargo-make standardization |
| 2025-12 | [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization) | Research | Handler ergonomics analysis |

### Lessons Learned to Extract

| Ticket | Lesson |
| -- | -- |
| [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery) | "Processor UUID ownership was redundant protection with harmful side effects" |
| [TAS-100](https://linear.app/tasker-systems/issue/TAS-100/bun-and-nodejs-typescript-worker) | "FFI wins over WASM for pragmatic reasons - WASI networking immature" |
| [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization) | "Batchable already uses mixin pattern - this is the TARGET architecture" |
| TAS-67/69 | "Actor pattern enables testability and clear boundaries" |
| RCA | "Parallel execution timing bugs came from ordering assumptions" |

---

## Outdated/Superfluous Documentation Assessment

### Candidates for Removal or Archival

| Document | Status | Recommendation |
| -- | -- | -- |
| `docs/ticket-specs/TAS-50/archive/` | 22 files of config analysis | Keep 1 summary, archive rest |
| `docs/ticket-specs/TAS-60/` | 5 files on config bug (marked Duplicate) | Archive or delete |
| `docs/ticket-specs/TAS-61/` | 22 files on config - overlaps [TAS-50](https://linear.app/tasker-systems/issue/TAS-50/configuration-generator) | Consolidate with [TAS-50](https://linear.app/tasker-systems/issue/TAS-50/configuration-generator) |
| Detailed phase files in many ticket-specs | Implementation details | Keep key insights, summarize rest |
| `environment-configuration-comparison.md` (root) | May be outdated post-TAS-50 | Verify currency |

### ticket-specs/ Archival Strategy

For each ticket-spec directory:

1. **Keep**: [README.md](http://README.md) or main spec, any "lessons learned" or "summary" documents
2. **Archive**: Detailed phase breakdowns, analysis artifacts, intermediate files
3. **Delete**: Duplicate or superseded content

Create a `docs/ticket-specs/README.md` with:

* Status of each ticket (Done, In Progress, Cancelled)
* Links to key documents only
* Brief description of what was accomplished

---

## Deliverables

### Phase 1: Principles Extraction (Priority: High)

- [ ] Create `docs/principles/README.md` (index)
- [ ] Create `docs/principles/tasker-core-tenets.md` (10 core tenets)
- [ ] Create `docs/principles/cross-language-consistency.md`
- [ ] Create `docs/principles/composition-over-inheritance.md`
- [ ] Create `docs/principles/defense-in-depth.md`

### Phase 1.5: Claude Navigation Guide (Priority: High)

- [ ] Create `docs/CLAUDE-GUIDE.md` with trigger patterns and navigation structure
- [ ] Add reference to [CLAUDE-GUIDE.md](http://CLAUDE-GUIDE.md) in root `CLAUDE.md`
- [ ] Add note about [CLAUDE-GUIDE.md](http://CLAUDE-GUIDE.md) in `docs/README.md`
- [ ] Update Project Instructions if using [Claude.ai](http://Claude.ai) Projects

### Phase 2: Historical Chronology (Priority: High)

- [ ] Create `docs/CHRONOLOGY.md`
- [ ] Document major architectural decisions ([TAS-46](https://linear.app/tasker-systems/issue/TAS-46/actors-and-services), 54, 67, 100)
- [ ] Document feature milestones (batch, DLQ, decision points)
- [ ] Document lessons learned from RCAs and post-mortems

### Phase 3: Structural Reorganization (Priority: Medium)

- [ ] Create `docs/architecture/` and move 10 architecture docs
- [ ] Create `docs/guides/` and move 7 how-to docs
- [ ] Create `docs/reference/` and move 5 reference docs
- [ ] Rename `worker-crates/` to `workers/`
- [ ] Update all internal links
- [ ] Update [CLAUDE-GUIDE.md](http://CLAUDE-GUIDE.md) trigger table with final paths

### Phase 4: ADR Extraction (Priority: Medium)

- [ ] Rename `architecture-decisions/` to `decisions/`
- [ ] Extract ADR from [TAS-54](https://linear.app/tasker-systems/issue/TAS-54/processor-uuid-ownership-analysis-and-stale-task-recovery) (ownership removal)
- [ ] Extract ADR from [TAS-100](https://linear.app/tasker-systems/issue/TAS-100/bun-and-nodejs-typescript-worker) (FFI over WASM)
- [ ] Extract ADR from [TAS-46](https://linear.app/tasker-systems/issue/TAS-46/actors-and-services) (actor pattern)
- [ ] Extract ADR from [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization) (composition pattern)

### Phase 5: ticket-specs Management (Priority: Medium)

- [ ] Create `docs/ticket-specs/README.md` index
- [ ] Mark each ticket with status (Done/InProgress/Cancelled)
- [ ] Consolidate [TAS-50](https://linear.app/tasker-systems/issue/TAS-50/configuration-generator), [TAS-60](https://linear.app/tasker-systems/issue/TAS-60/config-merger-bug), [TAS-61](https://linear.app/tasker-systems/issue/TAS-61/configuration-architecture-redesign-system-orchestration-worker) config docs
- [ ] Archive TAS-50/archive/ (keep summary)
- [ ] Define ongoing ticket-spec archival process

### Phase 6: Validation (Priority: High)

- [ ] Update docs/README.md hub with new structure
- [ ] Verify all links work
- [ ] Update [CLAUDE.md](http://CLAUDE.md) if needed
- [ ] Test "finding documentation" scenarios
- [ ] Test [CLAUDE-GUIDE.md](http://CLAUDE-GUIDE.md) navigation patterns in fresh session

---

## Success Criteria

1. `docs/` has clear hierarchical structure matching cognitive categories
2. Core principles explicitly documented in 4-5 dedicated files
3. [**CLAUDE-GUIDE.md**](http://CLAUDE-GUIDE.md)** provides effective navigation for synthetic minds**
4. Historical chronology captures 20+ key development moments and lessons
5. ticket-specs reduced from \~150 files to essential summaries per ticket
6. ADRs extracted for 6+ major architectural decisions
7. No broken links in documentation
8. docs/README.md hub reflects new structure

---

## Non-Goals

* Rewriting content (only reorganizing and extracting)
* Adding new technical documentation beyond principles/chronology/navigation
* Changing [CLAUDE.md](http://CLAUDE.md) significantly (minor updates only)
* Documenting features still in development

---

## Estimated Effort

| Phase | Effort | Notes |
| -- | -- | -- |
| Phase 1: Principles | 2-3 days | High-value extraction, requires careful reading of ticket-specs |
| Phase 1.5: Claude Guide | 0.5-1 day | Structure defined above, needs refinement after Phase 3 |
| Phase 2: Chronology | 1-2 days | Straightforward extraction from Linear and ticket-specs |
| Phase 3: Restructure | 2-3 days | File moves, link updates, testing |
| Phase 4: ADRs | 1-2 days | Extract and format key decisions |
| Phase 5: ticket-specs | 1-2 days | Index creation, archival decisions |
| Phase 6: Validation | 0.5-1 day | Link checking, hub update, navigation testing |
| **Total** | **8-14 days** |  |

---

## Related Tickets

* [TAS-111](https://linear.app/tasker-systems/issue/TAS-111/makefiletoml-enhancements): Makefile.toml enhancements (cargo-make documentation patterns)
* [TAS-112](https://linear.app/tasker-systems/issue/TAS-112/cross-language-step-handler-ergonomics-analysis-and-harmonization): Cross-language ergonomics (documents needed for principles)
* **TAS-108/109/112**: Upcoming handler API changes (will need doc updates)
* [TAS-66](https://linear.app/tasker-systems/issue/TAS-66/tasker-blog-rebuild-plan-tasker-core-centric-documentation): Tasker Blog rebuild (separate docs site, related but distinct)

---

## References

### Key ticket-specs to Extract From

* `docs/ticket-specs/TAS-54/COMPREHENSIVE-FINDINGS-SUMMARY.md` - Defense in depth, ownership removal
* `docs/ticket-specs/TAS-100/analysis.md` - FFI vs WASM decision
* `docs/ticket-specs/TAS-112/recommendations.md` - Composition pattern
* `docs/ticket-specs/TAS-67/summary.md` - Dual event system
* `docs/ticket-specs/TAS-69/09-summary.md` - Worker decomposition

### Documentation Inspiration

* Claude Skills pattern (`/mnt/skills/` with [SKILL.md](http://SKILL.md) files)
* Rust API Guidelines ([https://rust-lang.github.io/api-guidelines/](https://rust-lang.github.io/api-guidelines/))
* ADR format ([https://adr.github.io/](https://adr.github.io/))

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-99/documentation-audit-and-restructuring](https://linear.app/tasker-systems/issue/TAS-99/documentation-audit-and-restructuring)
- Identifier: TAS-99
- Status: In Progress
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Docs](https://linear.app/tasker-systems/project/tasker-core-docs-e61593b81ed1). 
- Created: 2025-12-19T02:29:58.961Z
- Updated: 2025-12-29T12:59:17.804Z
