# Deployment Patterns and Configuration

**Last Updated**: 2026-01-15
**Audience**: Architects, Operators
**Status**: Active (TAS-133 Messaging Abstraction Complete)
**Related Docs**: [Documentation Hub](README.md) | [Quick Start](quick-start.md) | [Observability](observability/README.md) | [Messaging Abstraction](messaging-abstraction.md)

← Back to [Documentation Hub](README.md)

---

## Overview

Tasker Core supports three deployment modes, each optimized for different operational requirements and infrastructure constraints. This guide covers deployment patterns, configuration management, and production considerations.

**Key Deployment Modes**:
- **Hybrid Mode** (Recommended) - Event-driven with polling fallback
- **EventDrivenOnly Mode** - Pure event-driven for lowest latency
- **PollingOnly Mode** - Traditional polling for restricted environments

**Messaging Backend Options** (TAS-133):
- **PGMQ** (Default) - PostgreSQL-based, single infrastructure dependency
- **RabbitMQ** - AMQP broker, higher throughput for high-volume scenarios

---

## Messaging Backend Selection (TAS-133)

Tasker Core supports multiple messaging backends through a provider-agnostic abstraction layer. The choice of backend affects deployment architecture and operational requirements.

### Backend Comparison

| Feature | PGMQ | RabbitMQ |
|---------|------|----------|
| **Infrastructure** | PostgreSQL only | PostgreSQL + RabbitMQ |
| **Delivery Model** | Poll + pg_notify signals | Native push (basic_consume) |
| **Fallback Polling** | Required for reliability | Not needed |
| **Throughput** | Good | Higher |
| **Latency** | Low (~10-50ms) | Lowest (~5-20ms) |
| **Operational Complexity** | Lower | Higher |
| **Message Persistence** | PostgreSQL transactions | RabbitMQ durability |

### PGMQ (Default)

PostgreSQL Message Queue is the default backend, ideal for:
- **Simpler deployments**: Single database dependency
- **Transactional workflows**: Messages participate in PostgreSQL transactions
- **Smaller to medium scale**: Excellent for most workloads

**Configuration**:
```bash
# Default - no additional configuration needed
TASKER_MESSAGING_BACKEND=pgmq
```

**Deployment Mode Interaction**:
- Uses `pg_notify` for real-time notifications
- Fallback polling recommended for reliability
- Hybrid mode provides best balance

### RabbitMQ

AMQP-based messaging for high-throughput scenarios:
- **High-volume workloads**: Better throughput characteristics
- **Existing RabbitMQ infrastructure**: Leverage existing investments
- **Pure push delivery**: No fallback polling required

**Configuration**:
```bash
TASKER_MESSAGING_BACKEND=rabbitmq
RABBITMQ_URL=amqp://user:password@rabbitmq:5672/%2F
```

**Deployment Mode Interaction**:
- EventDrivenOnly mode is natural fit (no fallback needed)
- Native push delivery via `basic_consume()`
- Protocol-guaranteed message delivery

### Choosing a Backend

```
Decision Tree:
                              ┌─────────────────┐
                              │ Do you need the │
                              │ highest possible │
                              │ throughput?     │
                              └────────┬────────┘
                                       │
                            ┌──────────┴──────────┐
                            │                     │
                           Yes                    No
                            │                     │
                            ▼                     ▼
                   ┌────────────────┐   ┌────────────────┐
                   │ Do you have    │   │ Use PGMQ       │
                   │ existing       │   │ (simpler ops)  │
                   │ RabbitMQ?      │   └────────────────┘
                   └───────┬────────┘
                           │
                ┌──────────┴──────────┐
                │                     │
               Yes                    No
                │                     │
                ▼                     ▼
       ┌────────────────┐    ┌────────────────┐
       │ Use RabbitMQ   │    │ Evaluate       │
       └────────────────┘    │ operational    │
                             │ tradeoffs      │
                             └────────────────┘
```

**Recommendation**: Start with PGMQ. Migrate to RabbitMQ only when throughput requirements demand it.

---

## Production Deployment Strategy: Mixed Mode Architecture

**Important**: In production-grade Kubernetes environments, you typically run **multiple orchestration containers simultaneously with different deployment modes**. This is not just about horizontal scaling with identical configurations—it's about deploying containers with different coordination strategies to optimize for both throughput and reliability.

### Recommended Production Pattern

**High-Throughput + Safety Net Architecture**:

```yaml
# Most orchestration containers in EventDrivenOnly mode for maximum throughput
- EventDrivenOnly containers: 8-12 replicas (handles 80-90% of workload)
- PollingOnly containers: 2-3 replicas (safety net for missed events)
```

**Why this works**:
1. **EventDrivenOnly containers** handle the bulk of work with ~10ms latency
2. **PollingOnly containers** catch any events that might be missed during network issues or LISTEN/NOTIFY failures
3. Both sets of containers coordinate through atomic SQL operations (no conflicts)
4. Scale each mode independently based on throughput needs

### Alternative: All-Hybrid Deployment

You can also deploy all containers in Hybrid mode and scale horizontally:

```yaml
# All containers use Hybrid mode
- Hybrid containers: 10-15 replicas
```

This is simpler but less flexible. The mixed-mode approach lets you:
- **Tune for specific workload patterns** (event-heavy vs. polling-heavy)
- **Adapt to infrastructure constraints** (some networks better for events, others for polling)
- **Optimize resource usage** (EventDrivenOnly uses less CPU than Hybrid)
- **Scale dimensions independently** (scale up event listeners without scaling pollers)

### Key Insight

The different deployment modes exist **not just for config tuning**, but to enable sophisticated deployment strategies where you **mix coordination approaches across containers** to meet production throughput and reliability requirements.

---

## Deployment Mode Comparison

| Feature | Hybrid | EventDrivenOnly | PollingOnly |
|---------|--------|-----------------|-------------|
| **Latency** | Low (event-driven primary) | Lowest (~10ms) | Higher (~100-500ms) |
| **Reliability** | Highest (automatic fallback) | Good (requires stable connections) | Good (no dependencies) |
| **Resource Usage** | Medium (listeners + pollers) | Low (listeners only) | Medium (pollers only) |
| **Network Requirements** | Standard PostgreSQL | Persistent connections required | Standard PostgreSQL |
| **Production Recommended** | ✅ Yes | ⚠️ With stable network | ⚠️ For restricted environments |
| **Complexity** | Medium | Low | Low |

---

## Hybrid Mode (Recommended)

### Overview

Hybrid mode combines the best of both worlds: event-driven coordination for real-time performance with polling fallback for reliability.

**How it works**:
1. PostgreSQL LISTEN/NOTIFY provides real-time event notifications
2. If event listeners fail or lag, polling automatically takes over
3. System continuously monitors and switches between modes
4. No manual intervention required

### Configuration

```toml
# config/tasker/base/orchestration.toml
[orchestration]
deployment_mode = "Hybrid"

[orchestration.hybrid]
# Event listener settings
enable_event_listeners = true
listener_reconnect_interval_ms = 5000
listener_health_check_interval_ms = 30000

# Polling fallback settings
enable_polling_fallback = true
polling_interval_ms = 1000
fallback_activation_threshold_ms = 5000

# Worker event settings
[orchestration.worker_events]
enable_worker_listeners = true
worker_listener_reconnect_ms = 5000
```

### When to Use Hybrid Mode

**Ideal for**:
- Production deployments requiring high reliability
- Environments with occasional network instability
- Systems requiring both low latency and guaranteed delivery
- Multi-region deployments with variable network quality

**Example: Production E-commerce Platform**
```yaml
# docker-compose.production.yml
version: '3.8'

services:
  orchestration:
    image: tasker-orchestration:latest
    environment:
      - TASKER_ENV=production
      - TASKER_DEPLOYMENT_MODE=Hybrid
      - DATABASE_URL=postgresql://tasker:${DB_PASSWORD}@postgres:5432/tasker_production
      - RUST_LOG=info
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: '2'
          memory: 2G
        reservations:
          cpus: '1'
          memory: 1G
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  postgres:
    image: postgres:16
    environment:
      - POSTGRES_DB=tasker_production
      - POSTGRES_PASSWORD=${DB_PASSWORD}
    volumes:
      - postgres-data:/var/lib/postgresql/data
    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G

volumes:
  postgres-data:
```

### Monitoring Hybrid Mode

**Key Metrics**:
```rust
// Hybrid mode health indicators
tasker_event_listener_active{mode="hybrid"} = 1           // Listener is active
tasker_event_listener_lag_ms{mode="hybrid"} < 100         // Event lag is acceptable
tasker_polling_fallback_active{mode="hybrid"} = 0         // Not in fallback mode
tasker_mode_switches_total{mode="hybrid"} < 10/hour       // Infrequent mode switching
```

**Alert conditions**:
- Event listener down for > 60 seconds
- Polling fallback active for > 5 minutes
- Mode switches > 20 per hour (indicates instability)

---

## EventDrivenOnly Mode

### Overview

EventDrivenOnly mode provides the lowest possible latency by relying entirely on PostgreSQL LISTEN/NOTIFY for coordination.

**How it works**:
1. Orchestration and workers establish persistent PostgreSQL connections
2. LISTEN on specific channels for events
3. Immediate notification on queue changes
4. No polling overhead or delay

### Configuration

```toml
# config/tasker/base/orchestration.toml
[orchestration]
deployment_mode = "EventDrivenOnly"

[orchestration.event_driven]
# Listener configuration
listener_reconnect_interval_ms = 2000
listener_health_check_interval_ms = 15000
max_reconnect_attempts = 10

# Event channels
channels = [
    "pgmq_message_ready.orchestration",
    "pgmq_message_ready.*",
    "pgmq_queue_created"
]

# Connection pool for listeners
listener_pool_size = 5
connection_timeout_ms = 5000
```

### When to Use EventDrivenOnly Mode

**Ideal for**:
- High-throughput, low-latency requirements
- Stable network environments
- Development and testing environments
- Systems with reliable PostgreSQL infrastructure

**Not recommended for**:
- Unstable network connections
- Environments with frequent PostgreSQL failovers
- Systems requiring guaranteed operation during network issues

**Example: High-Performance Payment Processing**
```rust
// Worker configuration for event-driven mode
use tasker_worker::WorkerConfig;

let config = WorkerConfig {
    deployment_mode: DeploymentMode::EventDrivenOnly,
    namespaces: vec!["payments".to_string()],
    event_driven_settings: EventDrivenSettings {
        listener_reconnect_interval_ms: 2000,
        health_check_interval_ms: 15000,
        max_reconnect_attempts: 10,
    },
    ..Default::default()
};

// Start worker with event-driven mode
let worker = WorkerCore::from_config(config).await?;
worker.start().await?;
```

### Monitoring EventDrivenOnly Mode

**Critical Metrics**:
```rust
// Event-driven health indicators
tasker_event_listener_active{mode="event_driven"} = 1    // Must be 1
tasker_event_notifications_received_total                 // Should be > 0
tasker_event_processing_duration_seconds                  // Should be < 0.01
tasker_listener_reconnections_total                       // Should be low
```

**Alert conditions**:
- Event listener inactive
- No events received for > 60 seconds (when activity expected)
- Reconnections > 5 per hour

---

## PollingOnly Mode

### Overview

PollingOnly mode provides the most reliable operation in restricted or unstable network environments by using traditional polling.

**How it works**:
1. Orchestration and workers poll message queues at regular intervals
2. No dependency on persistent connections or LISTEN/NOTIFY
3. Configurable polling intervals for performance/resource trade-offs
4. Automatic retry and backoff on failures

### Configuration

```toml
# config/tasker/base/orchestration.toml
[orchestration]
deployment_mode = "PollingOnly"

[orchestration.polling]
# Polling intervals
task_request_poll_interval_ms = 1000
step_result_poll_interval_ms = 500
finalization_poll_interval_ms = 2000

# Batch processing
batch_size = 10
max_messages_per_poll = 100

# Backoff on errors
error_backoff_base_ms = 1000
error_backoff_max_ms = 30000
error_backoff_multiplier = 2.0
```

### When to Use PollingOnly Mode

**Ideal for**:
- Restricted network environments (firewalls blocking persistent connections)
- Environments with frequent PostgreSQL connection issues
- Systems prioritizing reliability over latency
- Legacy infrastructure with limited LISTEN/NOTIFY support

**Not recommended for**:
- High-frequency, low-latency requirements
- Systems with strict resource constraints
- Environments where polling overhead is problematic

**Example: Batch Processing System**
```toml
# config/tasker/environments/production/orchestration.toml
[orchestration]
deployment_mode = "PollingOnly"

[orchestration.polling]
# Longer intervals for batch processing
task_request_poll_interval_ms = 5000
step_result_poll_interval_ms = 2000
finalization_poll_interval_ms = 10000

# Large batches for efficiency
batch_size = 50
max_messages_per_poll = 500
```

### Monitoring PollingOnly Mode

**Key Metrics**:
```rust
// Polling health indicators
tasker_polling_cycles_total                               // Should be increasing
tasker_polling_messages_processed_total                   // Should be > 0
tasker_polling_duration_seconds                           // Should be stable
tasker_polling_errors_total                               // Should be low
```

**Alert conditions**:
- Polling stopped (no cycles in last 60 seconds)
- Polling duration > 10x interval (indicates overload)
- Error rate > 5% of polling cycles

---

## Configuration Management

### Component-Based Configuration

Tasker Core uses a component-based TOML configuration system with environment-specific overrides.

**Configuration Structure**:
```
config/tasker/
├── base/                          # Base configuration (all environments)
│   ├── database.toml             # Database connection pool settings
│   ├── orchestration.toml        # Orchestration and deployment mode
│   ├── circuit_breakers.toml    # Circuit breaker thresholds
│   ├── executor_pools.toml      # Executor pool sizing
│   ├── pgmq.toml                # Message queue configuration
│   ├── query_cache.toml         # Query caching settings
│   └── telemetry.toml           # Metrics and logging
│
└── environments/                  # Environment-specific overrides
    ├── development/
    │   └── *.toml               # Development overrides
    ├── test/
    │   └── *.toml               # Test overrides
    └── production/
        └── *.toml               # Production overrides
```

### Environment Detection

```bash
# Set environment via TASKER_ENV
export TASKER_ENV=production

# Validate configuration
cargo run --bin config-validator

# Expected output:
# ✓ Configuration loaded successfully
# ✓ Environment: production
# ✓ Deployment mode: Hybrid
# ✓ Database pool: 50 connections
# ✓ Circuit breakers: 10 configurations
```

### Example: Production Configuration

```toml
# config/tasker/environments/production/orchestration.toml
[orchestration]
deployment_mode = "Hybrid"
max_concurrent_tasks = 1000
task_timeout_seconds = 3600

[orchestration.hybrid]
enable_event_listeners = true
enable_polling_fallback = true
polling_interval_ms = 2000
fallback_activation_threshold_ms = 10000

[orchestration.health]
health_check_interval_ms = 30000
unhealthy_threshold = 3
recovery_threshold = 2
```

```toml
# config/tasker/environments/production/database.toml
[database]
max_connections = 50
min_connections = 10
connection_timeout_ms = 5000
idle_timeout_seconds = 600
max_lifetime_seconds = 1800

[database.query_cache]
enabled = true
max_size = 1000
ttl_seconds = 300
```

```toml
# config/tasker/environments/production/circuit_breakers.toml
[circuit_breakers.database]
enabled = true
error_threshold = 5
timeout_seconds = 60
half_open_timeout_seconds = 30

[circuit_breakers.message_queue]
enabled = true
error_threshold = 10
timeout_seconds = 120
half_open_timeout_seconds = 60
```

---

## Docker Compose Deployment

### Development Setup

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_USER: tasker
      POSTGRES_PASSWORD: tasker
      POSTGRES_DB: tasker_rust_test
    ports:
      - "5432:5432"
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U tasker"]
      interval: 5s
      timeout: 5s
      retries: 5

  orchestration:
    build:
      context: .
      target: orchestration
    environment:
      - TASKER_ENV=development
      - DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_rust_test
      - RUST_LOG=debug
    ports:
      - "8080:8080"
    depends_on:
      postgres:
        condition: service_healthy
    profiles:
      - server

  worker:
    build:
      context: .
      target: worker
    environment:
      - TASKER_ENV=development
      - DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_rust_test
      - RUST_LOG=debug
    ports:
      - "8081:8081"
    depends_on:
      postgres:
        condition: service_healthy
    profiles:
      - server

  ruby-worker:
    build:
      context: ./workers/ruby
      dockerfile: Dockerfile
    environment:
      - TASKER_ENV=development
      - DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_rust_test
      - RUST_LOG=debug
    ports:
      - "8082:8082"
    depends_on:
      postgres:
        condition: service_healthy
    profiles:
      - server

volumes:
  postgres-data:
```

### Production Deployment

```yaml
# docker-compose.production.yml
version: '3.8'

services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_USER: tasker
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password
      POSTGRES_DB: tasker_production
    volumes:
      - postgres-data:/var/lib/postgresql/data
    deploy:
      placement:
        constraints:
          - node.labels.database == true
      resources:
        limits:
          cpus: '4'
          memory: 8G
        reservations:
          cpus: '2'
          memory: 4G
    secrets:
      - db_password

  orchestration:
    image: tasker-orchestration:${VERSION}
    environment:
      - TASKER_ENV=production
      - DATABASE_URL_FILE=/run/secrets/database_url
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
        order: start-first
      rollback_config:
        parallelism: 0
        order: stop-first
      resources:
        limits:
          cpus: '2'
          memory: 2G
        reservations:
          cpus: '1'
          memory: 1G
    secrets:
      - database_url

  worker:
    image: tasker-worker:${VERSION}
    environment:
      - TASKER_ENV=production
      - DATABASE_URL_FILE=/run/secrets/database_url
    deploy:
      replicas: 5
      resources:
        limits:
          cpus: '1'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 512M
    secrets:
      - database_url

secrets:
  db_password:
    external: true
  database_url:
    external: true

volumes:
  postgres-data:
    driver: local
```

---

## Kubernetes Deployment

### Mixed-Mode Production Deployment (Recommended)

This example demonstrates the recommended production pattern: multiple orchestration deployments with different modes.

```yaml
# k8s/orchestration-event-driven.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tasker-orchestration-event-driven
  namespace: tasker
  labels:
    app: tasker-orchestration
    mode: event-driven
spec:
  replicas: 10  # Majority of orchestration capacity
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 2
      maxUnavailable: 0
  selector:
    matchLabels:
      app: tasker-orchestration
      mode: event-driven
  template:
    metadata:
      labels:
        app: tasker-orchestration
        mode: event-driven
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: orchestration
        image: tasker-orchestration:1.0.0
        env:
        - name: TASKER_ENV
          value: "production"
        - name: DEPLOYMENT_MODE
          value: "EventDrivenOnly"  # High-throughput mode
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tasker-secrets
              key: database-url
        - name: RUST_LOG
          value: "info"
        ports:
        - containerPort: 8080
          name: http
        resources:
          requests:
            cpu: 500m      # Lower CPU for event-driven
            memory: 512Mi
          limits:
            cpu: 1000m
            memory: 1Gi
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5

---
# k8s/orchestration-polling.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tasker-orchestration-polling
  namespace: tasker
  labels:
    app: tasker-orchestration
    mode: polling
spec:
  replicas: 3  # Safety net for missed events
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: tasker-orchestration
      mode: polling
  template:
    metadata:
      labels:
        app: tasker-orchestration
        mode: polling
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: orchestration
        image: tasker-orchestration:1.0.0
        env:
        - name: TASKER_ENV
          value: "production"
        - name: DEPLOYMENT_MODE
          value: "PollingOnly"  # Reliability safety net
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tasker-secrets
              key: database-url
        - name: RUST_LOG
          value: "info"
        ports:
        - containerPort: 8080
          name: http
        resources:
          requests:
            cpu: 750m      # Higher CPU for polling
            memory: 512Mi
          limits:
            cpu: 1500m
            memory: 1Gi
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5

---
# k8s/orchestration-service.yaml
apiVersion: v1
kind: Service
metadata:
  name: tasker-orchestration
  namespace: tasker
spec:
  selector:
    app: tasker-orchestration  # Matches BOTH deployments
  ports:
  - port: 8080
    targetPort: 8080
    protocol: TCP
    name: http
  type: ClusterIP
```

**Key points about this mixed-mode deployment**:
1. **10 EventDrivenOnly pods** handle 80-90% of work with ~10ms latency
2. **3 PollingOnly pods** catch anything missed by event listeners
3. **Single service** load balances across all 13 pods
4. **No conflicts** - atomic SQL operations prevent duplicate processing
5. **Independent scaling** - scale event-driven pods for throughput, polling pods for reliability

### Single-Mode Orchestration Deployment

```yaml
# k8s/orchestration-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tasker-orchestration
  namespace: tasker
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: tasker-orchestration
  template:
    metadata:
      labels:
        app: tasker-orchestration
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: orchestration
        image: tasker-orchestration:1.0.0
        env:
        - name: TASKER_ENV
          value: "production"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tasker-secrets
              key: database-url
        - name: RUST_LOG
          value: "info"
        ports:
        - containerPort: 8080
          name: http
          protocol: TCP
        resources:
          requests:
            cpu: 1000m
            memory: 1Gi
          limits:
            cpu: 2000m
            memory: 2Gi
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 2

---
apiVersion: v1
kind: Service
metadata:
  name: tasker-orchestration
  namespace: tasker
spec:
  selector:
    app: tasker-orchestration
  ports:
  - port: 8080
    targetPort: 8080
    protocol: TCP
    name: http
  type: ClusterIP
```

### Worker Deployment

```yaml
# k8s/worker-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tasker-worker-payments
  namespace: tasker
spec:
  replicas: 5
  selector:
    matchLabels:
      app: tasker-worker
      namespace: payments
  template:
    metadata:
      labels:
        app: tasker-worker
        namespace: payments
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8081"
    spec:
      containers:
      - name: worker
        image: tasker-worker:1.0.0
        env:
        - name: TASKER_ENV
          value: "production"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tasker-secrets
              key: database-url
        - name: RUST_LOG
          value: "info"
        - name: WORKER_NAMESPACES
          value: "payments"
        ports:
        - containerPort: 8081
          name: http
          protocol: TCP
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 1000m
            memory: 1Gi
        livenessProbe:
          httpGet:
            path: /health
            port: 8081
          initialDelaySeconds: 20
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8081
          initialDelaySeconds: 5
          periodSeconds: 5

---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: tasker-worker-payments
  namespace: tasker
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: tasker-worker-payments
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

---

## Health Monitoring

### Health Check Endpoints

**Orchestration Health**:
```bash
# Basic health check
curl http://localhost:8080/health

# Response:
{
  "status": "healthy",
  "database": "connected",
  "message_queue": "operational"
}

# Detailed health check
curl http://localhost:8080/health/detailed

# Response:
{
  "status": "healthy",
  "deployment_mode": "Hybrid",
  "event_listeners": {
    "active": true,
    "channels": 3,
    "lag_ms": 12
  },
  "polling": {
    "active": false,
    "fallback_triggered": false
  },
  "database": {
    "status": "connected",
    "pool_size": 50,
    "active_connections": 23
  },
  "circuit_breakers": {
    "database": "closed",
    "message_queue": "closed"
  },
  "executors": {
    "task_initializer": {
      "active": 3,
      "max": 10,
      "queue_depth": 5
    },
    "result_processor": {
      "active": 5,
      "max": 10,
      "queue_depth": 12
    }
  }
}
```

**Worker Health**:
```bash
# Worker health check
curl http://localhost:8081/health

# Response:
{
  "status": "healthy",
  "namespaces": ["payments", "inventory"],
  "active_executions": 8,
  "claimed_steps": 3
}
```

### Kubernetes Probes

```yaml
# Liveness probe - restart if unhealthy
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 30
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 3

# Readiness probe - remove from load balancer if not ready
readinessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 5
  timeoutSeconds: 3
  failureThreshold: 2
```

### gRPC Health Checks (TAS-177)

Tasker Core exposes gRPC health endpoints alongside REST for Kubernetes gRPC health probes.

**Port Allocation**:

| Service | REST Port | gRPC Port |
|---------|-----------|-----------|
| Orchestration | 8080 | 9190 |
| Rust Worker | 8081 | 9191 |
| Ruby Worker | 8082 | 9200 |
| Python Worker | 8083 | 9300 |
| TypeScript Worker | 8085 | 9400 |

**gRPC Health Endpoints**:
```bash
# Using grpcurl
grpcurl -plaintext localhost:9190 tasker.v1.HealthService/CheckLiveness
grpcurl -plaintext localhost:9190 tasker.v1.HealthService/CheckReadiness
grpcurl -plaintext localhost:9190 tasker.v1.HealthService/CheckDetailedHealth
```

**Kubernetes gRPC Probes** (Kubernetes 1.24+):
```yaml
# gRPC liveness probe
livenessProbe:
  grpc:
    port: 9190
    service: tasker.v1.HealthService
  initialDelaySeconds: 30
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 3

# gRPC readiness probe
readinessProbe:
  grpc:
    port: 9190
    service: tasker.v1.HealthService
  initialDelaySeconds: 10
  periodSeconds: 5
  timeoutSeconds: 3
  failureThreshold: 2
```

**Configuration** (`config/tasker/base/orchestration.toml`):
```toml
[orchestration.grpc]
enabled = true
bind_address = "${TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS:-0.0.0.0:9190}"
enable_reflection = true       # Service discovery via grpcurl
enable_health_service = true   # gRPC health checks
```

---

## Scaling Patterns

### Horizontal Scaling

#### Mixed-Mode Orchestration Scaling (Recommended)

Scale different deployment modes independently to optimize for throughput and reliability:

```yaml
# Scale event-driven pods for throughput
kubectl scale deployment tasker-orchestration-event-driven --replicas=15 -n tasker

# Scale polling pods for reliability
kubectl scale deployment tasker-orchestration-polling --replicas=5 -n tasker
```

**Scaling strategy by workload**:

| Scenario | Event-Driven Pods | Polling Pods | Rationale |
|----------|-------------------|--------------|-----------|
| **High throughput** | 15-20 | 3-5 | Maximize event-driven capacity |
| **Network unstable** | 5-8 | 5-8 | Balance between modes |
| **Cost optimization** | 10-12 | 2-3 | Minimize polling overhead |
| **Maximum reliability** | 8-10 | 8-10 | Ensure complete coverage |

#### Single-Mode Orchestration Scaling

If using single deployment mode (Hybrid or EventDrivenOnly):

```yaml
# Scale orchestration to 10 replicas (all same mode)
kubectl scale deployment tasker-orchestration --replicas=10 -n tasker
```

**Key principles**:
- Multiple orchestration instances process tasks independently
- Atomic finalization claiming prevents duplicate processing
- Load balancer distributes API requests across instances

#### Worker Scaling

Workers scale independently per namespace:

```yaml
# Scale payment workers to 10 replicas
kubectl scale deployment tasker-worker-payments --replicas=10 -n tasker
```

- Each worker claims steps from namespace-specific queues
- No coordination required between workers
- Scale per namespace based on queue depth

### Vertical Scaling

**Resource Allocation**:
```yaml
# High-throughput orchestration
resources:
  requests:
    cpu: 2000m
    memory: 4Gi
  limits:
    cpu: 4000m
    memory: 8Gi

# Standard worker
resources:
  requests:
    cpu: 500m
    memory: 512Mi
  limits:
    cpu: 1000m
    memory: 1Gi
```

### Auto-Scaling

**HPA Configuration**:
```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: tasker-orchestration
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: tasker-orchestration
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Pods
    pods:
      metric:
        name: tasker_tasks_active
      target:
        type: AverageValue
        averageValue: "100"
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
      - type: Percent
        value: 100
        periodSeconds: 30
```

---

## Production Considerations

### Database Configuration

**Connection Pooling**:
```toml
# config/tasker/environments/production/database.toml
[database]
max_connections = 50              # Total pool size
min_connections = 10              # Minimum maintained connections
connection_timeout_ms = 5000      # Connection acquisition timeout
idle_timeout_seconds = 600        # Close idle connections after 10 minutes
max_lifetime_seconds = 1800       # Recycle connections after 30 minutes
```

**Calculation**:
```
Total DB Connections = (Orchestration Replicas × Pool Size) + (Worker Replicas × Pool Size)
Example: (3 × 50) + (10 × 20) = 350 connections

Ensure PostgreSQL max_connections > Total DB Connections + Buffer
Recommended: max_connections = 500 for above example
```

### Circuit Breaker Tuning

```toml
# config/tasker/environments/production/circuit_breakers.toml
[circuit_breakers.database]
enabled = true
error_threshold = 5               # Open after 5 consecutive errors
timeout_seconds = 60              # Stay open for 60 seconds
half_open_timeout_seconds = 30    # Test recovery for 30 seconds

[circuit_breakers.message_queue]
enabled = true
error_threshold = 10
timeout_seconds = 120
half_open_timeout_seconds = 60
```

### Executor Pool Sizing

```toml
# config/tasker/environments/production/executor_pools.toml
[executor_pools.task_initializer]
min_executors = 2
max_executors = 10
queue_high_watermark = 100
queue_low_watermark = 10

[executor_pools.result_processor]
min_executors = 5
max_executors = 20
queue_high_watermark = 200
queue_low_watermark = 20

[executor_pools.step_enqueuer]
min_executors = 3
max_executors = 15
queue_high_watermark = 150
queue_low_watermark = 15
```

### Observability Integration

**Prometheus Metrics**:
```yaml
# Prometheus scrape config
scrape_configs:
  - job_name: 'tasker-orchestration'
    kubernetes_sd_configs:
      - role: pod
        namespaces:
          names:
            - tasker
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
        action: keep
        regex: true
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)
      - source_labels: [__address__, __meta_kubernetes_pod_annotation_prometheus_io_port]
        action: replace
        regex: ([^:]+)(?::\d+)?;(\d+)
        replacement: $1:$2
        target_label: __address__
```

**Key Alerts**:
```yaml
# alerts.yaml
groups:
  - name: tasker
    interval: 30s
    rules:
      - alert: TaskerOrchestrationDown
        expr: up{job="tasker-orchestration"} == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Tasker orchestration instance down"

      - alert: TaskerHighErrorRate
        expr: rate(tasker_step_errors_total[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High error rate in step execution"

      - alert: TaskerCircuitBreakerOpen
        expr: tasker_circuit_breaker_state{state="open"} == 1
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Circuit breaker {{ $labels.name }} is open"

      - alert: TaskerDatabasePoolExhausted
        expr: tasker_database_pool_active >= tasker_database_pool_max
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Database connection pool exhausted"
```

---

## Migration Strategies

### Migrating to Hybrid Mode

**Step 1: Enable event listeners**
```toml
# config/tasker/environments/production/orchestration.toml
[orchestration]
deployment_mode = "Hybrid"

[orchestration.hybrid]
enable_event_listeners = true
enable_polling_fallback = true    # Keep polling enabled during migration
```

**Step 2: Monitor event listener health**
```bash
# Check metrics for event listener stability
curl http://localhost:8080/health/detailed | jq '.event_listeners'
```

**Step 3: Gradually reduce polling frequency**
```toml
# Once event listeners are stable
[orchestration.hybrid]
polling_interval_ms = 5000        # Increase from 1000ms to 5000ms
```

**Step 4: Validate performance**
- Monitor latency metrics: `tasker_step_discovery_duration_seconds`
- Verify no missed events: `tasker_polling_messages_found_total` should be near zero

### Rollback Plan

**If event-driven mode fails**:
```toml
# Immediate rollback to PollingOnly
[orchestration]
deployment_mode = "PollingOnly"

[orchestration.polling]
task_request_poll_interval_ms = 500    # Aggressive polling
```

**Gradual rollback**:
1. Increase polling frequency in Hybrid mode
2. Monitor for stability
3. Disable event listeners once polling is stable
4. Switch to PollingOnly mode

---

## Troubleshooting

### Event Listener Issues

**Problem**: Event listeners not receiving notifications

**Diagnosis**:
```sql
-- Check PostgreSQL LISTEN/NOTIFY is working
NOTIFY pgmq_message_ready, 'test';
```

```bash
# Check listener status
curl http://localhost:8080/health/detailed | jq '.event_listeners'
```

**Solutions**:
- Verify PostgreSQL version supports LISTEN/NOTIFY (9.0+)
- Check firewall rules allow persistent connections
- Increase `listener_reconnect_interval_ms` if connections drop frequently
- Switch to Hybrid or PollingOnly mode if issues persist

### Polling Performance Issues

**Problem**: High CPU usage from polling

**Diagnosis**:
```bash
# Check polling frequency and batch sizes
curl http://localhost:8080/health/detailed | jq '.polling'
```

**Solutions**:
- Increase polling intervals
- Increase batch sizes to process more messages per poll
- Switch to Hybrid or EventDrivenOnly mode for better performance
- Scale horizontally to distribute polling load

### Database Connection Exhaustion

**Problem**: "connection pool exhausted" errors

**Diagnosis**:
```sql
-- Check active connections
SELECT count(*) FROM pg_stat_activity WHERE datname = 'tasker_production';

-- Check max connections
SHOW max_connections;
```

**Solutions**:
- Increase `max_connections` in database.toml
- Increase PostgreSQL `max_connections` setting
- Reduce number of replicas
- Implement connection pooling at infrastructure level (PgBouncer)

---

## Best Practices

### Configuration Management

1. **Use environment-specific overrides** instead of modifying base configuration
2. **Validate configuration** with `config-validator` before deployment
3. **Version control all configuration** including environment overrides
4. **Use secrets management** for sensitive values (passwords, keys)

### Deployment Strategy

1. **Use mixed-mode architecture** in production (EventDrivenOnly + PollingOnly)
   - Deploy 80-90% of orchestration pods in EventDrivenOnly mode for throughput
   - Deploy 10-20% of orchestration pods in PollingOnly mode as safety net
   - Single service load balances across all pods
2. **Alternative**: Deploy all pods in Hybrid mode for simpler configuration
   - Trade-off: Less tuning flexibility, slightly higher resource usage
3. **Scale each mode independently** based on workload characteristics
4. **Monitor deployment mode metrics** to adjust ratios over time
5. **Test mixed-mode deployments** in staging before production

### Deployment Operations

1. **Always test configuration changes** in staging first
2. **Use rolling updates** with health checks to prevent downtime
3. **Monitor deployment mode health** during and after deployments
4. **Keep polling capacity** available even when event-driven is primary

### Scaling Guidelines

1. **Mixed-mode orchestration**: Scale EventDrivenOnly and PollingOnly deployments independently
   - Scale event-driven pods based on throughput requirements
   - Scale polling pods based on reliability requirements
2. **Single-mode orchestration**: Scale based on API request rate and task initialization throughput
3. **Workers**: Scale based on namespace-specific queue depth
4. **Database connections**: Monitor and adjust pool sizes as replicas scale
5. **Use HPA** for automatic scaling based on CPU/memory and custom metrics

### Observability

1. **Enable comprehensive metrics** in production
2. **Set up alerts** for circuit breaker states, connection pool exhaustion
3. **Monitor deployment mode distribution** in mixed-mode deployments
4. **Track event listener lag** in EventDrivenOnly and Hybrid modes
5. **Monitor polling overhead** to optimize resource usage
6. **Track step execution latency** per namespace and handler

---

## Summary

Tasker Core's flexible deployment modes enable sophisticated production architectures:

### Deployment Modes

- **Hybrid Mode**: Event-driven with polling fallback in a single container
- **EventDrivenOnly Mode**: Maximum throughput with ~10ms latency
- **PollingOnly Mode**: Reliable safety net with traditional polling

### Recommended Production Pattern

**Mixed-Mode Architecture** (recommended for production at scale):
- Deploy majority of orchestration pods in **EventDrivenOnly** mode for high throughput
- Deploy minority of orchestration pods in **PollingOnly** mode as reliability safety net
- Both deployments coordinate through atomic SQL operations with no conflicts
- Scale each mode independently based on workload characteristics

**Alternative**: Deploy all pods in **Hybrid** mode for simpler configuration with automatic fallback.

The key insight: deployment modes exist not just for configuration tuning, but to enable mixing coordination strategies across containers to meet production requirements for both throughput and reliability.

---

← Back to [Documentation Hub](README.md)

**Next**: [Observability](observability/README.md) | [Benchmarks](benchmarks/README.md) | [Quick Start](quick-start.md)
