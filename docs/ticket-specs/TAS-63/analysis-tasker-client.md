# Coverage Analysis: tasker-client

**Current Coverage**: 5.06% line (1,422/28,104), 3.87% function (161/4,159)
**Target**: 60%
**Gap**: 54.94 percentage points

---

## Summary

The `tasker-client` crate has extremely low coverage at 5.06% line coverage against a 60% target. The crate contains two distinct components: a **library** (API clients, gRPC clients, transport abstraction, configuration, error types) and a **CLI binary** (`tasker-cli`). The CLI binary and all six command handler modules have 0% coverage, accounting for 1,991 uncovered lines. The library side has partial coverage in `config.rs` (59.45%) and `grpc_clients/common.rs` (59.47%) but most API client and gRPC client files remain below 35%.

The crate has 16 source files total, of which 7 have 0% coverage (all CLI-related). The remaining 9 files range from 3.01% to 59.47% coverage. Existing tests are concentrated in `config.rs` (16 tests), `transport.rs` (7 tests), `grpc_clients/common.rs` (8 tests), and an integration test file `tests/config_commands_test.rs` (22 tests covering ConfigMerger functionality).

**Key structural observation**: The CLI binary (`src/bin/`) is not testable via standard `cargo test` since binary crate code cannot be invoked from library tests. CLI testing requires either (a) extracting logic into the library crate, (b) subprocess/command-based integration tests, or (c) accepting that CLI dispatch code has inherently lower testability.

---

## Uncovered Files (0% Coverage)

| File | Lines | Functions | Category |
|------|------:|----------:|----------|
| `src/bin/cli/commands/config.rs` | 885 | 39 | CLI command handler |
| `src/bin/cli/commands/task.rs` | 360 | 20 | CLI command handler |
| `src/bin/cli/commands/auth.rs` | 258 | 23 | CLI command handler |
| `src/bin/cli/commands/system.rs` | 181 | 2 | CLI command handler |
| `src/bin/cli/commands/dlq.rs` | 150 | 4 | CLI command handler |
| `src/bin/cli/commands/worker.rs` | 122 | 2 | CLI command handler |
| `src/bin/tasker-cli.rs` | 35 | 2 | CLI entry point |
| **Total** | **1,991** | **92** | |

All 0% files are CLI binary code. The `tasker-cli.rs` entry point (502 source lines, 35 instrumented lines) defines the clap CLI structure and the `main()` function. The six command handler modules implement the actual command logic.

---

## Lowest Coverage Files

| File | Lines Covered | Lines Total | Line % | Function % |
|------|-------------:|------------:|-------:|----------:|
| `src/grpc_clients/worker_grpc_client.rs` | 4 | 133 | 3.01% | 4.35% |
| `src/grpc_clients/conversions.rs` | 98 | 1,137 | 8.62% | 9.63% |
| `src/error.rs` | 3 | 30 | 10.00% | 20.00% |
| `src/grpc_clients/orchestration_grpc_client.rs` | 64 | 597 | 10.72% | 12.35% |
| `src/api_clients/worker_client.rs` | 69 | 375 | 18.40% | 14.06% |
| `src/api_clients/orchestration_client.rs` | 398 | 1,205 | 33.03% | 21.52% |
| `src/transport.rs` | 119 | 357 | 33.33% | 20.45% |
| `src/config.rs` | 327 | 550 | 59.45% | 50.98% |
| `src/grpc_clients/common.rs` | 113 | 190 | 59.47% | 62.07% |

---

## Gap Analysis by Priority

### Critical Priority

**1. `error.rs` -- 10.00% (3/30 lines)**

This file defines the `ClientError` enum and helper constructors. It is the foundation of error handling for the entire crate. Currently only the `From` trait implementations are exercised (through deserialization paths). The `is_recoverable()` method, `api_error()`, `config_error()`, `service_unavailable()`, and `invalid_response()` constructors have no direct test coverage.

**Risk**: Error categorization logic directly affects retry behavior and user-facing error messages. Incorrect `is_recoverable()` classification could cause retries on non-recoverable errors or vice versa.

**Tests needed**:
- Unit tests for each `ClientError` variant construction
- Unit tests for `is_recoverable()` across all variants (HTTP timeout, connect errors, 5xx responses, protocol violations)
- Unit tests for `api_error()`, `config_error()`, `service_unavailable()`, `invalid_response()` constructors
- Tests verifying `Display` output for each variant

**Estimated lines covered**: ~27 additional lines (reaching ~100%)

---

**2. `api_clients/orchestration_client.rs` -- 33.03% (398/1,205 lines)**

This is the largest library file (1,771 source lines, 1,205 instrumented). It implements the HTTP REST client for all orchestration API operations: task CRUD, step management, DLQ operations, health checks, analytics/metrics, templates, and configuration endpoints. The covered portion includes `new()`, `list_templates()`, `get_template()`, and `resolve_step_manually()`. Uncovered methods include:
- `create_task()`, `get_task()`, `list_tasks()`, `cancel_task()`
- `list_task_steps()`, `get_step()`, `get_step_audit_history()`
- `health_check()`, `get_basic_health()`, `get_detailed_health()`
- `get_dlq_entry()`, `list_dlq_entries()`, `get_dlq_stats()`, `update_dlq_investigation()`
- `get_bottlenecks()`, `get_staleness_monitoring()`, `get_investigation_queue()`
- `get_performance_metrics()`, `get_prometheus_metrics()`, `get_config()`
- `handle_response::<T>()` for all type instantiations

**Risk**: These are the primary programmatic interfaces for external systems. Untested client methods could have incorrect URL construction, missing headers, or improper error mapping.

**Tests needed**:
- Mock HTTP server (using `wiremock` or `mockito`) tests for each API method
- Tests for URL path construction for each endpoint
- Tests for query parameter serialization (e.g., `TaskListQuery`)
- Tests for `handle_response()` with success, error, and deserialization failure scenarios
- Tests for authentication header injection (API key, bearer token)
- Tests for retry behavior on transient failures

**Estimated lines covered**: ~500-600 additional lines (reaching ~75-80%)

---

### High Priority

**3. `api_clients/worker_client.rs` -- 18.40% (69/375 lines)**

The worker API client implements health checks, template operations, and configuration retrieval for the worker service. Covered portions are limited to struct construction and some template operations. Uncovered methods include `health_check()`, `liveness_probe()`, `readiness_probe()`, `get_detailed_health()`, `get_template()`, `get_config()`, and `get_handler_registry()`.

**Risk**: Worker health checks are critical for Kubernetes liveness/readiness probes. Incorrect client behavior could mask worker failures.

**Tests needed**:
- Mock HTTP server tests for all worker API methods
- Tests for health check response parsing
- Tests for template query parameter handling
- Tests for authentication header propagation

**Estimated lines covered**: ~200-250 additional lines (reaching ~75-80%)

---

**4. `transport.rs` -- 33.33% (119/357 lines)**

The transport abstraction layer provides the `OrchestrationClient` and `WorkerClient` traits plus `RestOrchestrationClient`, `RestWorkerClient`, and unified enum wrappers. Existing tests cover client creation and transport name verification. The async trait implementations (all method delegations) and gRPC client variants are untested.

**Risk**: The unified client is the recommended interface for transport-agnostic code. Incorrect delegation could silently break gRPC or REST paths.

**Tests needed**:
- Tests for `RestOrchestrationClient` delegation methods (requires mock HTTP server)
- Tests for `RestWorkerClient` delegation methods
- Tests for `UnifiedOrchestrationClient::from_config()` with REST transport
- Tests for `UnifiedWorkerClient::from_config()` with REST transport
- Tests for `as_client()` trait object dispatch
- Tests for `list_tasks()` page/offset conversion logic in REST implementation
- Tests for gRPC transport selection error when feature is disabled

**Estimated lines covered**: ~100-150 additional lines (reaching ~65-75%)

---

**5. `grpc_clients/conversions.rs` -- 8.62% (98/1,137 lines)**

This is the second-largest file (1,468 source lines, 1,137 instrumented). It implements bidirectional conversions between Protobuf types and Rust domain types for tasks, steps, templates, health responses, and DLQ entries. The file contains `From` and `TryFrom` implementations for every gRPC message type. Only a small fraction of conversions are exercised by existing gRPC transport tests.

**Risk**: Incorrect conversions would silently corrupt data crossing the gRPC transport boundary. Missing field mappings or wrong default values are especially dangerous for production data.

**Tests needed**:
- Unit tests for each `From<Proto> -> Domain` conversion (round-trip testing)
- Unit tests for each `From<Domain> -> Proto` conversion
- Edge case tests: empty optional fields, zero values, empty strings
- Tests for `TryFrom` error paths (missing required fields in proto messages)
- Tests for timestamp/UUID serialization fidelity
- Tests for `StepManualAction` -> proto conversion variants

**Estimated lines covered**: ~700-800 additional lines (reaching ~75-80%)

---

### Medium Priority

**6. `grpc_clients/orchestration_grpc_client.rs` -- 10.72% (64/597 lines)**

The gRPC orchestration client wraps tonic-generated stubs and converts between proto and domain types. Most methods are untested. Coverage comes from struct initialization and a few method invocations during E2E tests.

**Risk**: gRPC client methods could have incorrect request construction or response conversion. Since gRPC is the recommended high-performance transport, these paths need validation.

**Tests needed**:
- Integration tests with a mock gRPC server (tonic test utilities)
- Tests for each orchestration gRPC method
- Tests for authentication interceptor integration
- Tests for error status mapping (already partially covered via `common.rs`)

**Estimated lines covered**: ~300-400 additional lines (reaching ~65-75%)

---

**7. `grpc_clients/worker_grpc_client.rs` -- 3.01% (4/133 lines)**

The gRPC worker client is the least-covered library file. Only struct construction is covered. All health check, template, and config methods are untested.

**Risk**: The gRPC worker transport is entirely untested, meaning any change could break gRPC worker health monitoring.

**Tests needed**:
- Integration tests with a mock gRPC server
- Tests for each worker gRPC method
- Tests for error handling and status mapping

**Estimated lines covered**: ~80-100 additional lines (reaching ~65-75%)

---

**8. `config.rs` -- 59.45% (327/550 lines)**

Nearly at the 60% target. The existing 16 unit tests cover default configuration, serialization, auth config resolution, transport parsing, profile application, and save/load round trips. Uncovered areas include:
- `load()` with environment variable overrides
- `load_profile()` error paths (missing profile, missing file)
- `apply_env_overrides()` for gRPC transport URL resolution
- `find_config_file()` and `find_profile_config_file()` path discovery
- `list_profiles()` with profile file present
- `default_config_path()` error handling

**Tests needed**:
- Tests for `apply_env_overrides()` with various environment variable combinations
- Tests for gRPC-specific URL environment override logic
- Tests for auth environment variable resolution (global vs. endpoint-specific)
- Tests for `load_profile()` error case (non-default profile without profile file)
- Tests for `save_to_file()` with nested directory creation

**Estimated lines covered**: ~50-80 additional lines (reaching ~70-75%)

---

### Lower Priority

**9. CLI Binary Code (0% -- 1,991 lines total)**

The CLI binary code consists of `tasker-cli.rs` (Clap argument definitions and main dispatch, 502 source lines / 35 instrumented), plus 6 command handler modules. These files are standard CLI glue code that:

- Parses command-line arguments (Clap derive macros, not instrumentable)
- Creates API client instances from configuration
- Calls API client methods and formats output to stdout/stderr

**Risk assessment**: The CLI handlers primarily delegate to the already-testable library code. The main risk areas are:
- Input validation (JSON parsing, UUID parsing) in `task.rs` and `dlq.rs`
- DLQ status string parsing in `dlq.rs`
- Config generation/validation/explain logic in `config.rs` (1,074 lines, most complex)
- Auth key generation and token management in `auth.rs`

**Testing strategy considerations**:
1. **Extract testable logic**: Move input validation, status parsing, and config generation logic into the library crate where it can be unit tested directly.
2. **Subprocess tests**: Use `assert_cmd` crate for end-to-end CLI testing.
3. **Accept partial coverage**: CLI output formatting and print statements provide low testing value. Focus on logic paths.

**Tests needed** (highest value):
- Extract and unit test `extract_error_position()` from `config.rs` commands
- Extract and unit test DLQ status string parsing from `dlq.rs`
- Subprocess tests for `tasker-cli config show` (no network required)
- Subprocess tests for `tasker-cli auth show-permissions` (no network required)
- Subprocess tests for `tasker-cli config validate` with test fixtures

**Estimated lines covered**: Depends on approach; extracting logic could cover ~200-400 lines from the library side. Subprocess tests would not appear in library coverage metrics since CLI binary code is excluded from `cargo test` instrumentation.

---

**10. `grpc_clients/common.rs` -- 59.47% (113/190 lines)**

Nearly at the 60% target. Has 8 existing tests covering auth config, client config, and status-to-error conversions. Uncovered areas include the `GrpcClientConfig::connect()` async method and the `AuthInterceptor::call()` method for the API key path.

**Tests needed**:
- Test `AuthInterceptor::call()` with bearer token
- Test `AuthInterceptor::call()` with API key and custom header
- Test `AuthInterceptor::call()` with no auth configured
- Test `GrpcClientConfig::connect()` with invalid endpoint (error path)
- Additional status code mapping tests (cancelled, resource exhausted)

**Estimated lines covered**: ~30-40 additional lines (reaching ~75-80%)

---

## Recommended Test Plan

### Phase 1: Quick Wins to Reach ~25% (from 5.06%)

**Focus**: Library unit tests that require no infrastructure.

1. **`error.rs` tests** -- Full coverage of error construction and classification (~27 lines)
2. **`grpc_clients/conversions.rs` unit tests** -- Proto/domain round-trip tests (~700 lines)
3. **`grpc_clients/common.rs` gap closure** -- AuthInterceptor and status mapping (~35 lines)
4. **`config.rs` gap closure** -- Environment override and profile tests (~60 lines)

**Estimated result**: ~2,244 lines covered / 28,104 total = ~8.0% (library-only gains)

### Phase 2: Mock Server Tests to Reach ~40%

**Focus**: API client and transport tests using `wiremock` or `mockito`.

5. **`api_clients/orchestration_client.rs` mock tests** -- All HTTP methods (~550 lines)
6. **`api_clients/worker_client.rs` mock tests** -- All worker methods (~230 lines)
7. **`transport.rs` delegation tests** -- REST wrapper methods (~130 lines)

**Estimated result**: ~3,154 lines covered / 28,104 total = ~11.2%

### Phase 3: gRPC Integration Tests to Reach ~50%

**Focus**: gRPC client tests using tonic test server.

8. **`grpc_clients/orchestration_grpc_client.rs`** -- Mock gRPC server tests (~350 lines)
9. **`grpc_clients/worker_grpc_client.rs`** -- Mock gRPC server tests (~90 lines)

**Estimated result**: ~3,594 lines covered / 28,104 total = ~12.8%

### Phase 4: CLI Testability to Approach 60%

**Focus**: Extracting logic and subprocess tests.

10. **Extract CLI logic into library** -- Move validation, parsing, config generation utilities
11. **Subprocess CLI tests** -- `assert_cmd` tests for non-network commands
12. **CLI config command tests** -- Test config generate/validate with fixtures

### Note on Coverage Denominator

The total instrumented line count (28,104) is significantly inflated beyond the actual source line count (~8,900 lines across 21 files). This inflation comes from monomorphized generic functions, async closures, and trait implementations that are counted multiple times in `cargo-llvm-cov` output. The coverage report lists 4,159 total functions vs. an expected ~200-300 unique functions, confirming heavy monomorphization.

**Practical implication**: Reaching 60% on the instrumented count requires covering ~16,862 lines, while the actual unique codebase is ~8,900 lines. Many of the "uncovered functions" in the report are monomorphized copies of `handle_response::<T>` (44 uncovered variants listed for `orchestration_client.rs` alone). A single mock-server test exercising `handle_response()` with one concrete type would cover the shared logic, but coverage tools count each monomorphized instantiation separately.

**Realistic target**: Achieving 60% of instrumented lines will require either (a) exercising most monomorphized paths through diverse API tests, or (b) adjusting the target to account for monomorphization inflation. A practical goal would be to achieve 80%+ coverage on unique source lines, which may map to ~40-50% on instrumented lines.

---

## Estimated Impact

| Phase | New Lines Covered | Cumulative Coverage | Key Files Improved |
|-------|------------------:|--------------------:|-------------------|
| Current | 1,422 | 5.06% | config.rs, common.rs at ~59% |
| Phase 1 | ~822 | ~8.0% | error.rs, conversions.rs, common.rs, config.rs |
| Phase 2 | ~910 | ~11.2% | orchestration_client.rs, worker_client.rs, transport.rs |
| Phase 3 | ~440 | ~12.8% | gRPC orchestration + worker clients |
| Phase 4 | Variable | ~15-20% | CLI logic extraction + subprocess tests |

**Assessment**: Due to monomorphization inflation in the instrumented line count, reaching 60% from the current 5.06% is extremely ambitious for this crate. The library code (excluding CLI) has an actual codebase of ~5,900 lines, and comprehensive testing of those ~5,900 lines would yield roughly 4,000-5,000 instrumented lines covered, translating to ~15-18% of the 28,104 instrumented total. Reaching 60% would require testing most monomorphized paths AND adding CLI test infrastructure.

**Recommended approach**: Focus on achieving high coverage for library code (phases 1-3), which provides the most safety value per test effort. Consider whether the 60% target should be applied to the library code separately from the CLI binary code, given the fundamentally different testability characteristics.
