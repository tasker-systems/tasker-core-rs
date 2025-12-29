# Cross-Language Step Handler Ergonomics Analysis & Harmonization

## Executive Summary

Comprehensive 9-phase research completed analyzing cross-language ergonomics for step handlers across Rust, Ruby, Python, and TypeScript. **Pre-alpha greenfield project** - breaking changes encouraged to achieve correct architecture.

**Key Architectural Discoveries:**

1. **Composition Over Inheritance** - Batchable handlers already use mixin pattern; this is the TARGET architecture for all handler types
2. **Orchestration vs Worker Features** - Domain events and conditional workflows are orchestration-owned with worker-side APIs
3. **FFI Boundaries Matter** - Batch processing and decision outcomes require exact serialization alignment

**Overall Language Assessment:**

* ✅ **Ruby**: Most mature, feature-complete (reference implementation)
* ⚠️ **Python**: Good core APIs but missing lifecycle hooks
* ⚠️ **TypeScript**: Good core APIs but **missing domain events entirely**
* ✅ **Rust**: Excellent orchestration, needs handler-level ergonomics (traits/helpers)

---

## Critical Gaps Identified

### CRITICAL Priority

| Gap | Affected | Action |
| -- | -- | -- |
| TypeScript domain events | TS only | Create complete module (BasePublisher, BaseSubscriber, InProcessDomainEventPoller) |
| Composition pattern | All | Migrate from inheritance to mixin pattern |

### HIGH Priority

| Gap | Affected | Action |
| -- | -- | -- |
| Lifecycle hooks | Python/TS | Add before_publish/after_publish/on_error hooks |
| FFI boundary types | Python/TS | Create explicit BatchProcessingOutcome, CursorConfig classes |
| Rust handler traits | Rust | Add version, capabilities, decision/batch helpers |
| Checkpoint write APIs | Python/TS | Add update_checkpoint() or update_step_results() |
| Conditional workflow examples | Python/TS | Complete examples with convergence patterns |
| Ruby result unification | Ruby | Unify Success/Error into single result class |

### MEDIUM Priority

| Gap | Affected | Action |
| -- | -- | -- |
| Batch aggregation flexibility | Python/TS | Add custom aggregation function support |
| Ruby cursor indexing | Ruby | Change from 1-indexed to 0-indexed |
| Ruby API helpers | Ruby | Add api_success(), api_failure(), PATCH support |

---

## Migration Roadmap

### Now

* TypeScript Domain Events Implementation (CRITICAL)
* Python/TypeScript Lifecycle Hooks
* FFI Boundary Type Standardization
* Conditional Workflow Examples (Python/TypeScript)
* Checkpoint Write APIs
* Batch Aggregation Flexibility

### Next

* Composition Pattern Migration (BREAKING)
* Ruby Result Unification (BREAKING)
* Ruby Cursor Indexing Fix (BREAKING)
* Rust Handler Traits
* Ruby API Handler Enhancements
* Documentation Overhaul

---

## Breaking Changes (Plan Together)

These breaking changes should be coordinated in a single release:

1. **Composition Pattern Migration** - Change `class Handler < API` to `class Handler < Base; include API`
2. **Ruby Result Unification** - Change type checks from `is_a?(Success)` to `result.success?`
3. **Ruby Cursor Indexing** - Change from 1-indexed to 0-indexed

---

## New Documentation Required

1. `docs/ffi-boundary-types.md` - FFI structure serialization reference
2. `docs/domain-events-publishing-flow.md` - Publishing flow and custom publishers
3. `docs/conditional-workflows-convergence.md` - Convergence and intersection semantics
4. `docs/batch-processing-checkpoints.md` - Checkpoint read/write patterns
5. `docs/composition-pattern-guide.md` - Mixin/trait composition patterns

---

## Success Criteria

**Technical:**

- [ ] All four languages have equivalent handler APIs
- [ ] FFI boundary types explicitly defined and documented
- [ ] All handlers use composition pattern
- [ ] TypeScript has complete domain events module
- [ ] Python/TypeScript have checkpoint write APIs
- [ ] Ruby uses unified result class
- [ ] Rust has ergonomic handler traits

**Documentation:**

- [ ] 5 new documentation files created
- [ ] All worker crate docs updated with complete examples
- [ ] Cross-language comparison tables in pattern docs
- [ ] Migration guides for breaking changes

**Quality:**

- [ ] All examples passing E2E tests
- [ ] Zero serialization bugs at FFI boundaries
- [ ] "One obvious way" to implement each pattern

---

## Research Artifacts

All 9 phase documents completed in `docs/ticket-specs/TAS-112/`:

* `research-plan.md` - Phase 1: Core Documentation Analysis
* `base.md` - Phase 2: Base Handler Pattern
* `api.md` - Phase 3: API Handler Pattern
* `decision.md` - Phase 4: Decision Handler Pattern
* `batchable.md` - Phase 5: Batchable Handler Pattern
* `batch-processing.md` - Phase 6: Batch Processing Lifecycle
* `domain-events.md` - Phase 7: Domain Events Integration
* `conditional-workflows.md` - Phase 8: Conditional Workflows
* `recommendations.md` - Phase 9: Synthesis & Recommendations (12,000+ lines total)

---

**Key Insight:** Architecture is sound (orchestration separation, FFI boundaries, composition pattern emerging). We need to complete the migration and fill the documented gaps.
