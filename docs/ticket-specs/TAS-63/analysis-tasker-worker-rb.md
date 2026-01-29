# Coverage Analysis: tasker-worker-rb (Ruby Worker)

**Current Coverage**: 67.58% line (1989/2943 lines covered)
**Target**: 70%
**Gap**: 2.42 percentage points (need ~71 more lines covered)

---

## Summary

The Ruby worker (`workers/ruby/`) provides the Ruby-side API for the tasker-core system, handling step execution via FFI bindings to the Rust orchestration layer. Coverage is at 67.58%, only 2.42 percentage points below the 70% threshold. The codebase has 55 files with 54 tested; 12 files are at 100% coverage.

The gap is concentrated in a small number of files with low coverage, primarily in the runtime infrastructure layer (event pollers, event bridge, subscriber) and the handler framework (API mixin, batchable mixin, task handler base). The critical application logic types and domain event system are well-covered (80-100%).

Closing the gap requires approximately 71 additional lines of coverage. The most impactful targets are files with large uncovered line counts that can be tested in isolation without FFI dependencies.

---

## Uncovered Files (0% Coverage)

### `lib/tasker_core/version.rb` -- 0% (0/11 lines)

**Contents**: Module constants (`VERSION`, `RUST_CORE_VERSION`) and a `version_info` class method that returns a hash of version strings.

**Risk**: Very Low. This is a static version declaration file with a simple accessor method.

**Test Strategy**: A single unit test requiring the file and asserting the constant values and `version_info` return structure. This is trivial to cover.

**Lines recoverable**: 11

---

## Lowest Coverage Files

Files sorted by coverage percentage (ascending), excluding 0% above:

| File | Coverage | Lines Covered | Lines Total | Uncovered |
|------|----------|---------------|-------------|-----------|
| `subscriber.rb` | 18.46% | 12/65 | 53 |
| `task_handler/base.rb` | 22.89% | 19/83 | 64 |
| `step_handler/mixins/api.rb` | 23.36% | 32/137 | 105 |
| `template_discovery.rb` | 25.64% | 20/78 | 58 |
| `worker/event_poller.rb` | 26.67% | 16/60 | 44 |
| `types/batch_processing_outcome.rb` | 34.72% | 25/72 | 47 |
| `worker/in_process_domain_event_poller.rb` | 38.61% | 39/101 | 62 |
| `step_handler/mixins/batchable.rb` | 47.92% | 23/48 | 25 |
| `tracing.rb` | 50.00% | 21/42 | 21 |
| `registry/handler_registry.rb` | 51.50% | 86/167 | 81 |
| `event_bridge.rb` | 51.90% | 41/79 | 38 |
| `step_handler/base.rb` | 57.97% | 40/69 | 29 |
| `logger.rb` | 60.00% | 30/50 | 20 |
| `types/simple_message.rb` | 60.00% | 21/35 | 14 |
| `step_handler/api.rb` | 63.64% | 7/11 | 4 |
| `types/step_types.rb` | 66.23% | 51/77 | 26 |
| `errors/common.rb` | 68.00% | 51/75 | 24 |
| `types/step_message.rb` | 69.77% | 30/43 | 13 |
| `types/step_context.rb` | 69.86% | 51/73 | 22 |

---

## Gap Analysis by Priority

### Critical Priority

These files have functional runtime importance and very low coverage. Failures here would be undetected.

#### 1. `lib/tasker_core/subscriber.rb` -- 18.46% (53 uncovered lines)

**What it does**: `StepExecutionSubscriber` is the main event handler that receives step execution events from the Rust layer, resolves handlers via the registry, invokes them with a `StepContext`, and publishes success/failure/checkpoint results back to Rust.

**Why it matters**: This is the central dispatch loop for Ruby step execution. Bugs here break all Ruby handler execution. The subscriber handles three result paths (success, handler-returned failure, exception) and checkpoint yields -- all of which need verification.

**What is uncovered**:
- The `call(event)` method -- the main execution path including handler resolution, context creation, result standardization, and checkpoint handling
- The `publish_step_completion` and `publish_checkpoint_yield` private methods
- Error handling branches (handler exceptions, missing handlers, retryable vs permanent failures)

**Recommended tests**:
- Unit tests with mocked `HandlerRegistry`, `EventBridge`, and `StepContext`
- Test successful handler execution and completion publication
- Test handler returning an error `StepHandlerCallResult` (preserving retryable flag)
- Test handler raising `StandardError` with error classification
- Test handler raising a `ConfigurationError` (no handler found)
- Test checkpoint yield path for batch processing
- Test trace context propagation (trace_id, span_id)

#### 2. `lib/tasker_core/task_handler/base.rb` -- 22.89% (64 uncovered lines)

**What it does**: `TaskHandler::Base` is the main task processing entry point. It supports two orchestration modes (embedded FFI and distributed pgmq), validates task UUIDs, initializes tasks via pgmq messaging, and provides status/readiness checks.

**Why it matters**: This is the top-level task handler that routes to embedded or distributed orchestration. Untested mode switching or validation could lead to silent failures.

**What is uncovered**:
- `handle(task_uuid)` method with mode switching (embedded/distributed)
- `initialize_task(task_request)` -- pgmq message construction and sending
- `orchestration_ready?` and `status` methods
- Error handling for orchestration and standard errors
- Private helpers: `handle_embedded_mode`, `handle_distributed_mode`, `orchestration_mode`, `load_task_config_from_path`

**Recommended tests**:
- Unit tests with mocked `TaskerCore.embedded_orchestrator` and `TaskerCore::Messaging::PgmqClient`
- Test handle in embedded mode (running and not-running orchestrator)
- Test handle in distributed mode
- Test handle with invalid task_uuid (not integer)
- Test initialize_task with valid and invalid request data
- Test status method in both modes
- Test error recovery paths

### High Priority

Important framework components with moderate complexity.

#### 3. `lib/tasker_core/step_handler/mixins/api.rb` -- 23.36% (105 uncovered lines)

**What it does**: The API mixin provides HTTP client functionality to step handlers via Faraday. Includes convenience HTTP methods (GET, POST, PUT, DELETE), error classification (permanent vs retryable based on HTTP status), retry-after header extraction, file upload, paginated requests, SSL/auth configuration, and API-specific result helpers.

**Why it matters**: Any handler making HTTP calls depends on this mixin for correct error classification. Misclassification of a 429 as permanent would break retry behavior.

**What is uncovered**:
- HTTP methods (`get`, `post`, `put`, `delete`) and `process_response`
- Error classification by HTTP status (400-499 permanent, 429/503 retryable with backoff, 500+ retryable)
- `api_upload_file` multipart upload
- `api_paginated_request` cursor/offset pagination
- `api_success` and `api_failure` result helpers
- Authentication configuration (bearer, basic, api_key)
- Private connection building (`build_faraday_connection`, `apply_connection_config`)
- Retry-after header extraction and link header pagination parsing

**Recommended tests**:
- Create a test handler class including the mixin
- Use WebMock or Faraday test adapter to stub HTTP responses
- Test each HTTP method (GET, POST, PUT, DELETE) with success and error responses
- Test error classification for status codes: 400, 401, 403, 404, 422 (permanent), 429 (rate limit with Retry-After), 503 (service unavailable), 500-599 (server error)
- Test `api_success` and `api_failure` result structure
- Test authentication types (bearer, basic, api_key)
- Test paginated request with multiple pages and max_pages safety limit

#### 4. `lib/tasker_core/template_discovery.rb` -- 25.64% (58 uncovered lines)

**What it does**: Three classes for discovering YAML task templates: `TemplatePath` finds template directories via environment variables or workspace detection, `TemplateParser` extracts handler callables from YAML templates, and `HandlerDiscovery` coordinates discovery and grouping by namespace.

**Why it matters**: Handler discovery at startup depends on this module. If templates are not found, no handlers get registered.

**What is uncovered**:
- `TemplatePath.find_template_config_directory` -- environment variable and workspace detection logic
- `TemplatePath.discover_template_files` -- YAML glob discovery
- `TemplatePath.find_workspace_root` -- upward directory traversal for Cargo.toml/.git
- `TemplateParser.extract_handler_callables` and `extract_template_metadata`
- `HandlerDiscovery.discover_all_handlers`, `discover_template_metadata`, `discover_handlers_by_namespace`

**Recommended tests**:
- Unit tests with temp directories containing fixture YAML files
- Test `TemplatePath` with TASKER_TEMPLATE_PATH env var set
- Test `TemplatePath` with WORKSPACE_PATH env var set
- Test `TemplatePath.find_workspace_root` from a directory containing Cargo.toml
- Test `TemplateParser` with valid YAML containing task and step handlers
- Test `TemplateParser` with invalid/missing YAML
- Test `HandlerDiscovery` grouping by namespace

#### 5. `lib/tasker_core/worker/event_poller.rb` -- 26.67% (44 uncovered lines)

**What it does**: Singleton that polls for step execution events from Rust via FFI in a dedicated background thread. Includes starvation detection, error recovery with exponential backoff, and FFI dispatch metrics.

**Why it matters**: This is the primary mechanism for Ruby to receive work from Rust. If the polling loop fails silently, Ruby handlers stop executing.

**What is uncovered**:
- `start!` and `stop!` lifecycle methods
- `poll_events_loop` -- the main polling loop
- `check_starvation_periodically` -- TAS-67 starvation detection
- `process_event` -- forwarding events to EventBridge
- `ffi_dispatch_metrics` -- metrics retrieval

**Recommended tests**:
- Unit tests with mocked `TaskerCore::FFI` module
- Test start/stop lifecycle (active? state transitions)
- Test poll loop behavior: event received -> forwarded to EventBridge
- Test poll loop behavior: nil event -> sleep
- Test error handling in poll loop (StandardError -> error sleep)
- Test starvation check triggers at correct interval
- Test ffi_dispatch_metrics with successful and failing FFI call

#### 6. `lib/tasker_core/worker/in_process_domain_event_poller.rb` -- 38.61% (62 uncovered lines)

**What it does**: Singleton that polls for "fast" domain events from the Rust in-process event bus. Supports pattern-based subscription (exact match, wildcard `payment.*`, global `*`) and fire-and-forget handler dispatch.

**Why it matters**: Domain event integration (Sentry, DataDog, Slack notifications) depends on this poller. Pattern matching bugs would cause missed events.

**What is uncovered**:
- `subscribe(pattern, &handler)` with validation
- `unsubscribe(pattern)` and `clear_subscriptions!`
- Pattern matching logic: exact match, wildcard `payment.*`, global `*`
- `dispatch_event` with JSON parsing of business_payload and error fields
- `find_matching_handlers` with mutex synchronization
- `invoke_handler_safely` -- fire-and-forget error handling
- `stats` method

**Recommended tests**:
- Unit tests with mocked `TaskerCore::FFI`
- Test subscription management (subscribe, unsubscribe, clear, count)
- Test pattern matching: exact `payment.processed`, wildcard `payment.*`, global `*`
- Test event dispatch to matching subscribers
- Test JSON parsing of business_payload string fields
- Test handler error isolation (fire-and-forget)
- Test stats method structure

### Medium Priority

These files contribute to the gap but are either partially covered or lower risk.

#### 7. `lib/tasker_core/types/batch_processing_outcome.rb` -- 34.72% (47 uncovered lines)

**What it does**: Defines `NoBatches` and `CreateBatches` outcome types for batch processing using dry-struct. Includes factory methods, validation (worker_count/cursor_config consistency, batch_size math), and `from_hash` deserialization.

**What is uncovered**:
- `CreateBatches` struct attributes and `to_h` method
- Factory `create_batches` with validations (empty name, empty configs, count mismatch, batch_size consistency)
- `from_hash` deserialization with deep key symbolization
- `deep_symbolize_keys` private method

**Recommended tests**:
- Test `no_batches` factory and `requires_batch_creation?` => false
- Test `create_batches` factory with valid parameters
- Test validation errors: empty name, empty configs, count mismatch, bad batch_size
- Test `from_hash` with string-keyed and symbol-keyed hashes
- Test `from_hash` with unknown outcome type

#### 8. `lib/tasker_core/step_handler/mixins/batchable.rb` -- 47.92% (25 uncovered lines)

**What it does**: Batchable mixin providing cursor config creation, batch context extraction, no-op worker handling, outcome builders, and aggregation helpers.

**What is uncovered**:
- `create_cursor_configs` with boundary math (0-indexed)
- `no_batches_outcome` and `create_batches_outcome` outcome builders
- `batch_worker_success` result helper
- `checkpoint_yield` for TAS-125 batch checkpointing
- `aggregate_batch_worker_results` with scenario routing

**Recommended tests**:
- Test `create_cursor_configs(1000, 3)` returns correct 0-indexed ranges
- Test `create_cursor_configs` with block customization
- Test `create_cursor_configs` with worker_count <= 0 raises ArgumentError
- Test outcome builders return correct StepHandlerCallResult structures
- Test `batch_worker_success` with all parameters
- Test `aggregate_batch_worker_results` for NoBatches and WithBatches scenarios

#### 9. `lib/tasker_core/registry/handler_registry.rb` -- 51.50% (81 uncovered lines)

**What it does**: Singleton registry with ResolverChain for handler discovery, registration, and method dispatch. Bootstraps handlers from YAML templates or test environment preloads.

**What is uncovered**:
- Bootstrap logic: `bootstrap_handlers!`, `discover_handlers_from_templates`, template path determination
- Handler file loading: `find_and_load_handler_class`, `load_handler_file`, `handler_search_paths`
- Test environment integration: `test_environment_active?`, `test_handlers_preloaded?`, `register_preloaded_handlers`
- `normalize_to_definition` for HandlerWrapper and duck-typed objects
- `instantiate_handler` with config kwarg detection
- `template_discovery_info` and `add_resolver`

**Recommended tests**:
- Test handler registration and resolution by string
- Test resolution with HandlerDefinition (with handler_method)
- Test `handler_available?` and `registered_handlers`
- Test bootstrap with mocked template discovery returning known handler names
- Test `normalize_to_definition` for each input type (String, HandlerDefinition, HandlerWrapper, duck-typed)
- Test `find_loaded_handler_class_by_full_name`

#### 10. `lib/tasker_core/event_bridge.rb` -- 51.90% (38 uncovered lines)

**What it does**: Singleton event bridge using dry-events for bidirectional Rust-Ruby communication. Publishes step execution events, sends completions back to Rust via FFI, and handles checkpoint yields.

**What is uncovered**:
- `publish_step_execution` with event wrapping
- `publish_step_completion` with validation and FFI call
- `publish_step_checkpoint_yield` for TAS-125
- `wrap_step_execution_event` with correlation/trace context
- `validate_completion!` and `validate_checkpoint_yield!`
- Event schema setup

**Recommended tests**:
- Test `publish_step_execution` with valid event data (wrapped correctly)
- Test `publish_step_completion` with valid and invalid completion data
- Test `validate_completion!` missing required fields raises ArgumentError
- Test `publish_step_checkpoint_yield` with valid checkpoint data
- Test `validate_checkpoint_yield!` with missing fields and invalid items_processed
- Test inactive bridge returns early

#### 11. `lib/tasker_core/tracing.rb` -- 50.00% (21 uncovered lines)

**What it does**: Static class wrapping Rust FFI tracing calls for each log level (error, warn, info, debug, trace). Includes field normalization converting Ruby types to strings.

**What is uncovered**:
- `warn`, `debug`, `trace` methods
- `normalize_fields` and `normalize_value` for various Ruby types (nil, String, Symbol, Numeric, Boolean, Exception)
- FFI fallback behavior when logging fails

**Recommended tests**:
- Test each log level method with mocked FFI calls
- Test `normalize_value` for nil, String, Symbol, Integer, Float, Boolean, Exception
- Test `normalize_fields` with mixed key types
- Test fallback to stderr when FFI raises

### Lower Priority

Files close to or at the 70% threshold, or with small uncovered counts.

#### 12. `lib/tasker_core/step_handler/base.rb` -- 57.97% (29 uncovered lines)

**What is uncovered**: Private helper methods (`classify_unexpected_error`, `format_success_result`, `format_error_result`, `validate_process_result`, `extract_step_info`, `extract_task_info`). These are internal formatting methods.

**Recommended tests**: Test `classify_unexpected_error` for ArgumentError (permanent), Timeout::Error (retryable), IOError (retryable), and unknown errors. Test `validate_process_result` with valid and missing status.

#### 13. `lib/tasker_core/logger.rb` -- 60.00% (20 uncovered lines)

**What is uncovered**: Structured logging methods (`log_task`, `log_queue_worker`, `log_orchestrator`, `log_step`, `log_database`, `log_ffi`, `log_config`, `log_registry`). These all delegate to Tracing.

**Recommended tests**: Test each structured logging method delegates correctly to Tracing with expected fields.

#### 14. `lib/tasker_core/types/simple_message.rb` -- 60.00% (14 uncovered lines)

**What is uncovered**: Database validation methods (`valid_references?`, `task_exists?`, `step_exists?`, `all_dependencies_exist?`) and fetch methods (`fetch_task`, `fetch_step`, `fetch_dependencies`). These require ActiveRecord/database.

**Recommended tests**: These are database-dependent methods. Could be tested with mocked ActiveRecord models or skipped in favor of integration tests.

#### 15. `lib/tasker_core/errors/common.rb` -- 68.00% (24 uncovered lines)

**What is uncovered**: Constructor and accessor methods for `TimeoutError`, `NetworkError`, `ValidationError`, `NotFoundError`, `FFIError`, `ServerError` classes.

**Recommended tests**: Test each error class constructor with all parameters and verify `error_class` method returns correct string.

#### 16. `lib/tasker_core/types/step_context.rb` -- 69.86% (22 uncovered lines)

**What is uncovered**: Checkpoint accessors (`checkpoint_cursor`, `checkpoint_items_processed`, `accumulated_results`, `has_checkpoint?`), retry helpers (`is_retry?`, `is_last_retry?`), and additional accessors (`namespace_name`, `retryable?`, `to_s`, `inspect`).

**Recommended tests**: Test checkpoint accessor methods with and without checkpoint data. Test retry helpers with various attempt counts.

#### 17. `lib/tasker_core/types/step_types.rb` -- 66.23% (26 uncovered lines)

**What is uncovered**: `StepCompletion` struct methods (`valid?`, `completed?`, `failed?`, `pending?`, `duration_seconds`, `to_s`), `StepResult.failure` factory, and `map_status_to_rust_enum`.

**Recommended tests**: Test `StepCompletion` status query methods and string formatting. Test `StepResult` factory methods.

#### 18. `lib/tasker_core/types/step_message.rb` -- 69.77% (13 uncovered lines)

**What is uncovered**: `SimpleQueueMessageData` convenience accessors (`step_message`, `queue_message`, `task_uuid`, `step_uuid`, `ready_dependency_step_uuids`).

**Recommended tests**: Create a `SimpleQueueMessageData` struct and test each accessor returns correct values.

---

## Recommended Test Plan

### Phase 1: Quick Wins to Cross 70% Threshold (estimated +80-100 lines)

These tests can be written without FFI or database dependencies, using pure Ruby mocks:

1. **`version.rb`** (11 lines) -- Require file, assert constants and `version_info` method
2. **`tracing.rb`** (21 lines) -- Mock FFI module, test each log level and normalize_value
3. **`errors/common.rb`** (24 lines) -- Test each error class constructor and accessors
4. **`types/step_context.rb`** (22 lines) -- Test checkpoint and retry helpers with mock wrappers
5. **`types/step_types.rb`** (26 lines) -- Test StepCompletion and StepResult methods

**Total estimated recovery**: ~104 lines, bringing coverage to approximately **70.9%** (above threshold)

### Phase 2: Core Framework Coverage (estimated +150-200 lines)

6. **`subscriber.rb`** -- Mock-based tests for the step execution dispatch loop
7. **`event_bridge.rb`** -- Test event wrapping, validation, and publication
8. **`template_discovery.rb`** -- Temp directory fixtures for template discovery
9. **`step_handler/mixins/batchable.rb`** -- Cursor config math and outcome builders

### Phase 3: HTTP and Registry Coverage (estimated +150 lines)

10. **`step_handler/mixins/api.rb`** -- WebMock/Faraday test adapter for HTTP tests
11. **`registry/handler_registry.rb`** -- Mock-based bootstrap and resolution tests
12. **`task_handler/base.rb`** -- Mock orchestrator and pgmq client

### Phase 4: Runtime Infrastructure (estimated +100 lines)

13. **`worker/event_poller.rb`** -- Mock FFI, test lifecycle and polling behavior
14. **`worker/in_process_domain_event_poller.rb`** -- Pattern matching and subscription management
15. **`types/batch_processing_outcome.rb`** -- Validation and deserialization

---

## Estimated Impact

| Phase | Files | Lines Recovered (est.) | Projected Coverage |
|-------|-------|------------------------|--------------------|
| Current | -- | -- | 67.58% (1989/2943) |
| Phase 1 | 5 files | ~104 lines | ~71.1% |
| Phase 2 | 4 files | ~170 lines | ~76.9% |
| Phase 3 | 3 files | ~150 lines | ~82.0% |
| Phase 4 | 3 files | ~100 lines | ~85.4% |

**Minimum to reach 70%**: Phase 1 alone (5 simple test files) should be sufficient to cross the threshold. These are all pure unit tests with no infrastructure dependencies.

**Key observation**: The gap of 2.42 percentage points translates to only ~71 lines of code. The 5 files in Phase 1 have a combined 104 uncovered lines and are all testable in isolation. Even partial coverage of Phase 1 files would close the gap.
