# TAS-50: Configuration Generation and Validation - Implementation Plan

**Document Version**: 1.0
**Date**: 2025-10-14
**Status**: Ready for Development
**Dependencies**:
- Analysis complete (configuration-usage-matrix.md)
- Categories defined (configuration-categories-and-mapping.md)

---

## Executive Summary

This implementation plan details the complete redesign of the Tasker configuration system to address issues discovered during TAS-51 MPSC channel migration. The plan uses a **phased, non-breaking migration** strategy that maintains backward compatibility while enabling context-specific configuration.

### Problem Statement

Current monolithic `TaskerConfig` forces unnecessary configuration loading:
- **Orchestration**: 62.5% efficiency (50/80 fields used, 37.5% waste)
- **Worker** (any language): 25% efficiency (20/80 fields used, **75% waste**)

### Solution Overview

1. **Context-Specific Configuration**: Split monolithic config into **2 language-agnostic contexts**
   - **CommonConfig** (~40 fields) - Shared infrastructure
   - **OrchestrationConfig** (~51 fields) - Orchestration-specific
   - **WorkerConfig** (~70 fields) - **Language-agnostic worker config** (Rust/Ruby/Python/WASM)

2. **Environment Variable Override Pattern**: Support K8s secrets and testing flexibility
   - `DATABASE_URL` env var overrides TOML for secrets rotation
   - `TASKER_TEMPLATE_PATH` env var overrides TOML for testing
   - All other config from TOML (not ENV)

3. **Worker Foundation Pattern**: `tasker-worker` library used by all languages
   - Rust workers: Use tasker-worker library directly
   - Ruby workers: Bootstrap tasker-worker via FFI, read same WorkerConfig TOML
   - Future workers (Python/WASM): Same pattern, same TOML

4. **CLI Tooling**: Build comprehensive configuration generation and validation tools
   - TOML generator for both contexts
   - Robust validator with actionable error messages
   - Interactive wizard for guided setup

5. **Phased Migration**: 4-phase rollout maintaining backward compatibility
   - Phase 1: Add context-specific structs (non-breaking)
   - Phase 2: Remove unused config (non-breaking)
   - Phase 3: Context-specific loading (breaking, opt-in)
   - Phase 4: CLI tools and migration complete

---

## Part 1: Configuration Redesign Plan

### 1.1 New Module Structure

```
tasker-shared/src/config/
├── mod.rs                          # Public API, re-exports
├── loader.rs                       # ConfigManager with context support
├── validator.rs                    # Validation logic
├── environment.rs                  # Environment detection
├── overrides.rs                    # ENV variable override logic
├── contexts/
│   ├── mod.rs
│   ├── common.rs                   # CommonConfig
│   ├── orchestration.rs            # OrchestrationConfig
│   └── worker.rs                   # WorkerConfig (language-agnostic)
├── components/                     # Individual config components
│   ├── mod.rs
│   ├── database.rs                 # DatabaseConfig
│   ├── queues.rs                   # QueuesConfig
│   ├── backoff.rs                  # BackoffConfig
│   ├── orchestration_system.rs     # OrchestrationSystemConfig
│   ├── worker_system.rs            # WorkerSystemConfig
│   ├── event_systems.rs            # Event system configs
│   ├── mpsc_channels.rs            # MPSC channel configs
│   ├── web.rs                      # Web API configs (shared)
│   └── circuit_breakers.rs         # CircuitBreakerConfig (if not deprecated)
├── legacy.rs                       # TaskerConfig (backward compat)
└── conversions.rs                  # From<TaskerConfig> traits
```

**New Files**: 18 new files
**Modified Files**: Existing `mod.rs`, `loader.rs`
**Deprecated Files**: Current monolithic config structure (kept for backward compat)

**Key Addition**: `overrides.rs` - Handles ENV var overrides for DATABASE_URL and TASKER_TEMPLATE_PATH

---

### 1.2 Core Configuration Traits

```rust
/// Trait for all configuration contexts
pub trait ConfigurationContext: Serialize + Deserialize + Clone + Debug {
    /// Validate this configuration context
    fn validate(&self) -> Result<(), Vec<ValidationError>>;

    /// Get the environment this configuration is for
    fn environment(&self) -> &str;

    /// Get configuration summary for logging
    fn summary(&self) -> String;
}

/// Configuration loading context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigContext {
    /// Load common + orchestration configuration
    Orchestration,

    /// Load common + worker configuration (language-agnostic)
    Worker,

    /// Load all configuration (combined deployment)
    Combined,

    /// Load legacy monolithic TaskerConfig (backward compatibility)
    Legacy,
}

/// Configuration manager with context support
pub struct ConfigManager {
    /// Configuration context
    context: ConfigContext,

    /// Current environment
    environment: String,

    /// Base configuration directory
    config_root: PathBuf,

    /// Loaded configuration (boxed to support different types)
    config: Box<dyn Any + Send + Sync>,
}

impl ConfigManager {
    /// Load configuration for specific context
    pub fn load_for_context(context: ConfigContext) -> Result<Self, ConfigError> {
        // Environment detection
        let environment = Environment::detect()?;

        // Base directory discovery
        let config_root = Self::discover_config_root()?;

        // Load configuration based on context
        let config: Box<dyn Any + Send + Sync> = match context {
            ConfigContext::Orchestration => {
                let common = Self::load_common(&config_root, &environment)?;
                let orchestration = Self::load_orchestration(&config_root, &environment)?;
                Box::new((common, orchestration))
            }
            ConfigContext::Worker => {
                let common = Self::load_common(&config_root, &environment)?;
                let worker = Self::load_worker(&config_root, &environment)?;
                Box::new((common, worker))
            }
            ConfigContext::Combined => {
                let common = Self::load_common(&config_root, &environment)?;
                let orchestration = Self::load_orchestration(&config_root, &environment)?;
                let worker = Self::load_worker(&config_root, &environment)?;
                Box::new((common, orchestration, worker))
            }
            ConfigContext::Legacy => {
                let legacy = Self::load_legacy(&config_root, &environment)?;
                Box::new(legacy)
            }
        };

        Ok(Self {
            context,
            environment,
            config_root,
            config,
        })
    }

    /// Get common configuration (if available)
    pub fn common(&self) -> Option<&CommonConfig> {
        match self.context {
            ConfigContext::Orchestration | ConfigContext::Worker | ConfigContext::Combined => {
                self.config.downcast_ref::<(CommonConfig, _)>()
                    .map(|(common, _)| common)
            }
            _ => None,
        }
    }

    /// Get orchestration configuration (if available)
    pub fn orchestration(&self) -> Option<&OrchestrationConfig> {
        match self.context {
            ConfigContext::Orchestration => {
                self.config.downcast_ref::<(CommonConfig, OrchestrationConfig)>()
                    .map(|(_, orch)| orch)
            }
            ConfigContext::Combined => {
                self.config.downcast_ref::<(CommonConfig, OrchestrationConfig, WorkerConfig)>()
                    .map(|(_, orch, _)| orch)
            }
            _ => None,
        }
    }

    /// Get worker configuration (if available) - language-agnostic
    pub fn worker(&self) -> Option<&WorkerConfig> {
        match self.context {
            ConfigContext::Worker => {
                self.config.downcast_ref::<(CommonConfig, WorkerConfig)>()
                    .map(|(_, worker)| worker)
            }
            ConfigContext::Combined => {
                self.config.downcast_ref::<(CommonConfig, OrchestrationConfig, WorkerConfig)>()
                    .map(|(_, _, worker)| worker)
            }
            _ => None,
        }
    }

    /// Get legacy configuration (if available)
    pub fn legacy(&self) -> Option<&TaskerConfig> {
        match self.context {
            ConfigContext::Legacy => {
                self.config.downcast_ref::<TaskerConfig>()
            }
            _ => None,
        }
    }
}
```

---

### 1.3 Validation Framework

```rust
/// Validation error with context
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("Missing required field: {field} in {context}")]
    MissingField { field: String, context: String },

    #[error("Invalid value for {field}: {value} (reason: {reason})")]
    InvalidValue {
        field: String,
        value: String,
        reason: String,
    },

    #[error("Conflicting configuration: {field1} and {field2} cannot both be {value}")]
    ConflictingFields {
        field1: String,
        field2: String,
        value: String,
    },

    #[error("Environment variable {var} is required but not set")]
    MissingEnvironmentVariable { var: String },

    #[error("Invalid {item}: {value} (allowed values: {allowed})")]
    InvalidEnum {
        item: String,
        value: String,
        allowed: String,
    },

    #[error("File not found: {path} (context: {context})")]
    FileNotFound { path: String, context: String },

    #[error("Custom validation failed: {message}")]
    Custom { message: String },
}

impl ValidationError {
    /// Get actionable suggestion for fixing this error
    pub fn suggestion(&self) -> String {
        match self {
            ValidationError::MissingField { field, context } => {
                format!(
                    "Add '{}' to your configuration file in the {} section",
                    field, context
                )
            }
            ValidationError::InvalidValue { field, reason, .. } => {
                format!("Fix field '{}': {}", field, reason)
            }
            ValidationError::MissingEnvironmentVariable { var } => {
                format!("Set environment variable: export {}=<value>", var)
            }
            ValidationError::InvalidEnum { item, allowed, .. } => {
                format!("Change {} to one of: {}", item, allowed)
            }
            ValidationError::FileNotFound { path, .. } => {
                format!("Create file at path: {}", path)
            }
            _ => "Check your configuration and try again".to_string(),
        }
    }
}

/// Validation result with comprehensive error reporting
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Check if validation passed
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Format errors for display
    pub fn format_errors(&self) -> String {
        let mut output = String::new();

        if !self.errors.is_empty() {
            output.push_str(&format!("\n{} Validation Error(s):\n\n", self.errors.len()));
            for (i, error) in self.errors.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, error));
                output.push_str(&format!("   Suggestion: {}\n\n", error.suggestion()));
            }
        }

        if !self.warnings.is_empty() {
            output.push_str(&format!("\n{} Warning(s):\n\n", self.warnings.len()));
            for (i, warning) in self.warnings.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, warning));
            }
        }

        output
    }
}
```

---

## Part 2: Migration Plan

### Phase 1: Add Context-Specific Structs (Non-Breaking)

**Timeline**: 1-2 weeks
**Risk**: Low
**Breaking Changes**: None

#### Tasks:

1. **Create new module structure** (2 days)
   - Create `config/contexts/` directory
   - Create `config/components/` directory
   - Set up module hierarchy

2. **Implement CommonConfig** (1 day)
   - Define struct with database, queues, environment, mpsc_channels.shared
   - Implement `From<&TaskerConfig>` conversion
   - Add unit tests

3. **Implement OrchestrationConfig** (2 days)
   - Define struct with backoff, orchestration, event_systems, mpsc_channels
   - Implement `From<&TaskerConfig>` conversion
   - Add unit tests

4. **Implement WorkerConfig** (2 days)
   - Define struct with worker, event_systems.worker, mpsc_channels.worker
   - Add `template_path: Option<PathBuf>` field for TOML-based template discovery
   - Implement `From<&TaskerConfig>` conversion
   - Add unit tests

5. **Implement ENV override logic** (1 day)
   - Add `overrides.rs` module
   - Implement `database_url()` method with ENV > TOML precedence
   - Implement `template_path()` method with ENV > TOML precedence
   - Add unit tests for override behavior

6. **Add ConfigManager context support** (2 days)
   - Implement `load_for_context(ConfigContext)` method
   - Keep existing `load()` method for backward compatibility
   - Add integration tests

**Deliverables**:
- New context-specific configuration structs
- Conversion traits from TaskerConfig
- ConfigManager with context support
- 100% backward compatibility maintained

**Validation**:
```rust
#[test]
fn test_backward_compatibility() {
    // Old code still works
    let config = ConfigManager::load().unwrap();
    assert!(config.database.pool.max_connections > 0);

    // New code also works
    let config = ConfigManager::load_for_context(ConfigContext::Orchestration).unwrap();
    let common = config.common().unwrap();
    assert!(common.database.pool.max_connections > 0);
}
```

---

### Phase 2: Remove Unused Configuration (Non-Breaking)

**Timeline**: 1 week
**Risk**: Low
**Breaking Changes**: Deprecation warnings only

#### Tasks:

1. **Add deprecation warnings** (1 day)
   - Mark unused fields with `#[deprecated]` attribute
   - Add console warnings when loading deprecated config
   - Document deprecated fields in migration guide

2. **Remove unused TOML files** (1 day)
   - Delete 11 unused TOML files:
     - `base/telemetry.toml`
     - `base/task_templates.toml`
     - `base/engine.toml`
     - `base/state_machine.toml`
     - `base/execution.toml`
     - Environment overrides for above
   - Update documentation

3. **Remove unused fields from structs** (2 days)
   - Remove from TaskerConfig (with deprecation):
     - `telemetry.*`
     - `task_templates.*`
     - `engine.*`
     - `state_machine.*`
     - `execution.max_concurrent_tasks`
     - `database.enable_secondary_database`
     - `queues.rabbitmq`
   - Update tests

4. **Verify circuit breaker deprecation** (1 day)
   - Analyze CircuitBreakerManager usage
   - If deprecated, remove from configs
   - If used, keep in CommonConfig

5. **Update documentation** (1 day)
   - Document deprecated fields
   - Update migration guide
   - Update examples

**Deliverables**:
- 11 fewer TOML files (24% reduction)
- ~20-30 fewer configuration fields
- Deprecation warnings guide users
- Backward compatibility maintained

**Validation**:
```bash
# Run tests with deprecated config
TASKER_ENV=test cargo test --all-features

# Should see deprecation warnings but still pass
Warning: Configuration field 'telemetry' is deprecated and will be removed in 2.0
Warning: Configuration file 'telemetry.toml' is no longer used
```

---

### Phase 3: Context-Specific Loading (Breaking, Opt-In)

**Timeline**: 2-3 weeks
**Risk**: Medium
**Breaking Changes**: Yes (opt-in via environment variable)

#### Tasks:

1. **Implement context detection** (2 days)
   - Add `TASKER_CONFIG_CONTEXT` environment variable
   - Auto-detect from binary name (`tasker-orchestration` vs `tasker-worker`)
   - Add `--config-context` CLI flag

2. **Update OrchestrationCore bootstrap** (2 days)
   - Change to `ConfigManager::load_for_context(ConfigContext::Orchestration)`
   - Update SystemContext creation
   - Add feature flag for opt-in: `context-specific-config`

3. **Update WorkerBootstrap** (2 days)
   - Change to `ConfigManager::load_for_context(ConfigContext::Worker)`
   - Update SystemContext creation
   - Add feature flag for opt-in
   - Apply ENV overrides for DATABASE_URL and TASKER_TEMPLATE_PATH

5. **Add configuration summary logging** (1 day)
   - Log which context was loaded
   - Log configuration summary on startup
   - Include in health endpoints

6. **Comprehensive testing** (3 days)
   - Test orchestration-only deployment
   - Test rust-worker-only deployment
   - Test ruby-worker-only deployment
   - Test combined deployment
   - Test legacy deployment

**Opt-In Strategy**:
```rust
// Default: Legacy mode (backward compatible)
#[cfg(not(feature = "context-specific-config"))]
let config_manager = ConfigManager::load()?;

// Opt-in: Context-specific mode (breaking change)
#[cfg(feature = "context-specific-config")]
let config_manager = ConfigManager::load_for_context(ConfigContext::Orchestration)?;
```

**Deliverables**:
- Context-specific configuration loading
- 100% configuration efficiency
- Feature flag for opt-in
- Comprehensive test coverage

**Validation**:
```bash
# Test with feature flag enabled
cargo test --all-features --features context-specific-config

# Test orchestration deployment
TASKER_CONFIG_CONTEXT=orchestration cargo run --bin tasker-orchestration

# Test worker deployment
TASKER_CONFIG_CONTEXT=worker cargo run --bin tasker-worker

# Test legacy deployment (no feature flag)
cargo run --bin tasker-orchestration
```

---

### Phase 4: CLI Tools Development

**Timeline**: 3-4 weeks
**Risk**: Low
**Breaking Changes**: None (new tools)

---

## Part 3: CLI Design Plan

### 3.1 CLI Architecture

```
tasker-client/src/bin/
├── tasker-cli.rs                   # Main CLI entry point (existing)
├── commands/
│   ├── mod.rs
│   ├── config/                     # New: Configuration commands
│   │   ├── mod.rs
│   │   ├── generate.rs             # Generate configuration
│   │   ├── validate.rs             # Validate configuration
│   │   ├── wizard.rs               # Interactive wizard
│   │   └── convert.rs              # Convert legacy → context-specific
│   └── ... (other commands)
```

### 3.2 CLI Command Structure

```bash
# Main CLI
tasker-cli config <subcommand> [options]

# Subcommands:
tasker-cli config generate [options]        # Generate configuration
tasker-cli config validate [options]        # Validate configuration
tasker-cli config wizard                    # Interactive wizard
tasker-cli config convert [options]         # Convert legacy config
```

---

### 3.3 Generate Command

**Purpose**: Generate context-specific TOML or ENV configuration files

```bash
tasker-cli config generate \
    --context <orchestration|worker|combined> \
    --environment <test|development|production> \
    --output <file|directory> \
    [--template <template-file>] \
    [--interactive]

# Examples:

# Generate orchestration config
tasker-cli config generate --context orchestration --environment production --output config/tasker.toml

# Generate worker config (used by Rust/Ruby/Python workers)
tasker-cli config generate --context worker --environment production --output config/worker.toml

# Generate all configs for combined deployment
tasker-cli config generate --context combined --environment production --output config/
```

**Implementation**:
```rust
pub struct GenerateCommand {
    /// Configuration context
    pub context: ConfigContext,

    /// Target environment
    pub environment: String,

    /// Output file or directory
    pub output: PathBuf,

    /// Template file (optional)
    pub template: Option<PathBuf>,

    /// Interactive mode
    pub interactive: bool,
}

impl GenerateCommand {
    pub async fn execute(&self) -> Result<(), GenerateError> {
        // 1. Load base configuration for context
        let base_config = self.load_base_config()?;

        // 2. Apply environment-specific overrides
        let config = self.apply_environment_overrides(base_config, &self.environment)?;

        // 3. If interactive, prompt for customizations
        if self.interactive {
            config = self.interactive_customization(config)?;
        }

        // 4. Validate configuration
        config.validate()?;

        // 5. Generate TOML output file(s)
        // All contexts use TOML (ENV overrides handled at runtime)
        self.generate_toml(&config, &self.output)?;

        println!("✓ Configuration generated successfully: {}", self.output.display());
        Ok(())
    }

    fn generate_toml(&self, config: &dyn ConfigurationContext, output: &Path) -> Result<(), GenerateError> {
        let toml_string = toml::to_string_pretty(config)?;

        // Add header comment
        let header = format!(
            "# Tasker Configuration - {} Context\n\
             # Environment: {}\n\
             # Generated: {}\n\
             # DO NOT EDIT: This file is auto-generated. Use 'tasker-cli config generate' to regenerate.\n\
             #\n\
             # Environment Variable Overrides:\n\
             # - DATABASE_URL: Override database.url (K8s secrets rotation)\n\
             # - TASKER_TEMPLATE_PATH: Override worker.template_path (testing)\n\n",
            self.context,
            self.environment,
            chrono::Utc::now().to_rfc3339()
        );

        fs::write(output, format!("{}{}", header, toml_string))?;
        Ok(())
    }
}
```

---

### 3.4 Validate Command

**Purpose**: Validate configuration files with detailed, actionable error messages

```bash
tasker-cli config validate \
    --config <file|directory> \
    --context <orchestration|worker|combined> \
    [--environment <test|development|production>] \
    [--strict]

# Examples:

# Validate orchestration config
tasker-cli config validate --config config/tasker.toml --context orchestration

# Validate with environment-specific rules
tasker-cli config validate --config config/tasker.toml --context orchestration --environment production --strict

# Validate worker config (used by all worker languages)
tasker-cli config validate --config config/worker.toml --context worker
```

**Implementation**:
```rust
pub struct ValidateCommand {
    /// Configuration file or directory to validate
    pub config: PathBuf,

    /// Expected configuration context
    pub context: ConfigContext,

    /// Expected environment (optional)
    pub environment: Option<String>,

    /// Strict mode (fail on warnings)
    pub strict: bool,
}

impl ValidateCommand {
    pub async fn execute(&self) -> Result<(), ValidateError> {
        println!("Validating configuration: {}", self.config.display());
        println!("Context: {:?}", self.context);

        // 1. Load configuration
        let config = self.load_config()?;

        // 2. Validate structure
        let validation_result = config.validate()?;

        // 3. Check environment-specific rules
        if let Some(ref env) = self.environment {
            self.validate_environment_specific(&config, env)?;
        }

        // 4. Display results
        if validation_result.is_valid() {
            println!("\n✓ Configuration is valid!");

            if !validation_result.warnings.is_empty() {
                println!("\n⚠ Warnings:");
                for warning in &validation_result.warnings {
                    println!("  - {}", warning);
                }

                if self.strict {
                    return Err(ValidateError::WarningsInStrictMode);
                }
            }
        } else {
            println!("\n✗ Configuration validation failed!\n");
            println!("{}", validation_result.format_errors());
            return Err(ValidateError::ValidationFailed);
        }

        // 5. Print summary
        self.print_summary(&config);

        Ok(())
    }

    fn print_summary(&self, config: &dyn ConfigurationContext) {
        println!("\nConfiguration Summary:");
        println!("  Context: {:?}", self.context);
        println!("  Environment: {}", config.environment());
        println!("{}", config.summary());
    }
}
```

**Validation Error Examples**:
```
✗ Configuration validation failed!

3 Validation Error(s):

1. Missing required field: database.url in CommonConfig
   Suggestion: Add 'database.url' to your configuration file in the CommonConfig section

2. Invalid value for orchestration.web.bind_address: "invalid" (reason: must be in format "host:port")
   Suggestion: Fix field 'orchestration.web.bind_address': must be in format "host:port"

3. Environment variable DATABASE_URL is required but not set
   Suggestion: Set environment variable: export DATABASE_URL=<value>
```

---

### 3.5 Interactive Wizard

**Purpose**: Guide users through configuration setup with intelligent prompts

```bash
tasker-cli config wizard [options]

# Options:
--context <orchestration|worker|combined>   # Target context
--environment <test|development|production> # Target environment
--output <directory>                        # Output directory
--from-common <file>                        # Use existing common config
```

**User Experience Flow**:

```
$ tasker-cli config wizard

╔═══════════════════════════════════════════════════════╗
║  Tasker Configuration Wizard                          ║
║  Interactive configuration generation                 ║
╚═══════════════════════════════════════════════════════╝

? What are you deploying?
  > Orchestration System
    Worker (Rust/Ruby/Python/WASM)
    Combined (Orchestration + Worker)

? Which environment?
    Test
  > Development
    Production

? Do you already have a common configuration?
  > No, create from scratch
    Yes, use existing file

┌─────────────────────────────────────────────────────┐
│ Database Configuration                              │
└─────────────────────────────────────────────────────┘

? Database URL: postgresql://tasker:tasker@localhost/tasker_dev
? Max connections: 30
? Min connections: 8
? Connection timeout (seconds): 30

┌─────────────────────────────────────────────────────┐
│ Queue Configuration                                 │
└─────────────────────────────────────────────────────┘

? Orchestration namespace: orchestration
? Worker namespace: worker
? PGMQ poll interval (ms): 250

┌─────────────────────────────────────────────────────┐
│ Orchestration Configuration                         │
└─────────────────────────────────────────────────────┘

? Enable web API? Yes
? Web API bind address: 0.0.0.0:8080
? Enable authentication? No
? Deployment mode: Hybrid

┌─────────────────────────────────────────────────────┐
│ Summary                                             │
└─────────────────────────────────────────────────────┘

Context: Orchestration
Environment: Development
Output: config/orchestration/

Configuration:
  ✓ CommonConfig (12 fields)
  ✓ OrchestrationConfig (35 fields)

? Generate configuration? Yes

✓ Configuration generated successfully!

Files created:
  - config/orchestration/common.toml
  - config/orchestration/orchestration.toml

Next steps:
  1. Review generated configuration
  2. Validate: tasker-cli config validate --config config/orchestration --context orchestration
  3. Start orchestration: cargo run --bin tasker-orchestration
```

**Implementation**:
```rust
pub struct WizardCommand {
    /// Configuration context (if pre-selected)
    pub context: Option<ConfigContext>,

    /// Target environment (if pre-selected)
    pub environment: Option<String>,

    /// Output directory
    pub output: PathBuf,

    /// Existing common config file
    pub from_common: Option<PathBuf>,
}

impl WizardCommand {
    pub async fn execute(&self) -> Result<(), WizardError> {
        // 1. Welcome and context selection
        let context = self.select_context()?;
        let environment = self.select_environment()?;

        // 2. Check for existing common config
        let common_config = if let Some(ref path) = self.from_common {
            println!("Using existing common configuration: {}", path.display());
            Self::load_common_config(path)?
        } else {
            self.create_common_config()?
        };

        // 3. Create context-specific config
        let context_config = match context {
            ConfigContext::Orchestration => self.create_orchestration_config()?,
            ConfigContext::Worker => self.create_worker_config()?,
            ConfigContext::Combined => self.create_combined_config()?,
            _ => unreachable!(),
        };

        // 4. Summary and confirmation
        self.print_summary(&common_config, &context_config);

        if !self.confirm_generation()? {
            println!("Configuration generation cancelled.");
            return Ok(());
        }

        // 5. Generate files
        self.generate_files(&common_config, &context_config)?;

        // 6. Next steps
        self.print_next_steps(&context);

        Ok(())
    }

    fn select_context(&self) -> Result<ConfigContext, WizardError> {
        use dialoguer::Select;

        let options = vec![
            "Orchestration System",
            "Worker (Rust/Ruby/Python/WASM)",
            "Combined (Orchestration + Worker)",
        ];

        let selection = Select::new()
            .with_prompt("What are you deploying?")
            .items(&options)
            .default(0)
            .interact()?;

        Ok(match selection {
            0 => ConfigContext::Orchestration,
            1 => ConfigContext::Worker,
            2 => ConfigContext::Combined,
            _ => unreachable!(),
        })
    }

    fn create_common_config(&self) -> Result<CommonConfig, WizardError> {
        use dialoguer::Input;

        println!("\n┌─────────────────────────────────────────────────────┐");
        println!("│ Database Configuration                              │");
        println!("└─────────────────────────────────────────────────────┘\n");

        let database_url: String = Input::new()
            .with_prompt("Database URL")
            .default("postgresql://tasker:tasker@localhost/tasker_dev".to_string())
            .interact_text()?;

        let max_connections: u32 = Input::new()
            .with_prompt("Max connections")
            .default(30)
            .interact_text()?;

        // ... more prompts

        Ok(CommonConfig {
            database: DatabaseConfig {
                url: database_url,
                pool: DatabasePoolConfig {
                    max_connections,
                    // ... other fields
                },
                // ... other fields
            },
            // ... other fields
        })
    }
}
```

---

### 3.6 Convert Command

**Purpose**: Convert legacy monolithic configuration to context-specific

```bash
tasker-cli config convert \
    --input <directory> \
    --output <directory> \
    --target <orchestration|worker|combined>

# Example:

# Convert existing config to orchestration-specific
tasker-cli config convert \
    --input config/tasker \
    --output config/orchestration \
    --target orchestration
```

**Implementation**:
```rust
pub struct ConvertCommand {
    /// Input directory (legacy config)
    pub input: PathBuf,

    /// Output directory (context-specific config)
    pub output: PathBuf,

    /// Target context
    pub target: ConfigContext,
}

impl ConvertCommand {
    pub async fn execute(&self) -> Result<(), ConvertError> {
        println!("Converting legacy configuration...");
        println!("Input: {}", self.input.display());
        println!("Output: {}", self.output.display());
        println!("Target: {:?}", self.target);

        // 1. Load legacy configuration
        let legacy_config = Self::load_legacy_config(&self.input)?;

        // 2. Convert to context-specific
        let (common, context_specific) = self.convert_to_context_specific(&legacy_config)?;

        // 3. Validate conversion
        common.validate()?;
        context_specific.validate()?;

        // 4. Generate output files
        self.generate_output_files(&common, &context_specific)?;

        println!("\n✓ Conversion complete!");
        println!("\nGenerated files:");
        println!("  - {}/common.toml", self.output.display());
        println!("  - {}/{}.toml", self.output.display(), self.target.name());

        Ok(())
    }
}
```

---

## Part 4: Testing and Validation Strategy

### 4.1 Unit Tests

**Coverage Target**: 90%+

```rust
// tasker-shared/src/config/contexts/common.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_config_validation() {
        let config = CommonConfig {
            database: DatabaseConfig {
                url: "postgresql://localhost/test".to_string(),
                pool: DatabasePoolConfig {
                    max_connections: 30,
                    min_connections: 8,
                    // ...
                },
                // ...
            },
            // ...
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_common_config_missing_database_url() {
        let config = CommonConfig {
            database: DatabaseConfig {
                url: "".to_string(),  // Invalid
                // ...
            },
            // ...
        };

        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::MissingField { .. }));
    }

    #[test]
    fn test_conversion_from_legacy_config() {
        let legacy = TaskerConfig::load_for_environment("test").unwrap();
        let common: CommonConfig = (&legacy).into();

        assert_eq!(common.database.url, legacy.database_url());
        assert_eq!(
            common.database.pool.max_connections,
            legacy.database.pool.max_connections
        );
    }
}
```

---

### 4.2 Integration Tests

**Coverage Target**: Key deployment scenarios

```rust
// tests/config_integration_test.rs

#[test]
fn test_orchestration_only_deployment() {
    let config = ConfigManager::load_for_context(ConfigContext::Orchestration).unwrap();

    // Common config should be available
    assert!(config.common().is_some());

    // Orchestration config should be available
    assert!(config.orchestration().is_some());

    // Worker config should NOT be available
    assert!(config.worker().is_none());
}

#[test]
fn test_worker_only_deployment() {
    let config = ConfigManager::load_for_context(ConfigContext::Worker).unwrap();

    // Common config should be available
    assert!(config.common().is_some());

    // Worker config should be available
    assert!(config.worker().is_some());

    // Orchestration config should NOT be available
    assert!(config.orchestration().is_none());
}

#[test]
fn test_worker_env_overrides() {
    // Test ENV variable overrides for DATABASE_URL and TASKER_TEMPLATE_PATH
    std::env::set_var("TASKER_ENV", "test");
    std::env::set_var("DATABASE_URL", "postgresql://localhost/test");
    std::env::set_var("TASKER_TEMPLATE_PATH", "/tmp/templates");

    let config = ConfigManager::load_for_context(ConfigContext::Worker).unwrap();

    let worker_config = config.worker().unwrap();
    // ENV overrides should take precedence
    assert_eq!(worker_config.database_url(), "postgresql://localhost/test");
    assert_eq!(worker_config.template_path().unwrap(), PathBuf::from("/tmp/templates"));
}

#[test]
fn test_combined_deployment() {
    let config = ConfigManager::load_for_context(ConfigContext::Combined).unwrap();

    // All configs should be available
    assert!(config.common().is_some());
    assert!(config.orchestration().is_some());
    assert!(config.worker().is_some());
}
```

---

### 4.3 CLI Tests

```rust
// tasker-client/tests/cli_config_test.rs

#[test]
fn test_generate_orchestration_config() {
    let output = test_cli()
        .args(&[
            "config",
            "generate",
            "--context", "orchestration",
            "--environment", "test",
            "--output", "/tmp/test-orchestration.toml",
        ])
        .output()
        .expect("Failed to execute CLI");

    assert!(output.status.success());
    assert!(Path::new("/tmp/test-orchestration.toml").exists());
}

#[test]
fn test_validate_valid_config() {
    let output = test_cli()
        .args(&[
            "config",
            "validate",
            "--config", "config/tasker/base",
            "--context", "orchestration",
        ])
        .output()
        .expect("Failed to execute CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Configuration is valid"));
}

#[test]
fn test_validate_invalid_config() {
    let output = test_cli()
        .args(&[
            "config",
            "validate",
            "--config", "/tmp/invalid.toml",
            "--context", "orchestration",
        ])
        .output()
        .expect("Failed to execute CLI");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Configuration validation failed"));
}
```

---

### 4.4 End-to-End Tests

```bash
#!/bin/bash
# tests/e2e/config_workflow_test.sh

set -e

# Test 1: Generate orchestration config
echo "Test 1: Generate orchestration config"
cargo run --bin tasker-cli -- config generate \
    --context orchestration \
    --environment test \
    --output /tmp/e2e-test/orchestration.toml

# Test 2: Validate generated config
echo "Test 2: Validate generated config"
cargo run --bin tasker-cli -- config validate \
    --config /tmp/e2e-test/orchestration.toml \
    --context orchestration

# Test 3: Start orchestration with generated config
echo "Test 3: Start orchestration with generated config"
export TASKER_CONFIG_ROOT=/tmp/e2e-test
timeout 10s cargo run --bin tasker-orchestration --features context-specific-config || true

# Test 4: Generate worker config
echo "Test 4: Generate worker config"
cargo run --bin tasker-cli -- config generate \
    --context worker \
    --environment test \
    --output /tmp/e2e-test/worker.toml

# Test 5: Validate worker config
echo "Test 5: Validate worker config"
cargo run --bin tasker-cli -- config validate \
    --config /tmp/e2e-test/worker.toml \
    --context worker

# Test 6: Start Rust worker with generated config
echo "Test 6: Start Rust worker with generated config"
timeout 10s cargo run --bin workers/rust --features context-specific-config || true

echo "All E2E tests passed!"
```

---

## Part 5: Rollout and Deployment Strategy

### 5.1 Rollout Timeline

**Total Duration**: 8-10 weeks

| Phase | Timeline | Risk | Breaking | Feature Flag |
|-------|----------|------|----------|--------------|
| Phase 1: Context Structs | Weeks 1-2 | Low | No | None |
| Phase 2: Remove Unused | Week 3 | Low | No | Deprecation warnings |
| Phase 3: Context Loading | Weeks 4-6 | Medium | Yes | `context-specific-config` |
| Phase 4: CLI Tools | Weeks 7-10 | Low | No | None |

---

### 5.2 Deployment Phases

#### Phase 1 Deployment (Weeks 1-2)

**Deliverables**:
- Context-specific structs added to `tasker-shared`
- Conversion traits implemented
- 100% backward compatibility

**Deployment Steps**:
1. Merge PR with new structs
2. Run full test suite
3. No configuration changes needed
4. Monitor for any issues

**Success Criteria**:
- All tests passing
- No performance regression
- Existing deployments unaffected

---

#### Phase 2 Deployment (Week 3)

**Deliverables**:
- Unused TOML files removed (11 files)
- Deprecated fields marked
- Migration guide published

**Deployment Steps**:
1. Merge PR with deprecations
2. Update documentation
3. Announce deprecations to users
4. Monitor deprecation warnings in logs

**Success Criteria**:
- All tests passing
- Deprecation warnings logged but non-fatal
- Users notified of upcoming changes

---

#### Phase 3 Deployment (Weeks 4-6)

**Deliverables**:
- Context-specific configuration loading
- Feature flag for opt-in
- Updated bootstrap systems

**Deployment Steps**:
1. Deploy with feature flag disabled (legacy mode)
2. Test legacy mode in staging
3. Enable feature flag in development environment
4. Test context-specific mode in development
5. Enable feature flag in staging
6. Test context-specific mode in staging
7. Gradual rollout to production (canary → full)

**Rollback Plan**:
- Disable feature flag immediately
- Revert to legacy configuration
- No data migration needed (stateless config change)

**Success Criteria**:
- Context-specific mode works in production
- 100% configuration efficiency
- No performance regression
- Health checks passing

---

#### Phase 4 Deployment (Weeks 7-10)

**Deliverables**:
- CLI tools for configuration generation
- Interactive wizard
- Comprehensive documentation

**Deployment Steps**:
1. Release CLI tools as separate binary
2. Publish documentation and examples
3. Create video tutorials
4. Support users in migration

**Success Criteria**:
- CLI tools working for all contexts
- Users can generate valid configurations
- Positive user feedback

---

### 5.3 Monitoring and Observability

**Metrics to Track**:

1. **Configuration Loading Time**
   ```rust
   metrics::histogram!("config.load_duration_ms", load_duration_ms);
   metrics::counter!("config.load_success", 1, "context" => context.name());
   metrics::counter!("config.load_failure", 1, "context" => context.name(), "error" => error_type);
   ```

2. **Configuration Efficiency**
   ```rust
   metrics::gauge!("config.fields_loaded", fields_loaded, "context" => context.name());
   metrics::gauge!("config.fields_used", fields_used, "context" => context.name());
   metrics::gauge!("config.efficiency_percent", efficiency_percent, "context" => context.name());
   ```

3. **Deprecation Warnings**
   ```rust
   metrics::counter!("config.deprecation_warning", 1, "field" => field_name);
   ```

4. **Validation Errors**
   ```rust
   metrics::counter!("config.validation_error", 1, "error_type" => error_type);
   ```

**Health Check Integration**:
```rust
pub struct ConfigHealthCheck {
    config_manager: Arc<ConfigManager>,
}

impl HealthCheck for ConfigHealthCheck {
    fn check(&self) -> HealthCheckResult {
        let validation_result = self.config_manager.validate();

        if validation_result.is_valid() {
            HealthCheckResult::Healthy {
                details: format!(
                    "Configuration valid: {} context, {} environment",
                    self.config_manager.context(),
                    self.config_manager.environment()
                ),
            }
        } else {
            HealthCheckResult::Unhealthy {
                reason: "Configuration validation failed".to_string(),
                details: validation_result.format_errors(),
            }
        }
    }
}
```

---

## Part 6: Documentation Plan

### 6.1 Documentation Structure

```
docs/
├── configuration/
│   ├── README.md                           # Overview
│   ├── quickstart.md                       # Quick start guide
│   ├── contexts.md                         # Configuration contexts
│   ├── common-config.md                    # Common configuration
│   ├── orchestration-config.md             # Orchestration configuration
│   ├── worker-config.md                    # Worker configuration (language-agnostic)
│   ├── env-overrides.md                    # ENV variable override patterns
│   ├── cli-tools.md                        # CLI tools guide
│   ├── migration-guide.md                  # Legacy → context-specific migration
│   ├── validation.md                       # Validation guide
│   ├── troubleshooting.md                  # Troubleshooting guide
│   └── examples/
│       ├── orchestration-development.toml  # Example configs
│       ├── orchestration-production.toml
│       ├── worker-development.toml
│       ├── worker-production.toml
│       └── combined.toml                   # Combined deployment example
└── ticket-specs/TAS-50/                    # Design documents (this file)
```

---

### 6.2 Key Documentation Sections

#### Quick Start Guide

```markdown
# Configuration Quick Start

## Orchestration Deployment

1. Generate configuration:
   ```bash
   tasker-cli config generate \
       --context orchestration \
       --environment production \
       --output config/tasker.toml
   ```

2. Review and customize:
   ```bash
   nano config/tasker.toml
   ```

3. Validate:
   ```bash
   tasker-cli config validate \
       --config config/tasker.toml \
       --context orchestration
   ```

4. Deploy:
   ```bash
   export TASKER_CONFIG_ROOT=config
   cargo run --bin tasker-orchestration --features context-specific-config
   ```

## Worker Deployment

[Similar sections for rust worker and ruby worker...]
```

---

#### Migration Guide

```markdown
# Migration Guide: Legacy → Context-Specific Configuration

## Overview

This guide explains how to migrate from legacy monolithic TaskerConfig to context-specific configuration.

## Step 1: Assess Your Deployment

Determine which context you need:
- **Orchestration only**: Use `orchestration` context
- **Worker only** (Rust/Ruby/Python/WASM): Use `worker` context
- **Combined**: Use `combined` context

## Step 2: Convert Configuration

Use the convert command:
```bash
tasker-cli config convert \
    --input config/tasker \
    --output config/orchestration \
    --target orchestration
```

## Step 3: Validate

```bash
tasker-cli config validate \
    --config config/orchestration \
    --context orchestration \
    --strict
```

## Step 4: Test

Enable feature flag and test:
```bash
cargo test --features context-specific-config
```

## Step 5: Deploy

Update your deployment scripts:
```bash
export TASKER_CONFIG_CONTEXT=orchestration
cargo run --bin tasker-orchestration --features context-specific-config
```

## Rollback Plan

If issues occur:
1. Disable feature flag
2. Revert to legacy configuration
3. Restart services

No data migration needed - this is a stateless configuration change.
```

---

## Part 7: Success Metrics

### 7.1 Technical Metrics

| Metric | Current | Target | Measurement |
|--------|---------|--------|-------------|
| Configuration Efficiency (Orchestration) | 62.5% | 100% | Fields used / fields loaded |
| Configuration Efficiency (Worker) | 25% | 100% | Fields used / fields loaded |
| TOML Files | 45 | 34 | File count |
| Unused Configuration Fields | 20-30 | 0 | Deprecated field count |
| Configuration Load Time | Baseline | <50ms | p50 latency |
| Validation Error Clarity | Subjective | 90%+ satisfaction | User survey |

---

### 7.2 User Experience Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| CLI Usage | 80% of new deployments use CLI | CLI invocation telemetry |
| Configuration Generation Success | 95%+ | CLI success rate |
| Validation Error Fix Rate | 90%+ | Errors fixed on first attempt |
| Time to First Valid Config | <5 minutes | Wizard completion time |
| Documentation Clarity | 4.5/5 rating | User feedback |

---

### 7.3 Adoption Metrics

| Phase | Metric | Target |
|-------|--------|--------|
| Phase 1 | New structs available | 100% code coverage |
| Phase 2 | Deprecation warnings seen | 100% of users |
| Phase 3 | Context-specific config adopted | 80% within 3 months |
| Phase 4 | CLI tool usage | 80% of new deployments |

---

## Part 8: Risk Assessment and Mitigation

### 8.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Breaking existing deployments | Medium | High | Feature flag, phased rollout, clear migration guide |
| Performance regression | Low | Medium | Benchmarking, monitoring, rollback plan |
| Configuration validation bugs | Medium | Medium | Comprehensive testing, user feedback, quick fixes |
| CLI tool complexity | Low | Low | User testing, documentation, examples |
| Migration confusion | Medium | Medium | Clear documentation, support, examples |

---

### 8.2 Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Production incidents | Low | High | Gradual rollout, monitoring, immediate rollback capability |
| User adoption resistance | Medium | Medium | Clear value proposition, easy migration, support |
| Documentation gaps | Medium | Low | Comprehensive docs, examples, video tutorials |
| Support burden | Medium | Medium | Self-service tools, FAQ, community support |

---

### 8.3 Rollback Procedures

**Phase 3 Rollback** (Context-Specific Loading):
1. Disable feature flag: `context-specific-config`
2. Restart services with legacy configuration
3. Monitor health checks
4. Investigate and fix issues
5. Re-enable when ready

**No Data Migration Needed** - Configuration is stateless, rollback is safe and fast.

---

## Summary and Next Steps

### Implementation Summary

**Total Effort**: 8-10 weeks
**Team Size**: 1-2 developers
**Risk Level**: Low-Medium (mitigated with phased approach)

### Immediate Next Steps

1. **Review and Approval** (1 week)
   - Review this implementation plan
   - Get stakeholder approval
   - Prioritize in sprint planning

2. **Phase 1 Development** (2 weeks)
   - Implement context-specific structs
   - Add conversion traits
   - Write unit tests
   - Merge to main

3. **Phase 2 Development** (1 week)
   - Remove unused configuration
   - Add deprecation warnings
   - Update documentation

4. **Phase 3 Development** (2-3 weeks)
   - Implement context-specific loading
   - Add feature flag
   - Integration testing
   - Staged rollout

5. **Phase 4 Development** (3-4 weeks)
   - Build CLI tools
   - Write documentation
   - User testing
   - Public release

---

**Document Status**: Ready for development
**Next Review**: After Phase 1 completion
