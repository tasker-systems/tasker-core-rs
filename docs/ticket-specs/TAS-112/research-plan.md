# TAS-112: Cross-Language Step Handler Ergonomics Analysis

**Status**: Research Phase  
**Created**: 2025-12-27  
**Owner**: Engineering Team  
**Related Tickets**: TAS-109 (Domain Events), TAS-88 (Batch Processing), TAS-53 (Conditional Workflows)

---

## Executive Summary

This ticket addresses the cross-language ergonomic inconsistencies in our step handler APIs across Rust, Ruby, Python, and TypeScript. As a greenfield pre-alpha open source project, we have an opportunity to establish consistent developer-facing interfaces before hitting 1.0. This research plan breaks down the analysis into manageable phases to identify and document discrepancies in method signatures, data structures, and handler patterns.

## Problem Statement

### Current State

Our four language implementations (Rust, Ruby, Python, TypeScript) have evolved independently, leading to:

1. **Cognitive Drift**: Developers switching between languages encounter different method names, parameter orders, and return types
2. **Documentation Mismatches**: Cross-language documentation must explain variations rather than presenting a unified mental model
3. **Onboarding Friction**: New contributors must learn language-specific quirks rather than core concepts
4. **Maintenance Burden**: Changes to handler interfaces require language-specific adaptations

### Known Inconsistencies

From preliminary analysis, we've identified several areas of concern:

**Base Handler Result APIs**:
- Ruby: `success(result:, metadata:)` (keyword required) → returns `Success`/`Error` types
- Python: `success(result, metadata)` (positional allowed) → returns single `StepHandlerResult` class
- TypeScript: `success(result, metadata)` → returns `StepHandlerResult` class
- Ruby has `error_code` field, Python/TypeScript do not

**Batchable Handler Patterns**:
- Ruby: Uses subclass inheritance (`class Handler < Batchable`)
- Python: Uses mixin pattern (`class Handler(StepHandler, Batchable)`)
- TypeScript: Implementation status unclear
- Different method names for equivalent operations

**Context Access**:
- Ruby: `context.get_task_field('field')`, `context.get_dependency_result('step')`
- Python: `context.input_data.get("field")`, `context.dependency_results.get("step")`
- TypeScript: Pattern unclear

### Goals

1. **Document Current State**: Create comprehensive inventory of handler APIs across all languages
2. **Identify Gaps**: Find features present in some languages but missing in others
3. **Propose Harmonization**: Develop recommendations for consistent APIs without breaking existing code
4. **Update Documentation**: Revise docs/worker-crates/*.md to reflect findings

---

## Research Approach

This analysis is intentionally broken into **sequential phases** to avoid overwhelming scope. Each phase produces a standalone document that informs the next phase.

### Phase Structure

```
Phase 1: Core Documentation Analysis (this document)
    ↓
Phase 2: Base Handler APIs (base.md)
    ↓
Phase 3: API Handler Pattern (api.md)
    ↓
Phase 4: Decision Handler Pattern (decision.md)
    ↓
Phase 5: Batchable Handler Pattern (batchable.md)
    ↓
Phase 6: Batch Processing Lifecycle (batch-processing.md)
    ↓
Phase 7: Domain Events Integration (domain-events.md)
    ↓
Phase 8: Conditional Workflows (conditional-workflows.md)
    ↓
Phase 9: Synthesis & Recommendations (recommendations.md)
```

---

## Phase 1: Core Documentation Analysis

**Objective**: Review existing documentation to understand intended patterns and identify documentation gaps.

**Scope**:
- `docs/batch-processing.md` (reviewed)
- `docs/domain-events.md` (reviewed)
- `docs/conditional-workflows.md` (reviewed)
- `docs/worker-crates/*.md` (reviewed)

**Deliverables**:
1. Summary of documented patterns per language
2. List of undocumented features found in code but not in docs
3. Identification of conflicting guidance across documents

**Key Questions**:
- What is the "canonical" API according to documentation?
- Are there patterns documented in one language but not others?
- Do code examples match prose descriptions?

**Status**: ✅ **COMPLETED** (via this document)

### Findings Summary

**Documentation Coverage**:
- ✅ Rust: Well-documented with clear handler trait patterns
- ✅ Ruby: Good coverage of base/API/decision/batchable handlers
- ✅ Python: Solid documentation of handler patterns and event system
- ⚠️ TypeScript: Documentation exists but needs expansion

**Documented Patterns**:
All languages document:
- Base handler with `call(context)` method
- Success/failure result helpers
- API handler for HTTP integration
- Decision handler for conditional workflows
- Batchable handlers for batch processing

**Known Documentation Issues**:
1. Result API differences between Ruby (keyword args) and Python/TypeScript (positional) not highlighted
2. Batchable inheritance vs mixin pattern not explained as intentional design choice
3. TypeScript batch processing patterns need more examples
4. Domain event publishing integration varies by language (covered in separate docs)

---

## Phase 2: Base Handler APIs

**Objective**: Analyze core handler interfaces that all handlers inherit/implement.

**Files to Analyze**:
- `workers/rust/src/step_handlers/mod.rs`
- `workers/ruby/lib/tasker_core/step_handler/base.rb`
- `workers/python/python/tasker_core/step_handler/base.py`
- `workers/typescript/src/handler/base.ts`

**Analysis Matrix**:

| Aspect | Rust | Ruby | Python | TypeScript |
|--------|------|------|--------|------------|
| Handler trait/class name | `RustStepHandler` | `Base` | `StepHandler` | `StepHandler` |
| Call signature | `async fn call(&self, &TaskSequenceStep)` | `def call(context)` | `def call(self, context)` | `async call(context)` |
| Result type | `StepExecutionResult` | `StepHandlerCallResult` | `StepHandlerResult` | `StepHandlerResult` |
| Success helper | `success_result()` | `success(result:, metadata:)` | `success(result, metadata)` | `success(result, metadata)` |
| Failure helper | `failure_result()` | `failure(message:, error_type:, ...)` | `failure(message, error_type, ...)` | `failure(message, errorType, ...)` |
| Error code field | ❓ | ✅ `error_code` | ❌ | ❓ |
| Metadata support | ✅ | ✅ | ✅ | ✅ |
| Config schema | ❓ | `config_schema()` | `config_schema()` | `configSchema()` |
| Capabilities | ❓ | `capabilities()` | `capabilities` property | `capabilities` getter |

**Key Questions**:
1. Should all languages support `error_code` field?
2. Should Ruby move to positional args or others adopt keyword args?
3. Are async/sync differences acceptable or should we align?
4. Should `success()`/`failure()` return types be unified?

**Next Steps**:
- Map out complete method signatures for each language
- Identify missing methods in each implementation
- Document intentional vs. accidental differences

**Output**: `TAS-112/base.md`

---

## Phase 3: API Handler Pattern

**Objective**: Compare HTTP/REST API integration handlers.

**Files to Analyze**:
- `workers/ruby/lib/tasker_core/step_handler/api.rb`
- `workers/python/python/tasker_core/step_handler/api.py`
- `workers/typescript/src/handler/api.ts`
- Rust: ❓ (may not have dedicated API handler - needs investigation)

**Analysis Dimensions**:
- HTTP method helpers (get, post, put, delete, patch)
- Error classification (4xx vs 5xx, rate limiting)
- Response type structures
- Retry-After header handling
- Timeout configuration
- Connection pooling

**Key Questions**:
1. Does Rust need an API handler base class?
2. Are HTTP status code classifications consistent?
3. Do all implementations handle Retry-After headers?
4. Should there be a shared error classification mapping?

**Output**: `TAS-112/api.md`

---

## Phase 4: Decision Handler Pattern

**Objective**: Compare conditional workflow routing handlers (TAS-53).

**Files to Analyze**:
- `workers/ruby/lib/tasker_core/step_handler/decision.rb`
- `workers/python/python/tasker_core/step_handler/decision.py`
- `workers/typescript/src/handler/decision.ts`
- Rust: ❓ (likely in conditional workflow examples)

**Analysis Dimensions**:
- `DecisionPointOutcome` data structure
- `create_steps()` vs `no_branches()` methods
- Routing context propagation
- Template step resolution

**Key Questions**:
1. Are outcome structures identical across languages?
2. Do all languages support both `create_steps` and `no_branches`?
3. How is routing context passed to created steps?
4. Are helper method names consistent?

**Output**: `TAS-112/decision.md`

---

## Phase 5: Batchable Handler Pattern

**Objective**: Compare batch processing handler interfaces.

**Files to Analyze**:
- `workers/ruby/lib/tasker_core/step_handler/batchable.rb`
- `workers/python/python/tasker_core/batch_processing/batchable.py`
- `workers/typescript/src/handler/batchable.ts`
- `workers/rust/src/step_handlers/batch_processing_*.rs`

**Analysis Dimensions**:
- Inheritance vs mixin pattern
- Analyzer vs worker vs aggregator roles
- Cursor config creation helpers
- Batch context extraction methods
- No-op worker handling

**Key Questions**:
1. Why did Ruby choose subclass, Python choose mixin?
2. Are helper method names aligned (`get_batch_context`, `handle_no_op_worker`)?
3. Do all languages have equivalent cursor config builders?
4. Is the three-phase pattern (analyze/process/aggregate) consistent?

**Output**: `TAS-112/batchable.md`

---

## Phase 6: Batch Processing Lifecycle

**Objective**: Trace the full batch processing workflow across languages.

**Reference Documentation**:
- `docs/batch-processing.md` (comprehensive reference)

**Analysis Dimensions**:
- `BatchProcessingOutcome` structure (NoBatches vs CreateBatches)
- `CursorConfig` structure and flexibility
- `BatchWorkerInputs` and `BatchMetadata`
- `BatchAggregationScenario` detection
- Checkpoint mechanisms

**Key Questions**:
1. Are data structures identical at FFI boundaries?
2. Do all languages support flexible cursor types (int, UUID, timestamp)?
3. Are checkpoint APIs consistent?
4. Is aggregation scenario detection identical?

**Output**: `TAS-112/batch-processing.md`

---

## Phase 7: Domain Events Integration

**Objective**: Understand how domain event publishing integrates with handlers (relates to TAS-109).

**Files to Analyze**:
- `workers/ruby/lib/tasker_core/domain_events.rb`
- `workers/python/python/tasker_core/domain_events.py`
- `workers/rust/src/step_handlers/domain_event_*.rs`
- TypeScript: ❓ (needs investigation)

**Analysis Dimensions**:
- Event publisher registration patterns
- Post-execution vs in-execution publishing
- Event routing (durable vs fast paths)
- Context propagation to event handlers

**Key Questions**:
1. Is TAS-65 (post-execution publishing) fully implemented in all languages?
2. Do all languages support dual-path delivery?
3. Are event payload structures consistent?
4. Should event publishing be a mixin/trait or base class feature?

**Output**: `TAS-112/domain-events.md`

---

## Phase 8: Conditional Workflows

**Objective**: Analyze conditional workflow patterns and convergence handling (TAS-53).

**Reference Documentation**:
- `docs/conditional-workflows.md`

**Analysis Dimensions**:
- Decision point outcome creation
- Deferred convergence step handling
- Intersection semantics
- NoBranches vs WithBranches scenarios

**Key Questions**:
1. Are conditional workflow patterns documented in all language docs?
2. Do all languages have convergence step examples?
3. Is intersection semantics handling consistent?
4. Are helper methods for scenario detection aligned?

**Output**: `TAS-112/conditional-workflows.md`

---

## Phase 9: Synthesis & Recommendations

**Objective**: Consolidate findings and propose harmonization strategy.

**Inputs**:
- All phase documents (base.md through conditional-workflows.md)
- Stakeholder feedback
- Breaking change budget

**Deliverables**:
1. **Inconsistency Matrix**: Complete table of all API differences
2. **Impact Assessment**: Breaking vs non-breaking changes
3. **Migration Path**: How to evolve toward consistency
4. **Updated Documentation**: Revised worker-crates docs

**Recommendation Categories**:
- **Immediate Harmonization**: Non-breaking additions (e.g., add missing methods)
- **Deprecation Path**: Breaking changes with migration period (e.g., unify result APIs)
- **Intentional Differences**: Language-idiomatic variations to preserve (e.g., Ruby keyword args)
- **Future Considerations**: Defer to post-1.0 (e.g., major API redesign)

**Output**: `TAS-112/recommendations.md`

---

## Success Criteria

This research phase is successful if:

1. ✅ All 9 phases produce standalone analysis documents
2. ✅ We identify every API inconsistency across the 4 languages
3. ✅ We distinguish intentional from accidental differences
4. ✅ We have actionable recommendations with clear priorities
5. ✅ Updated documentation accurately reflects current state
6. ✅ Follow-up tickets are scoped for implementation

---

## Timeline and Resourcing

**Estimated Effort**: 3-5 days of focused research

**Phase Breakdown**:
- Phase 1 (Documentation): 0.5 days ✅ **COMPLETED**
- Phase 2 (Base APIs): 0.5 days
- Phase 3 (API Handler): 0.5 days
- Phase 4 (Decision Handler): 0.5 days
- Phase 5 (Batchable): 0.5 days
- Phase 6 (Batch Processing): 0.5 days
- Phase 7 (Domain Events): 0.5 days
- Phase 8 (Conditional Workflows): 0.5 days
- Phase 9 (Synthesis): 1.0 days

**Parallel Work**: Phases 2-8 can be partially parallelized, but sequential review ensures findings inform later phases.

---

## Related Documentation

### Primary References
- `docs/batch-processing.md` - Comprehensive batch processing patterns
- `docs/domain-events.md` - Domain event publishing architecture
- `docs/conditional-workflows.md` - Conditional workflow and decision patterns
- `docs/worker-crates/patterns-and-practices.md` - Cross-language patterns

### Worker Documentation
- `docs/worker-crates/rust.md` - Rust worker implementation
- `docs/worker-crates/ruby.md` - Ruby worker implementation
- `docs/worker-crates/python.md` - Python worker implementation
- `docs/worker-crates/typescript.md` - TypeScript worker implementation (if exists)

### Related Tickets
- **TAS-109**: Domain event completeness across languages
- **TAS-88**: Batch processing support
- **TAS-53**: Conditional workflows and deferred convergence
- **TAS-65**: Post-execution domain event publishing

---

## Next Steps

1. ✅ Phase 1 complete (this document)
2. Begin Phase 2: Base Handler API analysis
3. Use findings to inform subsequent phases
4. Schedule review sessions after Phase 5 and Phase 9

---

## Appendix: Quick Reference

### File Locations by Language

**Rust**:
- Base: `workers/rust/src/step_handlers/mod.rs`
- Registry: `workers/rust/src/step_handlers/registry.rs`
- Examples: `workers/rust/src/step_handlers/*.rs`

**Ruby**:
- Base: `workers/ruby/lib/tasker_core/step_handler/base.rb`
- API: `workers/ruby/lib/tasker_core/step_handler/api.rb`
- Decision: `workers/ruby/lib/tasker_core/step_handler/decision.rb`
- Batchable: `workers/ruby/lib/tasker_core/step_handler/batchable.rb`
- Batch Processing: `workers/ruby/lib/tasker_core/batch_processing/`
- Domain Events: `workers/ruby/lib/tasker_core/domain_events.rb`

**Python**:
- Base: `workers/python/python/tasker_core/step_handler/base.py`
- API: `workers/python/python/tasker_core/step_handler/api.py`
- Decision: `workers/python/python/tasker_core/step_handler/decision.py`
- Batchable: `workers/python/python/tasker_core/batch_processing/batchable.py`
- Batch Processing: `workers/python/python/tasker_core/batch_processing/`
- Domain Events: `workers/python/python/tasker_core/domain_events.py`

**TypeScript**:
- Base: `workers/typescript/src/handler/base.ts`
- API: `workers/typescript/src/handler/api.ts`
- Decision: `workers/typescript/src/handler/decision.ts`
- Batchable: `workers/typescript/src/handler/batchable.ts`
- Registry: `workers/typescript/src/handler/registry.ts`

### Documentation to Update

After Phase 9:
- `docs/worker-crates/rust.md` - Add missing patterns, clarify APIs
- `docs/worker-crates/ruby.md` - Document keyword arg rationale
- `docs/worker-crates/python.md` - Explain mixin pattern choice
- `docs/worker-crates/patterns-and-practices.md` - Add consistency guidelines
- `docs/batch-processing.md` - Add TypeScript examples if missing
- `docs/domain-events.md` - Harmonize event publishing examples
- `docs/conditional-workflows.md` - Ensure all language examples present

---

## Metadata

**Document Version**: 1.0  
**Last Updated**: 2025-12-27  
**Reviewers**: TBD  
**Approval Status**: Draft
