# Queue Lifecycle Management

**Last Updated**: 2026-01-07

This document examines how queue creation/management integrates with Tasker's task template discovery, and considerations for RabbitMQ.

---

## Current Architecture: PGMQ

### Queue Creation Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    TASKER WORKER BOOTSTRAP                               │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   1. Load Task Templates from Disk                                       │
│      └── config/tasker/task_templates/*.yaml                            │
│                                                                          │
│   2. TaskTemplateManager parses templates                                │
│      └── Extracts unique namespaces: [fulfillment, inventory, ...]      │
│                                                                          │
│   3. TaskHandlerRegistry registers handlers                              │
│      └── Maps handler_key → handler implementation                       │
│                                                                          │
│   4. Messaging Service creates namespace queues                          │
│      └── For each namespace: pgmq.create("{namespace}_queue")           │
│                                                                          │
│   5. pg_notify listeners established                                     │
│      └── LISTEN pgmq_message_ready.{namespace}_queue                    │
│                                                                          │
│   6. Worker ready to process                                             │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### PGMQ Queue Creation Characteristics

```rust
// PGMQ queue creation is idempotent and transactional
pub async fn ensure_namespace_queues(
    &self,
    namespaces: &[String],
) -> Result<(), MessagingError> {
    for namespace in namespaces {
        let queue_name = format!("{}_queue", namespace);
        
        // This is just a SQL call - same connection pool, same transaction semantics
        // CREATE TABLE IF NOT EXISTS pgmq.q_{queue_name} ...
        self.pgmq_client.create(&queue_name).await?;
    }
    Ok(())
}
```

**Key Properties**:
| Property | PGMQ Behavior |
|----------|---------------|
| Idempotency | ✅ `CREATE IF NOT EXISTS` |
| Transactional | ✅ Same PostgreSQL transaction |
| Connection | ✅ Uses existing DB pool |
| Latency | ~1-5ms per queue |
| Failure mode | DB connection failure |

---

## RabbitMQ Considerations

### Queue Declaration Semantics

RabbitMQ uses `queue_declare` which is also idempotent, but with important differences:

```rust
// RabbitMQ queue declaration
pub async fn ensure_namespace_queues(
    &self,
    namespaces: &[String],
) -> Result<(), MessagingError> {
    let channel = self.get_channel().await?;
    
    for namespace in namespaces {
        let queue_name = format!("{}_queue", namespace);
        
        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,      // Survive broker restart
                    exclusive: false,   // Allow multiple consumers
                    auto_delete: false, // Don't delete when consumers disconnect
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| MessagingError::QueueCreation(e.to_string()))?;
    }
    Ok(())
}
```

### Key Differences from PGMQ

| Aspect | PGMQ | RabbitMQ |
|--------|------|----------|
| **Connection** | PostgreSQL pool (shared) | Separate AMQP connection |
| **Channel** | N/A | Requires channel per operation set |
| **Durability** | Inherent (PostgreSQL) | Must specify `durable: true` |
| **Declaration conflict** | N/A | Fails if queue exists with different properties |
| **Startup dependency** | PostgreSQL up | RabbitMQ broker up |

### Declaration Conflict Risk

RabbitMQ will **error** if you declare a queue with different properties than an existing queue:

```rust
// First declaration
channel.queue_declare("fulfillment_queue", QueueDeclareOptions {
    durable: true,
    ..Default::default()
}).await?;

// Later declaration with different properties - THIS FAILS
channel.queue_declare("fulfillment_queue", QueueDeclareOptions {
    durable: false,  // Conflict!
    ..Default::default()
}).await?;  // Error: PRECONDITION_FAILED
```

**Mitigation**: Always use consistent queue options, ideally from configuration.

---

## Trait Design for Queue Lifecycle

### Current Trait Method

```rust
#[async_trait]
pub trait MessagingService: Send + Sync + 'static {
    /// Create a queue if it doesn't exist
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError>;
}
```

### Enhanced for Bulk Creation

```rust
#[async_trait]
pub trait MessagingService: Send + Sync + 'static {
    /// Create a queue if it doesn't exist
    async fn ensure_queue(&self, queue_name: &str) -> Result<(), MessagingError>;
    
    /// Bulk queue creation (providers may optimize)
    /// 
    /// Called during worker bootstrap after task template discovery.
    /// Default implementation calls ensure_queue in a loop.
    async fn ensure_queues(&self, queue_names: &[String]) -> Result<(), MessagingError> {
        for queue_name in queue_names {
            self.ensure_queue(queue_name).await?;
        }
        Ok(())
    }
    
    /// Verify all expected queues exist (health check)
    async fn verify_queues(&self, queue_names: &[String]) -> Result<QueueHealthReport, MessagingError>;
}

#[derive(Debug)]
pub struct QueueHealthReport {
    pub healthy: Vec<String>,
    pub missing: Vec<String>,
    pub errors: Vec<(String, String)>,
}
```

### Provider-Specific Optimizations

**PGMQ**: Could use a single transaction for all queue creations:
```rust
impl MessagingService for PgmqMessagingService {
    async fn ensure_queues(&self, queue_names: &[String]) -> Result<(), MessagingError> {
        let mut tx = self.pool.begin().await?;
        for queue_name in queue_names {
            sqlx::query("SELECT pgmq.create($1)")
                .bind(queue_name)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
```

**RabbitMQ**: Reuse single channel for all declarations:
```rust
impl MessagingService for RabbitMqMessagingService {
    async fn ensure_queues(&self, queue_names: &[String]) -> Result<(), MessagingError> {
        let channel = self.get_channel().await?;
        for queue_name in queue_names {
            channel.queue_declare(queue_name, self.default_queue_options(), FieldTable::default()).await?;
        }
        Ok(())
    }
}
```

---

## Integration with Task Template Discovery

### Current Flow (Conceptual)

```rust
// In worker bootstrap
pub async fn bootstrap_worker(config: &WorkerConfig) -> Result<WorkerHandle> {
    // 1. Load task templates
    let template_manager = TaskTemplateManager::load_from_disk(&config.template_path)?;
    
    // 2. Extract namespaces
    let namespaces: HashSet<String> = template_manager
        .templates()
        .map(|t| t.namespace.clone())
        .collect();
    
    // 3. Create messaging service
    let messaging = MessagingServiceFactory::create(&config.messaging).await?;
    
    // 4. Ensure queues exist for all namespaces
    let queue_names: Vec<String> = namespaces
        .iter()
        .map(|ns| format!("{}_queue", ns))
        .collect();
    
    messaging.ensure_queues(&queue_names).await?;
    
    // 5. Set up push listeners (if supported)
    if let Some(push_service) = messaging.as_push_capable() {
        for queue_name in &queue_names {
            push_service.subscribe(queue_name).await?;
        }
    }
    
    // 6. Ready
    Ok(WorkerHandle { messaging, template_manager, ... })
}
```

### RabbitMQ-Specific Considerations

#### 1. Exchange Topology (Optional Enhancement)

RabbitMQ could use a topic exchange for more flexible routing:

```
                    ┌─────────────────────┐
                    │  tasker.steps       │  (topic exchange)
                    │  (topic exchange)   │
                    └─────────┬───────────┘
                              │
           ┌──────────────────┼──────────────────┐
           │                  │                  │
           ▼                  ▼                  ▼
    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
    │ fulfillment  │  │  inventory   │  │  payments    │
    │    _queue    │  │    _queue    │  │    _queue    │
    └──────────────┘  └──────────────┘  └──────────────┘
    
    Routing keys:
    - fulfillment.* → fulfillment_queue
    - inventory.*   → inventory_queue
    - payments.*    → payments_queue
```

**Pros**: More flexible routing, easier to add new namespaces
**Cons**: Added complexity, may not be needed for Tasker's use case

#### 2. Startup Order Dependencies

```
PGMQ:
  PostgreSQL up → Worker starts → Queues created (same system)

RabbitMQ:
  PostgreSQL up ─┐
                 ├→ Worker starts → Queues created
  RabbitMQ up ───┘
                 
  (Two infrastructure dependencies)
```

**Mitigation**: Health checks, retry logic, clear error messages about which dependency is missing.

#### 3. Queue Property Consistency

Task templates could define queue properties:

```yaml
# config/tasker/task_templates/fulfillment.yaml
namespace: fulfillment
queue_options:
  # These would be used for RabbitMQ queue declaration
  durable: true
  max_priority: 10  # If using priority queues
  dead_letter_exchange: "tasker.dlx"
```

---

## Research Questions

1. **Should queue creation be part of MessagingService trait or separate?**
   - Current: Part of trait
   - Alternative: Separate `QueueManager` trait

2. **How to handle queue property mismatches?**
   - Fail fast on startup?
   - Log warning and continue?
   - Auto-migrate (dangerous)?

3. **Dynamic namespace discovery?**
   - Currently: All namespaces known at startup
   - Future: Hot-reload task templates → create new queues?

4. **RabbitMQ exchange topology**
   - Simple: Direct to queue (like PGMQ)
   - Advanced: Topic exchange for routing flexibility
   - Recommendation: Start simple, enhance if needed

---

## Recommendations

1. **Add `ensure_queues` bulk method** to trait (with default implementation)
2. **Add `verify_queues` health check** for operational visibility
3. **Keep simple direct-to-queue model** initially (no exchanges)
4. **Document queue property requirements** in configuration
5. **Add startup health check** that verifies all expected queues exist
