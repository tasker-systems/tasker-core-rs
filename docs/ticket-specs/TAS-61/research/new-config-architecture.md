# TAS-61: New Configuration Architecture

**Status**: Design Phase
**Created**: 2025-11-03
**Last Updated**: 2025-11-03

## Executive Summary

This document defines the complete redesigned configuration architecture for tasker-core. The design is informed by empirical analysis of the current codebase and follows the principle of "shortest path to ideal state" for this greenfield project.

### Key Design Decisions

1. **Single File Loading**: Runtime loads one pre-merged TOML file per context
2. **Optional Contexts**: `TaskerConfig` has required `common` and optional `orchestration`/`worker` fields
3. **Simple Deserialization**: `ConfigLoader::load()` is ~50 lines using `toml::from_str()`
4. **CLI-Only Merging**: `ConfigMerger` handles base + environment merging
5. **Minimal Breaking Changes**: SystemContext signature unchanged, just field access patterns

## Analysis Findings

### SystemContext Dependencies (Confirmed)

From `systemcontext-deps.txt` analysis:

**Required Fields** (will go in CommonConfig):
- `database.pool.*` - All pool configuration fields
- `database_url` - Connection string
- `circuit_breakers.*` - Resilience configuration
- `queues.*` - PGMQ queue configuration
- `mpsc_channels.shared.*` - Shared channel configuration

**Usage Pattern**:
```rust
// Current pattern in SystemContext::new()
let pool_config = &config.database.pool;
let circuit_breakers = config.circuit_breakers.clone();
let queues = config.queues.clone();
```

### Config Struct Inventory

From `config-usage.json` analysis (100 total structs):

**Our Target Structs** (Already Heavily Used):
- `CommonConfig`: 86 references
- `OrchestrationConfig`: 79 references
- `WorkerConfig`: 84 references
- `TaskerConfig`: 194 references (most used)

**Unused Structs** (Can be deleted):
- `CorsConfig`: 1 reference (definition only)
- `HealthConfig`: 1 reference (definition only)
- `RateLimitConfig`: 1 reference (definition only)
- `ResilienceConfig`: 1 reference (definition only)

### Breaking Changes Scope

From `breaking-changes-map.md` analysis:

**Tier 1** (Critical Path): 11 files using SystemContext::new()
**Tier 2** (Bootstrap): 9 files using UnifiedConfigLoader
**Tier 3** (Access Patterns): ~25 files in orchestration/worker/shared
**Tier 4** (Tests): ~15 test files
**Total**: ~60 files to update

## Architecture Design

### 0. Validation Strategy with `validator`

**Decision**: Use the [`validator`](https://github.com/Keats/validator) crate (v0.19) for all configuration validation.

#### Why validator?

1. **Greenfield Perfect Timing**: Rebuilding config from scratch - ideal for best practices adoption
2. **Boilerplate Reduction**: Declarative validation vs manual checks for 100 config structs
3. **Self-Documenting**: `#[validate(range(min = 10, max = 1000000))]` immediately shows constraints
4. **Better Error Messages**: Structured errors with field paths, multiple errors reported at once
5. **TAS-58 Alignment**: Ecosystem-standard tool (4M+ downloads, actively maintained)
6. **Nested Validation**: `#[validate]` on fields automatically validates nested structs
7. **Pragmatic**: Focus on domain logic validation, not operational deployment concerns

#### Validation Patterns for TAS-61

**Philosophy**: Validate domain logic, not operational concerns.

```rust
use validator::{Validate, ValidationError};

// Range validation for numeric configs (use directly, no custom validator)
#[validate(range(min = 10, max = 1000000))]
pub command_buffer_size: usize,

// Length validation for lists
#[validate(length(min = 1))]
pub namespaces: Vec<String>,

// Nested struct validation (automatic recursive validation)
#[validate]
pub database: DatabaseConfig,

// Custom validators ONLY for domain-specific logic
#[validate(custom = "validate_postgres_url")]
pub url: String,  // Validates ${DATABASE_URL} template expansion

// Enum deserialization (no validator needed - serde handles it)
pub deployment_mode: DeploymentMode,  // Fails at deserialization if invalid

// Operational concerns (NO validation)
// If DevOps sets max_connections=5 and min_connections=10, that's their problem.
// Tasker validates domain constraints, not deployment configurations.
pub struct PoolConfig {
    #[validate(range(min = 1))]  // Just ensure > 0
    pub max_connections: u32,

    #[validate(range(min = 1))]  // No cross-field check
    pub min_connections: u32,
}
```

#### Custom Validators for TAS-61

```rust
// tasker-shared/src/config/validators.rs

use validator::ValidationError;

/// Validate PostgreSQL URL format
///
/// Allows ${DATABASE_URL} template substitution or actual postgres:// URLs.
/// This is domain-specific validation for Tasker's template expansion feature.
///
/// # Examples
/// - `${DATABASE_URL}` - Template (will be expanded at runtime)
/// - `postgresql://localhost/mydb` - Valid PostgreSQL URL
/// - `postgres://localhost/mydb` - Valid PostgreSQL URL (alternative scheme)
/// - `mysql://localhost/mydb` - **Invalid** (wrong database type)
pub fn validate_postgres_url(url: &str) -> Result<(), ValidationError> {
    // Allow template substitution (Tasker-specific feature)
    if url.contains("${DATABASE_URL}") || url.contains("$DATABASE_URL") {
        return Ok(());
    }

    // Validate actual PostgreSQL URL format
    if !url.starts_with("postgresql://") && !url.starts_with("postgres://") {
        let mut err = ValidationError::new("invalid_postgres_url");
        err.message = Some(
            "Database URL must start with postgresql:// or postgres:// or use ${DATABASE_URL}".into()
        );
        return Err(err);
    }

    Ok(())
}
```

#### Integration with ConfigLoader

```rust
use validator::Validate;

pub fn load(path: &Path) -> ConfigResult<TaskerConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigurationError::config_file_not_found(vec![path.to_path_buf()]))?;

    let config: TaskerConfig = toml::from_str(&content)
        .map_err(|e| ConfigurationError::deserialization_error("TaskerConfig", e))?;

    // Use validator's Validate trait
    config.validate().map_err(|validation_errors| {
        // Convert validator::ValidationErrors to our ConfigurationError
        let messages: Vec<String> = validation_errors
            .field_errors()
            .iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |error| {
                    let msg = error.message.as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| format!("validation failed for {}", error.code));
                    format!("{}: {}", field, msg)
                })
            })
            .collect();

        ConfigurationError::validation_error(format!(
            "Configuration validation failed:\n  - {}",
            messages.join("\n  - ")
        ))
    })?;

    Ok(config)
}
```

### 1. Core Configuration Structs

#### 1.1 CommonConfig (Shared Across All Contexts)

```rust
// tasker-shared/src/config/contexts/common.rs

use validator::Validate;

/// Configuration shared across all contexts (orchestration, worker, complete)
///
/// This struct contains only the fields that are genuinely shared across
/// all system contexts. Analysis shows SystemContext needs these 5 components.
///
/// # Validation
///
/// All nested structs are validated automatically via `#[validate]` attributes.
/// Use `config.validate()?` to check all constraints.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CommonConfig {
    /// System-level configuration (version, environment, recursion limits)
    #[validate]
    pub system: SystemConfig,

    /// Database connection and pool configuration
    ///
    /// Required by SystemContext for connection pool initialization.
    #[validate]
    pub database: DatabaseConfig,

    /// Message queue configuration (PGMQ)
    ///
    /// Required by SystemContext for queue initialization.
    #[validate]
    pub queues: QueuesConfig,

    /// Circuit breaker resilience configuration
    ///
    /// Required by SystemContext for protected client initialization.
    #[validate]
    pub circuit_breakers: CircuitBreakerConfig,

    /// Shared MPSC channel configuration
    ///
    /// Required by SystemContext for event publishing channels.
    #[serde(default)]
    #[validate]
    pub mpsc_channels: SharedChannelsConfig,
}

impl CommonConfig {
    /// Get database URL with environment variable substitution
    ///
    /// Expands ${DATABASE_URL} template if present in configuration.
    ///
    /// # Example
    /// ```rust
    /// // In TOML: url = "${DATABASE_URL}"
    /// // Environment: DATABASE_URL=postgresql://localhost/mydb
    /// let url = config.database_url(); // Returns: "postgresql://localhost/mydb"
    ///
    pub fn database_url(&self) -> String {
        // Expand ${DATABASE_URL} if present
        if self.database.url.contains("${DATABASE_URL}") {
            std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| self.database.url.clone())
        } else {
            self.database.url.clone()
        }
    }
}

/// Database configuration with validation
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DatabaseConfig {
    /// Database connection URL
    ///
    /// Accepts PostgreSQL URLs or ${DATABASE_URL} template substitution.
    #[validate(custom = "crate::config::validators::validate_postgres_url")]
    pub url: String,

    /// Connection pool configuration
    #[validate]
    pub pool: PoolConfig,
}

/// Pool configuration
///
/// Note: No cross-field validation (e.g., max >= min).
/// If DevOps misconfigures the pool in deployment, that's an operational
/// issue to be caught by deployment validation, not application validation.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool
    #[validate(range(min = 1))]
    pub max_connections: u32,

    /// Minimum number of connections to maintain
    #[validate(range(min = 1))]
    pub min_connections: u32,

    /// Idle connection timeout in seconds
    #[validate(range(min = 1))]
    pub idle_timeout_seconds: u64,

    /// Maximum connection lifetime in seconds
    #[validate(range(min = 1))]
    pub max_lifetime_seconds: u64,

    /// Connection acquisition timeout in seconds
    #[validate(range(min = 1))]
    pub acquire_timeout_seconds: u64,
}
```

#### 1.2 OrchestrationConfig (Orchestration-Specific)

```rust
// tasker-shared/src/config/contexts/orchestration.rs

use validator::Validate;

/// Orchestration-specific configuration
///
/// Contains all configuration needed for the orchestration service.
/// Present when context is "orchestration" or "complete".
///
/// # Validation
///
/// All nested configurations are validated recursively.
/// This ensures orchestration service starts with valid settings.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OrchestrationConfig {
    /// Event system configuration for orchestration
    ///
    /// Configures listeners, pollers, and event processing for orchestration.
    #[validate]
    pub event_systems: OrchestrationEventSystemsConfig,

    /// Decision point workflow configuration (TAS-53)
    ///
    /// Controls dynamic workflow decision points and step creation limits.
    #[validate]
    pub decision_points: DecisionPointsConfig,

    /// Executor pool configuration
    ///
    /// Configures thread pools for different executor types.
    #[validate]
    pub executor_pools: ExecutorPoolsConfig,

    /// Orchestration-specific MPSC channels
    ///
    /// Buffer sizes for command processors, event listeners, etc.
    #[validate]
    pub mpsc_channels: OrchestrationChannelsConfig,

    /// Orchestration system configuration
    ///
    /// Max concurrent tasks, namespaces, web API settings.
    #[validate]
    pub system: OrchestrationSystemConfig,
}

/// Deployment mode for event systems
///
/// Enum deserialization will fail if TOML contains an invalid value,
/// so no custom validator needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DeploymentMode {
    /// Pure event-driven using PostgreSQL LISTEN/NOTIFY
    EventDrivenOnly,
    /// Traditional polling-based coordination
    PollingOnly,
    /// Event-driven with polling fallback (recommended)
    Hybrid,
}

/// Orchestration event systems configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OrchestrationEventSystemsConfig {
    /// Deployment mode for event systems
    ///
    /// Deserialization will fail if invalid value provided in TOML.
    pub deployment_mode: DeploymentMode,

    /// Orchestration-specific event configuration
    #[validate]
    pub orchestration: OrchestrationEventsConfig,

    /// Task readiness event configuration
    #[validate]
    pub task_readiness: TaskReadinessEventsConfig,
}

/// Orchestration MPSC channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OrchestrationChannelsConfig {
    /// Command processor channels
    #[validate]
    pub command_processor: CommandProcessorChannels,

    /// Event listener channels
    #[validate]
    pub event_listeners: EventListenerChannels,
}

/// Command processor channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CommandProcessorChannels {
    /// Buffer size for command processing
    ///
    /// Reasonable range validation. If DevOps wants to set extreme values
    /// in deployment, that's their operational concern.
    #[validate(range(min = 10, max = 1000000))]
    pub command_buffer_size: usize,
}

/// Event listener channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EventListenerChannels {
    /// Buffer size for orchestration events
    #[validate(range(min = 10, max = 1000000))]
    pub orchestration_buffer_size: usize,

    /// Buffer size for task readiness events
    #[validate(range(min = 10, max = 1000000))]
    pub task_readiness_buffer_size: usize,
}

/// Orchestration system configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OrchestrationSystemConfig {
    /// Maximum concurrent tasks to process
    #[validate(range(min = 1, max = 100000))]
    pub max_concurrent_tasks: usize,

    /// Namespaces to process (must have at least one)
    #[validate(length(min = 1))]
    pub namespaces: Vec<String>,

    /// Enable web API server
    pub enable_web_api: bool,

    /// Bind address for web API (if enabled)
    pub bind_address: String,
}
```

#### 1.3 WorkerConfig (Worker-Specific)

```rust
// tasker-shared/src/config/contexts/worker.rs

use validator::Validate;

/// Worker-specific configuration
///
/// Contains all configuration needed for the worker service.
/// Present when context is "worker" or "complete".
///
/// # Validation
///
/// All nested configurations are validated recursively.
/// This ensures worker service starts with valid settings.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkerConfig {
    /// Event system configuration for workers
    ///
    /// Configures listeners, pollers, and event processing for workers.
    #[validate]
    pub event_systems: WorkerEventSystemsConfig,

    /// Step processing configuration
    ///
    /// Timeouts, concurrency limits, and retry behavior for step execution.
    #[validate]
    pub step_processing: StepProcessingConfig,

    /// Worker health monitoring configuration
    ///
    /// Health check intervals, failure thresholds, recovery behavior.
    #[validate]
    pub health_monitoring: HealthMonitoringConfig,

    /// Worker-specific MPSC channels
    ///
    /// Buffer sizes for command processors, FFI events, etc.
    #[validate]
    pub mpsc_channels: WorkerChannelsConfig,
}

/// Worker event systems configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkerEventSystemsConfig {
    /// Deployment mode for event systems
    ///
    /// Uses DeploymentMode enum - deserialization will fail if invalid.
    pub deployment_mode: DeploymentMode,

    /// Worker-specific event configuration
    #[validate]
    pub worker: WorkerEventsConfig,
}

/// Step processing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StepProcessingConfig {
    /// Maximum concurrent steps to process
    #[validate(range(min = 1, max = 100000))]
    pub max_concurrent_steps: usize,

    /// Step execution timeout in seconds
    #[validate(range(min = 1))]
    pub step_execution_timeout_seconds: u64,

    /// Result submission timeout in seconds
    #[validate(range(min = 1))]
    pub result_submission_timeout_seconds: u64,

    /// Enable step execution metrics
    pub enable_step_metrics: bool,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HealthMonitoringConfig {
    /// Enable health checks
    pub enable_health_checks: bool,

    /// Health check interval in seconds
    #[validate(range(min = 1))]
    pub health_check_interval_seconds: u64,

    /// Failure threshold before marking unhealthy
    #[validate(range(min = 1))]
    pub failure_threshold: u32,

    /// Recovery check interval in seconds
    #[validate(range(min = 1))]
    pub recovery_check_interval_seconds: u64,
}

/// Worker MPSC channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkerChannelsConfig {
    /// Command processor channels
    #[validate]
    pub command_processor: WorkerCommandProcessorChannels,

    /// Event subscriber channels
    #[validate]
    pub event_subscribers: WorkerEventSubscriberChannels,
}

/// Worker command processor channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkerCommandProcessorChannels {
    /// Buffer size for command processing
    ///
    /// Reasonable range validation. Deployment-level tuning is operational.
    #[validate(range(min = 10, max = 1000000))]
    pub command_buffer_size: usize,
}

/// Worker event subscriber channel configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkerEventSubscriberChannels {
    /// Buffer size for worker events
    #[validate(range(min = 10, max = 1000000))]
    pub worker_buffer_size: usize,

    /// Buffer size for in-process events
    #[validate(range(min = 10, max = 1000000))]
    pub in_process_events_buffer_size: usize,
}
```

#### 1.4 TaskerConfig (Unified with Optional Contexts)

```rust
// tasker-shared/src/config/tasker.rs (redesigned)

use validator::Validate;

/// Unified configuration for all Tasker contexts
///
/// This struct contains:
/// - `common`: Required configuration shared across all contexts
/// - `orchestration`: Optional orchestration-specific configuration
/// - `worker`: Optional worker-specific configuration
///
/// The presence of optional fields depends on the context:
/// - **Orchestration context**: common + orchestration (Some), worker (None)
/// - **Worker context**: common + worker (Some), orchestration (None)
/// - **Complete context**: common + orchestration (Some) + worker (Some)
///
/// ## Runtime Loading
///
/// Bootstrap code loads a single pre-merged TOML file:
///
/// ```rust
/// // Orchestration bootstrap
/// let config = ConfigLoader::load_for_orchestration(&config_path)?;
/// // config.common is always present
/// // config.orchestration is Some(_)
/// // config.worker is None
///
/// // Pass to SystemContext (accesses config.common internally)
/// let ctx = SystemContext::new(Arc::new(config.clone()))?;
///
/// // Access orchestration-specific config
/// let orch = config.orchestration.as_ref()
///     .expect("validated by load_for_orchestration");
///
///
/// ## CLI Generation
///
/// ConfigMerger generates the single file from base + environment sources:
///
/// ```bash
/// cargo run --bin tasker-cli config generate \
///   --context orchestration \
///   --environment production \
///   --output generated/orchestration-production.toml
///
///
/// ## Validation
///
/// All fields are validated recursively via the `validator` crate.
/// Use `config.validate()?` to check all constraints at once.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TaskerConfig {
    /// Shared configuration (database, queues, circuit breakers, etc.)
    ///
    /// Always present for all contexts.
    #[validate]
    pub common: CommonConfig,

    /// Orchestration-specific configuration
    ///
    /// Present when context is "orchestration" or "complete".
    /// Validated by `ConfigLoader::load_for_orchestration()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub orchestration: Option<OrchestrationConfig>,

    /// Worker-specific configuration
    ///
    /// Present when context is "worker" or "complete".
    /// Validated by `ConfigLoader::load_for_worker()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub worker: Option<WorkerConfig>,
}

impl TaskerConfig {
    /// Check if orchestration context is present
    pub fn has_orchestration(&self) -> bool {
        self.orchestration.is_some()
    }

    /// Check if worker context is present
    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }

    /// Check if this is a complete configuration
    pub fn is_complete(&self) -> bool {
        self.orchestration.is_some() && self.worker.is_some()
    }
}
```

### 2. ConfigLoader (Simple Single-File Loading)

```rust
// tasker-shared/src/config/loader.rs

use validator::Validate;

/// Simple configuration loader for pre-merged TOML files
///
/// Loads a single TOML file that has been pre-merged by the CLI config generator.
/// The file contains `[common]`, and optionally `[orchestration]` and/or `[worker]` sections.
///
/// ## Design Philosophy
///
/// ConfigLoader is intentionally simple (~50 lines):
/// - Reads file from disk
/// - Deserializes TOML into TaskerConfig struct
/// - Validates configuration using `validator` crate
/// - Returns typed struct
///
/// All merging logic lives in ConfigMerger (CLI tool only).
///
/// ## Config Path Resolution
///
/// Config path **MUST** be set via `TASKER_CONFIG_PATH` environment variable.
/// No fallback paths - fail-fast if not configured explicitly.
///
/// ```bash
/// export TASKER_CONFIG_PATH=/app/config/orchestration-production.toml
/// ```
///
/// If `TASKER_CONFIG_PATH` is not set or points to a non-existent file,
/// the application will abort with a clear error message.
///
/// **Rationale**: Explicit configuration prevents deployment issues.
/// Better to fail loudly than silently use wrong config
///
/// ## Validation
///
/// Uses the `validator` crate for comprehensive validation:
/// - All nested structs validated recursively via `#[validate]` attributes
/// - Returns multiple validation errors at once (not fail-fast)
/// - Structured error messages with field paths
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from a pre-merged TOML file
    ///
    /// # Arguments
    /// * `path` - Path to the merged config file
    ///
    /// # Returns
    /// * `TaskerConfig` with common always present, orchestration/worker as applicable
    ///
    /// # Errors
    /// * `ConfigurationError::ConfigFileNotFound` - File doesn't exist
    /// * `ConfigurationError::DeserializationError` - Invalid TOML or struct mismatch
    /// * `ConfigurationError::ValidationError` - Config values invalid (uses validator crate)
    ///
    /// # Example
    /// ```rust
    /// let config = ConfigLoader::load("generated/orchestration-production.toml")?;
    /// assert!(config.common.database.url.starts_with("postgresql://"));
    /// assert!(config.orchestration.is_some());
    /// assert!(config.worker.is_none());
    /// ```
    pub fn load(path: &Path) -> ConfigResult<TaskerConfig> {
        // Read file
        let content = std::fs::read_to_string(path).map_err(|e| {
            ConfigurationError::config_file_not_found(vec![path.to_path_buf()])
        })?;

        // Deserialize TOML
        let config: TaskerConfig = toml::from_str(&content).map_err(|e| {
            ConfigurationError::deserialization_error("TaskerConfig", e)
        })?;

        // Validate using validator crate (validates all nested structs)
        config.validate().map_err(|validation_errors| {
            // Convert validator::ValidationErrors to our ConfigurationError
            let messages: Vec<String> = validation_errors
                .field_errors()
                .iter()
                .flat_map(|(field, errors)| {
                    errors.iter().map(move |error| {
                        let msg = error.message.as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_else(|| format!("validation failed for {}", error.code));
                        format!("{}: {}", field, msg)
                    })
                })
                .collect();

            ConfigurationError::validation_error(format!(
                "Configuration validation failed:\n  - {}",
                messages.join("\n  - ")
            ))
        })?;

        Ok(config)
    }

    /// Load orchestration configuration with validation
    ///
    /// Ensures the `[orchestration]` section is present.
    ///
    /// # Example
    /// ```rust
    /// let config = ConfigLoader::load_for_orchestration(
    ///     "generated/orchestration-production.toml"
    /// )?;
    /// // Guaranteed to have orchestration config
    /// let orch = config.orchestration.as_ref().unwrap();
    /// ```
    pub fn load_for_orchestration(path: &Path) -> ConfigResult<TaskerConfig> {
        let config = Self::load(path)?;

        if config.orchestration.is_none() {
            return Err(ConfigurationError::validation_error(
                format!(
                    "Orchestration config file '{}' missing [orchestration] section",
                    path.display()
                )
            ));
        }

        Ok(config)
    }

    /// Load worker configuration with validation
    ///
    /// Ensures the `[worker]` section is present.
    pub fn load_for_worker(path: &Path) -> ConfigResult<TaskerConfig> {
        let config = Self::load(path)?;

        if config.worker.is_none() {
            return Err(ConfigurationError::validation_error(
                format!(
                    "Worker config file '{}' missing [worker] section",
                    path.display()
                )
            ));
        }

        Ok(config)
    }

    /// Load complete configuration with validation
    ///
    /// Ensures both `[orchestration]` and `[worker]` sections are present.
    /// Used for integration tests and all-in-one deployments.
    pub fn load_complete(path: &Path) -> ConfigResult<TaskerConfig> {
        let config = Self::load(path)?;

        if config.orchestration.is_none() {
            return Err(ConfigurationError::validation_error(
                "Complete config missing [orchestration] section"
            ));
        }

        if config.worker.is_none() {
            return Err(ConfigurationError::validation_error(
                "Complete config missing [worker] section"
            ));
        }

        Ok(config)
    }

    /// Determine config path from environment (fail-fast, no fallback)
    ///
    /// **Requires** `TASKER_CONFIG_PATH` environment variable to be set.
    /// No fallback paths - explicit configuration only.
    ///
    /// # Returns
    /// Absolute path to config file
    ///
    /// # Errors
    /// * `ConfigurationError::MissingEnvironmentVariable` - TASKER_CONFIG_PATH not set
    /// * `ConfigurationError::ConfigFileNotFound` - File doesn't exist at specified path
    ///
    /// # Example
    /// ```bash
    /// export TASKER_CONFIG_PATH=/app/config/orchestration-production.toml
    /// ```
    ///
    /// # Rationale
    /// Fail-fast configuration prevents deployment issues. Better to abort
    /// than silently use a wrong config file.
    pub fn determine_config_path() -> ConfigResult<PathBuf> {
        // Require explicit path - no fallback
        let path_str = std::env::var("TASKER_CONFIG_PATH")
            .map_err(|_| ConfigurationError::missing_environment_variable(
                "TASKER_CONFIG_PATH",
                "Configuration file path must be explicitly set. No fallback paths."
            ))?;

        let path = PathBuf::from(path_str);

        if !path.exists() {
            return Err(ConfigurationError::config_file_not_found(
                vec![path],
                "TASKER_CONFIG_PATH points to non-existent file"
            ));
        }

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;

    #[test]
    fn test_load_orchestration_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("orchestration-test.toml");

        let toml_content = r#"
[common]
[common.system]
version = "0.1.0"
environment = "test"

[common.database]
url = "postgresql://localhost/test"
pool_size = 5

[orchestration]
[orchestration.system]
max_concurrent_tasks = 10
"#;
        std::fs::write(&config_path, toml_content).unwrap();

        let config = ConfigLoader::load_for_orchestration(&config_path).unwrap();

        assert_eq!(config.common.database.url, "postgresql://localhost/test");
        assert!(config.orchestration.is_some());
        assert!(config.worker.is_none());
    }

    #[test]
    fn test_determine_config_path_requires_env_var() {
        // Ensure TASKER_CONFIG_PATH is not set
        env::remove_var("TASKER_CONFIG_PATH");

        // Should fail with missing environment variable error
        let result = ConfigLoader::determine_config_path();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("TASKER_CONFIG_PATH"));
    }

    #[test]
    fn test_determine_config_path_validates_file_exists() {
        // Set path to non-existent file
        env::set_var("TASKER_CONFIG_PATH", "/nonexistent/config.toml");

        // Should fail with file not found error
        let result = ConfigLoader::determine_config_path();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-existent"));

        env::remove_var("TASKER_CONFIG_PATH");
    }

    #[test]
    fn test_determine_config_path_success() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.toml");
        std::fs::write(&config_path, "").unwrap();

        env::set_var("TASKER_CONFIG_PATH", config_path.to_str().unwrap());

        let result = ConfigLoader::determine_config_path();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), config_path);

        env::remove_var("TASKER_CONFIG_PATH");
    }

    #[test]
    fn test_load_complete_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("complete-test.toml");

        let toml_content = r#"
[common]
[common.system]
version = "0.1.0"

[common.database]
url = "postgresql://localhost/test"

[orchestration]
[orchestration.system]
max_concurrent_tasks = 10

[worker]
[worker.step_processing]
max_concurrent_steps = 100
"#;
        std::fs::write(&config_path, toml_content).unwrap();

        let config = ConfigLoader::load_complete(&config_path).unwrap();

        assert!(config.orchestration.is_some());
        assert!(config.worker.is_some());
        assert!(config.is_complete());
    }

    #[test]
    fn test_missing_orchestration_section() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("incomplete.toml");

        let toml_content = r#"
[common]
[common.database]
url = "postgresql://localhost/test"
"#;
        std::fs::write(&config_path, toml_content).unwrap();

        let result = ConfigLoader::load_for_orchestration(&config_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("[orchestration]"));
    }
}
```

### 3. TOML Structure

#### 3.1 Source Files (Base + Environment Overrides)

These are the source files that ConfigMerger reads and merges:

```toml
# config/tasker/base/common.toml
# Shared configuration for all contexts

[system]
version = "0.1.0"
environment = "${TASKER_ENV:-development}"
max_recursion_depth = 50

[database]
url = "${DATABASE_URL}"
pool_size = 20
min_connections = 5
[database.pool]
max_connections = 20
min_connections = 5
idle_timeout_seconds = 600
max_lifetime_seconds = 1800
acquire_timeout_seconds = 30

[queues]
backend = "pgmq"
[queues.pgmq]
connection_pool_size = 10
[queues.orchestration_namespace]
task_requests = "orchestration_task_requests"
step_results = "orchestration_step_results"
task_finalization = "orchestration_task_finalization"

[circuit_breakers]
enabled = true
[circuit_breakers.global]
failure_threshold = 5
timeout_seconds = 60
half_open_max_requests = 3

[mpsc_channels]
[mpsc_channels.shared]
event_publisher_buffer_size = 5000
ruby_ffi_buffer_size = 1000
```

```toml
# config/tasker/base/orchestration.toml
# Orchestration-specific configuration

[event_systems]
deployment_mode = "Hybrid"
[event_systems.orchestration]
queue_name = "orchestration_step_results"
enable_fallback_poller = true
poller_interval_ms = 5000
[event_systems.task_readiness]
enable_fallback_poller = true
poller_interval_ms = 3000

[decision_points]
enabled = true
max_steps_per_decision = 50
max_decision_depth = 10
warn_threshold_steps = 20
warn_threshold_depth = 5
enable_detailed_logging = false
enable_metrics = true

[executor_pools]
[executor_pools.task_execution]
core_size = 10
max_size = 50
idle_timeout_seconds = 300
[executor_pools.step_discovery]
core_size = 5
max_size = 20

[mpsc_channels]
[mpsc_channels.command_processor]
command_buffer_size = 5000
[mpsc_channels.event_listeners]
orchestration_buffer_size = 10000
task_readiness_buffer_size = 5000

[system]
max_concurrent_tasks = 100
namespaces = ["default", "high_priority"]
enable_web_api = true
bind_address = "0.0.0.0:8080"
```

```toml
# config/tasker/base/worker.toml
# Worker-specific configuration

[event_systems]
deployment_mode = "Hybrid"
[event_systems.worker]
enable_fallback_poller = true
poller_interval_ms = 2000

[step_processing]
max_concurrent_steps = 1000
step_execution_timeout_seconds = 300
result_submission_timeout_seconds = 30
enable_step_metrics = true

[health_monitoring]
enable_health_checks = true
health_check_interval_seconds = 30
failure_threshold = 3
recovery_check_interval_seconds = 10

[mpsc_channels]
[mpsc_channels.command_processor]
command_buffer_size = 2000
[mpsc_channels.event_subscribers]
worker_buffer_size = 5000
in_process_events_buffer_size = 1000
```

#### 3.2 Environment Overrides

```toml
# config/tasker/environments/test/common.toml
# Test environment overrides (small buffers, fast timeouts)

[database]
pool_size = 5
[database.pool]
max_connections = 5
min_connections = 1

[mpsc_channels]
[mpsc_channels.shared]
event_publisher_buffer_size = 100
```

```toml
# config/tasker/environments/production/common.toml
# Production environment overrides (large buffers, monitoring)

[database]
pool_size = 50
[database.pool]
max_connections = 50
min_connections = 10

[circuit_breakers]
enabled = true
[circuit_breakers.global]
failure_threshold = 10

[mpsc_channels]
[mpsc_channels.shared]
event_publisher_buffer_size = 50000
```

#### 3.3 Generated Deployment File (What Runtime Loads)

```toml
# generated/orchestration-production.toml
# Generated by: cargo run --bin tasker-cli config generate \
#   --context orchestration --environment production

[common]
[common.system]
version = "0.1.0"
environment = "production"
max_recursion_depth = 50

[common.database]
url = "${DATABASE_URL}"
pool_size = 50  # From production override
min_connections = 10  # From production override
[common.database.pool]
max_connections = 50
min_connections = 10
idle_timeout_seconds = 600
max_lifetime_seconds = 1800
acquire_timeout_seconds = 30

[common.queues]
backend = "pgmq"
[common.queues.pgmq]
connection_pool_size = 10
[common.queues.orchestration_namespace]
task_requests = "orchestration_task_requests"
step_results = "orchestration_step_results"
task_finalization = "orchestration_task_finalization"

[common.circuit_breakers]
enabled = true
[common.circuit_breakers.global]
failure_threshold = 10  # From production override
timeout_seconds = 60
half_open_max_requests = 3

[common.mpsc_channels]
[common.mpsc_channels.shared]
event_publisher_buffer_size = 50000  # From production override
ruby_ffi_buffer_size = 1000

[orchestration]
[orchestration.event_systems]
deployment_mode = "Hybrid"
[orchestration.event_systems.orchestration]
queue_name = "orchestration_step_results"
enable_fallback_poller = true
poller_interval_ms = 5000
[orchestration.event_systems.task_readiness]
enable_fallback_poller = true
poller_interval_ms = 3000

[orchestration.decision_points]
enabled = true
max_steps_per_decision = 50
max_decision_depth = 10
warn_threshold_steps = 20
warn_threshold_depth = 5
enable_detailed_logging = false
enable_metrics = true

[orchestration.executor_pools]
[orchestration.executor_pools.task_execution]
core_size = 10
max_size = 50
idle_timeout_seconds = 300
[orchestration.executor_pools.step_discovery]
core_size = 5
max_size = 20

[orchestration.mpsc_channels]
[orchestration.mpsc_channels.command_processor]
command_buffer_size = 5000
[orchestration.mpsc_channels.event_listeners]
orchestration_buffer_size = 10000
task_readiness_buffer_size = 5000

[orchestration.system]
max_concurrent_tasks = 100
namespaces = ["default", "high_priority"]
enable_web_api = true
bind_address = "0.0.0.0:8080"

# Note: No [worker] section in orchestration context file
```

### 4. ConfigMerger Updates

```rust
// tasker-shared/src/config/merger.rs (updated merge_context method)

impl ConfigMerger {
    /// Merge a specific context configuration into a single deployable file
    ///
    /// This loads base + environment configs and merges them into a single
    /// TOML file with [common] and context-specific sections.
    ///
    /// # Workflow
    ///
    /// 1. Load base/common.toml
    /// 2. Merge with environments/{env}/common.toml
    /// 3. Load base/{context}.toml (orchestration, worker, or both for complete)
    /// 4. Merge with environments/{env}/{context}.toml
    /// 5. Create output with [common] and [{context}] top-level sections
    /// 6. Strip _docs sections
    /// 7. Serialize to TOML string
    ///
    /// # Arguments
    /// * `context` - "orchestration", "worker", or "complete"
    ///
    /// # Returns
    /// TOML string ready to write to deployment file
    ///
    /// # Example
    /// ```rust
    /// let mut merger = ConfigMerger::new(
    ///     PathBuf::from("config/tasker"),
    ///     "production"
    /// )?;
    ///
    /// let merged = merger.merge_context("orchestration")?;
    /// std::fs::write("generated/orchestration-production.toml", merged)?;
    /// ```
    pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
        info!(
            "Merging context '{}' for environment '{}'",
            context, self.environment
        );

        // 1. Load and merge common config
        let mut common_base = self.loader.load_context_toml("common")?;
        debug!("Loaded base common.toml");

        // Merge with environment override if it exists
        if let Ok(common_env) = self.loader.load_environment_toml(&self.environment, "common") {
            debug!("Merging environment common.toml");
            deep_merge_toml(&mut common_base, common_env)?;
        }

        // 2. Create output structure with [common] section
        let mut output = toml::Value::Table(toml::map::Map::new());
        output
            .as_table_mut()
            .unwrap()
            .insert("common".to_string(), common_base);

        // 3. Load and merge context-specific config
        match context {
            "orchestration" => {
                let mut orch_base = self.loader.load_context_toml("orchestration")?;
                debug!("Loaded base orchestration.toml");

                if let Ok(orch_env) =
                    self.loader
                        .load_environment_toml(&self.environment, "orchestration")
                {
                    debug!("Merging environment orchestration.toml");
                    deep_merge_toml(&mut orch_base, orch_env)?;
                }

                output
                    .as_table_mut()
                    .unwrap()
                    .insert("orchestration".to_string(), orch_base);
            }
            "worker" => {
                let mut worker_base = self.loader.load_context_toml("worker")?;
                debug!("Loaded base worker.toml");

                if let Ok(worker_env) =
                    self.loader
                        .load_environment_toml(&self.environment, "worker")
                {
                    debug!("Merging environment worker.toml");
                    deep_merge_toml(&mut worker_base, worker_env)?;
                }

                output
                    .as_table_mut()
                    .unwrap()
                    .insert("worker".to_string(), worker_base);
            }
            "complete" => {
                // Load both orchestration and worker
                let mut orch_base = self.loader.load_context_toml("orchestration")?;
                if let Ok(orch_env) =
                    self.loader
                        .load_environment_toml(&self.environment, "orchestration")
                {
                    deep_merge_toml(&mut orch_base, orch_env)?;
                }
                output
                    .as_table_mut()
                    .unwrap()
                    .insert("orchestration".to_string(), orch_base);

                let mut worker_base = self.loader.load_context_toml("worker")?;
                if let Ok(worker_env) =
                    self.loader
                        .load_environment_toml(&self.environment, "worker")
                {
                    deep_merge_toml(&mut worker_base, worker_env)?;
                }
                output
                    .as_table_mut()
                    .unwrap()
                    .insert("worker".to_string(), worker_base);
            }
            _ => {
                return Err(ConfigurationError::validation_error(&format!(
                    "Unknown context: {}. Valid contexts: orchestration, worker, complete",
                    context
                )))
            }
        }

        // 4. Strip _docs sections from output
        Self::strip_docs_from_value(&mut output);

        // 5. Serialize to TOML string
        let merged_toml = toml::to_string_pretty(&output).map_err(|e| {
            ConfigurationError::json_serialization_error("merged config output", e)
        })?;

        info!(
            "Successfully merged {} configuration ({} bytes)",
            context,
            merged_toml.len()
        );

        Ok(merged_toml)
    }
}
```

### 5. Bootstrap Usage Patterns

#### 5.1 Orchestration Bootstrap

```rust
// tasker-orchestration/src/orchestration/bootstrap.rs (updated)

pub async fn bootstrap_orchestration() -> TaskerResult<OrchestrationCore> {
    info!("Bootstrapping orchestration service");

    // 1. Determine config path (fail-fast if TASKER_CONFIG_PATH not set)
    let config_path = ConfigLoader::determine_config_path()?;
    info!("Loading config from: {}", config_path.display());

    // 2. Load and validate configuration
    let config = ConfigLoader::load_for_orchestration(&config_path)?;
    info!("Configuration loaded and validated successfully");

    // 3. Create SystemContext (accesses config.common internally)
    let system_context = SystemContext::new(Arc::new(config.clone()))?;
    info!("SystemContext initialized");

    // 4. Access orchestration-specific config
    let orch_config = config
        .orchestration
        .as_ref()
        .expect("orchestration config validated by loader");

    // 5. Initialize orchestration components
    let event_systems = initialize_event_systems(&config, orch_config)?;
    let executor_pools = initialize_executor_pools(&orch_config.executor_pools)?;
    let actor_registry = initialize_actors(&system_context, orch_config)?;

    // 6. Build OrchestrationCore
    Ok(OrchestrationCore {
        system_context,
        event_systems,
        executor_pools,
        actor_registry,
        config: Arc::new(config),
    })
}
```

#### 5.2 Worker Bootstrap

```rust
// tasker-worker/src/bootstrap.rs (updated)

pub async fn bootstrap_worker() -> TaskerResult<WorkerCore> {
    info!("Bootstrapping worker service");

    // 1. Determine config path (fail-fast if TASKER_CONFIG_PATH not set)
    let config_path = ConfigLoader::determine_config_path()?;
    info!("Loading config from: {}", config_path.display());

    // 2. Load and validate configuration
    let config = ConfigLoader::load_for_worker(&config_path)?;
    info!("Configuration loaded and validated successfully");

    // 3. Create SystemContext (accesses config.common internally)
    let system_context = SystemContext::new(Arc::new(config.clone()))?;
    info!("SystemContext initialized");

    // 4. Access worker-specific config
    let worker_config = config
        .worker
        .as_ref()
        .expect("worker config validated by loader");

    // 5. Initialize worker components
    let event_systems = initialize_event_systems(&config, worker_config)?;
    let step_processor = initialize_step_processor(worker_config)?;
    let health_monitor = initialize_health_monitor(&worker_config.health_monitoring)?;

    // 6. Build WorkerCore
    Ok(WorkerCore {
        system_context,
        event_systems,
        step_processor,
        health_monitor,
        config: Arc::new(config),
    })
}
```

#### 5.3 SystemContext Field Access Updates

```rust
// tasker-shared/src/system_context.rs (field access pattern updates)

impl SystemContext {
    pub fn new(config: Arc<TaskerConfig>) -> TaskerResult<Self> {
        info!("Initializing SystemContext");

        // OLD: let pool_config = &config.database.pool;
        // NEW: Access through config.common
        let pool_config = &config.common.database.pool;

        // OLD: let database_url = config.database_url();
        // NEW: Access through config.common
        let database_url = config.common.database_url();

        // Create connection pool
        let pool = Self::create_pool(&database_url, pool_config).await?;

        // OLD: let circuit_breakers = config.circuit_breakers.clone();
        // NEW: Access through config.common
        let circuit_breakers = config.common.circuit_breakers.clone();

        // Create circuit breaker manager
        let circuit_breaker_manager =
            CircuitBreakerManager::new(circuit_breakers, pool.clone())?;

        // OLD: let queues = config.queues.clone();
        // NEW: Access through config.common
        let queues = config.common.queues.clone();

        // Initialize PGMQ queues
        Self::initialize_queues(&pool, &queues).await?;

        Ok(Self {
            pool,
            circuit_breaker_manager,
            queues,
            config,
        })
    }
}
```

### 6. CLI Workflow

#### 6.1 Config Generation

```bash
# Generate orchestration production config
cargo run --bin tasker-cli config generate \
  --context orchestration \
  --environment production \
  --output generated/orchestration-production.toml

# Generate worker development config
cargo run --bin tasker-cli config generate \
  --context worker \
  --environment development \
  --output generated/worker-development.toml

# Generate complete config for integration tests
cargo run --bin tasker-cli config generate \
  --context complete \
  --environment test \
  --output generated/complete-test.toml
```

#### 6.2 Config Validation

```bash
# Validate generated config can be loaded
cargo run --bin tasker-cli config dump \
  --file generated/orchestration-production.toml \
  --format json

# Check what's in the config
cargo run --bin tasker-cli config dump \
  --file generated/orchestration-production.toml \
  --format toml \
  | less
```

#### 6.3 Deployment Workflow

```bash
# 1. Generate config for target environment
make generate-config ENV=production CONTEXT=orchestration

# 2. Copy to deployment location
cp generated/orchestration-production.toml /app/config/

# 3. Set REQUIRED environment variables
export TASKER_CONFIG_PATH=/app/config/orchestration-production.toml  # REQUIRED - no fallback
export DATABASE_URL=postgresql://prod-db:5432/tasker

# 4. Run orchestration service (will fail-fast if TASKER_CONFIG_PATH not set)
./bin/orchestration-server
```

**Critical**: `TASKER_CONFIG_PATH` **must** be set. If missing or pointing to non-existent file,
the service will immediately abort with a clear error message. This prevents silent misconfigurations.

## Migration Strategy

### Phase 2d Migration Tiers

**Tier 1**: SystemContext field access patterns
- Change: `config.field` → `config.common.field`
- Effort: 1-2 hours
- Risk: Low (no signature changes)

**Tier 2**: Bootstrap config loading
- Change: `UnifiedConfigLoader` → `ConfigLoader`
- Effort: 2-3 hours
- Risk: Low (well-defined interfaces)

**Tier 3**: Config access throughout codebase
- Change: Add `.common` or `.orchestration.as_ref()?` to field access
- Effort: 3-4 hours
- Risk: Medium (many files, but mechanical)

**Tier 4**: Integration tests
- Change: Use `ConfigLoader::load_complete()` for test environments
- Effort: 2-3 hours
- Risk: Low (tests are flexible)

**Total Estimated Effort**: 11-17 hours

## Design Rationale

### Why Use `validator` Crate?

**Benefits**:
- **Greenfield timing**: Perfect opportunity to adopt ecosystem best practices
- **Boilerplate reduction**: 100 config structs benefit from declarative validation
- **Self-documenting**: `#[validate(range(min = 10, max = 1000000))]` is clear and explicit
- **Better DX**: Multiple validation errors returned at once (not fail-fast)
- **Nested validation**: `#[validate]` on fields automatically validates nested structs
- **TAS-58 alignment**: Uses ecosystem-standard tool (4M+ downloads, actively maintained)
- **Structured errors**: Field paths included in error messages
- **Domain-focused**: Custom validators only for Tasker-specific logic (e.g., `${DATABASE_URL}` templates)

**Trade-offs**:
- Additional dependency (~100KB, but well-maintained and stable)
- Team learns validator macro syntax (straightforward, excellent documentation)
- Must resist over-validation temptation (operational concerns belong in deployment, not app)

**Pragmatic Scope**:
- ✅ **Validate**: PostgreSQL URL format, templates, basic ranges (> 0)
- ✅ **Let serde handle**: Enum variants (deserialization fails naturally)
- ❌ **Don't validate**: Operational tuning (pool max vs min, extreme buffer sizes)
- ❌ **Don't validate**: Deployment concerns (DevOps owns those decisions)

**Decision**: The benefits far outweigh the costs. Validation focuses on domain logic, not second-guessing deployment configurations.

### Why Single File Loading?

**Benefits**:
- Simple runtime: Just read + deserialize + validate, ~50 lines
- Clear separation: Runtime vs CLI responsibilities
- Easy deployment: Single file to copy/version
- Fast startup: No file system traversal or merging
- Testable: Can inspect generated file directly
- Validation at load: All constraints checked before service starts

**Trade-offs**:
- Must run config generation before deployment (acceptable for greenfield)
- Config changes require regeneration (but this is explicit and trackable)

### Why Optional Contexts?

**Benefits**:
- Type-safe: `Option<T>` enforces context presence
- Single struct: No need for CompleteConfig wrapper
- Flexible: Same struct works for all contexts
- Validated: Loader methods check correct sections present
- Nested validation: validator crate validates Option<T> fields automatically

**Trade-offs**:
- Some `.as_ref()?.field` patterns needed (but validates at load time)
- Could be `None` at runtime (but prevented by loader validation)

### Why Keep TaskerConfig?

**Benefits**:
- Minimal breaking changes: SystemContext signature unchanged
- Migration path: Change field access, not parameters
- Single source: Config passed through whole stack
- Familiar: Existing code already uses TaskerConfig

**Trade-offs**:
- Extra layer of nesting (but matches TOML structure exactly)
- Not as "pure" as separate types (but pragmatic for migration)

## Success Criteria

- [ ] `validator` crate integrated in tasker-shared
- [ ] All config structs have `#[derive(Validate)]` and validation attributes
- [ ] Custom validator `validate_postgres_url` implemented in `config/validators.rs`
- [ ] DeploymentMode enum defined (no custom validator - serde handles it)
- [ ] ConfigLoader implemented with validator integration (~75 lines total)
- [ ] `determine_config_path()` requires `TASKER_CONFIG_PATH` - no fallback, fail-fast
- [ ] Clear error messages when TASKER_CONFIG_PATH missing or invalid
- [ ] Config generation produces valid single files
- [ ] SystemContext accesses config.common fields
- [ ] Bootstrap loads single file via ConfigLoader
- [ ] All 377+ tests pass (including fail-fast behavior tests)
- [ ] Zero clippy warnings
- [ ] Config loading < 10ms (including validation)
- [ ] Validation errors include field paths and helpful messages
- [ ] Validation focuses on domain logic, not operational concerns
- [ ] Documentation complete

## References

- [Phase 2 Refactor Plan](./phase-2-refactor-plan.md)
- [SystemContext Dependencies Analysis](./systemcontext-deps.txt)
- [Config Usage Analysis](./config-usage.json)
- [Breaking Changes Map](./breaking-changes-map.md)

---

**Document Status**: Complete - Ready for Implementation
**Next Step**: Phase 2b - Archive existing TOML and rebuild structure
