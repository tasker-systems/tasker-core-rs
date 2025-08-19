# TAS-28: Axum Web API Implementation Plan

## Executive Summary
Implement a **production-ready REST API** using Axum that provides both orchestration system visibility and task creation capabilities. The API will serve as the HTTP interface for both the orchestration system and future worker systems as outlined in TAS-40.

## Context from TAS-40
Per TAS-40 (Worker Foundations), we will NOT implement an embedded orchestration system. The orchestration core and worker systems will run as separate services from day one, communicating via HTTP API. This makes the Axum web service a critical foundation component that must be implemented before the worker system.

## API Scope Analysis

### ✅ Endpoints to Implement

#### 1. Health & Monitoring (No Auth Required) - Root Level
- `GET /health` - Basic health check
- `GET /ready` - Kubernetes readiness probe
- `GET /live` - Kubernetes liveness probe
- `GET /health/detailed` - Detailed health status (optional auth)
- `GET /metrics` - Prometheus metrics export

#### 2. Tasks API v1
- `POST /v1/tasks` - Create new task (REQUIRED for TAS-40 worker integration)
- `GET /v1/tasks` - List tasks with pagination
- `GET /v1/tasks/{uuid}` - Get task details with dependency graph
- `DELETE /v1/tasks/{uuid}` - Cancel task

#### 3. Workflow Steps API v1
- `GET /v1/tasks/{uuid}/workflow_steps` - List steps for a task
- `GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}` - Get step details
- `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}` - Update step for manual resolution only

#### 4. Handlers API v1 (Read-Only)
- `GET /v1/handlers` - List registered namespaces
- `GET /v1/handlers/{namespace}` - List handlers in namespace
- `GET /v1/handlers/{namespace}/{name}` - Get handler details with dependency graph

#### 5. Analytics API v1 (Read-Only)
- `GET /v1/analytics/performance` - System performance metrics
- `GET /v1/analytics/bottlenecks` - Bottleneck analysis

### ❌ Endpoints NOT to Implement
- `PUT/PATCH /v1/tasks/{uuid}` - Full task updates (except cancellation)
- `PUT /v1/tasks/{uuid}/workflow_steps/{step_uuid}` - Full step updates
- Direct step enqueueing endpoints (handled internally by orchestration)

## Technical Architecture

### Core Components

```rust
// src/web/mod.rs
pub mod routes;
pub mod handlers;
pub mod middleware;
pub mod extractors;
pub mod responses;
pub mod errors;

// Main app factory
pub fn create_app(config: WebConfig, pool: PgPool) -> Router {
    Router::new()
        .nest("/v1", api_v1_routes())  // Versioned API routes
        .merge(health_routes())        // Health/metrics at root level
        .layer(middleware::stack())
        .with_state(AppState { config, pool })
}
```

### Key Design Decisions

#### 1. Task Creation API (NEW - Required for TAS-40)
The `POST /v1/tasks` endpoint is critical for worker system integration. It accepts a TaskRequest and returns task metadata:
```rust
#[derive(Serialize, Deserialize)]
pub struct TaskCreationRequest {
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub context: serde_json::Value,
    pub initiator: String,
    pub source_system: String,
    pub reason: String,
    pub priority: Option<i32>,
    pub claim_timeout_seconds: Option<i32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct TaskCreationResponse {
    pub task_uuid: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
}
```

#### 2. UUID-based Identifiers
Changed from integer IDs to UUID strings in URLs for better compatibility with distributed systems and future scaling.

#### 3. Step Result Alignment
Align step results with Ruby's `StepHandlerCallResult`:
```rust
#[derive(Serialize, Deserialize)]
pub struct StepResult {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error_type: Option<String>,
    pub message: Option<String>,
    pub error_code: Option<String>,
    pub retryable: bool,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

#### 4. Authentication Strategy
- **JWT with Shared Keys**: Public/private key pair shared between orchestration server and workers
- **Environment Variables**: Store keys via `TASKER_JWT_PUBLIC_KEY` and `TASKER_JWT_PRIVATE_KEY` for now
- **Future Secrets Management**: Designed for easy migration to secrets manager with key rotation
- Health/metrics endpoints: No auth required (Kubernetes standard)
- Read endpoints: Optional auth (configurable)
- Task creation endpoint: Required auth (configurable for worker systems)
- Mutation endpoints: Required auth

```rust
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub private_key: String,  // Environment variable: TASKER_JWT_PRIVATE_KEY
    pub public_key: String,   // Environment variable: TASKER_JWT_PUBLIC_KEY
    pub token_expiry_hours: u64,
    pub issuer: String,       // "tasker-orchestration"
    pub audience: String,     // "tasker-workers"
}

// JWT Claims for worker authentication
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerClaims {
    pub sub: String,          // Worker identifier
    pub worker_namespaces: Vec<String>, // Authorized namespaces
    pub iss: String,          // Issuer
    pub aud: String,          // Audience
    pub exp: i64,             // Expiration
    pub iat: i64,             // Issued at
}
```

#### 5. Database Connection & Resource Management
- **Dedicated Web API Pool**: Separate connection pool for web operations to prevent resource contention
- **Shared Read Access**: Reference to orchestration pool for read-heavy operations when appropriate
- **Pool Sizing Strategy**: Web API pool sized based on expected concurrent HTTP connections
- **Configuration Integration**: Pool sizes configured via existing component-based TOML system
- Custom extractor for database connections
- Read replicas for analytics queries (future enhancement)
- Reuse existing models from orchestration system

```rust
#[derive(Debug, Clone)]
pub struct DatabasePoolConfig {
    pub web_api_pool_size: u32,           // Dedicated for HTTP operations
    pub web_api_max_connections: u32,     // Connection limit for web pool
    pub orchestration_shared_pool: PgPool, // Reference to shared pool for reads
    pub connection_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
}

// AppState uses dedicated pool strategy
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<WebServerConfig>,
    pub web_db_pool: PgPool,                    // Dedicated for web operations
    pub orchestration_db_pool: PgPool,          // Shared reference for read operations
    pub pgmq_client: UnifiedPgmqClient,         // Shared with orchestration
    pub task_initializer: Arc<TaskInitializer>, // Shared component
    pub orchestration_status: Arc<RwLock<OrchestrationStatus>>,
}
```

#### 6. Error Handling & Resilience
Leverage existing circuit breaker patterns and Axum/Tokio native capabilities for resilience:

```rust
#[derive(thiserror::Error)]
pub enum ApiError {
    #[error("Not found")]
    NotFound,
    #[error("Forbidden")]
    Forbidden,
    #[error("Invalid request: {0}")]
    BadRequest(String),
    #[error("Service temporarily unavailable")]
    ServiceUnavailable,
    #[error("Request timeout")]
    Timeout,
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
    #[error("Internal error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Resource not found"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "Access denied"),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.as_str()),
            ApiError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Service temporarily unavailable"),
            ApiError::Timeout => (StatusCode::REQUEST_TIMEOUT, "Request timeout"),
            ApiError::CircuitBreakerOpen => (StatusCode::SERVICE_UNAVAILABLE, "Service temporarily unavailable"),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
        };
        
        (status, Json(json!({"error": message}))).into_response()
    }
}

// Request timeout middleware using Axum tower integration
pub fn timeout_layer() -> TimeoutLayer {
    TimeoutLayer::new(Duration::from_secs(30)) // 30s default timeout
}

// Web-specific circuit breaker for database operations
pub async fn database_circuit_breaker_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Check if database operations are healthy for web API
    // This would be based on recent database operation failures, not PGMQ
    if state.web_db_pool_health.is_circuit_open() {
        return Err(ApiError::ServiceUnavailable);
    }
    
    Ok(next.run(request).await)
}

// Web API specific circuit breaker for database health
#[derive(Debug, Clone)]
pub struct WebDatabaseCircuitBreaker {
    failure_threshold: u32,
    recovery_timeout: Duration,
    current_failures: Arc<AtomicU32>,
    last_failure_time: Arc<AtomicU64>,
    state: Arc<AtomicU8>, // 0 = Closed, 1 = Open, 2 = HalfOpen
}

impl WebDatabaseCircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            current_failures: Arc::new(AtomicU32::new(0)),
            last_failure_time: Arc::new(AtomicU64::new(0)),
            state: Arc::new(AtomicU8::new(0)), // Start closed
        }
    }

    pub fn is_circuit_open(&self) -> bool {
        match self.state.load(Ordering::Relaxed) {
            1 => {
                // Check if recovery timeout has passed
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);
                
                if now - last_failure > self.recovery_timeout.as_secs() {
                    // Move to half-open state
                    self.state.store(2, Ordering::Relaxed);
                    false
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    pub fn record_success(&self) {
        self.current_failures.store(0, Ordering::Relaxed);
        self.state.store(0, Ordering::Relaxed); // Closed
    }

    pub fn record_failure(&self) {
        let failures = self.current_failures.fetch_add(1, Ordering::Relaxed) + 1;
        
        if failures >= self.failure_threshold {
            self.state.store(1, Ordering::Relaxed); // Open
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            self.last_failure_time.store(now, Ordering::Relaxed);
        }
    }
}
```

## Implementation Phases

Implementation will proceed organically as a personal project, but phases provide logical organization and ensure comprehensive testing at each stage.

### Phase 1: Foundation & Infrastructure
1. **Project Setup**
   - Add Axum dependencies
   - Create web module structure with proper separation
   - Setup basic routing framework

2. **Core Middleware & Resilience**
   - Request ID generation and tracing integration
   - Request/response logging
   - Error handling with circuit breaker integration
   - CORS configuration
   - JWT authentication middleware
   - Request timeout middleware using Axum's native tower integration

3. **Database Integration & Resource Management**
   - Dedicated SQLx connection pool setup for web operations
   - Custom extractors for database connections
   - Integration with existing orchestration database pools
   - Resource contention prevention strategies

4. **Integration Testing**
   - Foundation component testing
   - Database pool resource contention testing
   - Middleware integration validation

### Phase 2: Health & Core Infrastructure
1. **Health Endpoints** (Kubernetes-ready)
   - Implement `/health`, `/ready`, `/live` checks following K8s standards
   - Database connectivity checks using dedicated pool
   - Queue health status via existing PGMQ client
   - Orchestration system operational state integration

2. **Basic Metrics Integration**
   - Setup lightweight metrics at `/metrics` endpoint
   - HTTP request metrics using existing prometheus patterns
   - Integration with existing orchestration metrics registry
   - Avoid over-engineering - foundation for future comprehensive observability

3. **Integration Testing**
   - Health endpoint validation under various system states
   - Metrics collection verification
   - Kubernetes probe compatibility testing

### Phase 3: Task Creation API - CRITICAL for TAS-40
1. **Core Task Creation**
   - `POST /v1/tasks` endpoint implementation
   - Integration with existing TaskInitializer
   - UUID-based response format
   - Comprehensive error handling and validation
   - JWT authentication for worker systems

2. **Task Status API**
   - `GET /v1/tasks/{uuid}` for worker correlation
   - Task status tracking
   - Dependency graph serialization

3. **Integration Testing**
   - End-to-end task creation workflow testing
   - Worker authentication simulation
   - Task correlation and status tracking validation
   - Performance testing under concurrent load

### Phase 4: Read-Only Operations
1. **Task List Endpoints**
   - List with pagination/filtering
   - Get by UUID with complete relationships

2. **Step Endpoints**
   - List steps by task UUID
   - Get step details by UUID
   - Result formatting per Ruby StepHandlerCallResult spec

3. **Handler Endpoints**
   - Namespace listing
   - Handler enumeration
   - Basic dependency visualization

4. **Integration Testing**
   - Read operation performance validation
   - Data consistency verification
   - Pagination and filtering correctness

### Phase 5: Controlled Mutations
1. **Task Cancellation**
   - Validate task state and cancellation eligibility
   - Update database using existing patterns
   - Trigger orchestration events

2. **Manual Step Resolution**
   - Validate step state and resolution eligibility
   - Update results per Ruby StepHandlerCallResult format
   - State transition to `resolved_manually`

3. **Integration Testing**
   - Mutation operation validation
   - State consistency verification
   - Event triggering confirmation

### Phase 6: Analytics & Production Readiness
1. **Analytics Endpoints**
   - Performance aggregation using existing database functions
   - Basic bottleneck detection
   - Query optimization

2. **Production Hardening**
   - Rate limiting implementation
   - Security validation
   - Load testing and performance optimization
   - Docker and Kubernetes deployment validation

3. **Comprehensive Testing**
   - Full API contract testing
   - Security penetration testing
   - Production deployment simulation

## Dependencies to Add

```toml
[dependencies]
# Web framework and middleware
axum = "0.7"
axum-extra = { version = "0.9", features = ["typed-header"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace", "timeout"] }

# Database and serialization (extend existing)
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "chrono", "uuid"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Authentication and security
jsonwebtoken = "9"
rsa = "0.9"  # For RSA key pair management

# Observability (lightweight integration)
prometheus = "0.13"  # Already included
metrics = "0.21"     # For additional web metrics

# Error handling
thiserror = "1.0"    # Already included

# Additional utilities
uuid = { version = "1.0", features = ["v4", "serde"] }  # Already included
```

## Security Considerations

### 1. Authentication Strategy (Concrete Implementation)
- **JWT with RSA Keys**: Public/private key pairs for secure worker authentication
- **Environment-based Key Management**: Keys stored in environment variables for development
- **Namespace-based Authorization**: Workers authorized for specific namespaces only
- **Configurable Requirements**: Authentication can be disabled for internal development

```rust
// Concrete JWT implementation
#[derive(Debug, Clone)]
pub struct JwtAuthenticator {
    private_key: RsaPrivateKey,
    public_key: RsaPublicKey,
    config: JwtConfig,
}

impl JwtAuthenticator {
    pub fn from_env() -> Result<Self, AuthError> {
        let private_key_pem = env::var("TASKER_JWT_PRIVATE_KEY")?;
        let public_key_pem = env::var("TASKER_JWT_PUBLIC_KEY")?;
        // Key parsing and validation
    }

    pub fn validate_worker_token(&self, token: &str) -> Result<WorkerClaims, AuthError> {
        // JWT validation with namespace authorization
    }
}
```

### 2. Input Validation & Security
- **UUID Validation**: Strict UUID parsing with proper error handling
- **Payload Size Limits**: Configurable request size limits via middleware
- **SQL Injection Prevention**: All database operations use parameterized queries
- **Request Sanitization**: Input validation for all user-provided data

### 3. Rate Limiting & Circuit Breakers
- **Per-client Rate Limiting**: Individual client request limits
- **Endpoint-specific Throttling**: Different limits for read vs write operations
- **Circuit Breaker Integration**: Leverage existing orchestration circuit breaker patterns
- **Backpressure Handling**: Graceful degradation when orchestration system is overloaded

## Testing Strategy

### 1. Unit Tests
- Handler logic isolation
- Serialization/deserialization
- Error handling paths

### 2. Integration Tests
```rust
#[tokio::test]
async fn test_task_creation() {
    let app = test_app();
    let task_request = TaskCreationRequest {
        name: "test_task".to_string(),
        namespace: "test".to_string(),
        version: "1.0.0".to_string(),
        context: json!({"test": true}),
        initiator: "test_worker".to_string(),
        source_system: "test".to_string(),
        reason: "Integration test".to_string(),
        priority: None,
        claim_timeout_seconds: None,
        tags: None,
    };

    let response = app.oneshot(
        Request::post("/v1/tasks")
            .header("content-type", "application/json")
            .body(serde_json::to_string(&task_request).unwrap())
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_task_cancellation() {
    let app = test_app();
    let response = app.oneshot(
        Request::delete("/v1/tasks/550e8400-e29b-41d4-a716-446655440000")
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

### 3. Contract Tests
- OpenAPI spec validation
- Response schema verification
- Breaking change detection

## Monitoring & Observability

### 1. Metrics
- Request latency histograms
- Error rate counters
- Business metric gauges

### 2. Logging
- Structured JSON logs
- Request/response tracing
- Error context capture

### 3. Tracing
- OpenTelemetry integration
- Distributed trace correlation
- Performance profiling

## Success Criteria

### Core Functionality
- ✅ Task creation endpoint (`POST /v1/tasks`) fully functional for worker integration with JWT authentication
- ✅ All read endpoints return data matching database state with UUID identifiers
- ✅ Health endpoints (`/health`, `/ready`, `/live`) follow Kubernetes standards
- ✅ Prometheus metrics exposed at `/metrics` and queryable
- ✅ Task cancellation properly updates state and triggers events
- ✅ Manual step resolution follows Ruby StepHandlerCallResult format

### Security & Authentication
- ✅ JWT authentication working with RSA public/private key pairs
- ✅ Namespace-based authorization for worker systems
- ✅ Input validation prevents malicious payloads
- ✅ Zero unauthorized mutations possible

### Performance & Resilience
- ✅ API responses are fast (<100ms p99 for reads, <500ms p99 for task creation)
- ✅ Dedicated database pools prevent resource contention with orchestration
- ✅ Circuit breaker integration provides graceful degradation
- ✅ Request timeout middleware prevents hanging requests

### Production Readiness
- ✅ Operational state integration with graceful shutdown support
- ✅ Rate limiting protects against abuse
- ✅ Comprehensive test coverage (>80%) with integration testing per phase
- ✅ Worker systems can successfully create and track tasks via HTTP API
- ✅ Configuration integration with existing component-based TOML system

## Implementation Notes

### Why These Choices

1. **Task Creation Priority**: Per TAS-40, worker systems must be able to POST to `/v1/tasks` to create tasks and receive UUIDs for correlation. This is fundamental to the distributed architecture.

2. **UUID-based URLs**: Future-proof for distributed systems and horizontal scaling. Avoids integer ID conflicts.

3. **Health Endpoints at Root**: Follows Kubernetes and industry standards for `/health`, `/ready`, `/live`, and `/metrics` endpoints.

4. **Version Prefix**: `/v1` allows for future API evolution without breaking existing integrations.

5. **Controlled Mutations**:
   - Task creation enables worker integration (TAS-40 requirement)
   - Task cancellation is necessary for operational control
   - Manual step resolution allows human intervention when automation fails

6. **StepHandlerCallResult Alignment**: Ensures consistency between Ruby workers and API responses

7. **Optional Authentication**: Allows flexible deployment (internal vs external facing)

### Example Implementations

#### Task Creation Handler (NEW - Required for TAS-40)
```rust
async fn create_task(
    State(pool): State<PgPool>,
    _auth: RequireAuth,  // Configurable auth requirement
    Json(request): Json<TaskCreationRequest>,
) -> Result<Json<TaskCreationResponse>, ApiError> {
    // Validate request
    if request.name.is_empty() || request.namespace.is_empty() {
        return Err(ApiError::BadRequest("Name and namespace are required".into()));
    }

    // Create TaskRequest for orchestration system
    let task_request = TaskRequest::new(request.name, request.namespace)
        .with_version(request.version)
        .with_context(request.context)
        .with_initiator(request.initiator)
        .with_source_system(request.source_system)
        .with_reason(request.reason);

    // Use existing TaskInitializer
    let initializer = TaskInitializer::for_production(pool.clone());
    let result = initializer
        .create_task_from_request(task_request)
        .await
        .map_err(|_| ApiError::Internal)?;

    // Return UUID and metadata for worker correlation
    let response = TaskCreationResponse {
        task_uuid: result.task_uuid.to_string(),
        status: "created".to_string(),
        created_at: chrono::Utc::now(),
        estimated_completion: None, // Could be calculated from workflow complexity
    };

    Ok(Json(response))
}
```

#### Health Check Handler
```rust
async fn health_ready(State(pool): State<PgPool>) -> Result<Json<HealthStatus>, ApiError> {
    // Check database connectivity
    let db_healthy = sqlx::query("SELECT 1")
        .fetch_one(&pool)
        .await
        .is_ok();

    // Check queue connectivity (if applicable)
    let queue_healthy = check_queue_health().await;

    let status = HealthStatus {
        status: if db_healthy && queue_healthy { "ready" } else { "not_ready" },
        checks: vec![
            HealthCheck { name: "database", status: db_healthy },
            HealthCheck { name: "queue", status: queue_healthy },
        ],
    };

    Ok(Json(status))
}
```

#### Task Cancellation Handler
```rust
async fn cancel_task(
    Path(task_uuid): Path<String>,
    State(pool): State<PgPool>,
    _auth: RequireAuth,  // Ensures authentication
) -> Result<Json<TaskResponse>, ApiError> {
    // Parse and validate UUID
    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::BadRequest("Invalid task UUID".into()))?;

    // Validate task exists and is cancellable
    let task = get_task_by_uuid(&pool, task_uuid).await?;

    if !task.is_cancellable() {
        return Err(ApiError::BadRequest("Task cannot be cancelled".into()));
    }

    // Update task status
    update_task_status(&pool, task_uuid, TaskStatus::Cancelled).await?;

    // Trigger orchestration events
    publish_cancellation_event(task_uuid).await?;

    Ok(Json(TaskResponse::from(task)))
}
```

#### Manual Step Resolution Handler
```rust
async fn resolve_step_manually(
    Path((task_uuid, step_uuid)): Path<(String, String)>,
    State(pool): State<PgPool>,
    Json(payload): Json<ManualResolutionRequest>,
    _auth: RequireAuth,
) -> Result<Json<StepResponse>, ApiError> {
    // Parse and validate UUIDs
    let task_uuid = Uuid::parse_str(&task_uuid)
        .map_err(|_| ApiError::BadRequest("Invalid task UUID".into()))?;
    let step_uuid = Uuid::parse_str(&step_uuid)
        .map_err(|_| ApiError::BadRequest("Invalid step UUID".into()))?;

    // Validate step can be manually resolved
    let step = get_step_by_uuid(&pool, task_uuid, step_uuid).await?;

    if step.status != StepStatus::Failed && step.status != StepStatus::Pending {
        return Err(ApiError::BadRequest("Step cannot be manually resolved".into()));
    }

    // Format results per Ruby StepHandlerCallResult
    let results = StepResult {
        success: true,
        result: Some(payload.resolution_data),
        error_type: None,
        message: Some(format!("Manually resolved by {}", payload.resolved_by)),
        error_code: None,
        retryable: false,
        metadata: hashmap! {
            "resolved_at" => json!(Utc::now()),
            "resolution_reason" => json!(payload.reason),
        },
    };

    // Update step in database
    update_step_results(&pool, step_uuid, results).await?;
    update_step_status(&pool, step_uuid, StepStatus::ResolvedManually).await?;

    Ok(Json(StepResponse::from(step)))
}
```

## Next Steps

1. **Priority 1**: Create web module structure and health endpoints (Kubernetes readiness)
2. **Priority 2**: Implement `POST /v1/tasks` endpoint (CRITICAL for TAS-40 worker integration)
3. **Priority 3**: Add `GET /v1/tasks/{uuid}` endpoint (worker correlation)
4. Add SQLx and database extractors with UUID support
5. Build out remaining read-only endpoints incrementally
6. Implement controlled mutations (cancellation and manual resolution)
7. Add comprehensive testing with UUID-based examples
8. Document API with OpenAPI/Swagger spec

## Integration with Existing Orchestration System

### Boot Process Integration with OrchestrationBootstrap

The Axum web server will be integrated into our existing `OrchestrationBootstrap` system to ensure unified component lifecycle management. This follows the established pattern in `src/orchestration/bootstrap.rs`:

```rust
// Enhanced BootstrapConfig to include web server configuration
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Namespaces to initialize queues for
    pub namespaces: Vec<String>,
    /// Whether to start processors immediately (vs manual control)
    pub auto_start_processors: bool,
    /// Environment override (None = auto-detect)
    pub environment_override: Option<String>,
    /// Web server configuration (NEW)
    pub web_server_config: Option<WebServerConfig>,
}

// NEW: Web server configuration integrated with existing config system
#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub cors_enabled: bool,
    pub auth_enabled: bool,
    pub rate_limiting_enabled: bool,
}

impl WebServerConfig {
    /// Create from existing ConfigManager (leverages TOML configuration)
    pub fn from_config_manager(config_manager: &ConfigManager) -> Option<Self> {
        let config = config_manager.config();
        
        // Check if web server is enabled in configuration
        if !config.web_server.enabled {
            return None;
        }
        
        Some(Self {
            enabled: config.web_server.enabled,
            bind_address: config.web_server.bind_address.clone(),
            cors_enabled: config.web_server.cors.enabled,
            auth_enabled: config.web_server.auth.enabled,
            rate_limiting_enabled: config.web_server.rate_limiting.enabled,
        })
    }
}

// Enhanced OrchestrationBootstrap to include web server
impl OrchestrationBootstrap {
    /// Enhanced bootstrap method with web server integration
    pub async fn bootstrap(config: BootstrapConfig) -> Result<OrchestrationSystemHandle> {
        info!("🚀 BOOTSTRAP: Starting unified orchestration system bootstrap with web server support");

        // Existing bootstrap logic for orchestration core...
        let orchestration_core = Arc::new(OrchestrationCore::from_config(config_manager.clone()).await?);
        
        // NEW: Start web server if configured
        let web_server_handle = if let Some(web_config) = config.web_server_config {
            Some(Self::start_web_server(web_config, orchestration_core.clone()).await?)
        } else {
            None
        };

        // Enhanced handle to include web server
        let handle = OrchestrationSystemHandle::new_with_web_server(
            orchestration_core,
            shutdown_sender,
            runtime_handle,
            config_manager,
            web_server_handle,
        );

        info!("🎉 BOOTSTRAP: Unified orchestration system with web server bootstrap completed");
        Ok(handle)
    }

    /// Start Axum web server integrated with orchestration system
    async fn start_web_server(
        web_config: WebServerConfig,
        orchestration_core: Arc<OrchestrationCore>,
    ) -> Result<WebServerHandle> {
        info!("🌐 BOOTSTRAP: Starting Axum web server on {}", web_config.bind_address);

        // Create shared app state using orchestration resources
        let app_state = AppState {
            config: Arc::new(web_config.clone()),
            db_pool: orchestration_core.database_pool().clone(),
            pgmq_client: orchestration_core.pgmq_client().clone(),
            metrics_registry: orchestration_core.metrics_registry().clone(),
            task_initializer: orchestration_core.task_initializer().clone(),
            orchestration_status: orchestration_core.status().clone(),
        };

        // Create Axum app with shared state
        let app = crate::web::create_app(app_state);
        
        // Bind to configured address
        let listener = tokio::net::TcpListener::bind(&web_config.bind_address)
            .await
            .map_err(|e| TaskerError::ConfigurationError(
                format!("Failed to bind web server to {}: {}", web_config.bind_address, e)
            ))?;
        
        // Start server with graceful shutdown integration
        let server_future = axum::serve(listener, app);
        let server_handle = tokio::spawn(server_future);
        
        info!("✅ BOOTSTRAP: Axum web server started successfully on {}", web_config.bind_address);
        
        Ok(WebServerHandle {
            server_handle,
            bind_address: web_config.bind_address,
        })
    }

    /// Convenience method for starting orchestration with web server
    pub async fn bootstrap_with_web_server(
        namespaces: Vec<String>,
        environment: Option<String>,
    ) -> Result<OrchestrationSystemHandle> {
        // Load configuration to determine if web server should be enabled
        let config_manager = ConfigManager::load()?;
        let web_server_config = WebServerConfig::from_config_manager(&config_manager);
        
        let config = BootstrapConfig {
            namespaces,
            auto_start_processors: true,
            environment_override: environment,
            web_server_config,
        };

        Self::bootstrap(config).await
    }
}

// Enhanced system handle to manage both orchestration and web server
pub struct OrchestrationSystemHandle {
    pub orchestration_core: Arc<OrchestrationCore>,
    pub shutdown_sender: Option<oneshot::Sender<()>>,
    pub runtime_handle: tokio::runtime::Handle,
    pub config_manager: Arc<ConfigManager>,
    pub web_server_handle: Option<WebServerHandle>, // NEW
}

impl OrchestrationSystemHandle {
    pub fn new_with_web_server(
        orchestration_core: Arc<OrchestrationCore>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
        config_manager: Arc<ConfigManager>,
        web_server_handle: Option<WebServerHandle>,
    ) -> Self {
        Self {
            orchestration_core,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
            config_manager,
            web_server_handle,
        }
    }

    /// Enhanced stop method that gracefully shuts down both orchestration and web server
    pub async fn stop(&mut self) -> Result<()> {
        info!("🛑 SYSTEM: Stopping orchestration system and web server");

        // Stop web server first
        if let Some(web_handle) = &self.web_server_handle {
            if let Err(e) = web_handle.stop().await {
                error!("❌ Failed to stop web server gracefully: {}", e);
            } else {
                info!("✅ Web server stopped successfully");
            }
        }

        // Stop orchestration system
        if let Some(sender) = self.shutdown_sender.take() {
            sender.send(()).map_err(|_| {
                TaskerError::OrchestrationError("Failed to send shutdown signal".to_string())
            })?;
            info!("✅ Orchestration system shutdown requested");
        }

        Ok(())
    }
}

// Web server handle for lifecycle management
pub struct WebServerHandle {
    server_handle: tokio::task::JoinHandle<()>,
    bind_address: String,
}

impl WebServerHandle {
    pub async fn stop(&self) -> Result<()> {
        self.server_handle.abort();
        info!("🌐 Web server on {} stopped", self.bind_address);
        Ok(())
    }
}
```

### Configuration Integration

The web server will use our existing component-based TOML configuration system with enhanced authentication and database pool management:

```toml
# config/tasker/base/web_server.toml
[web_server]
enabled = true
bind_address = "0.0.0.0:8080"
request_timeout_ms = 30000
max_request_size_mb = 10

[web_server.database_pools]
# Dedicated pool for web API operations to prevent resource contention
web_api_pool_size = 10
web_api_max_connections = 15
web_api_connection_timeout_seconds = 30
web_api_idle_timeout_seconds = 600

[web_server.cors]
enabled = true
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "DELETE", "PATCH", "OPTIONS"]
allowed_headers = ["Content-Type", "Authorization", "X-Request-ID"]

[web_server.auth]
enabled = false  # Optional for internal deployments
jwt_private_key = "${TASKER_JWT_PRIVATE_KEY}"
jwt_public_key = "${TASKER_JWT_PUBLIC_KEY}"
jwt_token_expiry_hours = 24
jwt_issuer = "tasker-orchestration"
jwt_audience = "tasker-workers"
api_key_header = "X-API-Key"

[web_server.rate_limiting]
enabled = true
requests_per_minute = 1000
burst_size = 100
per_client_limit = true

[web_server.resilience]
# Leverage existing circuit breaker patterns
circuit_breaker_enabled = true
request_timeout_seconds = 30
max_concurrent_requests = 1000

# Environment-specific overrides
# config/tasker/environments/production/web_server.toml
[web_server]
bind_address = "0.0.0.0:8080"

[web_server.database_pools]
# Production sizing for concurrent load
web_api_pool_size = 25
web_api_max_connections = 30

[web_server.auth]
enabled = true
jwt_private_key = "${TASKER_JWT_PRIVATE_KEY}"
jwt_public_key = "${TASKER_JWT_PUBLIC_KEY}"

[web_server.rate_limiting]
# Higher limits for production worker traffic
requests_per_minute = 10000
burst_size = 1000

# Development environment for easier testing
# config/tasker/environments/development/web_server.toml
[web_server.auth]
enabled = false  # No auth required for development

[web_server.database_pools]
# Smaller pools for development
web_api_pool_size = 5
web_api_max_connections = 8
```

### Shared Resource Management & Resource Contention Prevention

The web server will reuse orchestration system components while maintaining dedicated resources for critical operations:

```rust
// src/web/state.rs
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<WebServerConfig>,
    pub web_db_pool: PgPool,                        // Dedicated for web operations
    pub orchestration_db_pool: PgPool,              // Shared reference for reads
    pub web_db_pool_health: WebDatabaseCircuitBreaker, // Circuit breaker for web DB operations
    pub metrics_registry: Arc<Registry>,            // Shared with orchestration
    pub task_initializer: Arc<TaskInitializer>,     // Shared component
    pub orchestration_status: Arc<RwLock<OrchestrationStatus>>,
}

impl AppState {
    pub async fn from_orchestration_core(
        web_config: WebServerConfig,
        orchestration_core: &OrchestrationCore,
    ) -> Result<Self, TaskerError> {
        // Create dedicated database pool for web operations
        let database_url = orchestration_core.config_manager().database_url();
        let web_db_pool = PgPoolOptions::new()
            .max_connections(web_config.database_pools.web_api_max_connections)
            .min_connections(web_config.database_pools.web_api_pool_size / 2)
            .connect_timeout(Duration::from_secs(web_config.database_pools.web_api_connection_timeout_seconds))
            .idle_timeout(Duration::from_secs(web_config.database_pools.web_api_idle_timeout_seconds))
            .connect(&database_url)
            .await?;

        Ok(Self {
            config: Arc::new(web_config),
            web_db_pool,
            orchestration_db_pool: orchestration_core.database_pool().clone(),
            pgmq_client: orchestration_core.pgmq_client().clone(),
            metrics_registry: orchestration_core.metrics_registry().clone(),
            task_initializer: orchestration_core.task_initializer().clone(),
            orchestration_status: orchestration_core.status().clone(),
        })
    }

    // Smart pool selection based on operation type
    pub fn select_db_pool(&self, operation_type: DbOperationType) -> &PgPool {
        match operation_type {
            DbOperationType::WebWrite | DbOperationType::WebCritical => &self.web_db_pool,
            DbOperationType::ReadOnly | DbOperationType::Analytics => &self.orchestration_db_pool,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DbOperationType {
    WebWrite,       // Task creation, cancellation, etc.
    WebCritical,    // High-priority web operations
    ReadOnly,       // Task status, listing, etc.
    Analytics,      // Performance queries, metrics
}
```

### Operational State Coordination

The web server will integrate with existing operational state management using simple, effective patterns:

```rust
// src/web/middleware/operational_state.rs
pub async fn operational_state_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let orchestration_status = state.orchestration_status.read().await;
    let path = request.uri().path();
    
    // Simple operational state handling - lean on existing patterns
    match orchestration_status.operational_state {
        SystemOperationalState::Normal => {
            // All endpoints available
            Ok(next.run(request).await)
        }
        SystemOperationalState::GracefulShutdown => {
            // Health and metrics remain available during graceful shutdown
            if path.starts_with("/health") || path == "/metrics" {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
        SystemOperationalState::Emergency | SystemOperationalState::Stopped => {
            // Only basic health check available
            if path == "/health" {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
        _ => Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

// Simple graceful shutdown integration leveraging Axum/Tokio patterns
pub async fn graceful_shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
```

## Docker Integration

### Orchestration System Dockerfile

```dockerfile
# Dockerfile.orchestration
# Multi-stage build for optimized production image

FROM rust:1.75-bullseye as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY config ./config
COPY migrations ./migrations

# Build for release
ENV SQLX_OFFLINE=true
RUN cargo build --release --all-features

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -u 1000 tasker

# Create app directory
WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/tasker-orchestration /usr/local/bin/
COPY --from=builder /app/config ./config

# Set permissions
RUN chown -R tasker:tasker /app
USER tasker

# Configuration via environment variables
ENV TASKER_ENV=production
ENV RUST_LOG=info
ENV TASKER_DATABASE_URL=""
ENV TASKER_WEB_SERVER_ENABLED=true
ENV TASKER_WEB_SERVER_BIND_ADDRESS="0.0.0.0:8080"

# Health check using our health endpoint
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose web server port
EXPOSE 8080

# Start orchestration system with integrated web server
CMD ["tasker-orchestration", "--namespaces", "all"]
```

### Docker Compose for Development

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: tasker_development
      POSTGRES_USER: tasker
      POSTGRES_PASSWORD: tasker
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U tasker"]
      interval: 10s
      timeout: 5s
      retries: 5

  tasker-orchestration:
    build:
      context: .
      dockerfile: Dockerfile.orchestration
    environment:
      TASKER_ENV: development
      RUST_LOG: debug,tasker_core=trace
      TASKER_DATABASE_URL: postgresql://tasker:tasker@postgres:5432/tasker_development
      TASKER_WEB_SERVER_ENABLED: true
      TASKER_WEB_SERVER_BIND_ADDRESS: "0.0.0.0:8080"
    ports:
      - "8080:8080"  # Web API
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    volumes:
      - ./config:/app/config:ro
      
  # Future worker service (TAS-40)
  # tasker-worker:
  #   build:
  #     context: .
  #     dockerfile: Dockerfile.worker
  #   environment:
  #     TASKER_ORCHESTRATION_URL: http://tasker-orchestration:8080
  #     TASKER_WORKER_NAMESPACES: "payments,inventory"
  #   depends_on:
  #     tasker-orchestration:
  #       condition: service_healthy

volumes:
  postgres_data:
```

### Production Kubernetes Deployment

```yaml
# k8s/orchestration-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tasker-orchestration
  labels:
    app: tasker-orchestration
spec:
  replicas: 3
  selector:
    matchLabels:
      app: tasker-orchestration
  template:
    metadata:
      labels:
        app: tasker-orchestration
    spec:
      containers:
      - name: tasker-orchestration
        image: tasker/orchestration:latest
        ports:
        - containerPort: 8080
          name: http
        env:
        - name: TASKER_ENV
          value: "production"
        - name: RUST_LOG
          value: "info"
        - name: TASKER_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tasker-secrets
              key: database-url
        - name: TASKER_WEB_SERVER_ENABLED
          value: "true"
        - name: TASKER_WEB_SERVER_BIND_ADDRESS
          value: "0.0.0.0:8080"
        livenessProbe:
          httpGet:
            path: /live
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
---
apiVersion: v1
kind: Service
metadata:
  name: tasker-orchestration-service
spec:
  selector:
    app: tasker-orchestration
  ports:
  - port: 80
    targetPort: 8080
    name: http
  type: ClusterIP
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: tasker-orchestration-ingress
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
spec:
  rules:
  - host: tasker-api.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: tasker-orchestration-service
            port:
              number: 80
```

## TAS-40 Integration Points

The following endpoints are **REQUIRED** for TAS-40 worker system integration:
- `POST /v1/tasks` - Workers must be able to create tasks
- `GET /v1/tasks/{uuid}` - Workers need task status for correlation
- `GET /health` - Workers need to validate orchestration system health
- `GET /metrics` - Monitoring and observability

### Configuration for Worker Communication

```toml
# config/tasker/environments/production/web_server.toml
[web_server]
# Bind to all interfaces for container networking
bind_address = "0.0.0.0:8080"

[web_server.auth]
# Enable auth for production worker communication
enabled = true
jwt_secret = "${TASKER_JWT_SECRET}"

[web_server.rate_limiting]
# Higher limits for worker traffic
requests_per_minute = 10000
burst_size = 1000
```

### Main Binary Implementation

The main binary (`src/bin/tasker-orchestration.rs`) would use the integrated bootstrap system:

```rust
// src/bin/tasker-orchestration.rs
use clap::{Arg, Command};
use tasker_core::orchestration::bootstrap::OrchestrationBootstrap;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let matches = Command::new("tasker-orchestration")
        .version("1.0.0")
        .about("Tasker Core Orchestration System with Web API")
        .arg(
            Arg::new("namespaces")
                .long("namespaces")
                .value_name("NAMESPACES")
                .help("Comma-separated list of namespaces to initialize")
                .default_value("all")
        )
        .arg(
            Arg::new("environment")
                .long("environment")
                .short('e')
                .value_name("ENV")
                .help("Environment override (development, staging, production)")
        )
        .get_matches();

    let namespaces_str = matches.get_one::<String>("namespaces").unwrap();
    let namespaces = if namespaces_str == "all" {
        vec![] // Empty means all available namespaces
    } else {
        namespaces_str.split(',').map(|s| s.trim().to_string()).collect()
    };

    let environment = matches.get_one::<String>("environment").cloned();

    info!("🚀 Starting Tasker Orchestration System with Web API");
    info!("   Namespaces: {:?}", if namespaces.is_empty() { vec!["all".to_string()] } else { namespaces.clone() });
    info!("   Environment: {:?}", environment.as_ref().unwrap_or(&"auto-detect".to_string()));

    // Use integrated bootstrap that includes web server
    let mut handle = OrchestrationBootstrap::bootstrap_with_web_server(namespaces, environment).await?;

    info!("✅ System started successfully");
    info!("   Running: {}", handle.status().running);
    info!("   Environment: {}", handle.status().environment);
    if let Some(web_handle) = &handle.web_server_handle {
        info!("   Web API: Available on {}", web_handle.bind_address);
        info!("   Health Check: GET /health");
        info!("   Task Creation: POST /v1/tasks");
        info!("   Metrics: GET /metrics");
    }

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("🛑 Shutdown signal received");

    // Graceful shutdown
    handle.stop().await?;
    info!("✅ System stopped successfully");

    Ok(())
}
```

### Integration Benefits

This integration approach ensures the Axum web server leverages all existing orchestration infrastructure while providing the HTTP interface required for TAS-40's distributed worker architecture:

1. **Unified Lifecycle**: Both orchestration and web server start/stop together
2. **Shared Resources**: Database pools, PGMQ clients, metrics all shared
3. **Configuration Consistency**: Single TOML configuration system
4. **Operational State**: Web server respects orchestration operational state
5. **Graceful Shutdown**: Coordinated shutdown prevents data loss
