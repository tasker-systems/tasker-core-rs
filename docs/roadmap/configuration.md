# Configuration Management Strategy

**Eliminating Hardcoded Values for Production Deployment**

## Overview

Configuration management is critical for Phase 2 (Week 2) to eliminate hardcoded values that prevent production deployment. The current codebase has numerous hardcoded timeouts, limits, paths, and settings that must be externalized.

## Current Hardcoded Values

### Critical Configuration Issues

#### Timeout Values
- `task_handler.rs:397` - Hardcoded 300 second timeout: `timeout_seconds: 300`
- Multiple location with default timeouts scattered throughout

#### System Limits  
- `workflow_coordinator.rs:116` - Missing `max_discovery_attempts` configuration
- `workflow_coordinator.rs:125` - Missing `step_batch_size` configuration
- Various concurrency limits hardcoded

#### Path Configuration
- `handlers.rs:709` - Hardcoded task config directory path resolution
- Template and YAML file paths not configurable

#### Database Settings
- `handlers.rs:607` - Hardcoded `dependent_system_id = 1`
- Connection pool settings not configurable
- Query timeout values hardcoded

## Configuration Architecture

### YAML Configuration Files

#### Primary Configuration: `config/tasker-core.yaml`
```yaml
# Core orchestration settings
orchestration:
  max_parallel_steps: 10
  step_batch_size: 50
  max_discovery_attempts: 3
  step_timeout_seconds: 300
  task_timeout_seconds: 3600

# Database configuration
database:
  pool_size: 10
  connection_timeout_seconds: 30
  query_timeout_seconds: 60
  
# Event publishing
events:
  buffer_size: 1000
  batch_size: 100
  flush_interval_ms: 1000

# Task handler configuration
task_handlers:
  config_directory: "config/tasker/tasks"
  default_dependent_system_id: 1
  registry_cache_ttl_seconds: 300

# Backoff and retry configuration
backoff:
  initial_delay_ms: 1000
  max_delay_ms: 300000
  multiplier: 2.0
  jitter_factor: 0.1
  
# Framework integration
framework:
  ruby:
    step_handler_timeout_seconds: 600
    max_concurrent_steps: 5
  
# System health monitoring
monitoring:
  health_check_interval_seconds: 30
  metrics_retention_hours: 24
```

#### Environment-Specific Overrides: `config/environments/`
```yaml
# config/environments/development.yaml
database:
  pool_size: 2
orchestration:
  max_parallel_steps: 2

# config/environments/production.yaml  
database:
  pool_size: 50
orchestration:
  max_parallel_steps: 20
monitoring:
  health_check_interval_seconds: 10
```

### Configuration Management Implementation

#### Configuration Manager Enhancement
```rust
// src/orchestration/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskerCoreConfig {
    pub orchestration: OrchestrationConfig,
    pub database: DatabaseConfig,
    pub events: EventConfig,
    pub task_handlers: TaskHandlerConfig,
    pub backoff: BackoffConfig,
    pub framework: FrameworkConfig,
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub max_parallel_steps: usize,
    pub step_batch_size: usize,
    pub max_discovery_attempts: u32,
    pub step_timeout_seconds: u64,
    pub task_timeout_seconds: u64,
}

impl ConfigurationManager {
    pub fn load() -> Result<TaskerCoreConfig, ConfigurationError> {
        let mut settings = Config::builder();
        
        // Load base configuration
        settings = settings.add_source(
            config::File::with_name("config/tasker-core")
                .required(true)
        );
        
        // Load environment-specific overrides
        if let Ok(env) = std::env::var("TASKER_ENV") {
            settings = settings.add_source(
                config::File::with_name(&format!("config/environments/{}", env))
                    .required(false)
            );
        }
        
        // Environment variable overrides
        settings = settings.add_source(
            config::Environment::with_prefix("TASKER")
                .prefix_separator("_")
                .separator("__")
        );
        
        let config: TaskerCoreConfig = settings.build()?.try_deserialize()?;
        
        // Validate configuration
        config.validate()?;
        
        Ok(config)
    }
}
```

### Environment Variable Support

#### Environment Variable Mapping
```bash
# Database configuration
TASKER_DATABASE__POOL_SIZE=20
TASKER_DATABASE__CONNECTION_TIMEOUT_SECONDS=45

# Orchestration settings  
TASKER_ORCHESTRATION__MAX_PARALLEL_STEPS=15
TASKER_ORCHESTRATION__STEP_TIMEOUT_SECONDS=600

# Event publishing
TASKER_EVENTS__BUFFER_SIZE=2000
TASKER_EVENTS__BATCH_SIZE=200

# Task handler configuration
TASKER_TASK_HANDLERS__CONFIG_DIRECTORY=/app/config/tasker/tasks
TASKER_TASK_HANDLERS__DEFAULT_DEPENDENT_SYSTEM_ID=2
```

#### Docker Environment Support
```dockerfile
# Environment variables for containerized deployment
ENV TASKER_DATABASE__POOL_SIZE=25
ENV TASKER_ORCHESTRATION__MAX_PARALLEL_STEPS=20
ENV TASKER_TASK_HANDLERS__CONFIG_DIRECTORY=/app/config/tasker/tasks
ENV TASKER_EVENTS__BUFFER_SIZE=5000
```

### Configuration Usage Patterns

#### Orchestration Components
```rust
// WorkflowCoordinator using configuration
impl WorkflowCoordinator {
    pub fn new(pool: PgPool) -> Result<Self, OrchestrationError> {
        let config = ConfigurationManager::load()?;
        
        Ok(Self {
            pool,
            max_parallel_steps: config.orchestration.max_parallel_steps,
            step_batch_size: config.orchestration.step_batch_size,
            max_discovery_attempts: config.orchestration.max_discovery_attempts,
            config: Arc::new(config),
        })
    }
    
    pub async fn execute_task_workflow(
        &self,
        task_id: i64,
        framework_integration: Arc<dyn FrameworkIntegration>,
    ) -> Result<TaskOrchestrationResult, OrchestrationError> {
        let timeout = Duration::from_secs(self.config.orchestration.task_timeout_seconds);
        
        tokio::time::timeout(timeout, async {
            // Use configured values throughout execution
            self.execute_with_config(task_id, framework_integration).await
        }).await.map_err(|_| OrchestrationError::Timeout)?
    }
}
```

#### Task Handler Configuration
```rust
// TaskHandlerRegistry using configuration
impl TaskHandlerRegistry {
    pub fn new() -> Result<Self, Error> {
        let config = ConfigurationManager::load()
            .map_err(|e| Error::runtime_error(format!("Config load failed: {}", e)))?;
            
        let pool = Self::get_database_pool(&config.database)?;
        
        Ok(Self {
            config_manager: Arc::new(ConfigurationManager::new()),
            pool,
            config_directory: config.task_handlers.config_directory,
            default_dependent_system_id: config.task_handlers.default_dependent_system_id,
        })
    }
}
```

### Configuration Validation

#### Validation Rules
```rust
impl TaskerCoreConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        // Orchestration validation
        if self.orchestration.max_parallel_steps == 0 {
            return Err(ConfigurationError::InvalidValue(
                "orchestration.max_parallel_steps must be > 0".to_string()
            ));
        }
        
        if self.orchestration.step_timeout_seconds < 1 {
            return Err(ConfigurationError::InvalidValue(
                "orchestration.step_timeout_seconds must be >= 1".to_string()
            ));
        }
        
        // Database validation
        if self.database.pool_size == 0 {
            return Err(ConfigurationError::InvalidValue(
                "database.pool_size must be > 0".to_string()
            ));
        }
        
        // Event validation
        if self.events.buffer_size < self.events.batch_size {
            return Err(ConfigurationError::InvalidValue(
                "events.buffer_size must be >= events.batch_size".to_string()
            ));
        }
        
        // Path validation
        if !Path::new(&self.task_handlers.config_directory).exists() {
            return Err(ConfigurationError::InvalidValue(
                format!("task_handlers.config_directory does not exist: {}", 
                       self.task_handlers.config_directory)
            ));
        }
        
        Ok(())
    }
}
```

## Implementation Priority

### Week 2 Day 1-2: Configuration Extraction
1. **Create configuration structs** - Define all configuration options
2. **Extract hardcoded values** - Replace with config reads
3. **Implement validation** - Ensure configuration integrity
4. **Add environment support** - Enable environment variable overrides

### Week 2 Day 3-4: Integration
1. **Update all components** - Use configuration throughout codebase
2. **Test configuration loading** - Validate YAML parsing and env vars
3. **Document configuration** - Complete configuration reference
4. **Add configuration tests** - Validate all configuration scenarios

### Validation Criteria
- [ ] Zero hardcoded timeout values in production code
- [ ] All component limits configurable via YAML or environment variables
- [ ] Path configuration externalized and validated
- [ ] Database settings fully configurable
- [ ] Configuration validation prevents invalid deployments
- [ ] Environment-specific overrides working correctly

---
**Purpose**: Enable production deployment through configuration management  
**Timeline**: Week 2 of Phase 1  
**Success Metric**: Zero hardcoded values, full environment configurability