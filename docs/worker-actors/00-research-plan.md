# TAS-69 Worker Command-Actor-Service Refactor Analysis

## Research Plan

This document outlines the analysis approach for the TAS-69 refactor migration from main to the feature branch.

### Objective

Thoroughly analyze and document the changes made in TAS-69 to:
1. Map before-and-after states clearly
2. Identify any missed nuances or edge cases
3. Verify that behavior is preserved or intentionally changed
4. Document where test coverage may have shifted

### Analysis Dimensions

| Document | Focus Area | Status |
|----------|------------|--------|
| `01-structural-changes.md` | File additions, removals, reorganization | ✅ Complete |
| `02-command-processor-migration.md` | WorkerProcessor → ActorCommandProcessor | ✅ Complete |
| `03-service-decomposition.md` | Service layer extraction and responsibilities | ✅ Complete |
| `04-actor-implementations.md` | Actor traits, handlers, and registry | ✅ Complete |
| `05-bootstrap-and-core.md` | WorkerCore and bootstrap changes | ✅ Complete |
| `06-test-coverage-analysis.md` | Tests removed, added, changed | ✅ Complete |
| `07-behavior-mapping.md` | Function-by-function before/after mapping | ✅ Complete |
| `08-edge-cases-and-risks.md` | Potential gaps and recommendations | ✅ Complete |
| `09-summary.md` | Executive summary of findings | ✅ Complete |

### Branch Comparison

- **Main Branch**: `/Users/petetaylor/projects/tasker-systems/tasker-core-main/tasker-worker/`
- **Feature Branch**: `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/`

### Key Files to Compare

#### Main Branch (Before)
```
tasker-worker/src/worker/
├── command_processor.rs      # 1575 lines - monolithic implementation
├── core.rs                   # WorkerCore initialization
├── orchestration_result_sender.rs
├── step_claim.rs
├── task_template_manager.rs
└── event_systems/
```

#### Feature Branch (After)
```
tasker-worker/src/worker/
├── actor_command_processor.rs  # NEW: Pure routing (~300 LOC)
├── actors/                     # NEW: 5 actors + registry
│   ├── traits.rs
│   ├── registry.rs
│   ├── step_executor_actor.rs
│   ├── ffi_completion_actor.rs
│   ├── template_cache_actor.rs
│   ├── domain_event_actor.rs
│   └── worker_status_actor.rs
├── hydration/                  # NEW: Message transformation
│   └── step_message_hydrator.rs
├── services/                   # NEW: Decomposed services
│   ├── step_execution/
│   ├── ffi_completion/
│   └── worker_status/
├── command_processor.rs        # REDUCED: Types only (123 LOC)
├── core.rs                     # MODIFIED: Uses ActorRegistry
└── [existing files preserved]
```

### Analysis Method

1. **Structural diff**: Identify all file additions, modifications, and logical removals
2. **Function mapping**: Map each function in WorkerProcessor to its new location
3. **Behavior verification**: Trace execution paths before and after
4. **Test coverage delta**: Compare test assertions and coverage
5. **Risk assessment**: Identify potential gaps in migration
