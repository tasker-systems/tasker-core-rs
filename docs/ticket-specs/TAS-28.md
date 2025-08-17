# TAS-28: Axum Web API Implementation Plan

## Executive Summary
Implement a **read-focused REST API** using Axum that provides visibility into the Tasker orchestration system. The API will be primarily read-only with only two carefully controlled mutations: task cancellation and manual step resolution.

## API Scope Analysis

### ✅ Endpoints to Implement

#### 1. Health & Monitoring (No Auth Required)
- `GET /tasker/health/ready` - Readiness check
- `GET /tasker/health/live` - Liveness check  
- `GET /tasker/health/status` - Detailed health (optional auth)
- `GET /tasker/metrics` - Prometheus metrics export

#### 2. Tasks (Read-Only + Cancel)
- `GET /tasker/tasks` - List tasks with pagination
- `GET /tasker/tasks/{task_id}` - Get task details with dependency graph
- `DELETE /tasker/tasks/{task_id}` - Cancel task (only allowed mutation)

#### 3. Workflow Steps (Read-Only + Manual Resolution)
- `GET /tasker/tasks/{task_id}/workflow_steps` - List steps for a task
- `GET /tasker/tasks/{task_id}/workflow_steps/{step_id}` - Get step details
- `PATCH /tasker/tasks/{task_id}/workflow_steps/{step_id}` - Update step for manual resolution only

#### 4. Handlers (Read-Only)
- `GET /tasker/handlers` - List registered namespaces
- `GET /tasker/handlers/{namespace}` - List handlers in namespace
- `GET /tasker/handlers/{namespace}/{name}` - Get handler details with dependency graph

#### 5. Analytics (Read-Only)
- `GET /tasker/analytics/performance` - System performance metrics
- `GET /tasker/analytics/bottlenecks` - Bottleneck analysis

### ❌ Endpoints NOT to Implement
- `POST /tasker/tasks` - Task creation (handled internally by orchestration)
- `PUT/PATCH /tasker/tasks/{task_id}` - Task updates (except cancellation)
- `PUT /tasker/tasks/{task_id}/workflow_steps/{step_id}` - Full step updates

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
        .nest("/tasker", api_routes())
        .layer(middleware::stack())
        .with_state(AppState { config, pool })
}
```

### Key Design Decisions

#### 1. Step Result Alignment
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

#### 2. Authentication Strategy
- Use optional JWT authentication via middleware
- Health endpoints: No auth required
- Read endpoints: Optional auth (configurable)
- Mutation endpoints: Required auth

#### 3. Database Connection
- Use SQLx with connection pooling
- Custom extractor for database connections
- Read replicas for analytics queries

#### 4. Error Handling
```rust
#[derive(thiserror::Error)]
pub enum ApiError {
    #[error("Not found")]
    NotFound,
    #[error("Forbidden")]
    Forbidden,
    #[error("Invalid request: {0}")]
    BadRequest(String),
    #[error("Internal error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Convert to proper HTTP responses
    }
}
```

## Implementation Phases

### Phase 1: Foundation (Week 1)
1. **Project Setup**
   - Add Axum dependencies
   - Create web module structure
   - Setup basic routing

2. **Core Middleware**
   - Request ID generation
   - Request/response logging
   - Error handling
   - CORS configuration

3. **Database Integration**
   - SQLx connection pool setup
   - Custom extractors
   - Query builders

### Phase 2: Health & Metrics (Week 1)
1. **Health Endpoints**
   - Implement readiness/liveness checks
   - Database connectivity checks
   - Queue health status

2. **Prometheus Metrics**
   - Setup metrics registry
   - HTTP request metrics
   - Custom business metrics

### Phase 3: Read-Only Endpoints (Week 2)
1. **Task Endpoints**
   - List with pagination/filtering
   - Get by ID with relationships
   - Dependency graph serialization

2. **Step Endpoints**
   - List steps by task
   - Get step details
   - Result formatting per Ruby spec

3. **Handler Endpoints**
   - Namespace listing
   - Handler enumeration
   - Dependency visualization

### Phase 4: Controlled Mutations (Week 2)
1. **Task Cancellation**
   - Validate task state
   - Update database
   - Trigger orchestration events

2. **Manual Step Resolution**
   - Validate step state
   - Update results per Ruby format
   - State transition to `resolved_manually`

### Phase 5: Analytics & Testing (Week 3)
1. **Analytics Endpoints**
   - Performance aggregation
   - Bottleneck detection
   - Query optimization

2. **Testing Suite**
   - Unit tests for handlers
   - Integration tests with test database
   - API contract tests

## Dependencies to Add

```toml
[dependencies]
axum = "0.7"
axum-extra = { version = "0.9", features = ["typed-header"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "chrono", "uuid"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
jsonwebtoken = "9"
prometheus = "0.13"
thiserror = "1.0"
```

## Security Considerations

### 1. Authentication
- Optional JWT support
- API key authentication fallback
- Configurable per endpoint

### 2. Authorization
- Role-based access control
- Resource-level permissions
- Audit logging

### 3. Rate Limiting
- Per-client limits
- Endpoint-specific throttling
- Circuit breaker integration

## Testing Strategy

### 1. Unit Tests
- Handler logic isolation
- Serialization/deserialization
- Error handling paths

### 2. Integration Tests
```rust
#[tokio::test]
async fn test_task_cancellation() {
    let app = test_app();
    let response = app.oneshot(
        Request::delete("/tasker/tasks/123")
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

- ✅ All read endpoints return data matching database state
- ✅ Task cancellation properly updates state and triggers events
- ✅ Manual step resolution follows Ruby StepHandlerCallResult format
- ✅ Prometheus metrics exposed and queryable
- ✅ Health checks accurately reflect system state
- ✅ API responses are fast (<100ms p99 for reads)
- ✅ Comprehensive test coverage (>80%)
- ✅ Zero unauthorized mutations possible

## Implementation Notes

### Why These Choices

1. **Read-Only Focus**: The orchestration system is designed to be internally driven. External mutations could break workflow integrity.

2. **Two Allowed Mutations**: 
   - Task cancellation is necessary for operational control
   - Manual step resolution allows human intervention when automation fails

3. **StepHandlerCallResult Alignment**: Ensures consistency between Ruby workers and API responses

4. **Optional Authentication**: Allows flexible deployment (internal vs external facing)

### Example Implementations

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
    Path(task_id): Path<i64>,
    State(pool): State<PgPool>,
    _auth: RequireAuth,  // Ensures authentication
) -> Result<Json<TaskResponse>, ApiError> {
    // Validate task exists and is cancellable
    let task = get_task_by_id(&pool, task_id).await?;
    
    if !task.is_cancellable() {
        return Err(ApiError::BadRequest("Task cannot be cancelled".into()));
    }
    
    // Update task status
    update_task_status(&pool, task_id, TaskStatus::Cancelled).await?;
    
    // Trigger orchestration events
    publish_cancellation_event(task_id).await?;
    
    Ok(Json(TaskResponse::from(task)))
}
```

#### Manual Step Resolution Handler
```rust
async fn resolve_step_manually(
    Path((task_id, step_id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
    Json(payload): Json<ManualResolutionRequest>,
    _auth: RequireAuth,
) -> Result<Json<StepResponse>, ApiError> {
    // Validate step can be manually resolved
    let step = get_step(&pool, task_id, step_id).await?;
    
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
    update_step_results(&pool, step_id, results).await?;
    update_step_status(&pool, step_id, StepStatus::ResolvedManually).await?;
    
    Ok(Json(StepResponse::from(step)))
}
```

## Next Steps

1. Create web module structure
2. Implement health endpoints first
3. Add SQLx and database extractors
4. Build out read-only endpoints incrementally
5. Carefully implement the two allowed mutations
6. Add comprehensive testing
7. Document API with OpenAPI/Swagger

This approach ensures we maintain the integrity of the orchestration system while providing necessary visibility and limited, controlled mutation capabilities for operational needs.