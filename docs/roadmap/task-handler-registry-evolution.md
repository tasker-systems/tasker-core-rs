# TaskHandlerRegistry Evolution: From In-Memory to Database-Backed Architecture

## Executive Summary

This document outlines the evolution of Tasker's task handler registration system from an in-memory registry designed for monolithic applications to a database-backed system supporting distributed workers. This evolution addresses fundamental architectural conflicts between single-process assumptions and the reality of distributed TCP/Unix socket workers.

## Current Architecture Problems

### 1. Conflicting Registration Systems

**In-Memory TaskHandlerRegistry** (`src/registry/task_handler_registry.rs`)
- Designed for single-process, bootstrap-time registration
- Stores handler configurations in memory
- Lost on process restart
- No awareness of distributed workers

**WorkerManagementHandler** (`src/execution/command_handlers/worker_management_handler.rs`)
- Tracks TCP-connected workers and capabilities
- Also stores configurations in memory
- Duplicates registry functionality
- No persistence across restarts

### 2. Configuration Management Issues
- YAML configurations stored with handlers, not workers
- No association between specific workers and their configurations
- Configuration changes require code deployment
- No ability to have different configs for different workers

### 3. Namespace Confusion
- Workers register with "supported namespaces" (strings)
- TaskHandlers register with namespace/name/version tuples
- Matching logic is fragile and error-prone
- No explicit association between workers and the exact tasks they support

### 4. Debugging and Observability Challenges
- Cannot query which workers are available for a task
- No historical view of worker health
- Lost registration history on restart
- Difficult to diagnose "no workers available" errors

## Proposed Database-Backed Architecture

### Core Design Principles

1. **Explicit Associations**: Workers explicitly declare which `named_tasks` they support
2. **Configuration Per Association**: Each worker-task pair has its own configuration
3. **Health Tracking**: Persistent tracking of worker status and availability
4. **Multi-Worker Support**: Multiple workers can register for the same task
5. **Historical Visibility**: Full audit trail of registrations and health status

### Database Schema

```sql
-- Table: workers
-- Purpose: Core worker identity and metadata
CREATE TABLE workers (
    id SERIAL PRIMARY KEY,
    worker_id VARCHAR(255) UNIQUE NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for workers
CREATE INDEX idx_workers_worker_id ON workers(worker_id);

-- Comments
COMMENT ON TABLE workers IS 'Registered worker instances that can execute tasks';
COMMENT ON COLUMN workers.worker_id IS 'Unique identifier for the worker (e.g., payment_processor_1)';
COMMENT ON COLUMN workers.metadata IS 'Worker metadata: version, runtime, custom capabilities';

-- Table: worker_named_tasks
-- Purpose: Associates workers with tasks they can handle, including configuration
CREATE TABLE worker_named_tasks (
    id SERIAL PRIMARY KEY,
    worker_id INTEGER NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    named_task_id INTEGER NOT NULL REFERENCES tasker_named_tasks(named_task_id),
    configuration JSONB NOT NULL DEFAULT '{}',
    priority INTEGER DEFAULT 100,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(worker_id, named_task_id)
);

-- Indexes for worker_named_tasks
CREATE INDEX idx_worker_named_tasks_worker ON worker_named_tasks(worker_id);
CREATE INDEX idx_worker_named_tasks_task ON worker_named_tasks(named_task_id);
CREATE INDEX idx_worker_named_tasks_priority ON worker_named_tasks(priority DESC);

-- Comments
COMMENT ON TABLE worker_named_tasks IS 'Maps workers to the named tasks they support with configuration';
COMMENT ON COLUMN worker_named_tasks.configuration IS 'Task-specific configuration for this worker (converted from YAML)';
COMMENT ON COLUMN worker_named_tasks.priority IS 'Priority for worker selection (higher = preferred)';

-- Table: worker_registrations
-- Purpose: Track worker connection status and health
CREATE TABLE worker_registrations (
    id SERIAL PRIMARY KEY,
    worker_id INTEGER NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    status VARCHAR(50) NOT NULL CHECK (status IN ('registered', 'healthy', 'unhealthy', 'disconnected')),
    connection_type VARCHAR(50) NOT NULL,
    connection_details JSONB NOT NULL DEFAULT '{}',
    last_heartbeat_at TIMESTAMP,
    registered_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    unregistered_at TIMESTAMP,
    failure_reason TEXT
);

-- Indexes for worker_registrations
CREATE INDEX idx_worker_registrations_active ON worker_registrations(worker_id, status, unregistered_at);
CREATE INDEX idx_worker_registrations_heartbeat ON worker_registrations(last_heartbeat_at) WHERE unregistered_at IS NULL;

-- Comments
COMMENT ON TABLE worker_registrations IS 'Tracks worker registration lifecycle and health status';
COMMENT ON COLUMN worker_registrations.status IS 'Current worker status: registered, healthy, unhealthy, disconnected';
COMMENT ON COLUMN worker_registrations.connection_type IS 'Connection type: tcp, unix, etc.';
COMMENT ON COLUMN worker_registrations.connection_details IS 'Connection info: host, port, listener_port, etc.';
```

### Data Model Relationships

```
workers (1) ----< (N) worker_named_tasks
   |                           |
   |                           v
   |                    tasker_named_tasks
   |
   v
workers (1) ----< (N) worker_registrations
```

## Implementation Plan

### Phase 1: Complete Architecture Replacement (Week 1)

> **Note**: This is a clean-slate rewrite on a new branch. No backward compatibility or migration concerns - we can aggressively implement the optimal architecture.

#### 1.1 Create Database Migrations
```bash
# Create migration files
migrations/20250128_create_workers_tables.sql
migrations/20250128_drop_legacy_registry_state.sql  # Remove any in-memory state persistence
```

#### 1.2 Implement Rust Models
```rust
// src/models/core/worker.rs
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Worker {
    pub id: i32,
    pub worker_id: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// src/models/core/worker_named_task.rs
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerNamedTask {
    pub id: i32,
    pub worker_id: i32,
    pub named_task_id: i32,
    pub configuration: serde_json::Value,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

// src/models/core/worker_registration.rs
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerRegistration {
    pub id: i32,
    pub worker_id: i32,
    pub status: WorkerStatus,
    pub connection_type: String,
    pub connection_details: serde_json::Value,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub registered_at: DateTime<Utc>,
    pub unregistered_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum WorkerStatus {
    Registered,
    Healthy,
    Unhealthy,
    Disconnected,
}
```

#### 1.3 Create Repository Layer
```rust
// src/repositories/worker_repository.rs
impl WorkerRepository {
    pub async fn create_or_update(&self, worker_id: &str, metadata: Value) -> Result<Worker>;
    pub async fn find_by_worker_id(&self, worker_id: &str) -> Result<Option<Worker>>;
    pub async fn list_active(&self) -> Result<Vec<Worker>>;
}

// src/repositories/worker_registration_repository.rs
impl WorkerRegistrationRepository {
    pub async fn register(&self, worker_id: i32, connection: ConnectionInfo) -> Result<WorkerRegistration>;
    pub async fn update_heartbeat(&self, registration_id: i32) -> Result<()>;
    pub async fn mark_disconnected(&self, registration_id: i32, reason: &str) -> Result<()>;
    pub async fn find_active_for_worker(&self, worker_id: i32) -> Result<Option<WorkerRegistration>>;
}
```

### Phase 2: Database-First Worker Registration (Week 2)

#### 2.1 New RegisterWorker Command (Replace Existing)
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterWorkerV2 {
    pub worker_id: String,
    pub metadata: WorkerMetadata,
    pub supported_tasks: Vec<SupportedTask>,
    pub connection_info: ConnectionInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SupportedTask {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub configuration: serde_json::Value, // YAML converted to JSON
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub listener_port: Option<u16>,
    pub host: String,
    pub protocol_version: String,
}
```

#### 2.2 Replace WorkerManagementHandler (Database-Only)
```rust
impl WorkerManagementHandler {
    async fn handle_register(&self, cmd: RegisterWorker) -> Result<WorkerRegistrationResponse> {
        // 1. Create/update worker record
        let worker = self.worker_repo.create_or_update(&cmd.worker_id, cmd.metadata).await?;
        
        // 2. Register supported tasks
        for task in cmd.supported_tasks {
            let named_task = self.named_task_repo.find_or_create(
                &task.namespace, 
                &task.name, 
                &task.version
            ).await?;
            
            self.worker_task_repo.create_or_update(
                worker.id,
                named_task.id,
                task.configuration
            ).await?;
        }
        
        // 3. Create registration record
        let registration = self.registration_repo.register(
            worker.id,
            cmd.connection_info
        ).await?;
        
        // 4. NO in-memory state - database is source of truth
        
        Ok(WorkerRegistrationResponse::success(worker.worker_id))
    }
    
    async fn handle_heartbeat(&self, worker_id: String) -> Result<HeartbeatResponse> {
        let worker = self.worker_repo.find_by_worker_id(&worker_id)?
            .ok_or_else(|| Error::WorkerNotFound)?;
            
        let registration = self.registration_repo.find_active_for_worker(worker.id)?
            .ok_or_else(|| Error::WorkerNotRegistered)?;
            
        self.registration_repo.update_heartbeat(registration.id)?;
        
        Ok(HeartbeatResponse::acknowledged())
    }
}
```

### Phase 3: Worker Selection Service (Week 3)

#### 3.1 Create WorkerSelectionService
```rust
// src/services/worker_selection_service.rs
pub struct WorkerSelectionService {
    db_pool: PgPool,
}

impl WorkerSelectionService {
    pub async fn find_available_worker(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<SelectedWorker>> {
        let query = r#"
            SELECT 
                w.worker_id,
                w.metadata as worker_metadata,
                wr.connection_details,
                wr.connection_type,
                wnt.configuration as task_configuration,
                wnt.priority
            FROM workers w
            INNER JOIN worker_registrations wr ON w.id = wr.worker_id
            INNER JOIN worker_named_tasks wnt ON w.id = wnt.worker_id
            INNER JOIN tasker_named_tasks nt ON wnt.named_task_id = nt.named_task_id
            WHERE nt.namespace = $1
              AND nt.name = $2
              AND nt.version = $3
              AND wr.status = 'healthy'
              AND wr.unregistered_at IS NULL
              AND wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes'
            ORDER BY 
                wnt.priority DESC,
                wr.last_heartbeat_at DESC
            LIMIT 1
        "#;
        
        let result = sqlx::query_as::<_, SelectedWorker>(query)
            .bind(namespace)
            .bind(name)
            .bind(version)
            .fetch_optional(&self.db_pool)
            .await?;
            
        Ok(result)
    }
    
    pub async fn list_workers_for_task(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<Vec<WorkerInfo>> {
        // Similar query but returns all matching workers
    }
}
```

#### 3.2 Replace BatchExecutionSender (Database-Only)
```rust
impl BatchExecutionSender {
    pub async fn send_batch_for_execution(&self, batch: &StepExecutionBatch) -> Result<()> {
        // Get task template for namespace/name/version
        let task_template = self.get_task_template(batch).await?;
        
        // Use WorkerSelectionService ONLY - no fallback to in-memory
        let selected_worker = self.worker_selection_service
            .find_available_worker(
                &task_template.namespace,
                &task_template.name,
                &task_template.version,
            )
            .await?
            .ok_or_else(|| Error::NoWorkersAvailable {
                namespace: task_template.namespace.clone(),
                name: task_template.name.clone(),
                version: task_template.version.clone(),
            })?;
            
        // Build ExecuteBatch command with worker's specific configuration
        let command = self.build_execute_batch_command(
            batch,
            task_template,
            &selected_worker.task_configuration,
        )?;
        
        // Send to worker using connection details
        self.send_to_worker(&selected_worker, command).await?;
        
        Ok(())
    }
}
```

### Phase 4: Remove Legacy Components (Week 4)

#### 4.1 Delete Old Architecture Files
```bash
# Remove legacy files completely
rm src/registry/task_handler_registry.rs
rm src/registry/mod.rs  # Update to remove task_handler_registry module

# Update imports across codebase to remove TaskHandlerRegistry references
# Replace with WorkerSelectionService everywhere
```

#### 4.2 Clean Up Configuration
```bash
# Remove in-memory registry configuration
# Update config/tasker-config.yaml to remove registry-related settings
# Keep only database and worker selection configurations
```

#### 4.3 Update Integration Points
- Remove all TaskHandlerRegistry.register() calls
- Update Ruby FFI to only use worker registration
- Remove handler_exists() and list_handlers() FFI methods that query in-memory registry

## Ruby Worker Updates

### Completely New Worker Registration (Replace Existing)
```ruby
class WorkerManager
  def register_with_tasks
    supported_tasks = discover_supported_tasks
    
    registration_data = {
      worker_id: @worker_id,
      metadata: {
        version: TaskerCore::VERSION,
        ruby_version: RUBY_VERSION,
        hostname: Socket.gethostname,
        pid: Process.pid,
        started_at: Time.now.iso8601
      },
      supported_tasks: supported_tasks.map do |task|
        {
          namespace: task[:namespace],
          name: task[:name],
          version: task[:version],
          configuration: load_task_configuration(task)
        }
      end,
      connection_info: {
        listener_port: @listener_port,
        host: discover_host_ip,
        protocol_version: '2.0',
        supported_commands: ['execute_batch', 'health_check', 'shutdown']
      }
    }
    
    # Only one registration method - no fallback
    @command_client.register_worker(registration_data)
  end
  
  private
  
  def discover_supported_tasks
    # Completely replace old TaskHandler scanning
    # Scan filesystem for YAML configs directly
    config_files = Dir.glob("config/tasker/**/*.yaml")
    
    config_files.map do |config_path|
      config = YAML.load_file(config_path)
      {
        namespace: config['namespace_name'],
        name: config['name'],
        version: config['version'] || '1.0.0'
      }
    end
  end
  
  def load_task_configuration(task)
    # Load YAML config and convert to JSON for database storage
    config_path = "config/tasker/#{task[:namespace]}/#{task[:name]}.yaml"
    if File.exist?(config_path)
      YAML.load_file(config_path)
    else
      {}  # Empty config if file not found
    end
  end
  
  def discover_host_ip
    # Better host discovery for container environments
    ENV['WORKER_HOST'] || '0.0.0.0'
  end
end
```

## Monitoring and Observability

### Key Metrics
1. **Worker Health**
   - Active workers per namespace
   - Average heartbeat latency
   - Registration churn rate

2. **Task Coverage**
   - Tasks with no available workers
   - Tasks with single worker (no redundancy)
   - Worker distribution across tasks

3. **Performance**
   - Worker selection query time
   - Registration processing time
   - Database query performance

### Useful Queries

```sql
-- Current worker availability by task
SELECT 
    nt.namespace,
    nt.name,
    nt.version,
    COUNT(DISTINCT w.id) as available_workers,
    ARRAY_AGG(w.worker_id) as worker_ids
FROM tasker_named_tasks nt
LEFT JOIN worker_named_tasks wnt ON nt.named_task_id = wnt.named_task_id
LEFT JOIN workers w ON wnt.worker_id = w.id
LEFT JOIN worker_registrations wr ON w.id = wr.worker_id
WHERE wr.status = 'healthy'
  AND wr.unregistered_at IS NULL
  AND wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes'
GROUP BY nt.namespace, nt.name, nt.version
ORDER BY available_workers ASC;

-- Worker registration history
SELECT 
    w.worker_id,
    wr.status,
    wr.registered_at,
    wr.unregistered_at,
    wr.failure_reason,
    EXTRACT(EPOCH FROM (COALESCE(wr.unregistered_at, NOW()) - wr.registered_at)) as duration_seconds
FROM workers w
JOIN worker_registrations wr ON w.id = wr.worker_id
WHERE w.worker_id = ?
ORDER BY wr.registered_at DESC;

-- Tasks without workers (coverage gaps)
SELECT 
    nt.namespace,
    nt.name,
    nt.version,
    nt.created_at as task_created
FROM tasker_named_tasks nt
WHERE NOT EXISTS (
    SELECT 1 
    FROM worker_named_tasks wnt
    JOIN workers w ON wnt.worker_id = w.id
    JOIN worker_registrations wr ON w.id = wr.worker_id
    WHERE wnt.named_task_id = nt.named_task_id
      AND wr.status = 'healthy'
      AND wr.unregistered_at IS NULL
);
```

## Benefits of New Architecture

1. **Persistence**: Worker registrations survive system restarts
2. **Explicit Associations**: No more namespace guessing - workers explicitly declare supported tasks
3. **Configuration Flexibility**: Different workers can have different configs for same task
4. **Load Balancing**: Built-in support for multiple workers per task with priority
5. **Debugging**: Complete visibility into worker availability and history
6. **Gradual Migration**: Can run both systems during transition
7. **Standards Compliance**: Follows established patterns from job queue systems

## Success Criteria

1. **Functional Requirements**
   - [ ] Workers can register with specific task support
   - [ ] Registration persists across restarts
   - [ ] Task execution finds appropriate workers
   - [ ] Health tracking updates in real-time

2. **Performance Requirements**
   - [ ] Worker selection query < 10ms
   - [ ] Registration processing < 50ms
   - [ ] Heartbeat update < 5ms

3. **Operational Requirements**
   - [ ] Zero downtime migration from old system
   - [ ] Monitoring dashboards available
   - [ ] Runbook for common issues

## Risks and Mitigations

1. **Risk**: Performance degradation from database queries
   - **Mitigation**: Proper indexes, connection pooling, caching layer if needed

2. **Risk**: Migration failures losing worker state
   - **Mitigation**: Dual-write period, extensive testing, rollback plan

3. **Risk**: Increased complexity for developers
   - **Mitigation**: Clear documentation, helper methods, migration guide

## Timeline

- **Week 1**: Database schema and models
- **Week 2**: Worker registration implementation  
- **Week 3**: Worker selection service
- **Week 4**: Migration and monitoring
- **Week 5**: Documentation and training
- **Week 6**: Production rollout

## Conclusion

This evolution from in-memory to database-backed worker registration resolves fundamental architectural conflicts while providing a robust foundation for distributed task execution. The new system offers better persistence, visibility, and flexibility while maintaining backward compatibility during migration.