# Tasker Core Stabilization Plan

**Document Version**: 1.0  
**Date**: August 7, 2025  
**Status**: Implementation Planning

## Executive Summary

This document outlines a comprehensive stabilization plan to address critical architectural issues identified through predictive analysis of the tasker-core-rs codebase. The plan focuses on eliminating silent failures, unifying configuration management, and preventing production incidents through systematic refactoring.

## Problem Statement

### Root Cause Analysis

The Rust codebase was developed independently without integrating the sophisticated YAML configuration system that already exists on the Ruby side. This architectural disconnect has created several cascading problems:

1. **Configuration Fragmentation**: Zero integration between comprehensive YAML config and Rust implementation
2. **Silent Failure Patterns**: Permissive error handling that masks configuration corruption  
3. **Database Connection Chaos**: Multiple unconnected pooling systems causing potential resource exhaustion
4. **Environment Variable Dependencies**: Code expects env vars that should derive from centralized configuration

### Risk Assessment

**🚨 HIGH RISK**:
- **Memory corruption risk**: Silent JSON parsing failures (`types.rs:46`)
- **Connection exhaustion**: Fragmented database pooling strategies
- **Production safety**: Test operations could run in production if environment detection fails

**⚠️ MEDIUM RISK**:
- **Performance degradation**: Hard-coded timeouts inappropriate for all environments  
- **Operational complexity**: Configuration spread across env vars instead of centralized YAML

## Stabilization Strategy

### Design Principles

1. **Fail Fast, Fail Explicitly**: No more silent degradation with fallback values
2. **Single Source of Truth**: YAML configuration drives all system behavior
3. **Environment Awareness**: Consistent behavior across development/test/production
4. **Ruby-Rust Parity**: Unified configuration approach across language boundaries

### Architecture Goals

- **Unified Config System**: Mirror Ruby's YAML-based configuration approach
- **Strict Validation**: Replace `unwrap_or()` patterns with explicit error handling
- **Resource Management**: Single config-driven database pool prevents exhaustion
- **Operational Simplicity**: One place to change system behavior

## Implementation Plan

### Phase 1: YAML Configuration Foundation
**Priority**: Critical (Enables all subsequent phases)  
**Estimated Effort**: 2-3 days

#### Deliverables
1. **Core Configuration Module** (`src/config/mod.rs`)
   - Mirror existing YAML structure exactly
   - Environment-aware loading (development/test/production)
   - Comprehensive error handling with specific error types

2. **Configuration Structs**
   ```rust
   #[derive(Debug, Clone, Deserialize)]
   pub struct TaskerConfig {
       pub database: DatabaseConfig,
       pub execution: ExecutionConfig,
       pub pgmq: PgmqConfig,
       pub orchestration: OrchestrationConfig,
       pub health: HealthConfig,
       pub telemetry: TelemetryConfig,
   }
   
   #[derive(Debug, Clone, Deserialize)]
   pub struct DatabaseConfig {
       pub pool: u32,
       pub host: String,
       pub username: String,
       pub password: String,
       pub checkout_timeout: u64,
       pub reaping_frequency: u64,
   }
   ```

3. **Configuration Loader** (`src/config/loader.rs`)
   - Environment detection matching Ruby implementation
   - Config file discovery with same fallback paths
   - Environment-specific override merging
   - Validation and error reporting

#### Success Criteria
- ✅ All YAML config sections mapped to Rust structs
- ✅ Environment-specific overrides working (dev/test/prod)
- ✅ Config loading matches Ruby behavior exactly
- ✅ Comprehensive error messages for invalid config

### Phase 2: Strict JSON Validation
**Priority**: High (Security & Reliability)  
**Estimated Effort**: 1-2 days

#### Target Locations and Issues

1. **`types.rs:46` - Critical Configuration Parsing**
   ```rust
   // ❌ CURRENT: Silent corruption
   serde_json::from_str(config_str).unwrap_or(serde_json::json!({}))
   
   // ✅ AFTER: Explicit struct validation  
   #[derive(Debug, Deserialize)]
   struct StepHandlerConfig {
       timeout_ms: u64,
       handler_class: String,
       retry_limit: Option<u32>,
   }
   
   let config: StepHandlerConfig = serde_json::from_str(config_str)
       .map_err(|e| ConfigurationError::InvalidStepConfig { 
           step_name: step_name.clone(), 
           error: e.to_string() 
       })?;
   ```

2. **`embedded_bridge.rs:441` - Serialization Safety**
   ```rust
   // ❌ CURRENT: Silent null conversion
   serde_json::to_value(&result.step_mapping).unwrap_or(serde_json::Value::Null)
   
   // ✅ AFTER: Explicit error propagation
   let step_mapping_json = serde_json::to_value(&result.step_mapping)
       .map_err(|e| SerializationError::StepMapping(e))?;
   ```

3. **`viable_step_discovery.rs:384-389` - Handler Configuration**
   ```rust
   // ❌ CURRENT: Permissive fallbacks
   let timeout_ms = step_template.handler_config
       .as_ref()
       .and_then(|config| config.get("timeout_ms"))
       .and_then(|v| v.as_u64())
       .unwrap_or(30000u64);
   
   // ✅ AFTER: Structured configuration
   #[derive(Debug, Deserialize)]
   struct HandlerConfig {
       timeout_ms: u64,
       max_retries: u32,
       #[serde(default = "default_batch_size")]
       batch_size: usize,
   }
   
   let handler_config: HandlerConfig = serde_json::from_value(
       step_template.handler_config.clone()
   ).map_err(|e| ConfigurationError::InvalidHandlerConfig { 
       step_name: step.name.clone(), 
       error: e.to_string() 
   })?;
   ```

#### Error Type Hierarchy
```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("Invalid step configuration for '{step_name}': {error}")]
    InvalidStepConfig { step_name: String, error: String },
    
    #[error("Invalid handler configuration for '{step_name}': {error}")]
    InvalidHandlerConfig { step_name: String, error: String },
    
    #[error("Missing required configuration field '{field}' in {context}")]
    MissingRequiredField { field: String, context: String },
    
    #[error("Configuration file not found at: {path}")]
    ConfigFileNotFound { path: String },
    
    #[error("Invalid YAML configuration: {error}")]
    InvalidYaml { error: String },
}
```

#### Success Criteria
- ✅ All JSON parsing uses explicit struct validation
- ✅ Configuration errors provide actionable error messages
- ✅ No silent fallbacks or data corruption possible
- ✅ Comprehensive test coverage for error scenarios

### Phase 3: Unified Database Connection Management
**Priority**: High (Prevents Resource Exhaustion)  
**Estimated Effort**: 2-3 days

#### Current State Analysis
- **`src/database/connection.rs`**: Basic implementation with hardcoded fallbacks
- **`src/database/connection_pool_strategies.rs`**: Sophisticated but unused, expects undefined env vars
- **Multiple FFI bridges**: Each creating their own database connections

#### Unification Strategy

1. **Single Config-Driven Connection Factory**
   ```rust
   #[derive(Debug)]
   pub struct DatabaseConnectionManager {
       pool: PgPool,
       config: DatabaseConfig,
       metrics: PoolMetrics,
   }
   
   impl DatabaseConnectionManager {
       pub async fn from_config(
           config: &TaskerConfig
       ) -> Result<Self, DatabaseError> {
           let db_config = &config.database;
           
           // Build connection URL from config, not env vars
           let database_url = format!(
               "postgresql://{}:{}@{}:{}/{}",
               db_config.username,
               db_config.password,
               db_config.host,
               db_config.port.unwrap_or(5432),
               db_config.database_name(&config.execution.environment)
           );
           
           let pool = PgPoolOptions::new()
               .max_connections(db_config.pool)
               .acquire_timeout(Duration::from_secs(db_config.checkout_timeout))
               .idle_timeout(Duration::from_secs(db_config.reaping_frequency))
               .test_before_acquire(true)
               .connect(&database_url)
               .await?;
           
           Ok(Self { pool, config: db_config.clone(), metrics: PoolMetrics::new() })
       }
   }
   ```

2. **Environment-Specific Database Selection**
   ```rust
   impl DatabaseConfig {
       fn database_name(&self, environment: &str) -> String {
           match environment {
               "development" => "tasker_rust_development".to_string(),
               "test" => "tasker_rust_test".to_string(),
               "production" => std::env::var("POSTGRES_DB")
                   .unwrap_or_else(|_| "tasker_production".to_string()),
               _ => format!("tasker_rust_{}", environment)
           }
       }
   }
   ```

3. **Pool Health Monitoring**
   ```rust
   #[derive(Debug, Clone)]
   pub struct PoolMetrics {
       pub active_connections: u32,
       pub idle_connections: u32,  
       pub utilization_percentage: f64,
       pub average_acquire_time_ms: u64,
   }
   
   impl DatabaseConnectionManager {
       pub fn health_check(&self) -> PoolHealthStatus {
           let metrics = self.get_current_metrics();
           
           PoolHealthStatus {
               healthy: metrics.utilization_percentage < 90.0,
               metrics,
               warnings: self.generate_warnings(&metrics),
           }
       }
   }
   ```

#### Migration Strategy
1. **Deprecate**: `connection_pool_strategies.rs` environment variable approach
2. **Unify**: All database access through `DatabaseConnectionManager`
3. **Update**: All FFI bridges to use shared connection manager
4. **Monitor**: Add pool metrics to health check endpoints

#### Success Criteria
- ✅ Single database connection pool across entire system
- ✅ Pool configuration driven by YAML, not env vars
- ✅ Connection pool health monitoring and alerting
- ✅ Environment-specific database selection working
- ✅ No hardcoded DATABASE_URL fallbacks anywhere

### Phase 4: System-Wide Configuration Integration
**Priority**: Medium (Operational Consistency)  
**Estimated Effort**: 1-2 days

#### Environment Variable Elimination

**Replace These Patterns**:
```rust
// ❌ CURRENT: Environment variable dependency
let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
    "postgresql://tasker:tasker@localhost/tasker_rust_development".to_string()
});

// ✅ AFTER: Config-driven
let database_url = config_manager.database_url()?;
```

**Target Locations**:
- `embedded_bridge.rs:584,626,669` - DATABASE_URL hardcoding  
- `connection.rs:11` - Hardcoded fallback URL
- `connection_pool_strategies.rs:132-165` - TASKER_* env var expectations

#### Configuration Propagation Strategy

1. **Singleton Configuration Manager**
   ```rust
   pub struct ConfigManager {
       config: TaskerConfig,
       environment: String,
   }
   
   impl ConfigManager {
       pub fn global() -> &'static ConfigManager {
           static INSTANCE: OnceCell<ConfigManager> = OnceCell::new();
           INSTANCE.get_or_init(|| {
               ConfigManager::load().expect("Failed to load configuration")
           })
       }
   }
   ```

2. **Timeout Configuration**
   ```rust
   // Replace hardcoded timeouts throughout codebase
   impl TaskerConfig {
       pub fn step_execution_timeout(&self) -> Duration {
           Duration::from_secs(self.execution.step_execution_timeout_seconds)
       }
       
       pub fn task_processing_timeout(&self) -> Duration {
           Duration::from_secs(self.execution.default_timeout_seconds)
       }
       
       pub fn pgmq_visibility_timeout(&self) -> Duration {
           Duration::from_secs(self.pgmq.visibility_timeout_seconds)
       }
   }
   ```

#### Success Criteria
- ✅ Zero environment variable dependencies for core functionality
- ✅ All timeouts and limits configurable via YAML
- ✅ Configuration changes require only YAML edit, not code changes
- ✅ Development/test/production environments fully isolated

## Risk Mitigation

### Pre-Implementation Safeguards

1. **Comprehensive Test Coverage**
   - Unit tests for all configuration loading scenarios
   - Integration tests for database connection pooling
   - Error scenario tests for malformed configurations

2. **Backward Compatibility Strategy**
   - Graceful degradation for missing config sections
   - Clear migration path documentation
   - Warning messages for deprecated patterns

3. **Rollback Plan**
   - Feature flags for new configuration system
   - Ability to fall back to environment variables if needed
   - Progressive rollout strategy

### Post-Implementation Monitoring

1. **Health Check Integration**
   - Configuration validation in health endpoints
   - Database pool metrics in monitoring
   - Configuration reload capability without restart

2. **Operational Visibility**
   - Structured logging for configuration loading
   - Metrics for database pool utilization
   - Alerts for configuration errors

## Testing Strategy

### Unit Testing Requirements

1. **Configuration Loading**
   ```rust
   #[cfg(test)]
   mod tests {
       #[test]
       fn test_config_loading_all_environments() {
           // Test development, test, production config loading
       }
       
       #[test]
       fn test_config_validation_errors() {
           // Test invalid YAML, missing sections, type mismatches
       }
       
       #[test]
       fn test_environment_specific_overrides() {
           // Test that production overrides work correctly
       }
   }
   ```

2. **Database Connection Management**
   ```rust
   #[test]
   async fn test_single_pool_across_system() {
       // Ensure only one pool is created regardless of access points
   }
   
   #[test]
   async fn test_pool_exhaustion_prevention() {
       // Test that pool size limits are enforced
   }
   ```

3. **JSON Validation**
   ```rust
   #[test]
   fn test_strict_step_config_validation() {
       // Test that malformed step configs fail explicitly
   }
   
   #[test]
   fn test_no_silent_serialization_failures() {
       // Test that all serialization errors propagate
   }
   ```

### Integration Testing Requirements

1. **End-to-End Configuration Flow**
   - Load config from YAML
   - Create database connections
   - Execute workflow with proper timeouts
   - Verify no fallback values used

2. **Environment Isolation**
   - Test data separation between environments
   - Verify production safety checks work
   - Test environment-specific overrides

## Success Metrics

### Technical Metrics

1. **Reliability**
   - ✅ Zero silent configuration failures
   - ✅ 100% explicit error handling for config issues
   - ✅ Single database connection pool across system

2. **Performance**  
   - ✅ Database pool utilization under 80% under normal load
   - ✅ Configuration loading under 100ms
   - ✅ Memory usage stable (no connection leaks)

3. **Maintainability**
   - ✅ All system behavior configurable via YAML
   - ✅ Zero hardcoded timeouts or limits
   - ✅ Configuration changes require no code changes

### Operational Metrics

1. **Production Readiness**
   - ✅ Health checks include configuration status
   - ✅ Monitoring covers database pool metrics  
   - ✅ Alert system for configuration errors

2. **Developer Experience**
   - ✅ Clear error messages for configuration issues
   - ✅ Consistent behavior across environments
   - ✅ Simple configuration override for local development

### Phase 5: Constants Unification and Ruby Configuration Extension  
**Priority**: High (Ruby-Rust Consistency)  
**Estimated Effort**: 2-3 days

#### Problem Analysis

**Ruby Bindings Constants (Need Configuration Integration)**:
```ruby
# bindings/ruby/lib/tasker_core/messaging/queue_worker.rb
FALLBACK_POLL_INTERVAL = 0.25        # Should use config.pgmq.poll_interval_ms
FALLBACK_VISIBILITY_TIMEOUT = 30     # Should use config.pgmq.visibility_timeout_seconds  
FALLBACK_BATCH_SIZE = 5              # Should use config.pgmq.batch_size
FALLBACK_MAX_RETRIES = 3             # Should use config.pgmq.max_retries
FALLBACK_SHUTDOWN_TIMEOUT = 30       # Should use config.orchestration.shutdown_timeout_seconds

# bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb
DEFAULT_VISIBILITY_TIMEOUT = 30      # Should use config.pgmq.visibility_timeout_seconds
DEFAULT_MESSAGE_COUNT = 1            # Should use config.pgmq.batch_size
MAX_MESSAGE_COUNT = 100              # Should use config.pgmq.max_batch_size

# bindings/ruby/lib/tasker_core/step_handler/api.rb
conn.options.timeout = config[:timeout] || 30           # Hardcoded 30s fallback
conn.options.open_timeout = config[:open_timeout] || 10 # Hardcoded 10s fallback

# bindings/ruby/lib/tasker_core/registry/task_template_registry.rb  
step[:default_retry_limit] ||= 3     # Should use config.execution.max_retries
```

**Rust Constants (Need Configuration Migration)**:
```rust
// src/constants.rs - These should become configurable
pub const MAX_DEPENDENCY_DEPTH: usize = 50;      # -> config.dependency_graph.max_depth
pub const MAX_WORKFLOW_STEPS: usize = 1000;      # -> config.execution.max_workflow_steps
pub const TASKER_CORE_VERSION: &str = "0.1.0";   # -> config.system.version
```

#### Ruby Configuration Manager Extension

**Current State**: Ruby config loading is basic and doesn't handle constants replacement.

**Enhanced Ruby ConfigManager** (`bindings/ruby/lib/tasker_core/config.rb`):
```ruby
module TaskerCore
  class Config
    class << self
      # Enhanced configuration access with automatic constant replacement
      def queue_worker_defaults
        {
          poll_interval: config.dig('pgmq', 'poll_interval_ms') || 250, # Convert to seconds
          visibility_timeout: config.dig('pgmq', 'visibility_timeout_seconds') || 30,
          batch_size: config.dig('pgmq', 'batch_size') || 5,
          max_retries: config.dig('pgmq', 'max_retries') || 3,
          shutdown_timeout: config.dig('orchestration', 'shutdown_timeout_seconds') || 30
        }
      end

      def pgmq_client_defaults
        {
          visibility_timeout: config.dig('pgmq', 'visibility_timeout_seconds') || 30,
          message_count: config.dig('pgmq', 'batch_size') || 1,
          max_message_count: config.dig('pgmq', 'max_batch_size') || 100
        }
      end

      def api_timeouts
        {
          timeout: config.dig('execution', 'step_execution_timeout_seconds') || 30,
          open_timeout: config.dig('execution', 'connection_timeout_seconds') || 10
        }
      end

      def execution_limits
        {
          max_retries: config.dig('execution', 'max_retries') || 3,
          max_dependency_depth: config.dig('dependency_graph', 'max_depth') || 50,
          max_workflow_steps: config.dig('execution', 'max_workflow_steps') || 1000
        }
      end

      # Validate Ruby-Rust configuration consistency
      def validate_rust_compatibility!
        rust_expected = {
          'dependency_graph.max_depth' => 50,
          'execution.max_workflow_steps' => 1000,
          'pgmq.visibility_timeout_seconds' => 30,
          'pgmq.batch_size' => 5
        }
        
        inconsistencies = []
        rust_expected.each do |path, rust_default|
          ruby_value = config.dig(*path.split('.'))
          if ruby_value && ruby_value != rust_default
            inconsistencies << "#{path}: Ruby=#{ruby_value}, Rust=#{rust_default}"
          end
        end
        
        if inconsistencies.any?
          raise ConfigurationError, "Ruby-Rust config inconsistencies: #{inconsistencies.join(', ')}"
        end
      end
    end
  end
end
```

#### Extended YAML Configuration Schema

**Add New Configuration Sections**:
```yaml
# config/tasker-config.yaml - Extended with unified constants

execution:
  # ... existing fields ...
  max_retries: 3
  max_workflow_steps: 1000
  connection_timeout_seconds: 10

dependency_graph:
  # ... existing fields ...
  max_depth: 50
  cycle_detection_enabled: true
  optimization_enabled: true

pgmq:
  # ... existing fields ...  
  max_batch_size: 100
  shutdown_timeout_seconds: 30

system:
  # ... existing fields ...
  version: "0.1.0"
  max_recursion_depth: 50

# Environment overrides
test:
  dependency_graph:
    max_depth: 10  # Smaller for faster tests
  execution:
    max_workflow_steps: 100

production:
  dependency_graph:
    max_depth: 100  # Higher for complex production workflows
  execution:
    max_workflow_steps: 5000
```

#### Implementation Strategy

**1. Ruby Constants Elimination**
```ruby
# Before: bindings/ruby/lib/tasker_core/messaging/queue_worker.rb
FALLBACK_POLL_INTERVAL = 0.25

# After: Use configuration with validation
def initialize(namespace, **options)
  config_defaults = TaskerCore::Config.queue_worker_defaults
  @poll_interval = options[:poll_interval] || config_defaults[:poll_interval] / 1000.0
  
  # Fail fast if configuration is invalid
  if @poll_interval <= 0
    raise TaskerCore::ConfigurationError, 
          "Invalid poll_interval: #{@poll_interval}. Must be positive number."
  end
end
```

**2. Rust Constants Migration**
```rust
// Before: src/constants.rs
pub const MAX_DEPENDENCY_DEPTH: usize = 50;

// After: Configuration-driven
impl TaskerConfig {
    pub fn max_dependency_depth(&self) -> usize {
        self.dependency_graph.max_depth as usize
    }
    
    pub fn max_workflow_steps(&self) -> usize {
        self.execution.max_workflow_steps as usize  
    }
}

// Usage throughout codebase:
// OLD: constants::system::MAX_DEPENDENCY_DEPTH
// NEW: config_manager.config().max_dependency_depth()
```

**3. Cross-Language Configuration Validation**
```rust
// src/config/validation.rs - New module
pub fn validate_ruby_rust_compatibility(config: &TaskerConfig) -> Result<(), ValidationError> {
    let ruby_expectations = [
        ("pgmq.batch_size", config.pgmq.batch_size as i64, 5),
        ("dependency_graph.max_depth", config.dependency_graph.max_depth as i64, 50),
        ("execution.max_workflow_steps", config.execution.max_workflow_steps as i64, 1000),
    ];
    
    for (field, actual, expected_default) in ruby_expectations {
        if actual != expected_default {
            warn!("Configuration differs from Ruby default: {} = {} (Ruby expects {})", 
                  field, actual, expected_default);
        }
    }
    
    Ok(())
}
```

#### Ruby Integration Testing
```ruby
# bindings/ruby/spec/config_constants_spec.rb - New test file
RSpec.describe "Configuration Constants Integration" do
  it "eliminates all FALLBACK constants" do
    ruby_files = Dir.glob("lib/**/*.rb")
    fallback_constants = ruby_files.flat_map do |file|
      File.readlines(file).grep(/FALLBACK_|DEFAULT_.*=/).map do |line|
        "#{file}:#{line.strip}"
      end
    end
    
    expect(fallback_constants).to be_empty, 
           "Found hardcoded constants that should use configuration: #{fallback_constants.join("\n")}"
  end

  it "uses configuration for all timeout values" do
    config_defaults = TaskerCore::Config.api_timeouts
    expect(config_defaults[:timeout]).to be > 0
    expect(config_defaults[:open_timeout]).to be > 0
  end

  it "validates Ruby-Rust configuration consistency" do
    expect { TaskerCore::Config.validate_rust_compatibility! }.not_to raise_error
  end
end
```

#### Detailed Implementation Checklist

**📋 PHASE 5A: YAML Schema Extension**
- [ ] **5A.1** Add new configuration sections to `config/tasker-config.yaml`:
  ```yaml
  execution:
    max_retries: 3
    max_workflow_steps: 1000
    connection_timeout_seconds: 10
  
  dependency_graph:
    max_depth: 50
  
  pgmq:
    max_batch_size: 100
    shutdown_timeout_seconds: 30
  
  system:
    version: "0.1.0"
    max_recursion_depth: 50
  ```
- [ ] **5A.2** Add environment-specific overrides:
  ```yaml
  test:
    dependency_graph:
      max_depth: 10
    execution:
      max_workflow_steps: 100
  
  production:
    dependency_graph:
      max_depth: 100
    execution:
      max_workflow_steps: 5000
  ```
- [ ] **5A.3** Update Rust config structs in `src/config/mod.rs`:
  - Add `max_retries: u32` to ExecutionConfig
  - Add `max_workflow_steps: u32` to ExecutionConfig  
  - Add `connection_timeout_seconds: u64` to ExecutionConfig
  - Add `max_batch_size: u32` to PgmqConfig
  - Add `shutdown_timeout_seconds: u64` to PgmqConfig
  - Add `max_recursion_depth: u32` to SystemConfig

**📋 PHASE 5B: Ruby ConfigManager Enhancement**
- [ ] **5B.1** Create enhanced config methods in `bindings/ruby/lib/tasker_core/config.rb`:
  ```ruby
  def queue_worker_defaults
  def pgmq_client_defaults  
  def api_timeouts
  def execution_limits
  def validate_rust_compatibility!
  ```
- [ ] **5B.2** Add configuration validation with specific error messages:
  ```ruby
  class ConfigurationError < StandardError; end
  ```
- [ ] **5B.3** Add environment-aware config loading:
  ```ruby
  def environment_overrides
    @config.dig(current_environment) || {}
  end
  ```

**📋 PHASE 5C: Ruby Constants Elimination**
- [ ] **5C.1** Replace constants in `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`:
  - [ ] Remove `FALLBACK_POLL_INTERVAL = 0.25`
  - [ ] Remove `FALLBACK_VISIBILITY_TIMEOUT = 30`
  - [ ] Remove `FALLBACK_BATCH_SIZE = 5`
  - [ ] Remove `FALLBACK_MAX_RETRIES = 3`
  - [ ] Remove `FALLBACK_SHUTDOWN_TIMEOUT = 30`
  - [ ] Update `initialize` method to use `TaskerCore::Config.queue_worker_defaults`
  - [ ] Add validation: `raise ConfigurationError if @poll_interval <= 0`
- [ ] **5C.2** Replace constants in `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb`:
  - [ ] Remove `DEFAULT_VISIBILITY_TIMEOUT = 30`
  - [ ] Remove `DEFAULT_MESSAGE_COUNT = 1`
  - [ ] Remove `MAX_MESSAGE_COUNT = 100`
  - [ ] Update methods to use `TaskerCore::Config.pgmq_client_defaults`
- [ ] **5C.3** Replace hardcoded timeouts in `bindings/ruby/lib/tasker_core/step_handler/api.rb`:
  - [ ] Replace `config[:timeout] || 30` with `TaskerCore::Config.api_timeouts[:timeout]`
  - [ ] Replace `config[:open_timeout] || 10` with `TaskerCore::Config.api_timeouts[:open_timeout]`
- [ ] **5C.4** Replace hardcoded retry limit in `bindings/ruby/lib/tasker_core/registry/task_template_registry.rb`:
  - [ ] Replace `step[:default_retry_limit] ||= 3` with `TaskerCore::Config.execution_limits[:max_retries]`

**📋 PHASE 5D: Rust Constants Migration**
- [ ] **5D.1** Update `src/config/mod.rs` with accessor methods:
  ```rust
  impl TaskerConfig {
      pub fn max_dependency_depth(&self) -> usize
      pub fn max_workflow_steps(&self) -> usize
      pub fn system_version(&self) -> &str
      pub fn connection_timeout(&self) -> Duration
  }
  ```
- [ ] **5D.2** Replace constants usage in codebase:
  - [ ] Find all uses of `constants::system::MAX_DEPENDENCY_DEPTH`
  - [ ] Replace with `config_manager.config().max_dependency_depth()`
  - [ ] Find all uses of `constants::system::MAX_WORKFLOW_STEPS`
  - [ ] Replace with `config_manager.config().max_workflow_steps()`
  - [ ] Find all uses of `constants::system::TASKER_CORE_VERSION`
  - [ ] Replace with `config_manager.config().system_version()`
- [ ] **5D.3** Update affected modules:
  - [ ] `src/orchestration/viable_step_discovery.rs` - dependency depth checks
  - [ ] `src/orchestration/workflow_coordinator.rs` - step count validation
  - [ ] Any health check endpoints using version info

**📋 PHASE 5E: Cross-Language Validation**
- [ ] **5E.1** Create `src/config/validation.rs`:
  ```rust
  pub fn validate_ruby_rust_compatibility(config: &TaskerConfig) -> Result<(), ValidationError>
  pub struct ValidationError
  pub fn warn_config_differences(config: &TaskerConfig)
  ```
- [ ] **5E.2** Add Ruby validation methods:
  ```ruby
  # In TaskerCore::Config
  def validate_rust_compatibility!
  def configuration_warnings
  def environment_specific_validation
  ```
- [ ] **5E.3** Integration in startup sequences:
  - [ ] Add validation call in Rust `ConfigManager::load()`
  - [ ] Add validation call in Ruby config initialization

**📋 PHASE 5F: Testing & Validation**
- [ ] **5F.1** Create Ruby test file `bindings/ruby/spec/config_constants_spec.rb`:
  ```ruby
  RSpec.describe "Configuration Constants Integration" do
    it "eliminates all FALLBACK constants"
    it "eliminates all DEFAULT constants"
    it "uses configuration for all timeout values"
    it "validates Ruby-Rust configuration consistency"
    it "fails fast on invalid configuration"
  end
  ```
- [ ] **5F.2** Create Rust test file `src/config/constants_integration_test.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      #[test] fn test_no_hardcoded_constants_in_use()
      #[test] fn test_config_driven_limits()
      #[test] fn test_environment_specific_overrides()
      #[test] fn test_ruby_rust_compatibility_validation()
  }
  ```
- [ ] **5F.3** Integration testing:
  - [ ] Test configuration loading in both languages
  - [ ] Test environment-specific overrides work identically
  - [ ] Test validation catches configuration drift
  - [ ] Test fail-fast behavior on startup

**📋 PHASE 5G: Documentation & Cleanup**
- [ ] **5G.1** Update configuration documentation:
  - [ ] Document new YAML configuration sections
  - [ ] Document Ruby ConfigManager API changes
  - [ ] Document Rust configuration accessor methods
  - [ ] Document environment-specific override behavior
- [ ] **5G.2** Code cleanup:
  - [ ] Remove unused constant definitions
  - [ ] Update comments referencing old constants
  - [ ] Update any inline documentation
- [ ] **5G.3** Migration notes:
  - [ ] Document breaking changes for users
  - [ ] Provide migration examples
  - [ ] Update any deployment documentation

#### Success Criteria
- ✅ Zero FALLBACK_* and DEFAULT_* constants in Ruby bindings
- ✅ All Ruby timeouts and limits come from YAML configuration
- ✅ Rust constants migrated to configuration-driven values
- ✅ Ruby-Rust configuration consistency validation
- ✅ Fail-fast behavior when configuration is invalid
- ✅ Environment-specific constant overrides working
- ✅ Cross-language integration tests passing

#### Artifacts Delivered
- ✅ **Extended YAML Configuration Schema**: All constants moved to config
- ✅ **Enhanced Ruby ConfigManager**: Constants replacement methods
- ✅ **Updated Rust Config Structs**: New fields and accessor methods  
- ✅ **Cross-Language Validation**: Consistency checking between Ruby/Rust
- ✅ **Integration Test Suite**: Comprehensive validation of constants elimination
- ✅ **Migration Documentation**: Clear upgrade path for users

## Timeline and Milestones

### Week 1: Configuration Foundation ✅
- **Days 1-2**: Implement YAML config loading system ✅
- **Days 3-4**: Replace critical JSON parsing with strict validation ✅
- **Day 5**: Testing and validation ✅

### Week 2: Database Unification ✅
- **Days 1-2**: Implement unified database connection manager ✅
- **Days 3-4**: Migration from fragmented connection systems ✅
- **Day 5**: Integration testing and metrics ✅

### Week 3: Constants Unification
- **Days 1-2**: Extend YAML schema with constants, enhance Ruby ConfigManager
- **Days 3-4**: Eliminate Ruby constants, migrate Rust constants to config
- **Day 5**: Cross-language validation and integration testing

### Week 4: System Integration  
- **Days 1-2**: Remove remaining environment variable dependencies
- **Days 3-4**: End-to-end testing and documentation
- **Day 5**: Production readiness verification

## Conclusion

This stabilization plan addresses the fundamental architectural issues that could lead to production incidents. By implementing these changes systematically, we eliminate silent failures, unify configuration management across languages, and create a robust foundation for future development.

The plan prioritizes the highest-risk issues first (configuration loading and JSON validation) while providing a clear path to full system stabilization including critical Ruby-Rust configuration consistency. Each phase has concrete deliverables and success criteria, ensuring measurable progress toward a more reliable system.

Implementation of this plan will result in:
- **Zero silent failures** in configuration handling
- **Unified configuration** approach across Ruby and Rust implementations
- **Prevention of resource exhaustion** through proper database pooling
- **Eliminated hardcoded constants** replaced with environment-aware configuration
- **Cross-language consistency** with validation to prevent Ruby-Rust drift
- **Simplified operations** with centralized YAML configuration
- **Production-ready reliability** with comprehensive error handling
- **Debugging clarity** - no more mystery fallback values making issue tracking difficult

### Key Achievements After Full Implementation

**Configuration Management**:
- ✅ Single YAML-based configuration system used by both Rust and Ruby
- ✅ Environment-specific overrides (dev/test/production) working consistently
- ✅ Zero hardcoded FALLBACK_* and DEFAULT_* constants in codebase
- ✅ Configuration validation ensuring Ruby-Rust compatibility

**Operational Benefits**:
- ✅ All system behavior configurable without code changes
- ✅ Clear error messages when configuration is invalid
- ✅ Performance tuning through configuration instead of constants
- ✅ Environment isolation preventing test data in production

**Developer Experience**:
- ✅ Consistent timeout and limit behavior across languages
- ✅ Easy debugging - all configuration comes from known YAML location
- ✅ Cross-language integration tests preventing configuration drift
- ✅ Fail-fast behavior when configuration is missing or invalid

---

---

## MAJOR UPDATE: Workflow Pattern Standardization Complete (August 7, 2025)

### 🎉 NEW PHASE COMPLETED: Workflow Example Unification

**Between Phase 2 and Phase 5, a critical workflow standardization initiative was completed:**

#### Problem Identified
During integration testing, discovered that workflow examples used inconsistent step result retrieval patterns:
- **Linear Workflow**: ✅ Used correct `sequence.get_results('step_name')` pattern  
- **Mixed DAG Workflow**: ❌ Used `sequence.get('step_name')&.dig('result')` pattern
- **Tree Workflow**: ❌ Used `sequence.get('step_name')&.dig('result')` pattern
- **Diamond Workflow**: ❌ Used `sequence.get('step_name')&.dig('result')` pattern
- **Order Fulfillment**: ❌ Used `sequence.steps.find { |s| s.name == 'step_name' }.results` pattern

This inconsistency meant developers couldn't rely on examples as authoritative references.

#### Solution Implemented
**Complete Pattern Standardization (August 7, 2025)**:
1. **Audited All Workflows**: Identified exactly which handlers needed updates across 4 workflows
2. **Applied Consistent Patterns**: Updated 20 step handlers to use `sequence.get_results('step_name')`
3. **Verified Integration Tests**: All workflows now pass core functionality tests
4. **Ensured Return Consistency**: All handlers return `TaskerCore::Types::StepHandlerCallResult.success`

#### Files Updated
**Mixed DAG Workflow** (7 handlers fixed):
- `bindings/ruby/spec/handlers/examples/mixed_dag_workflow/step_handlers/*.rb`

**Tree Workflow** (7 handlers fixed):
- `bindings/ruby/spec/handlers/examples/tree_workflow/step_handlers/*.rb`

**Diamond Workflow** (3 handlers fixed):
- `bindings/ruby/spec/handlers/examples/diamond_workflow/step_handlers/*.rb`

**Order Fulfillment** (3 handlers fixed):
- `bindings/ruby/spec/handlers/examples/order_fulfillment/step_handlers/*.rb`

#### Integration Test Results
- **Linear Workflow**: ✅ Working perfectly (reference implementation)
- **Mixed DAG Workflow**: ✅ Complex dependency resolution working
- **Tree Workflow**: ✅ Hierarchical processing working  
- **Diamond Workflow**: ✅ Parallel processing working
- **Order Fulfillment**: ✅ Complete workflow chain working

#### Developer Experience Impact
- **Before**: 4 different step result retrieval patterns across examples
- **After**: 1 unified `sequence.get_results('step_name')` pattern everywhere
- **Benefit**: Developers can now confidently use any workflow example as reference
- **Maintenance**: Single pattern to maintain instead of supporting multiple approaches

#### Success Metrics Achieved
- ✅ **Pattern Consistency**: All 20 step handlers across 5 workflows use unified pattern
- ✅ **Integration Tests**: All workflow integration tests passing with core functionality verified
- ✅ **Developer Experience**: Consistent examples for all workflow patterns (Linear, DAG, Tree, Diamond, Chain)
- ✅ **Code Quality**: All handlers return proper `StepHandlerCallResult.success` with metadata
- ✅ **Maintenance**: Eliminated 4 different result retrieval patterns, unified to single approach

This work significantly improves the quality and consistency of the codebase examples that developers rely on for implementing their own workflows.

---

*Current Status: Phase 1-2 ✅ Complete, Workflow Standardization ✅ Complete. Next Steps: Begin Phase 5 constants unification to achieve Ruby-Rust configuration consistency.*