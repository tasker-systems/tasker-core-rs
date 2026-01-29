# TAS-177 Phase 6: gRPC Testing Infrastructure Plan

## Overview

Add comprehensive gRPC testing infrastructure that allows existing E2E tests to run against both REST and gRPC transports. The key principle is **transport abstraction**: same tests, same fixtures, different transport.

## Goals

1. **Transport-agnostic testing** - Existing E2E tests run unmodified with both REST and gRPC
2. **Auth testing parity** - Same API key/JWT tests for gRPC as REST
3. **Configuration-driven** - Profile-based client config (like nextest)
4. **Service-level tests** - Endpoint consistency and correctness tests
5. **Cluster support** - Multi-instance gRPC testing

## Integration with Existing Infrastructure

This plan builds on existing infrastructure rather than creating parallel systems:

- **Composable dotenv system** (`config/dotenv/`) - Already handles env var layering
- **Generated TOML configs** (`config/tasker/generated/`) - Already provides `auth-test.toml`, etc.
- **tasker-client config** (`ClientConfig` in `tasker-client/src/config.rs`) - Extend with profiles

## Implementation Phases

### Phase 6.1: Client Profile System (like nextest)
**Files to modify:**
- `tasker-client/src/config.rs` - Add profile support
- `tasker-client/src/bin/tasker-cli.rs` - Add `--profile` flag

**Files to create:**
- `.config/tasker-client.toml` - Example profiles for the project

**Profile-based config schema:**
```toml
# .config/tasker-client.toml
# Profile-based configuration (like nextest.toml)

[profile.default]
# Default: REST transport
transport = "rest"

[profile.default.orchestration]
base_url = "http://localhost:8080"
timeout_ms = 30000

[profile.default.worker]
base_url = "http://localhost:8081"
timeout_ms = 30000

[profile.grpc]
# gRPC transport
transport = "grpc"

[profile.grpc.orchestration]
base_url = "http://localhost:9090"
timeout_ms = 30000

[profile.grpc.worker]
base_url = "http://localhost:9100"
timeout_ms = 30000

[profile.auth]
# REST with authentication (uses auth-test.toml server config)
transport = "rest"

[profile.auth.orchestration]
base_url = "http://localhost:8080"
auth.method.type = "ApiKey"
auth.method.value.key = "test-api-key-full-access"
auth.method.value.header_name = "X-API-Key"

[profile.grpc-auth]
# gRPC with authentication
transport = "grpc"

[profile.grpc-auth.orchestration]
base_url = "http://localhost:9090"
auth.method.type = "ApiKey"
auth.method.value.key = "test-api-key-full-access"
auth.method.value.header_name = "X-API-Key"

[profile.ci]
# CI profile with longer timeouts
transport = "rest"

[profile.ci.orchestration]
base_url = "http://localhost:8080"
timeout_ms = 60000
max_retries = 5
```

**Usage:**
```bash
# Use default profile (REST)
tasker-cli task list

# Use gRPC profile
tasker-cli --profile grpc task list

# Use auth profile
tasker-cli --profile auth task create ...

# Override profile with env var
TASKER_CLIENT_PROFILE=grpc tasker-cli task list
```

### Phase 6.2: Transport Selection in Client
**Files to modify:**
- `tasker-client/src/config.rs` - Add `transport` field to config
- `tasker-client/src/lib.rs` - Add unified client enum

**Transport enum in client library:**
```rust
// tasker-client/src/lib.rs

/// Transport protocol for API communication
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    #[default]
    Rest,
    Grpc,
}

/// Unified orchestration client that works with either transport
pub enum OrchestrationClient {
    Rest(OrchestrationApiClient),
    #[cfg(feature = "grpc")]
    Grpc(OrchestrationGrpcClient),
}

impl OrchestrationClient {
    /// Create client from config (respects transport setting)
    pub async fn from_config(config: &ClientConfig) -> ClientResult<Self> {
        match config.transport {
            Transport::Rest => {
                let api_config = config.orchestration.to_api_config();
                Ok(Self::Rest(OrchestrationApiClient::new(api_config)?))
            }
            #[cfg(feature = "grpc")]
            Transport::Grpc => {
                let grpc_config = config.orchestration.to_grpc_config();
                Ok(Self::Grpc(OrchestrationGrpcClient::with_config(grpc_config).await?))
            }
            #[cfg(not(feature = "grpc"))]
            Transport::Grpc => Err(ClientError::config_error(
                "gRPC transport requires 'grpc' feature"
            )),
        }
    }

    // Delegate methods to underlying client
    pub async fn create_task(&self, request: TaskRequest) -> ClientResult<TaskResponse> {
        match self {
            Self::Rest(c) => c.create_task(request).await,
            #[cfg(feature = "grpc")]
            Self::Grpc(c) => c.create_task(request).await,
        }
    }
    // ... other methods
}
```

**Config changes:**
```rust
// tasker-client/src/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Transport protocol (rest or grpc)
    #[serde(default)]
    pub transport: Transport,
    /// Orchestration API configuration
    pub orchestration: ApiEndpointConfig,
    /// Worker API configuration
    pub worker: ApiEndpointConfig,
    /// CLI-specific settings
    pub cli: CliConfig,
}
```

### Phase 6.3: Extended IntegrationTestManager
**Files to modify:**
- `tests/common/integration_test_manager.rs` - Add gRPC and profile-based setup

**New methods:**
```rust
impl IntegrationTestManager {
    /// Setup with gRPC transport (validates via gRPC health)
    pub async fn setup_grpc() -> Result<Self> {
        let config = IntegrationConfig {
            transport: Transport::Grpc,
            orchestration_url: env::var("TASKER_TEST_ORCHESTRATION_GRPC_URL")
                .unwrap_or_else(|_| "http://localhost:9090".to_string()),
            worker_url: env::var("TASKER_TEST_WORKER_GRPC_URL")
                .ok()
                .or_else(|| Some("http://localhost:9100".to_string())),
            ..Default::default()
        };
        Self::setup_with_config(config).await
    }

    /// Setup from client profile (reads .config/tasker-client.toml)
    pub async fn setup_with_profile(profile_name: &str) -> Result<Self> {
        let client_config = ClientConfig::load_profile(profile_name)?;
        Self::from_client_config(client_config).await
    }

    /// Setup from TASKER_TEST_TRANSPORT env var
    pub async fn setup_from_env() -> Result<Self> {
        match env::var("TASKER_TEST_TRANSPORT").as_deref() {
            Ok("grpc") => Self::setup_grpc().await,
            _ => Self::setup().await,  // Default to REST
        }
    }

    /// gRPC health validation
    async fn validate_grpc_orchestration_service(config: &IntegrationConfig) -> Result<()>
    async fn validate_grpc_worker_service(url: &str, config: &IntegrationConfig) -> Result<()>
}
```

**IntegrationConfig changes:**
```rust
#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    pub transport: Transport,  // NEW
    pub orchestration_url: String,
    pub worker_url: Option<String>,
    pub skip_health_check: bool,
    pub health_timeout_seconds: u64,
    pub health_retry_interval_seconds: u64,
}
```

### Phase 6.4: gRPC Auth Test Helpers
**Files to create:**
- `tests/common/grpc_test_helpers.rs` - gRPC-specific auth utilities

**Re-use existing constants from `auth_test_helpers.rs`:**
- `TEST_API_KEY_FULL`
- `TEST_API_KEY_READ_ONLY`
- `TEST_API_KEY_TASKS_ONLY`
- `TEST_API_KEY_NO_PERMS`
- `generate_jwt()`, `generate_expired_jwt()`

**New gRPC helpers:**
```rust
/// gRPC client with API key auth
pub async fn grpc_client_with_api_key(endpoint: &str, api_key: &str)
    -> Result<OrchestrationGrpcClient>

/// gRPC client with JWT auth
pub async fn grpc_client_with_jwt(endpoint: &str, permissions: &[&str])
    -> Result<OrchestrationGrpcClient>

/// gRPC client without auth
pub async fn grpc_client_unauthenticated(endpoint: &str)
    -> Result<OrchestrationGrpcClient>
```

### Phase 6.5: gRPC-Specific Tests
**Files to create:**
- `tests/grpc/mod.rs` - Module gate with `#[cfg(feature = "test-services")]`
- `tests/grpc/health_tests.rs` - Health endpoint tests
- `tests/grpc/auth_tests.rs` - Authentication/permission tests
- `tests/grpc/parity_tests.rs` - REST/gRPC response comparison

**Auth test scenarios (mirroring REST):**
1. Unauthenticated request rejected
2. Full access API key works
3. Read-only key can list but not create
4. Tasks-only key scoped correctly
5. No-permissions key denied
6. JWT authentication works
7. Expired JWT rejected
8. Invalid API key rejected

**Parity tests:**
1. Health response structure matches
2. Task creation response matches
3. Detailed health response matches
4. Template list response matches

### Phase 6.6: E2E Test Transport Support
**Files to modify:**
- `tests/e2e/rust/linear_workflow.rs` (and other E2E tests)
- `tests/common/test_helpers.rs` - Add transport-aware helpers

**Pattern for transport-aware tests:**
```rust
#[tokio::test]
async fn test_linear_workflow() -> Result<()> {
    // Uses TASKER_TEST_TRANSPORT env var
    let manager = IntegrationTestManager::setup_from_env().await?;

    // Same test logic works with both transports
    let task = manager.client.create_task(request).await?;
    wait_for_completion(&manager.client, task.task_uuid).await?;
}
```

### Phase 6.7: Cluster Testing Extension
**Files to modify:**
- `tests/common/multi_instance_test_manager.rs` - Add gRPC cluster support

**New types:**
```rust
pub struct GrpcClusterConfig {
    pub orchestration_grpc_urls: Vec<String>,  // from env or calculated
    pub worker_grpc_urls: Vec<String>,
}

impl MultiInstanceTestManager {
    pub async fn setup_grpc(orch_count: usize, worker_count: usize) -> Result<Self>
    pub async fn setup_grpc_from_env() -> Result<Self>
}
```

### Phase 6.8: Cargo Make Tasks
**Files to modify:**
- `Makefile.toml` - Add gRPC test targets

**New tasks:**
```toml
[tasks.test-grpc]         # tg  - All gRPC tests
[tasks.test-grpc-auth]    # tga - gRPC auth tests (uses auth-test.toml)
[tasks.test-grpc-parity]  # tgp - REST/gRPC parity tests
[tasks.test-e2e-grpc]     # Run E2E tests with gRPC transport
[tasks.test-grpc-cluster] # tgc - gRPC cluster tests (requires cluster-start)
[tasks.test-both]         # Run E2E with both REST and gRPC
```

## File Summary

### New Files
| File | Purpose |
|------|---------|
| `.config/tasker-client.toml` | Profile-based client config (like nextest) |
| `tests/common/grpc_test_helpers.rs` | gRPC auth utilities |
| `tests/grpc/mod.rs` | gRPC test module |
| `tests/grpc/health_tests.rs` | Health endpoint tests |
| `tests/grpc/auth_tests.rs` | Auth/permission tests |
| `tests/grpc/parity_tests.rs` | Response parity tests |
| `tests/grpc_tests.rs` | gRPC test entry point |
| `config/dotenv/grpc-test.env` | gRPC-specific env vars (layered with existing system) |

### Modified Files
| File | Changes |
|------|---------|
| `tasker-client/src/config.rs` | Add profile support, transport field |
| `tasker-client/src/lib.rs` | Add `Transport` enum, `OrchestrationClient` unified type |
| `tasker-client/src/bin/tasker-cli.rs` | Add `--profile` flag |
| `tests/common/mod.rs` | Export new modules |
| `tests/common/integration_test_manager.rs` | Add gRPC setup, profile support |
| `tests/common/multi_instance_test_manager.rs` | Add gRPC cluster support |
| `Makefile.toml` | Add cargo make tasks |
| `config/dotenv/README.md` | Document gRPC env vars |

## Environment Variables

### New gRPC Variables (add to `config/dotenv/grpc-test.env`)
| Variable | Default | Description |
|----------|---------|-------------|
| `TASKER_TEST_TRANSPORT` | `rest` | Transport: "rest" or "grpc" |
| `TASKER_TEST_ORCHESTRATION_GRPC_URL` | `http://localhost:9090` | gRPC orchestration endpoint |
| `TASKER_TEST_WORKER_GRPC_URL` | `http://localhost:9100` | gRPC worker endpoint |
| `TASKER_CLIENT_PROFILE` | (none) | Client config profile to use |

### Existing Variables (already in `config/dotenv/`)
| Variable | Source File | Description |
|----------|-------------|-------------|
| `TASKER_TEST_ORCHESTRATION_URL` | `test.env` | REST orchestration endpoint |
| `TASKER_TEST_WORKER_URL` | `test.env` | REST worker endpoint |
| `TASKER_TEST_ORCHESTRATION_URLS` | `cluster.env` | Multi-instance REST URLs |
| `TASKER_TEST_WORKER_URLS` | `cluster.env` | Multi-instance worker URLs |

### Integration with dotenv System
The new gRPC variables will be added to `config/dotenv/grpc-test.env` and assembled using the existing `setup-env.sh` script:

```bash
# New assembly mode
cargo make setup-env-grpc        # .env for gRPC tests
cargo make setup-env-grpc-auth   # .env for gRPC auth tests
```

Layer order: `base.env` → `test.env` → `grpc-test.env` → target-specific

## Verification Steps

1. **Unit tests pass:**
   ```bash
   cargo make test-rust-unit
   ```

2. **gRPC health tests pass:**
   ```bash
   # Start services
   docker compose -f docker/docker-compose.test.yml up -d
   cargo make test-grpc
   ```

3. **gRPC auth tests pass:**
   ```bash
   cargo make test-grpc-auth
   ```

4. **Response parity verified:**
   ```bash
   cargo make test-grpc-parity
   ```

5. **E2E tests pass with both transports:**
   ```bash
   cargo make test-both
   ```

6. **Cluster tests pass:**
   ```bash
   cargo make cluster-start
   cargo make test-grpc-cluster
   ```

## Implementation Order

1. **Phase 6.1**: Client profile system - Add profiles to `ClientConfig`, `--profile` flag to CLI
2. **Phase 6.2**: Transport selection - Add `Transport` enum, unified client wrapper
3. **Phase 6.3**: IntegrationTestManager gRPC - Add `setup_grpc()`, `setup_from_env()`
4. **Phase 6.4**: gRPC auth helpers - Create `grpc_test_helpers.rs` wrapping existing auth constants
5. **Phase 6.5**: gRPC-specific tests - Health, auth, parity tests
6. **Phase 6.6**: E2E transport support - Modify existing tests to use `setup_from_env()`
7. **Phase 6.7**: Cluster testing - Extend multi-instance manager for gRPC
8. **Phase 6.8**: Cargo make tasks - Add `test-grpc`, `test-grpc-auth`, etc.

## Key Design Decisions

1. **Profile-based config (like nextest)**: The `.config/tasker-client.toml` file provides named profiles for different scenarios. This pattern is familiar to developers and provides good documentation of available configurations.

2. **Extend existing client library**: Add `Transport` enum and unified `OrchestrationClient` to `tasker-client` rather than creating test-specific abstractions. This benefits both testing and production use.

3. **Leverage existing dotenv system**: Add `grpc-test.env` to the composable dotenv system rather than creating a separate test config system. The existing `setup-env.sh` script handles assembly.

4. **Re-use existing auth infrastructure**: The auth test helpers already have all the JWT/API key constants; gRPC helpers just wrap them with `GrpcAuthConfig`.

5. **No new fixtures**: Existing task templates and handlers are sufficient; tests verify both transports produce equivalent results.

6. **Feature-gated gRPC**: The `grpc` feature in `tasker-client` controls gRPC availability. Tests use `#[cfg(feature = "grpc")]` for gRPC-specific code.

7. **Config precedence**: `ENV VAR > CLI --profile > selected profile > [profile.default] > hardcoded defaults`. This matches the existing config precedence pattern.

8. **CI matrix strategy (2+1)**: Transport is orthogonal to messaging backend, so we use a 2+1 strategy rather than 2x2:
   - **Backend matrix**: (PGMQ, RabbitMQ) × REST = 2 jobs (existing)
   - **Transport job**: All E2E with gRPC = 1 job (new)
   - **Total**: 3 jobs with same coverage as 4-job 2x2 matrix

## CI Configuration

### Current Matrix (Backend)
```yaml
# .github/workflows/test.yml (existing)
strategy:
  matrix:
    messaging_backend: [pgmq, rabbitmq]
env:
  TASKER_TEST_TRANSPORT: rest  # Default transport
```

### New gRPC Transport Job
```yaml
# Add as separate job, not matrix expansion
test-grpc-transport:
  name: E2E Tests (gRPC Transport)
  needs: [build]  # Reuse build artifacts
  env:
    TASKER_TEST_TRANSPORT: grpc
    TASKER_TEST_ORCHESTRATION_GRPC_URL: http://localhost:9090
    TASKER_TEST_WORKER_GRPC_URL: http://localhost:9100
  steps:
    - name: Start services with gRPC
      run: docker compose -f docker/docker-compose.test.yml up -d
    - name: Run E2E tests with gRPC transport
      run: cargo make test-e2e-grpc
```

### Why 2+1 Works
- **Transport independence**: REST/gRPC is a client→server concern
- **Backend independence**: PGMQ/RabbitMQ is orchestration→worker concern
- **No cross-product bugs**: A bug in "gRPC + RabbitMQ" would manifest in either "gRPC + PGMQ" or "REST + RabbitMQ"
- **Cost savings**: 3 jobs vs 4 jobs = 25% reduction in CI time
