# Coverage Analysis: tasker-core-py (Python Worker)

**Current Coverage**: 71.12% line (1991/2631 lines)
**Target**: 80%
**Gap**: 8.88 percentage points (need ~234 additional lines covered)

---

## Summary

The Python worker (`tasker-core-py`) provides maturin/pyo3 FFI bindings exposing a Python API for task step handling. Coverage stands at 71.12%, short of the 80% target by 8.88 percentage points. The gap is concentrated in a few key areas:

1. **Example handlers** (0% -- 80 lines) are completely uncovered test fixtures
2. **Error classifier** (29.41% -- 22 uncovered lines) has minimal testing of its classification methods
3. **Event poller** (39.72% -- 63 uncovered lines) has untested threading/polling internals due to FFI dependency
4. **Handler registry** (62.07% -- 72 uncovered lines) has untested bootstrap, discovery, and template-loading paths
5. **API mixin** (63.64% -- 41 uncovered lines) has untested HTTP methods and error formatting
6. **Batch processing** (66.44% -- 68 uncovered lines) has untested aggregation, checkpoint, and worker outcome paths
7. **Template discovery** (66.81% -- 42 uncovered lines) has untested workspace detection and YAML parsing paths
8. **Domain events** (74.58% -- 76 uncovered lines) has untested publisher/subscriber lifecycle and poller internals

Fifteen files are already at or above 80%. Twelve files are at 100% coverage.

---

## Uncovered Files (0% Coverage)

| File | Lines | Missing | Description |
|------|-------|---------|-------------|
| `examples/__init__.py` | 2 | 2 | Package init for example handlers |
| `examples/checkpoint_yield/__init__.py` | 2 | 2 | Checkpoint yield example package init |
| `examples/checkpoint_yield/handlers.py` | 76 | 76 | Checkpoint yield E2E test handler examples (TAS-125): `CheckpointYieldAnalyzerHandler`, `CheckpointYieldWorkerHandler`, `CheckpointYieldAggregatorHandler` |

**Total uncovered**: 80 lines across 3 files.

**Nature**: These are example/demo handler classes packaged within the library for E2E testing of the TAS-125 checkpoint yielding feature. They contain real batch processing logic (analyzer, worker with checkpoint yielding, and aggregator patterns).

---

## Lowest Coverage Files

Files below the 80% target, ordered from lowest to highest coverage:

| File | Coverage | Lines Covered | Lines Total | Missing |
|------|----------|---------------|-------------|---------|
| `errors/error_classifier.py` | 29.41% | 15 | 37 | 22 |
| `event_poller.py` | 39.72% | 54 | 117 | 63 |
| `registry/registry_resolver.py` | 43.90% | 18 | 33 | 15 |
| `handler.py` | 62.07% | 140 | 212 | 72 |
| `step_handler/mixins/api.py` | 63.64% | 95 | 136 | 41 |
| `batch_processing/batchable.py` | 66.44% | 147 | 215 | 68 |
| `template_discovery.py` | 66.81% | 117 | 159 | 42 |
| `registry/resolvers/class_lookup.py` | 70.00% | 36 | 50 | 14 |
| `registry/resolver_chain.py` | 70.40% | 74 | 101 | 27 |
| `models.py` | 72.95% | 85 | 108 | 23 |
| `domain_events.py` | 74.58% | 275 | 351 | 76 |
| `registry/base_resolver.py` | 75.00% | 15 | 20 | 5 |

**Total missing across below-threshold files**: 468 lines.

---

## Gap Analysis by Priority

### Critical Priority

**Error Classifier (`errors/error_classifier.py`) -- 29.41%, 22 lines missing**

This module classifies exceptions for retry behavior, which is fundamental to error handling correctness across the worker. Uncovered code includes:

- `ErrorClassifier.retryable()` -- The core method that checks exceptions against permanent/retryable class lists. Only the `__init__` and top-level imports are covered.
- `ErrorClassifier.permanent()` -- Convenience inverse of `retryable()`.
- `ErrorClassifier.classify()` -- Returns full classification details including error_type, retryable flag, and classification source.
- `get_classifier()` -- Singleton accessor for default classifier.
- `is_retryable()` / `is_permanent()` -- Module-level convenience functions.

**Risk**: Incorrect error classification leads to either lost work (permanent errors retried) or infinite retry loops (transient errors not retried). This directly affects production reliability.

**Tests needed**:
- Unit tests for `retryable()` covering each permanent error class (13 classes), each retryable error class (12 classes), and default behavior for unknown exceptions
- Test `classify()` returns correct classification source strings
- Test `permanent()` returns inverse of `retryable()`
- Test singleton `get_classifier()` and convenience functions `is_retryable()` / `is_permanent()`
- Test `default_retryable=False` constructor override behavior
- Test exceptions with explicit `retryable` attribute (highest-priority classification)

---

### High Priority

**Event Poller (`event_poller.py`) -- 39.72%, 63 lines missing**

The `EventPoller` is the core threading component that polls FFI dispatch channels for step events. Missing coverage includes:

- `start()` / `stop()` -- Thread lifecycle management
- `is_running` property -- Thread alive check
- `get_metrics()` -- FFI dispatch metrics retrieval
- `_poll_loop()` -- Main polling loop (runs in separate thread)
- `_process_event()` -- Event processing through callbacks
- `_check_starvation()` -- Periodic starvation detection
- `_cleanup_timeouts()` -- Periodic timeout cleanup
- `_emit_error()` -- Error callback emission

**Risk**: The event poller is the primary mechanism for receiving and dispatching work in the Python worker. Untested thread lifecycle and error handling could lead to silent failures or resource leaks in production.

**Tests needed**:
- Mock `_tasker_core` FFI functions (`poll_step_events`, `get_ffi_dispatch_metrics`, `check_starvation_warnings`, `cleanup_timeouts`)
- Test `start()` creates thread, `stop()` joins it, double-start raises `RuntimeError`
- Test `_poll_loop()` processes events via callbacks when data is available
- Test `_poll_loop()` sleeps when no events available
- Test starvation check fires at configured interval
- Test cleanup fires at configured interval
- Test error handling -- `_emit_error()` invokes error callbacks, error in poll loop sleeps longer
- Test `get_metrics()` returns `FfiDispatchMetrics` or `None` on error

**Handler Registry (`handler.py`) -- 62.07%, 72 lines missing**

The `HandlerRegistry` is the central singleton for handler discovery, registration, and resolution. Missing coverage includes:

- `bootstrap_handlers()` -- Auto-bootstrap from environment (test vs. template-driven)
- `_test_environment_active()` -- Environment detection
- `_register_preloaded_handlers()` -- Scan loaded modules for StepHandler subclasses in test mode
- `_discover_handlers_from_templates()` -- YAML template-driven handler discovery
- `_determine_template_path()` -- Template directory resolution
- `_find_and_load_handler_class()` -- Dynamic import and class loading
- `template_discovery_info()` -- Debug information
- `discover_handlers()` -- Package scanning (partial: submodule scanning and error paths)
- `_scan_module_for_handlers()` -- Module scanning for handler classes

**Risk**: Handler registry bootstrap is the entry point for the entire worker handler system. Untested bootstrap paths could result in handlers not being discovered in production, leading to "handler not found" errors for every step event.

**Tests needed**:
- Test `bootstrap_handlers()` in test environment (TASKER_ENV=test) scans preloaded modules
- Test `bootstrap_handlers()` falls through to template discovery when no test handlers found
- Test `_discover_handlers_from_templates()` with mock YAML files
- Test `_find_and_load_handler_class()` for valid class, invalid module, non-StepHandler class
- Test `_determine_template_path()` with TASKER_TEMPLATE_PATH env var, test env, and standard discovery
- Test `discover_handlers()` with package containing both valid and invalid handler modules
- Test `_scan_module_for_handlers()` skips non-class, non-StepHandler, and handler_name-less classes
- Test `template_discovery_info()` returns expected structure

---

### Medium Priority

**API Mixin (`step_handler/mixins/api.py`) -- 63.64%, 41 lines missing**

HTTP functionality mixin for step handlers. Missing coverage includes:

- `ApiResponse` -- Body parsing for non-JSON content types, `is_client_error`, `is_server_error`, `is_retryable` properties, `retry_after` header parsing, `to_dict()`
- `APIMixin.put()`, `patch()`, `delete()`, `request()` -- Less common HTTP methods
- `api_failure()` -- Error classification from HTTP status codes, error message formatting
- `connection_error()`, `timeout_error()` -- Connection/timeout failure result helpers
- `_classify_error()` -- Status code to error type mapping
- `_format_error_message()` -- Response body error message extraction

**Risk**: Medium. API handlers using uncommon HTTP methods or relying on error classification may not work correctly. The error classification logic determines retry behavior for external API calls.

**Tests needed**:
- Test `ApiResponse` construction with JSON and non-JSON content types, body parsing fallback
- Test `ApiResponse` properties: `ok`, `is_client_error`, `is_server_error`, `is_retryable`, `retry_after`
- Test all HTTP methods (`put`, `patch`, `delete`, `request`) using httpx test transport
- Test `api_failure()` for different status codes (400, 401, 404, 429, 500, 503)
- Test `_classify_error()` maps known and unknown status codes correctly
- Test `_format_error_message()` extracts from various response body formats
- Test `connection_error()` and `timeout_error()` with and without context strings

**Batch Processing (`batch_processing/batchable.py`) -- 66.44%, 68 lines missing**

Batch processing mixin for analyzers, workers, and aggregators. Missing coverage includes:

- `BatchAggregationScenario.detect()` -- Aggregation scenario detection with DependencyResultsWrapper
- `Batchable.create_cursor_configs()` -- Ruby-style worker-count-based cursor division
- `Batchable.get_batch_worker_inputs()` -- FFI batch worker input extraction
- `Batchable.handle_no_op_worker()` -- No-op placeholder worker handling
- `Batchable.batch_analyzer_success()` -- Keyword argument style batch analyzer result
- `Batchable.no_batches_outcome()` -- No-batches result builder
- `Batchable.batch_worker_success()` -- Keyword argument style worker result
- `Batchable.checkpoint_yield()` -- TAS-125 checkpoint yield mechanism
- `Batchable.batch_worker_partial_failure()` -- Partial failure reporting
- `Batchable.aggregate_batch_worker_results()` -- Auto-aggregation with scenario detection
- `Batchable.aggregate_worker_results()` -- Static result aggregation

**Risk**: Medium-high. Batch processing is a core workflow pattern. Incorrect cursor boundary math or aggregation logic could cause data loss or processing gaps.

**Tests needed**:
- Test `BatchAggregationScenario.detect()` for NoBatches and WithBatches scenarios, missing batchable step, no workers found
- Test `create_cursor_configs()` for even/uneven division, edge cases (0 items, 1 worker)
- Test `get_batch_worker_inputs()` with valid and invalid step inputs
- Test `handle_no_op_worker()` returns success for no-op, None for non-no-op
- Test `batch_analyzer_success()` with keyword args and with BatchAnalyzerOutcome object
- Test `no_batches_outcome()` produces correct NoBatchesOutcome
- Test `batch_worker_success()` with both keyword args and BatchWorkerOutcome object
- Test `checkpoint_yield()` produces result with checkpoint_yield metadata
- Test `aggregate_batch_worker_results()` with both NoBatches and WithBatches scenarios

**Template Discovery (`template_discovery.py`) -- 66.81%, 42 lines missing**

YAML template-based handler discovery. Missing coverage includes:

- `TemplatePath.find_template_config_directory()` -- Multi-priority path resolution (env var, test, workspace, fallback)
- `TemplatePath._detect_workspace_root()` -- Upward directory traversal for workspace markers
- `TemplatePath._find_test_template_path()` -- Test fixture path resolution
- `TemplateParser.extract_template_metadata()` -- Full template metadata extraction
- `HandlerDiscovery.discover_template_metadata()` -- Multi-template metadata discovery
- `HandlerDiscovery.discover_handlers_by_namespace()` -- Namespace-grouped handler discovery

**Risk**: Medium. Template discovery only affects production bootstrap. Incorrect workspace detection could lead to handlers not being discovered, but this is mitigated by explicit TASKER_TEMPLATE_PATH override.

**Tests needed**:
- Test `find_template_config_directory()` priority: TASKER_TEMPLATE_PATH > test env > WORKSPACE_PATH > auto-detect > fallback
- Test `_detect_workspace_root()` finds markers (Cargo.toml, .git, etc.)
- Test `_find_test_template_path()` with correct directory structure
- Test `extract_template_metadata()` extracts name, namespace, version, handlers
- Test `discover_handlers_by_namespace()` groups correctly and deduplicates
- Test with invalid/missing YAML files (error paths)

**Domain Events (`domain_events.py`) -- 74.58%, 76 lines missing**

Domain event publishing/subscribing system. Missing coverage includes:

- `BasePublisher.publish()` -- Full lifecycle (should_publish, transform, before/after hooks)
- `BaseSubscriber.start()` / `stop()` -- Subscriber lifecycle with poller registration
- `BaseSubscriber._handle_if_matches()` -- Event pattern matching and dispatch
- `BaseSubscriber.handle_event_safely()` -- Safe event handling with lifecycle hooks
- `InProcessDomainEventPoller._poll_loop()` -- Threaded polling loop
- `InProcessDomainEventPoller._process_event()` -- Event processing through callbacks
- `SubscriberRegistry.start_all()` / `stop_all()` -- Bulk subscriber management
- `PublisherRegistry` -- Frozen registry operations, validation, clear

**Risk**: Medium. Domain events are used for non-critical real-time notifications. However, subscriber lifecycle bugs could lead to memory leaks or missed notifications.

**Tests needed**:
- Test `BasePublisher.publish()` lifecycle hooks (should_publish=False skips, before_publish=False aborts, error triggers on_publish_error)
- Test custom publisher with `transform_payload()` and `additional_metadata()`
- Test `BaseSubscriber.start()` registers callbacks, `stop()` deactivates
- Test `BaseSubscriber.matches()` with wildcard patterns ("*", "order.*", exact match)
- Test `handle_event_safely()` with before_handle=False, exception in handle()
- Test `InProcessDomainEventPoller` start/stop with mock FFI
- Test `SubscriberRegistry.start_all()` / `stop_all()` with multiple subscribers
- Test `PublisherRegistry.freeze()` prevents registration, `validate_required()` checks

---

### Lower Priority

**Registry Resolver (`registry/registry_resolver.py`) -- 43.90%, 15 lines missing**

Developer-facing base class for custom resolvers with pattern/prefix matching. Missing:

- `RegistryResolver.can_resolve()` -- Pattern and prefix matching
- `RegistryResolver.resolve()` -- Delegation to `resolve_handler()`
- `RegistryResolver.resolve_handler()` -- Abstract method (raises NotImplementedError)
- `RegistryResolver._match_pattern()` -- Convenience pattern matching

**Tests needed**: Unit tests creating concrete subclass with pattern and prefix, testing `can_resolve()` and `resolve()` flow.

**Class Lookup Resolver (`registry/resolvers/class_lookup.py`) -- 70.00%, 14 lines missing**

Inferential resolver that imports handler classes by module path. Missing:

- `_import_class()` -- Error paths (ImportError, non-type attribute, missing class)
- `_instantiate_handler()` -- Fallback to config keyword argument, instantiation failure

**Tests needed**: Test with non-existent module, non-class attribute, handler requiring config arg.

**Resolver Chain (`registry/resolver_chain.py`) -- 70.40%, 27 lines missing**

Priority-ordered handler resolution chain. Missing:

- `_resolve_with_hint()` -- Resolver hint resolution (unknown resolver, hint-resolved handler)
- `can_resolve()` -- Chain-wide resolution check
- `register()` -- Convenience registration on ExplicitMappingResolver
- `wrap_for_method_dispatch()` -- Method dispatch wrapping (handler missing target method)
- `chain_info()` -- Debug info
- Error paths in `_resolve_with_chain()` and `remove_resolver()`

**Tests needed**: Test resolver hint with known/unknown resolvers, method dispatch wrapping, chain operations.

**Models (`models.py`) -- 72.95%, 23 lines missing**

FFI data structure wrappers. Missing:

- `DependencyResultsWrapper.__iter__()`, `__contains__()`
- `WorkflowStepWrapper.from_dict()` -- Various field extraction paths
- `TaskWrapper.from_dict()` -- Nested task structure handling
- `TaskSequenceStepWrapper.get_task_field()`, `get_dependency_result()`, `get_results()`

**Tests needed**: Test wrapper construction with None data, nested task structures, iteration, and containment checks.

**Base Resolver (`registry/base_resolver.py`) -- 75.00%, 5 lines missing**

Abstract base class for resolvers. Missing: `registered_callables()` default implementation.

**Tests needed**: Minimal -- test concrete subclass returns empty list.

**Step Execution Subscriber (`step_execution_subscriber.py`) -- 86.36%, 18 lines missing**

Step event routing and execution. Missing:

- `_run_async_handler()` -- Async handler execution in event loop
- `_submit_checkpoint_yield()` -- Checkpoint yield FFI submission

**Tests needed**: Test async handler execution, checkpoint yield submission path.

---

## Recommended Test Plan

### Phase 1: Quick Wins (target: +100 lines, reach ~75%)

| File | Lines Recoverable | Effort | Notes |
|------|-------------------|--------|-------|
| `errors/error_classifier.py` | 22 | Low | Pure logic, no FFI or mocks needed |
| `registry/registry_resolver.py` | 15 | Low | Create test subclass, test pattern/prefix matching |
| `registry/base_resolver.py` | 5 | Low | Trivial concrete subclass test |
| `models.py` | 23 | Low | Pure data wrappers, test construction and accessors |
| `registry/resolvers/class_lookup.py` | 14 | Low | Test import error paths, instantiation |
| `registry/resolver_chain.py` | 20 | Low | Test hints, chain info, method dispatch |

**Subtotal**: ~99 lines, estimated coverage: ~74.9%

### Phase 2: Medium Effort (target: +135 lines, reach 80%+)

| File | Lines Recoverable | Effort | Notes |
|------|-------------------|--------|-------|
| `step_handler/mixins/api.py` | 35 | Medium | Requires httpx test transport for HTTP method testing |
| `batch_processing/batchable.py` | 50 | Medium | Test batch outcome builders, aggregation scenarios |
| `template_discovery.py` | 30 | Medium | Mock filesystem, test path resolution priority |
| `domain_events.py` | 40 | Medium | Test publisher/subscriber lifecycle, mock FFI poller |

**Subtotal**: ~155 lines, estimated coverage: ~80.8%

### Phase 3: FFI-Dependent Tests (target: +65 lines, reach 83%+)

| File | Lines Recoverable | Effort | Notes |
|------|-------------------|--------|-------|
| `event_poller.py` | 40 | High | Mock FFI functions, test thread lifecycle |
| `handler.py` | 45 | High | Mock filesystem, env vars, importlib for bootstrap |
| `step_execution_subscriber.py` | 15 | High | Mock FFI, test async handler and checkpoint yield |

**Subtotal**: ~100 lines, estimated coverage: ~83.5%

### Phase 4: Example Handlers (optional, +80 lines)

| File | Lines Recoverable | Effort | Notes |
|------|-------------------|--------|-------|
| `examples/checkpoint_yield/handlers.py` | 76 | Medium | Integration tests with mock batch context |
| `examples/__init__.py` files | 4 | Trivial | Import tests |

**Subtotal**: ~80 lines, estimated coverage: ~86.5%

---

## Estimated Impact

| Phase | Lines Added | Cumulative Coverage | Delta |
|-------|-------------|--------------------:|------:|
| Current | -- | 71.12% | -- |
| Phase 1 (Quick Wins) | ~99 | ~74.9% | +3.8pp |
| Phase 2 (Medium) | ~155 | ~80.8% | +5.9pp |
| Phase 3 (FFI-Dependent) | ~100 | ~83.5% | +2.7pp |
| Phase 4 (Examples) | ~80 | ~86.5% | +3.0pp |

**Minimum to reach 80% target**: Phases 1 + 2 (~254 lines, covering the 234-line gap with margin).

**Key observation**: Phases 1 and 2 are sufficient to meet the 80% target. They focus on pure Python logic and mockable interfaces, requiring no running services. Phase 3 adds resilience for FFI-dependent threading code. Phase 4 is optional but provides coverage of the checkpoint yielding example handlers that serve as integration test reference implementations.
