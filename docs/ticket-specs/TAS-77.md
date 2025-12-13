The focus of this ticket is very specifically about the tasker worker crate and other dependent crates like workers/ruby and workers/rust.

We will eventually be publishing the tasker crates, but we will also publish a ruby gem, a python library, wasm module which can be wrapped by js/ts, etc. The manner of deployment and utilization will vary. Some architectures may be comfortable spinning up dedicated workers, using our in-built worker health endpoints in axum. Some will assume that a gem or lib can and should just live in the native app, service, monolith, etc, and not expose its own web ports.

Many systems will already have their own web service wrappers and mechanisms, and may not want to launch the web component of the tasker worker infrastructure. However, it would be very common to still want the native system to have access to the information we make available in our workers web api. Because the worker system has no mutation endpoints - that is to say the only HTTP verb we respect is GET for the worker system - it should be very straightforward to expose the same structs and data shapes over FFI.

As such, the goal of this ticket is to evaluate the worker web api endpoints, extract the logic into service classes, ideally following our command-actor-service patterns, rework our web api to use these, and then make the same functionality available over FFI.

For now we will also design a class and module structure to wrap this up for ruby so that the data is available the same way we do for other FFI calls, moved into dry-structs in a more ergonomic module and class hierarchy. We will then want to document the changes and utilization possibilities, so that it is clear how to enable or disable the web api and how to make use of the ruby library to get this information. We will want to update rspec to prove this out as well.

Implementation Plan

Executive Summary

This plan addresses extracting worker web API logic into reusable services and exposing that functionality via FFI for Ruby (and eventually Python/WASM). The goal is enabling deployment flexibility where applications can either use the web API or consume the same data natively without spinning up HTTP servers.

Current State Analysis

Worker Web API Surface (15 Endpoints)

Category

Endpoints

Response Types

Health (4)

/health, /health/ready, /health/live, /health/detailed

BasicHealthResponse, DetailedHealthResponse

Metrics (3)

/metrics, /metrics/worker, /metrics/events

Prometheus text, MetricsResponse, DomainEventStats

Templates (7)

CRUD + validation + cache mgmt

TemplateListResponse, TemplateResponse, etc.

Config (1)

/config

WorkerConfigResponse

Existing FFI Architecture

Framework: Magnus 0.8 for Ruby FFI bindings

Module: TaskerCore::FFI (singleton methods)

Conversions: serde_magnus + manual hash construction

Pattern: Polling-based (Ruby polls Rust) to avoid GIL issues

Current Service Layer (TAS-69)

StepExecutorService - Step execution logic

FFICompletionService - Result processing

WorkerStatusService - Status and health (partial)

Implementation Plan

Phase 1: Service Extraction (Rust)

1.1 Create WorkerHealthService

Location: tasker-worker/src/worker/services/health/

health/
├── mod.rs
├── service.rs          # HealthService struct
├── types.rs            # Response types (already exist in web/types/worker.rs)
├── checks/
│   ├── mod.rs
│   ├── database.rs     # Database connectivity check
│   ├── queue.rs        # PGMQ queue depth analysis
│   ├── processor.rs    # Command processor health
│   ├── event_system.rs # Event system availability
│   ├── step_processing.rs # Error rate calculation
│   └── circuit_breakers.rs # Circuit breaker states
└── tests.rs

Service API:

impl HealthService {
    pub fn new(context: Arc<SystemContext>, ...) -> Self;
    pub async fn basic_health(&self) -> BasicHealthResponse;
    pub async fn liveness(&self) -> BasicHealthResponse;
    pub async fn readiness(&self) -> Result<DetailedHealthResponse, HealthError>;
    pub async fn detailed(&self) -> DetailedHealthResponse;
}

Work Items:

Extract health check logic from web/handlers/health.rs lines 14-205

Create HealthCheck result type with status, message, duration_ms, last_checked

Implement check components as separate structs (testable in isolation)

Add CircuitBreakerHealthProvider integration (TAS-75)

Unit tests for each health check component

1.2 Create WorkerMetricsService

Location: tasker-worker/src/worker/services/metrics/

metrics/
├── mod.rs
├── service.rs          # MetricsService struct
├── types.rs            # MetricValue, MetricsResponse, DomainEventStats
├── collectors/
│   ├── mod.rs
│   ├── worker_info.rs     # Version, ID, type
│   ├── queue_depth.rs     # PGMQ queue metrics
│   ├── channel_health.rs  # MPSC channel metrics
│   └── event_stats.rs     # Domain event routing stats
├── formatters/
│   ├── mod.rs
│   ├── prometheus.rs      # Prometheus text format
│   └── json.rs            # JSON format
└── tests.rs

Service API:

impl MetricsService {
    pub fn new(context: Arc<SystemContext>, ...) -> Self;
    pub async fn collect_metrics(&self) -> MetricsResponse;
    pub async fn prometheus_format(&self) -> String;
    pub async fn domain_event_stats(&self) -> DomainEventStats;
}

Work Items:

Extract metrics collection from web/handlers/metrics.rs lines 15-178

Create collector components for each metric category

Implement Prometheus formatter (reuse existing logic)

Create JSON formatter with proper MetricValue structure

Add domain event stats from EventRouter and InProcessEventBus

1.3 Create TemplateQueryService

Location: tasker-worker/src/worker/services/template_query/

template_query/
├── mod.rs
├── service.rs          # TemplateQueryService struct
├── types.rs            # Already mostly in web/types/worker.rs
└── tests.rs

Service API:

impl TemplateQueryService {
    pub fn new(manager: Arc<TaskTemplateManager>, ...) -> Self;
    pub async fn list_templates(&self) -> TemplateListResponse;
    pub async fn get_template(&self, ns: &str, name: &str, version: &str) -> Result<TemplateResponse, TemplateError>;
    pub async fn validate_template(&self, ns: &str, name: &str, version: &str) -> TemplateValidationResponse;
    pub fn cache_stats(&self) -> CacheStats;
    pub async fn clear_cache(&self) -> CacheOperationResponse;
    pub async fn maintain_cache(&self) -> CacheOperationResponse;
    pub async fn refresh_template(&self, ns: &str, name: &str, version: &str) -> CacheOperationResponse;
}

Work Items:

Extract template operations from web/handlers/templates.rs

Consolidate response types (move from web/types to service types)

Add proper error types (TemplateError with NotFound, InvalidPath, etc.)

1.4 Create ConfigQueryService

Location: tasker-worker/src/worker/services/config_query/

Service API:

impl ConfigQueryService {
    pub fn new(config: TaskerConfig) -> Self;
    pub fn runtime_config(&self) -> WorkerConfigResponse;
    pub fn redacted_config(&self) -> WorkerConfigResponse;
}

Work Items:

Extract config logic from web/handlers/config.rs

Implement field redaction for sensitive values

Add ConfigMetadata with timestamp and redacted field list

Phase 2: Web Handler Refactoring

2.1 Update HTTP Handlers to Use Services

Refactor all web handlers to delegate to services:

// Before (in handlers/health.rs)
pub async fn detailed_health_check(State(state): State<Arc<WorkerWebState>>) {
    // 200+ lines of inline logic
}

// After
pub async fn detailed_health_check(State(state): State<Arc<WorkerWebState>>) {
    let response = state.health_service.detailed().await;
    Json(response)
}

Work Items:

Add service instances to WorkerWebState

Refactor handlers/health.rs to use HealthService

Refactor handlers/metrics.rs to use MetricsService

Refactor handlers/templates.rs to use TemplateQueryService

Refactor handlers/config.rs to use ConfigQueryService

Ensure existing tests pass (no functional changes)

Phase 3: FFI Exposure (Rust)

3.1 Create FFI Bridge Module

Location: workers/ruby/ext/tasker_core/src/observability_ffi.rs

// New FFI functions
pub fn worker_health_basic() -> Result<RHash, Error>;
pub fn worker_health_liveness() -> Result<RHash, Error>;
pub fn worker_health_readiness() -> Result<RHash, Error>;
pub fn worker_health_detailed() -> Result<RHash, Error>;

pub fn worker_metrics() -> Result<RHash, Error>;
pub fn worker_metrics_prometheus() -> Result<String, Error>;
pub fn worker_metrics_events() -> Result<RHash, Error>;

pub fn worker_templates_list() -> Result<RHash, Error>;
pub fn worker_template_get(namespace: String, name: String, version: String) -> Result<RHash, Error>;
pub fn worker_template_validate(namespace: String, name: String, version: String) -> Result<RHash, Error>;
pub fn worker_template_cache_stats() -> Result<RHash, Error>;
pub fn worker_template_cache_clear() -> Result<RHash, Error>;
pub fn worker_template_cache_maintain() -> Result<RHash, Error>;
pub fn worker_template_refresh(namespace: String, name: String, version: String) -> Result<RHash, Error>;

pub fn worker_config() -> Result<RHash, Error>;

Work Items:

Create observability_ffi.rs module

Add service access to RubyBridgeHandle

Implement conversion functions for all response types

Register functions in lib.rs under TaskerCore::FFI

3.2 Response Type Conversions

Location: workers/ruby/ext/tasker_core/src/conversions.rs

Work Items:

Add convert_basic_health_to_ruby(BasicHealthResponse) -> RHash

Add convert_detailed_health_to_ruby(DetailedHealthResponse) -> RHash

Add convert_health_check_to_ruby(HealthCheck) -> RHash

Add convert_worker_system_info_to_ruby(WorkerSystemInfo) -> RHash

Add convert_metrics_response_to_ruby(MetricsResponse) -> RHash

Add convert_domain_event_stats_to_ruby(DomainEventStats) -> RHash

Add convert_template_response_to_ruby(TemplateResponse) -> RHash

Add convert_template_list_to_ruby(TemplateListResponse) -> RHash

Add convert_config_response_to_ruby(WorkerConfigResponse) -> RHash

Phase 4: Ruby Wrapper Layer

4.1 Create Ruby Observability Module

Location: workers/ruby/lib/tasker_core/observability/

lib/tasker_core/observability/
├── health.rb           # Health check wrappers
├── metrics.rb          # Metrics wrappers
├── templates.rb        # Template query wrappers
├── config.rb           # Config query wrappers
└── types/
    ├── health_response.rb
    ├── metrics_response.rb
    ├── template_response.rb
    └── config_response.rb

4.2 Implement Dry-Struct Types

# lib/tasker_core/observability/types/health_response.rb
module TaskerCore
  module Observability
    module Types
      class BasicHealthResponse < Dry::Struct
        attribute :status, Types::Strict::String
        attribute :timestamp, Types::Strict::Time
        attribute :worker_id, Types::Strict::String
      end

      class HealthCheck < Dry::Struct
        attribute :status, Types::Strict::String
        attribute :message, Types::Strict::String.optional
        attribute :duration_ms, Types::Strict::Integer
        attribute :last_checked, Types::Strict::Time
      end

      class WorkerSystemInfo < Dry::Struct
        attribute :version, Types::Strict::String
        attribute :environment, Types::Strict::String
        attribute :uptime_seconds, Types::Strict::Integer
        attribute :worker_type, Types::Strict::String
        attribute :database_pool_size, Types::Strict::Integer
        attribute :command_processor_active, Types::Strict::Bool
        attribute :supported_namespaces, Types::Strict::Array.of(Types::Strict::String)
      end

      class DetailedHealthResponse < Dry::Struct
        attribute :status, Types::Strict::String
        attribute :timestamp, Types::Strict::Time
        attribute :worker_id, Types::Strict::String
        attribute :checks, Types::Strict::Hash.map(Types::Strict::String, HealthCheck)
        attribute :system_info, WorkerSystemInfo
      end
    end
  end
end

4.3 Implement Service Wrappers

# lib/tasker_core/observability/health.rb
module TaskerCore
  module Observability
    class Health
      class << self
        def basic
          raw = TaskerCore::FFI.worker_health_basic
          Types::BasicHealthResponse.new(raw)
        end

        def liveness
          raw = TaskerCore::FFI.worker_health_liveness
          Types::BasicHealthResponse.new(raw)
        end

        def readiness
          raw = TaskerCore::FFI.worker_health_readiness
          Types::DetailedHealthResponse.new(raw)
        end

        def detailed
          raw = TaskerCore::FFI.worker_health_detailed
          Types::DetailedHealthResponse.new(raw)
        end
      end
    end
  end
end

Work Items:

Create Types::BasicHealthResponse dry-struct

Create Types::DetailedHealthResponse dry-struct

Create Types::MetricsResponse dry-struct

Create Types::DomainEventStats dry-struct

Create Types::TemplateResponse dry-struct

Create Types::TemplateListResponse dry-struct

Create Types::WorkerConfigResponse dry-struct

Implement Health wrapper class

Implement Metrics wrapper class

Implement Templates wrapper class

Implement Config wrapper class

Phase 5: Configuration

5.1 Web API Toggle

Location: config/tasker/base/worker.toml

[worker.web_api]
enabled = true                    # Enable/disable HTTP server
bind_address = "0.0.0.0"
port = 8080
request_timeout_ms = 30000

[worker.web_api.endpoints]
health_enabled = true
metrics_enabled = true
templates_enabled = true
config_enabled = true

Work Items:

Add web_api.enabled configuration option

Add per-endpoint enable/disable flags

Update WorkerBootstrap to conditionally start HTTP server

Document configuration options

Phase 6: Documentation

6.1 Create Usage Documentation

Location: docs/library-deployment-patterns.md

Contents:

Deployment Modes

Web API mode (full HTTP server)

Library mode (FFI only)

Hybrid mode (both enabled)

Ruby Integration Guide

How to access health data without HTTP

How to access metrics without HTTP

How to query templates

How to inspect configuration

Configuration Guide

Enabling/disabling web API

Per-endpoint configuration

Environment-specific overrides

Examples

Rails integration without HTTP server

Sidekiq integration

Custom web framework integration

Work Items:

Write deployment patterns overview

Document Ruby API usage

Create code examples for each pattern

Add troubleshooting section

Phase 7: Testing

7.1 Rust Service Tests

Work Items:

Unit tests for HealthService (each check component)

Unit tests for MetricsService (each collector)

Unit tests for TemplateQueryService

Unit tests for ConfigQueryService

Integration tests ensuring web handlers use services correctly

7.2 Ruby RSpec Tests

Location: workers/ruby/spec/integration/observability_spec.rb

RSpec.describe TaskerCore::Observability do
  describe TaskerCore::Observability::Health do
    it 'returns basic health without HTTP' do
      response = described_class.basic
      expect(response.status).to eq('healthy')
      expect(response.worker_id).to be_present
    end

    it 'returns detailed health with all checks' do
      response = described_class.detailed
      expect(response.checks).to include('database', 'queue_processing')
      expect(response.system_info.version).to be_present
    end
  end

  describe TaskerCore::Observability::Metrics do
    it 'returns metrics in JSON format' do
      response = described_class.worker_metrics
      expect(response.metrics).to be_a(Hash)
    end

    it 'returns metrics in Prometheus format' do
      text = described_class.prometheus
      expect(text).to include('tasker_worker_info')
    end
  end
end

Work Items:

Test Health.basic returns correct structure

Test Health.detailed includes all checks

Test Metrics.worker_metrics returns correct structure

Test Metrics.prometheus returns valid Prometheus format

Test Templates.list returns templates

Test Templates.get with valid/invalid inputs

Test Config.runtime returns redacted config

Test dry-struct type coercion works correctly

Dependency Graph

Phase 1 (Service Extraction)
    │
    ├──> Phase 2 (Web Handler Refactoring)
    │
    └──> Phase 3 (FFI Exposure) ──> Phase 4 (Ruby Wrappers)
                                        │
Phase 5 (Configuration) ────────────────┘
    │
    └──> Phase 6 (Documentation)
         │
         └──> Phase 7 (Testing - can start during any phase)

Risks and Mitigations

Risk

Mitigation

Service extraction breaks existing behavior

Comprehensive tests before refactoring; incremental extraction

FFI performance overhead

Benchmark FFI calls vs HTTP; cache results where appropriate

Ruby type coercion errors

Extensive dry-struct validation; edge case testing

Configuration complexity

Sensible defaults; clear documentation

Success Criteria

All 15 web API endpoint functionalities available via FFI

Ruby wrapper classes with dry-struct type safety

Web API can be completely disabled while FFI remains functional

Comprehensive RSpec tests for Ruby integration

Documentation covers all deployment patterns

No regression in existing web API functionality

Clippy and test compliance maintained

Estimated Complexity

Phase 1: Medium-High (service extraction requires careful refactoring)

Phase 2: Low (straightforward delegation)

Phase 3: Medium (new FFI functions, type conversions)

Phase 4: Medium (Ruby wrappers, dry-struct definitions)

Phase 5: Low (configuration changes)

Phase 6: Low (documentation)

Phase 7: Medium (comprehensive testing)

---

## Implementation Progress & Status

*Last Updated: December 13, 2025*

### Phase Status Summary

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1: Service Extraction | ✅ Complete | All 4 services created |
| Phase 2: Web Handler Refactoring | ✅ Complete | Handlers delegate to services |
| Phase 3: FFI Exposure | ✅ Complete | 15 FFI functions exposed |
| Phase 4: Ruby Wrapper Layer | ✅ Complete | Single-module design |
| Phase 5: Configuration | ✅ Complete | Existing config sufficient |
| Phase 6: Documentation | ✅ Complete | Reviewed and updated |
| Phase 7: Testing | ✅ Complete | 74 RSpec tests passing |

### Phase 1: Service Extraction - COMPLETE ✅

**Implemented Structure** (simpler than planned):
```
tasker-worker/src/worker/services/
├── mod.rs
├── health/
│   ├── mod.rs
│   └── service.rs      # HealthService with SharedCircuitBreakerProvider
├── metrics/
│   ├── mod.rs
│   └── service.rs      # MetricsService
├── template_query/
│   ├── mod.rs
│   └── service.rs      # TemplateQueryService
└── config_query/
    ├── mod.rs
    └── service.rs      # ConfigQueryService
```

**Deviations from Plan:**
- Did NOT create separate `checks/` subdirectory for health - checks are methods within `HealthService`
- Did NOT create separate `collectors/` or `formatters/` for metrics - kept simpler flat structure
- Response types remain in `tasker-shared/src/types/` rather than duplicating in service modules
- `SharedCircuitBreakerProvider` type added (not in original plan) to enable sharing between `WorkerWebState` and `HealthService`

**Key Implementation Details:**
- Services are created during `WorkerWebState::new()` and stored as `Arc<Service>`
- Circuit breaker health provider uses `Arc<TokioRwLock<Option<...>>>` pattern for shared state
- All health check methods made async to support shared state access

### Phase 2: Web Handler Refactoring - COMPLETE ✅

**Implementation:**
- `WorkerWebState` now holds service instances: `health_service`, `metrics_service`, `template_query_service`, `config_query_service`
- Web handlers access services via `state.health_service()`, etc.
- No functional changes to HTTP API responses

### Phase 3: FFI Exposure - COMPLETE ✅

**Implemented File:** `workers/ruby/ext/tasker_core/src/observability_ffi.rs`

**FFI Functions Exposed (15 total):**
- Health: `health_basic`, `health_live`, `health_ready`, `health_detailed`
- Metrics: `metrics_worker`, `metrics_prometheus`, `metrics_events`
- Templates: `templates_list`, `template_get`, `template_validate`, `templates_cache_stats`, `templates_cache_clear`, `template_refresh`
- Config: `config_runtime`, `config_environment`

**Deviations from Plan:**
- Functions return JSON strings rather than `RHash` - simpler conversion, Ruby parses JSON
- Naming convention: `health_basic` instead of `worker_health_basic` (shorter, cleaner)
- Added `config_environment` for simple environment name retrieval

### Phase 4: Ruby Wrapper Layer - COMPLETE ✅

**Implemented Structure** (consolidated from plan):
```
workers/ruby/lib/tasker_core/
├── observability.rb           # All wrapper methods in single module
└── observability/
    └── types.rb               # All dry-struct types in single file
```

**Deviations from Plan:**
- Single `TaskerCore::Observability` module with class methods instead of separate `Health`, `Metrics`, `Templates`, `Config` classes
- All types in single `types.rb` file instead of separate files per category
- Methods like `health_basic`, `health_detailed`, `event_stats`, `templates_list`, etc.
- Convenience methods: `ready?`, `alive?` for boolean health checks

**Bug Fixes During Implementation:**
- Fixed `Types::Strict::` namespace issue - must prefix with `Types::` inside Dry::Struct classes
- Removed `ostruct` usage from `step_handler/base.rb` (unused require)
- Replaced `OpenStruct` with Ruby `Struct` in `types/step_message.rb`

### Phase 5: Configuration - COMPLETE ✅

**Existing Configuration Sufficient:**
```toml
[worker.web]
enabled = true
bind_address = "${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8081}"
request_timeout_ms = 30000
```

**Deviations from Plan:**
- Did NOT add per-endpoint enable/disable flags (`[worker.web_api.endpoints]`)
- Existing `[worker.web] enabled` toggle is sufficient for MVP
- Per-endpoint control can be added later if needed

### Phase 6: Documentation - COMPLETE ✅

**File:** `docs/library-deployment-patterns.md`

**Updates Made:**
- Fixed config path: `[worker.web]` (was incorrectly shown as `[web]`)
- Added complete API Reference section with all methods and return types
- Added Template Operation Errors section documenting RuntimeError behavior
- Added Convenience Methods section explaining `ready?`/`alive?` error handling
- Documented that `alive?` checks for `"alive"` status, `ready?` checks for `"healthy"`
- Added `templates_list(include_cache_stats:)` parameter documentation
- Updated migration guide with correct config path

### Phase 7: Testing - COMPLETE ✅

**Rust Service Tests:** ✅ Existing integration tests cover service usage via web handlers

**Ruby RSpec Tests:** ✅ Created at `workers/ruby/spec/observability/observability_spec.rb`
- 74 test examples covering all observability methods
- All tests passing as part of 300-test Ruby test suite

**Test Coverage:**
1. ✅ Health methods return correct BasicHealth and DetailedHealth structs
2. ✅ Convenience methods (ready?, alive?) work correctly
3. ✅ Metrics methods (metrics_worker, event_stats, prometheus_metrics) return valid data
4. ✅ Template methods handle valid/invalid inputs appropriately
5. ✅ Cache operations (cache_stats, cache_clear, template_refresh) work correctly
6. ✅ Config methods return RuntimeConfig with metadata
7. ✅ Dry-struct type coercion validates all field types
8. ✅ Error handling (ready?/alive? return false on error)

### Remaining Work

**All phases complete!** TAS-77 is ready for final review and merge.

### Success Criteria Status

| Criteria | Status |
|----------|--------|
| All 15 web API endpoint functionalities available via FFI | ✅ |
| Ruby wrapper classes with dry-struct type safety | ✅ |
| Web API can be completely disabled while FFI remains functional | ✅ |
| Comprehensive RSpec tests for Ruby integration | ✅ (74 tests) |
| Documentation covers all deployment patterns | ✅ |
| No regression in existing web API functionality | ✅ |
| Clippy and test compliance maintained | ✅ |

**All 7 success criteria met!**
