# TAS-43: Event-Driven Task Claiming with pg_notify

## Problem Statement

The current orchestration system has an inherent throughput limitation due to its polling-based architecture. The `OrchestrationLoop::run_cycle()` method polls the `tasker_ready_tasks` view every second (configurable), creating several issues:

1. **Latency**: Tasks wait up to `cycle_interval` before being discovered
2. **Database Load**: Continuous polling even when no tasks are ready
3. **Scalability Ceiling**: Adding more coordinators increases database polling pressure
4. **Resource Inefficiency**: CPU cycles wasted on empty poll results

## Solution: Event-Driven Architecture with pg_notify

Replace polling with PostgreSQL's LISTEN/NOTIFY mechanism to create a truly event-driven system that reacts immediately to state changes while maintaining reliability through a hybrid approach.

## Technical Comparison: sqlx vs tokio_postgres

### sqlx::PgListener
```rust
use sqlx::postgres::PgListener;

// Pros:
// - Integrated with existing sqlx connection pool
// - Automatic reconnection handling
// - Clean async/await API
// - Type-safe notification parsing

let mut listener = PgListener::connect_with(&pool).await?;
listener.listen_all(vec!["task_ready", "namespace_created"]).await?;

while let Some(notification) = listener.recv().await? {
    let payload: TaskReadyEvent = serde_json::from_str(&notification.payload())?;
    // Process event
}
```

### tokio_postgres::AsyncNotification
```rust
use tokio_postgres::{AsyncMessage, Notification};

// Pros:
// - Lower-level control
// - Potentially lower overhead
// - Direct stream access
// - Fine-grained connection management

let (client, mut connection) = tokio_postgres::connect(&config, NoTls).await?;

// Spawn connection handler
tokio::spawn(async move {
    while let Some(message) = connection.recv().await {
        if let AsyncMessage::Notification(n) = message {
            tx.send(n).await.unwrap();
        }
    }
});

client.execute("LISTEN task_ready", &[]).await?;
```

### Recommendation: sqlx::PgListener

Given our existing sqlx infrastructure and the need for robust connection management, **sqlx::PgListener** is the better choice because:
1. Seamless integration with our connection pool
2. Built-in reconnection logic crucial for production
3. Consistent error handling with rest of codebase
4. Less boilerplate for notification management

## Architecture Design

### 1. Database Layer: Trigger System

Based on the actual database schema using UUID primary keys and state machine transitions:

```sql
-- Trigger function for task readiness based on step state changes
-- This leverages the existing get_task_execution_context() function for consistency
CREATE OR REPLACE FUNCTION notify_task_ready_on_step_transition() RETURNS TRIGGER AS $$
DECLARE
    task_info RECORD;
    namespace_name TEXT;
    payload JSONB;
    execution_context RECORD;
BEGIN
    -- Only process transitions that could affect task readiness
    -- Key insight: when steps transition TO 'enqueued', 'complete', 'error', or 'resolved_manually'
    -- other steps might become ready due to dependency satisfaction
    IF NEW.to_state IN ('enqueued', 'complete', 'error', 'resolved_manually') OR
       OLD.to_state IN ('in_progress', 'enqueued') THEN
        
        -- Get task and namespace information
        SELECT 
            t.task_uuid,
            t.priority,
            t.created_at,
            t.complete,
            tn.name as namespace
        INTO task_info
        FROM tasker_workflow_steps ws
        JOIN tasker_tasks t ON t.task_uuid = ws.task_uuid
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        WHERE ws.workflow_step_uuid = NEW.workflow_step_uuid;
        
        -- Skip if task is already complete
        IF task_info.complete THEN
            RETURN NEW;
        END IF;
        
        -- Use existing get_task_execution_context() for proven readiness logic
        SELECT ready_steps, execution_status
        INTO execution_context
        FROM get_task_execution_context(task_info.task_uuid);
        
        -- Only notify if task has ready steps (aligns with orchestration loop logic)
        IF execution_context.ready_steps > 0 AND execution_context.execution_status = 'has_ready_steps' THEN
            
            -- Build minimal payload for pg_notify 8KB limit
            payload := jsonb_build_object(
                'task_uuid', task_info.task_uuid,
                'namespace', task_info.namespace,
                'priority', task_info.priority,
                'ready_steps', execution_context.ready_steps,
                'triggered_by', 'step_transition',
                'step_uuid', NEW.workflow_step_uuid,
                'step_state', NEW.to_state
            );
            
            -- Send namespace-specific notification
            PERFORM pg_notify('task_ready.' || task_info.namespace, payload::text);
            
            -- Send global notification for coordinators listening to all namespaces
            PERFORM pg_notify('task_ready', payload::text);
            
        END IF;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply trigger to workflow_step_transitions table (the source of truth for readiness)
CREATE TRIGGER task_ready_on_step_transition
    AFTER INSERT OR UPDATE ON tasker_workflow_step_transitions
    FOR EACH ROW
    EXECUTE FUNCTION notify_task_ready_on_step_transition();

-- Trigger for task-level state changes (task transitions)
CREATE OR REPLACE FUNCTION notify_task_state_change() RETURNS TRIGGER AS $$
DECLARE
    task_info RECORD;
    namespace_name TEXT;
    payload JSONB;
    execution_context RECORD;
BEGIN
    -- Only process meaningful state transitions
    IF NEW.to_state != OLD.to_state OR OLD.to_state IS NULL THEN
        
        -- Get task and namespace information
        SELECT 
            t.task_uuid,
            t.priority,
            t.created_at,
            t.complete,
            tn.name as namespace
        INTO task_info
        FROM tasker_tasks t
        JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
        WHERE t.task_uuid = NEW.task_uuid;
        
        -- For task completion or error states, notify for cleanup/finalization
        IF NEW.to_state IN ('complete', 'error', 'cancelled') THEN
            payload := jsonb_build_object(
                'task_uuid', task_info.task_uuid,
                'namespace', task_info.namespace,
                'task_state', NEW.to_state,
                'triggered_by', 'task_transition',
                'action_needed', CASE 
                    WHEN NEW.to_state = 'complete' THEN 'finalization'
                    WHEN NEW.to_state = 'error' THEN 'error_handling'
                    WHEN NEW.to_state = 'cancelled' THEN 'cleanup'
                END
            );
            
            PERFORM pg_notify('task_state_change.' || task_info.namespace, payload::text);
            PERFORM pg_notify('task_state_change', payload::text);
            
        -- For new tasks (transitions to 'in_progress'), check for readiness
        ELSIF NEW.to_state = 'in_progress' AND (OLD.to_state IS NULL OR OLD.to_state = 'pending') THEN
            -- Use get_task_execution_context for consistency
            SELECT ready_steps, execution_status
            INTO execution_context
            FROM get_task_execution_context(task_info.task_uuid);
            
            IF execution_context.ready_steps > 0 THEN
                payload := jsonb_build_object(
                    'task_uuid', task_info.task_uuid,
                    'namespace', task_info.namespace,
                    'priority', task_info.priority,
                    'ready_steps', execution_context.ready_steps,
                    'triggered_by', 'task_start',
                    'task_state', NEW.to_state
                );
                
                PERFORM pg_notify('task_ready.' || task_info.namespace, payload::text);
                PERFORM pg_notify('task_ready', payload::text);
            END IF;
        END IF;
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply trigger to task_transitions table
CREATE TRIGGER task_state_change_notification
    AFTER INSERT OR UPDATE ON tasker_task_transitions
    FOR EACH ROW
    EXECUTE FUNCTION notify_task_state_change();

-- Namespace creation notification for dynamic coordinator spawning
CREATE OR REPLACE FUNCTION notify_namespace_created() RETURNS TRIGGER AS $$
DECLARE
    payload JSONB;
BEGIN
    payload := jsonb_build_object(
        'namespace_uuid', NEW.task_namespace_uuid,
        'namespace_name', NEW.name,
        'description', NEW.description,
        'created_at', NEW.created_at,
        'triggered_by', 'namespace_creation'
    );
    
    PERFORM pg_notify('namespace_created', payload::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER namespace_creation_notification
    AFTER INSERT ON tasker_task_namespaces
    FOR EACH ROW
    EXECUTE FUNCTION notify_namespace_created();

-- Index optimizations for trigger performance
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_workflow_step_transitions_task_ready 
ON tasker_workflow_step_transitions (workflow_step_uuid, to_state, created_at) 
WHERE to_state IN ('enqueued', 'complete', 'error', 'resolved_manually');

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_task_transitions_state_changes 
ON tasker_task_transitions (task_uuid, to_state, created_at) 
WHERE to_state IN ('in_progress', 'complete', 'error', 'cancelled');
```

### 2. Rust Implementation: EventDrivenOrchestrator

```rust
// tasker-orchestration/src/orchestration/event_driven_orchestrator.rs

use sqlx::postgres::{PgListener, PgNotification};
use tokio::sync::mpsc;
use std::sync::Arc;
use dashmap::DashMap;

pub struct EventDrivenOrchestrator {
    pool: Arc<PgPool>,
    coordinators: Arc<DashMap<String, NamespaceCoordinator>>,
    config: Arc<OrchestrationConfig>,
    shutdown: CancellationToken,
}

impl EventDrivenOrchestrator {
    pub async fn new(pool: PgPool, config: OrchestrationConfig) -> Result<Self> {
        Ok(Self {
            pool: Arc::new(pool),
            coordinators: Arc::new(DashMap::new()),
            config: Arc::new(config),
            shutdown: CancellationToken::new(),
        })
    }

    pub async fn start(&self) -> Result<()> {
        // Start global listener for namespace creation
        let global_listener = self.spawn_global_listener().await?;
        
        // Start namespace-specific coordinators for existing namespaces
        self.initialize_existing_namespaces().await?;
        
        // Start fallback poller for reliability
        let fallback_poller = self.spawn_fallback_poller().await?;
        
        // Wait for shutdown
        self.shutdown.cancelled().await;
        
        Ok(())
    }

    async fn spawn_global_listener(&self) -> Result<JoinHandle<()>> {
        let mut listener = PgListener::connect_with(&*self.pool).await?;
        listener.listen("namespace_created").await?;
        listener.listen("task_ready").await?; // Global channel for all tasks
        
        let coordinators = Arc::clone(&self.coordinators);
        let pool = Arc::clone(&self.pool);
        let shutdown = self.shutdown.clone();
        
        Ok(tokio::spawn(async move {
            loop {
                tokio::select! {
                    notification = listener.recv() => {
                        match notification {
                            Ok(n) => Self::handle_global_notification(n, &coordinators, &pool).await,
                            Err(e) => {
                                error!("Global listener error: {}", e);
                                // sqlx will auto-reconnect
                            }
                        }
                    }
                    _ = shutdown.cancelled() => {
                        info!("Global listener shutting down");
                        break;
                    }
                }
            }
        }))
    }

    async fn handle_global_notification(
        notification: PgNotification,
        coordinators: &DashMap<String, NamespaceCoordinator>,
        pool: &PgPool,
    ) {
        match notification.channel() {
            "namespace_created" => {
                if let Ok(namespace) = serde_json::from_str::<NamespaceCreated>(notification.payload()) {
                    info!("New namespace detected: {}", namespace.name);
                    Self::spawn_namespace_coordinator(
                        namespace.name,
                        coordinators,
                        pool,
                    ).await;
                }
            }
            "task_ready" => {
                // Route to appropriate coordinator or handle directly
                if let Ok(event) = serde_json::from_str::<TaskReadyEvent>(notification.payload()) {
                    if let Some(coordinator) = coordinators.get(&event.namespace) {
                        coordinator.handle_task_ready(event).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn spawn_namespace_coordinator(
        namespace: String,
        coordinators: &DashMap<String, NamespaceCoordinator>,
        pool: &PgPool,
    ) {
        if !coordinators.contains_key(&namespace) {
            let coordinator = NamespaceCoordinator::new(
                namespace.clone(),
                pool.clone(),
            ).await;
            
            if let Ok(coordinator) = coordinator {
                coordinator.start().await;
                coordinators.insert(namespace, coordinator);
            }
        }
    }
}

pub struct NamespaceCoordinator {
    namespace: String,
    pool: PgPool,
    task_channel: mpsc::Sender<TaskReadyEvent>,
    worker_handles: Vec<JoinHandle<()>>,
}

impl NamespaceCoordinator {
    pub async fn new(namespace: String, pool: PgPool) -> Result<Self> {
        let (tx, rx) = mpsc::channel(1000);
        
        Ok(Self {
            namespace,
            pool,
            task_channel: tx,
            worker_handles: Vec::new(),
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        // Start namespace-specific listener
        self.spawn_namespace_listener().await?;
        
        // Start worker pool
        self.spawn_worker_pool().await?;
        
        Ok(())
    }

    async fn spawn_namespace_listener(&self) -> Result<()> {
        let mut listener = PgListener::connect_with(&self.pool).await?;
        
        // Listen to namespace-specific channels
        listener.listen(&format!("task_ready.{}", self.namespace)).await?;
        listener.listen(&format!("step_ready.{}", self.namespace)).await?;
        
        let tx = self.task_channel.clone();
        let namespace = self.namespace.clone();
        
        tokio::spawn(async move {
            while let Ok(notification) = listener.recv().await {
                if let Ok(event) = serde_json::from_str(notification.payload()) {
                    if let Err(e) = tx.send(event).await {
                        error!("Failed to send event to workers in {}: {}", namespace, e);
                    }
                }
            }
        });
        
        Ok(())
    }

    async fn spawn_worker_pool(&mut self) -> Result<()> {
        let worker_count = self.calculate_worker_count();
        let mut rx = self.task_channel.subscribe();
        
        for i in 0..worker_count {
            let pool = self.pool.clone();
            let namespace = self.namespace.clone();
            let mut rx = rx.resubscribe();
            
            let handle = tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    match Self::process_task_ready_event(event, &pool).await {
                        Ok(_) => {},
                        Err(e) => error!("Worker {} in {} failed: {}", i, namespace, e),
                    }
                }
            });
            
            self.worker_handles.push(handle);
        }
        
        Ok(())
    }

    async fn process_task_ready_event(event: TaskReadyEvent, pool: &PgPool) -> Result<()> {
        // Use the existing claim_ready_tasks SQL function for atomic claiming
        // This aligns with current orchestration loop behavior
        let claim_result = sqlx::query!(
            r#"
            WITH claimed AS (
                UPDATE tasker_tasks 
                SET claimed_at = NOW(),
                    claimed_by = $2,
                    claim_id = $3
                WHERE task_uuid = $1 
                  AND (claimed_at IS NULL OR claimed_at < (NOW() - (claim_timeout_seconds || ' seconds')::interval))
                  AND complete = false
                RETURNING task_uuid
            )
            SELECT task_uuid FROM claimed
            "#,
            event.task_uuid,
            "event_driven_coordinator", 
            format!("evt_{}", uuid::Uuid::new_v4().simple())
        )
        .fetch_optional(pool)
        .await?;

        if let Some(_) = claim_result {
            // Task claimed successfully, enqueue ready steps using existing logic
            let step_enqueuer = StepEnqueuer::new(pool.clone());
            step_enqueuer.enqueue_ready_steps_for_task(event.task_uuid).await?;
            
            // Increment metrics
            EVENT_DRIVEN_METRICS.tasks_claimed_via_event.inc();
        } else {
            // Claim failed - another coordinator got it (expected in high concurrency)
            EVENT_DRIVEN_METRICS.claim_conflicts.inc();
        }
        
        Ok(())
    }
}
```

### 3. Hybrid Reliability Pattern

To ensure no tasks are missed during coordinator downtime or pg_notify delivery failures:

```rust
pub struct FallbackPoller {
    pool: PgPool,
    interval: Duration,
    coordinators: Arc<DashMap<String, NamespaceCoordinator>>,
}

impl FallbackPoller {
    pub async fn start(&self) -> Result<()> {
        let mut interval = tokio::time::interval(self.interval);
        
        loop {
            interval.tick().await;
            
            // Query for any ready tasks older than threshold using existing tasker_ready_tasks view
            // This view already uses get_task_execution_context() for proven readiness logic
            let stale_tasks = sqlx::query!(
                r#"
                SELECT 
                    rt.task_uuid, 
                    rt.namespace, 
                    rt.priority,
                    rt.ready_steps_count,
                    rt.age_hours
                FROM tasker_ready_tasks rt
                WHERE rt.created_at < NOW() - INTERVAL '5 seconds'
                  AND rt.claim_status = 'available'
                  AND rt.execution_status = 'has_ready_steps'
                  AND rt.ready_steps_count > 0
                ORDER BY rt.computed_priority DESC
                LIMIT 100
                "#
            )
            .fetch_all(&self.pool)
            .await?;

            for task in stale_tasks {
                // Send synthetic event to coordinator
                if let Some(coordinator) = self.coordinators.get(&task.namespace.unwrap_or("default".to_string())) {
                    coordinator.handle_task_ready(TaskReadyEvent {
                        task_uuid: task.task_uuid,
                        namespace: task.namespace.unwrap_or("default".to_string()),
                        priority: task.priority.unwrap_or(0),
                        ready_steps: task.ready_steps_count.unwrap_or(0) as u32,
                        source: EventSource::FallbackPoller,
                        age_hours: task.age_hours,
                    }).await;
                    
                    // Track fallback polling metrics
                    EVENT_DRIVEN_METRICS.tasks_claimed_via_polling.inc();
                }
            }
        }
    }
}
```

### 4. Integration with TAS-35 Queue Strategies

The event-driven architecture aligns perfectly with TAS-35's message service abstraction:

```rust
// Extend QueueProvider trait for event-driven support
#[async_trait]
pub trait EventDrivenQueueProvider: QueueProvider {
    /// Check if provider supports push notifications
    fn supports_push_notifications(&self) -> bool;
    
    /// Create event stream for queue
    async fn create_event_stream(
        &self,
        queue: &str,
    ) -> Result<Box<dyn Stream<Item = Result<QueueEvent>> + Send>>;
}

// Implementation for different providers
impl EventDrivenQueueProvider for PgmqProvider {
    fn supports_push_notifications(&self) -> bool {
        true // Via pg_notify
    }
    
    async fn create_event_stream(&self, queue: &str) -> Result<Box<dyn Stream<Item = Result<QueueEvent>> + Send>> {
        let listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(&format!("pgmq.{}", queue)).await?;
        
        Ok(Box::new(listener.into_stream().map(|n| {
            Ok(QueueEvent::from_notification(n?))
        })))
    }
}

impl EventDrivenQueueProvider for RabbitMqProvider {
    fn supports_push_notifications(&self) -> bool {
        true // Native AMQP streaming
    }
    
    async fn create_event_stream(&self, queue: &str) -> Result<Box<dyn Stream<Item = Result<QueueEvent>> + Send>> {
        // Use lapin's consumer stream directly
        let consumer = self.channel
            .basic_consume(queue, "event_consumer", BasicConsumeOptions::default())
            .await?;
            
        Ok(Box::new(consumer.map(|delivery| {
            Ok(QueueEvent::from_delivery(delivery?))
        })))
    }
}
```

## Implementation Phases

### Phase 1: Database Foundation (Week 1)
- [ ] Create trigger functions for task and step transitions
- [ ] Add namespace creation trigger
- [ ] Create notification payload types
- [ ] Test trigger firing and payload structure
- [ ] Add indexes for notification queries

### Phase 2: Core Event System (Week 2)
- [ ] Implement EventDrivenOrchestrator
- [ ] Create NamespaceCoordinator with worker pools
- [ ] Integrate sqlx::PgListener
- [ ] Add metrics and logging
- [ ] Handle connection failures and reconnection

### Phase 3: Dynamic Namespace Support (Week 3)
- [ ] Implement namespace discovery on startup
- [ ] Add dynamic coordinator spawning
- [ ] Handle coordinator lifecycle (scaling, shutdown)
- [ ] Implement coordinator registry
- [ ] Add namespace-level configuration

### Phase 4: Reliability Layer (Week 4)
- [ ] Implement FallbackPoller
- [ ] Add stale task detection
- [ ] Create synthetic event injection
- [ ] Add deduplication for duplicate events
- [ ] Implement circuit breaker for fallback

### Phase 5: Queue Provider Integration (Week 5)
- [ ] Extend QueueProvider trait
- [ ] Implement event streams for PGMQ
- [ ] Implement event streams for RabbitMQ
- [ ] Add provider selection logic
- [ ] Create unified event handling

### Phase 6: Migration and Rollout (Week 6)
- [ ] Create feature flag for event-driven mode
- [ ] Implement gradual rollout strategy
- [ ] Add A/B testing metrics
- [ ] Create rollback plan
- [ ] Document operations runbook

## Performance Considerations

### PostgreSQL Limits
- **pg_notify payload**: Maximum 8000 bytes (keep payloads minimal)
- **Channel names**: Maximum 63 characters
- **Delivery guarantee**: At-most-once (hence fallback poller)
- **Queue depth**: Notifications dropped if listeners can't keep up

### Optimization Strategies

1. **Namespace Sharding**: Separate channels per namespace to distribute load
2. **Payload Minimization**: Only send IDs, fetch full data when processing
3. **Batch Processing**: Accumulate events and process in batches
4. **Connection Pooling**: Dedicated connection for LISTEN to avoid blocking
5. **Selective Listening**: Coordinators only listen to their namespaces

### Scaling Path

When pg_notify becomes a bottleneck (>10k tasks/sec):

1. **Database Sharding**: Partition by namespace across multiple databases
2. **External Message Bus**: Migrate to Kafka/Pulsar for unlimited scale
3. **Hybrid Architecture**: Critical namespaces on dedicated infrastructure
4. **Read Replicas**: Distribute notification load across replicas

## Monitoring and Observability

### Key Metrics

```rust
pub struct EventDrivenMetrics {
    // Notification metrics
    notifications_received: Counter,
    notifications_processed: Counter,
    notifications_failed: Counter,
    notification_lag: Histogram,
    
    // Coordinator metrics
    coordinators_active: Gauge,
    coordinators_spawned: Counter,
    coordinator_worker_count: Gauge,
    
    // Task claiming metrics
    tasks_claimed_via_event: Counter,
    tasks_claimed_via_polling: Counter,
    claim_conflicts: Counter,
    
    // Channel metrics
    channel_queue_depth: Gauge,
    channel_throughput: Histogram,
}
```

### Health Checks

```rust
impl HealthCheck for EventDrivenOrchestrator {
    async fn check(&self) -> HealthStatus {
        let checks = vec![
            self.check_listeners_connected().await,
            self.check_coordinators_healthy().await,
            self.check_fallback_poller().await,
            self.check_notification_lag().await,
        ];
        
        HealthStatus::aggregate(checks)
    }
}
```

## Migration Strategy

### Gradual Rollout

1. **Phase 1**: Deploy triggers and listeners in shadow mode (no processing)
2. **Phase 2**: Enable event processing for single namespace
3. **Phase 3**: A/B test event-driven vs polling for performance
4. **Phase 4**: Gradually migrate namespaces to event-driven
5. **Phase 5**: Disable polling for migrated namespaces
6. **Phase 6**: Full production rollout

### Rollback Plan

```rust
pub struct OrchestrationMode {
    mode: Mode,
    fallback_enabled: bool,
}

pub enum Mode {
    PollingOnly,        // Original behavior
    EventDrivenOnly,    // New behavior  
    Hybrid {            // Transition phase
        event_namespaces: HashSet<String>,
        polling_namespaces: HashSet<String>,
    },
}
```

## Future Enhancements

### Dynamic Namespace Discovery
When workers declare new namespaces at runtime:

1. **Registration API**: Workers register namespace before first use
2. **Lazy Coordinator Spawning**: Create coordinator on first task
3. **Coordinator Pooling**: Pre-spawn generic coordinators and assign
4. **Namespace Templates**: Configuration inheritance for new namespaces

### Advanced Event Routing

```rust
pub struct EventRouter {
    rules: Vec<RoutingRule>,
    default_handler: Box<dyn EventHandler>,
}

pub struct RoutingRule {
    pattern: NamespacePattern,
    priority_threshold: Option<i32>,
    handler: Box<dyn EventHandler>,
}
```

### PGMQ Extension for Queue-Level Events

Build a custom PostgreSQL extension to hook into PGMQ queue operations:

```sql
-- PGMQ extension hook for queue creation
CREATE OR REPLACE FUNCTION pgmq_queue_created_notify() RETURNS event_trigger AS $$
BEGIN
    IF TG_TAG = 'CREATE TABLE' AND NEW.schemaname = 'pgmq' THEN
        PERFORM pg_notify('pgmq_queue_created', json_build_object(
            'queue_name', TG_TABLE_NAME,
            'created_at', NOW(),
            'namespace', CASE 
                WHEN TG_TABLE_NAME LIKE '%_queue' THEN 
                    substring(TG_TABLE_NAME from '^(.+)_queue$')
                ELSE 'default'
            END
        )::text);
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE EVENT TRIGGER pgmq_queue_creation_trigger
    ON ddl_command_end
    WHEN TAG IN ('CREATE TABLE')
    EXECUTE FUNCTION pgmq_queue_created_notify();

-- Hook into PGMQ message enqueueing
CREATE OR REPLACE FUNCTION pgmq_message_enqueued_notify() RETURNS TRIGGER AS $$
DECLARE
    queue_name TEXT;
    namespace TEXT;
BEGIN
    -- Extract queue name from table name
    queue_name := TG_TABLE_NAME;
    namespace := CASE 
        WHEN queue_name LIKE '%_queue' THEN 
            substring(queue_name from '^(.+)_queue$')
        ELSE 'default'
    END;
    
    -- Notify with minimal payload to avoid overwhelming pg_notify
    PERFORM pg_notify('pgmq_message_ready.' || namespace, json_build_object(
        'msg_id', NEW.msg_id,
        'queue', queue_name,
        'enqueued_at', NEW.enqueued_at
    )::text);
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply to all existing PGMQ queues
DO $$
DECLARE
    queue_record RECORD;
BEGIN
    FOR queue_record IN 
        SELECT schemaname, tablename 
        FROM pg_tables 
        WHERE schemaname = 'pgmq' 
          AND tablename NOT LIKE 'pgmq_%'
    LOOP
        EXECUTE format('CREATE TRIGGER pgmq_enqueue_notify_%s
            AFTER INSERT ON pgmq.%I
            FOR EACH ROW
            EXECUTE FUNCTION pgmq_message_enqueued_notify()', 
            queue_record.tablename, queue_record.tablename);
    END LOOP;
END;
$$;
```

This enables true event-driven PGMQ integration:
- Queue creation notifications spawn new coordinators automatically
- Message enqueue events trigger immediate processing
- Namespace extraction from queue names maintains coordination model
- Minimal payload overhead to avoid pg_notify limits

### Event Sourcing Integration

Store all state transitions as events for:
- Complete audit trail
- Time-travel debugging  
- Event replay for testing
- CQRS pattern implementation

## Success Criteria

1. **Latency**: <10ms from state change to task claiming (vs 500ms avg today)
2. **Throughput**: 10x increase in tasks/second per coordinator
3. **Database Load**: 90% reduction in polling queries
4. **Reliability**: No task lost during coordinator restarts
5. **Scalability**: Linear scaling with namespace sharding

## Dependencies

- TAS-35: Message service abstraction (for queue provider traits)
- TAS-40: Worker foundations (for simplified worker model)
- TAS-41: Pure Rust workers (for event-driven step processing)

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| pg_notify overwhelming database | High | Namespace sharding, payload minimization |
| Notification loss during downtime | High | Fallback poller, at-least-once processing |
| Complex debugging | Medium | Comprehensive logging, event replay tooling |
| Migration complexity | Medium | Feature flags, gradual rollout, rollback plan |
| Dynamic namespace spawning overhead | Low | Coordinator pooling, lazy initialization |

## Conclusion

Moving from polling to event-driven architecture with pg_notify will dramatically improve system responsiveness and efficiency. The hybrid approach with fallback polling ensures reliability, while integration with TAS-35's queue abstractions provides a path to even more sophisticated event-driven patterns with external message systems.

The key innovation is using PostgreSQL itself as the event bus initially, which keeps the architecture simple while delivering most of the benefits. As scale demands grow, the abstraction layers allow seamless migration to dedicated message infrastructure.
