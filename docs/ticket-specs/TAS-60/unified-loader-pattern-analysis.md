# Unified Loader Pattern Analysis

**Author**: Claude
**Date**: 2025-10-31
**Purpose**: TAS-60 Configuration Structure Analysis - Understanding the configuration loading, transformation, and merging patterns to support redesign

## Executive Summary

The configuration loading system has evolved through multiple phases (TAS-34, TAS-50) and now consists of **three interconnected components**: `UnifiedConfigLoader` (TOML parsing + env var substitution), `ConfigMerger` (base + environment merging), and `ConfigManager` (context-specific loading). The system attempts to support **four different loading patterns** simultaneously:

1. **Legacy monolithic** - Load complete TaskerConfig from merged TOML
2. **Context-specific from directory** - Load Common+Orchestration or Common+Worker from base/ + environments/
3. **Context-specific from single file** - Load from pre-merged TOML file (Docker/production)
4. **Backward compatibility bridge** - Convert context configs back to TaskerConfig

**Critical Issue**: The complexity has become unsustainable. TaskReadinessConfig alone has 8 nested subsections but is now modeled within the event system framework, representing configuration sprawl that may not be fully utilized.

## Component Architecture

### 1. UnifiedConfigLoader (unified_loader.rs)

**Location**: `tasker-shared/src/config/unified_loader.rs`
**Lines**: ~800 lines
**Purpose**: Core TOML loading with environment variable substitution

#### Key Responsibilities

1. **Environment Detection** (lines 38-55):
```rust
pub fn detect_environment() -> String {
    std::env::var("TASKER_ENV")
        .or_else(|_| std::env::var("RAILS_ENV"))
        .unwrap_or_else(|_| "development".to_string())
}
```

2. **TOML Loading with Env Var Substitution** (lines 638-714):
```rust
pub fn load_context_toml(&mut self, context_name: &str) -> ConfigResult<toml::Value> {
    // 1. Load base context configuration - REQUIRED
    let base_path = self.root.join("base").join(format!("{context_name}.toml"));
    let mut config = self.load_toml_with_env_substitution(&base_path)?;

    // 2. Apply environment overrides if they exist
    let env_path = self.root.join("environments")
        .join(&self.environment)
        .join(format!("{context_name}.toml"));

    if env_path.exists() {
        let overrides = self.load_toml_with_env_substitution(&env_path)?;
        self.merge_toml(&mut config, overrides)?;  // SHALLOW MERGE!
    }

    Ok(config)
}
```

**Critical Bug**: `merge_toml()` performs **SHALLOW MERGE** - nested tables are replaced, not merged!

3. **Environment Variable Expansion** (lines 716-762):
```rust
fn expand_env_vars(&self, value: &str) -> ConfigResult<String> {
    // Supports ${VAR} and ${VAR:-default}
    let re = Regex::new(r"\$\{([^}:]+)(?::-([^}]*))?\}").unwrap();

    for cap in re.captures_iter(value) {
        let var_name = cap.get(1).unwrap().as_str();
        let default_value = cap.get(2).map(|m| m.as_str());

        match env::var(var_name) {
            Ok(value) => result.push_str(&value),
            Err(_) => {
                if let Some(default) = default_value {
                    result.push_str(default);
                } else {
                    return Err(...);  // Fail fast
                }
            }
        }
    }
    Ok(result)
}
```

4. **TaskerConfig Loading** (lines 580-636):
```rust
pub fn load_tasker_config(&mut self) -> ConfigResult<TaskerConfig> {
    // Load all base files
    let common_toml = self.load_context_toml("common")?;
    let orchestration_toml = self.load_context_toml("orchestration")?;
    let worker_toml = self.load_context_toml("worker")?;

    // Merge into single TOML value
    let mut merged = toml::Value::Table(toml::value::Table::new());
    self.merge_toml(&mut merged, common_toml)?;
    self.merge_toml(&mut merged, orchestration_toml)?;
    self.merge_toml(&mut merged, worker_toml)?;

    // Deserialize into TaskerConfig
    let config: TaskerConfig = merged.try_into().map_err(...)?;

    Ok(config)
}
```

**Problem**: Uses shallow merge, so later files (orchestration, worker) replace nested tables from common instead of merging them!

5. **Context-Specific Loading** (lines 435-578):
```rust
pub fn load_common_config(&mut self) -> ConfigResult<CommonConfig> {
    let common_toml = self.load_context_toml("common")?;
    let config: CommonConfig = common_toml.try_into().map_err(...)?;
    config.validate()?;
    Ok(config)
}

pub fn load_orchestration_config(&mut self) -> ConfigResult<OrchestrationConfig> {
    // Loads common.toml + orchestration.toml
    let common_toml = self.load_context_toml("common")?;
    let orch_toml = self.load_context_toml("orchestration")?;

    let mut merged = common_toml.clone();
    self.merge_toml(&mut merged, orch_toml)?;  // SHALLOW!

    let config: OrchestrationConfig = merged.try_into().map_err(...)?;
    config.validate()?;
    Ok(config)
}

pub fn load_worker_config(&mut self) -> ConfigResult<WorkerConfig> {
    // Similar pattern
}
```

**Pattern**: Each context config includes common fields + context-specific fields.

#### Shallow Merge Bug (TAS-60 Root Cause)

**Location**: Lines 764-799

```rust
fn merge_toml(
    &self,
    base: &mut toml::Value,
    overlay: toml::Value,
) -> ConfigResult<()> {
    if let (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) =
        (base, overlay)
    {
        for (key, value) in overlay_table {
            // PROBLEM: Just inserts, doesn't recurse!
            base_table.insert(key, value);
        }
    }
    Ok(())
}
```

**Impact**: When orchestration.toml defines `[mpsc_channels.orchestration.command_processor]`, it **replaces** the entire `[mpsc_channels]` table from common.toml instead of merging the `orchestration` subtable with the `shared` subtable.

**Fixed in ConfigMerger**: The `deep_merge_tables()` function in merger.rs actually does recursive merging!

### 2. ConfigMerger (merger.rs)

**Location**: `tasker-shared/src/config/merger.rs`
**Lines**: 424 lines
**Purpose**: Merge base + environment configs for CLI config generation

#### Key Responsibilities

1. **Deep Merge (Correct Implementation)** (lines 212-251):
```rust
fn deep_merge_tables(base: &mut toml::value::Table, overlay: toml::value::Table) {
    for (key, value) in overlay {
        match base.get_mut(&key) {
            Some(base_value) => {
                let both_are_tables = matches!(
                    (&*base_value, &value),
                    (toml::Value::Table(_), toml::Value::Table(_))
                );

                if both_are_tables {
                    // RECURSIVE MERGE!
                    if let toml::Value::Table(base_table) = base_value {
                        if let toml::Value::Table(overlay_table) = value {
                            Self::deep_merge_tables(base_table, overlay_table);
                        }
                    }
                } else {
                    // Not both tables - overlay wins
                    *base_value = value;
                }
            }
            None => {
                // Key only in overlay - insert it
                base.insert(key, value);
            }
        }
    }
}
```

**This is the CORRECT implementation** that TAS-60 needs!

2. **Context Merging** (lines 111-210):
```rust
pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
    // For orchestration/worker, start with common as base
    let mut base = self.loader.load_context_toml("common")?;

    // Load context-specific configuration
    let context_config = self.loader.load_context_toml(context)?;

    // Deep merge context on top of common
    if let (toml::Value::Table(ref mut base_table), toml::Value::Table(context_table)) =
        (&mut base, context_config)
    {
        Self::deep_merge_tables(base_table, context_table);  // CORRECT!
    }

    // Strip documentation metadata
    Self::strip_docs_from_value(&mut base);

    // Serialize to TOML string
    Ok(toml::to_string_pretty(&base)?)
}
```

**For "complete" context** (lines 119-189):
```rust
if context == "complete" {
    // Merge common + orchestration + worker
    let mut base = self.loader.load_context_toml("common")?;

    let orch_config = self.loader.load_context_toml("orchestration")?;
    if let (toml::Value::Table(ref mut base_table), toml::Value::Table(orch_table)) =
        (&mut base, orch_config)
    {
        Self::deep_merge_tables(base_table, orch_table);
    }

    let worker_config = self.loader.load_context_toml("worker")?;
    if let toml::Value::Table(ref mut base_table), toml::Value::Table(worker_table)) =
        (&mut base, worker_config)
    {
        Self::deep_merge_tables(base_table, worker_table);
    }

    base
}
```

3. **Documentation Stripping** (lines 253-272):
```rust
fn strip_docs_from_value(value: &mut toml::Value) {
    if let toml::Value::Table(table) = value {
        // Remove any keys ending with "_docs"
        table.retain(|key, _| !key.ends_with("_docs"));

        // Recursively strip from remaining values
        for (_, v) in table.iter_mut() {
            Self::strip_docs_from_value(v);
        }
    }
}
```

**Purpose**: Remove `[some_section._docs.*]` metadata before serialization.

#### Issues with ConfigMerger

1. **Only used by CLI** - Not used by runtime loading path!
2. **Generates flat TOML** - No context separation in output
3. **Missing field problem** - Doesn't add missing TaskerConfig fields (telemetry, system, execution, etc.)

### 3. ConfigManager (manager.rs)

**Location**: `tasker-shared/src/config/manager.rs`
**Lines**: 950 lines (most complex component!)
**Purpose**: High-level configuration management with multiple loading strategies

#### Key Responsibilities

1. **Legacy Loading** (lines 45-62):
```rust
pub fn load_from_env(environment: &str) -> ConfigResult<Arc<ConfigManager>> {
    let mut loader = UnifiedConfigLoader::new(environment)?;
    let config = loader.load_tasker_config()?;  // Uses shallow merge!
    config.validate()?;

    Ok(Arc::new(ConfigManager {
        config,
        environment: environment.to_string(),
    }))
}
```

**Problem**: Uses UnifiedConfigLoader's shallow merge, so nested tables get replaced!

2. **Context-Specific Loading from Directory** (lines 117-158):
```rust
pub fn load_context_direct(context: ConfigContext) -> ConfigResult<ContextConfigManager> {
    let mut loader = UnifiedConfigLoader::new_from_env()?;

    let config: Box<dyn Any + Send + Sync> = match context {
        ConfigContext::Orchestration => {
            let common = loader.load_common_config()?;  // Uses shallow merge!
            let orchestration = loader.load_orchestration_config()?;  // Uses shallow merge!
            Box::new((common, orchestration))
        }
        // ... similar for Worker, Combined, Legacy
    };

    Ok(ContextConfigManager { context, environment, config })
}
```

**Problem**: Still uses UnifiedConfigLoader's shallow merge!

3. **Context-Specific Loading from Single File** (lines 188-303):
```rust
pub fn load_from_single_file(
    path: &std::path::Path,
    context: ConfigContext,
) -> ConfigResult<ContextConfigManager> {
    // Read TOML file
    let contents = fs::read_to_string(path)?;

    // Parse and substitute env vars
    let mut toml_value: toml::Value = toml::from_str(&contents)?;
    Self::substitute_env_vars_in_value(&mut toml_value)?;

    // Deserialize based on context
    let config: Box<dyn Any + Send + Sync> = match context {
        ConfigContext::Orchestration => {
            let common: CommonConfig = toml::from_str(&substituted_contents)?;
            let orchestration: OrchestrationConfig = toml::from_str(&substituted_contents)?;
            Box::new((common, orchestration))
        }
        // ... similar for Worker, Combined, Legacy
    };

    Ok(ContextConfigManager { context, environment, config })
}
```

**Key Insight**: This method deserializes the **same TOML file** multiple times into different structs!
- First pass: Extract `CommonConfig` fields
- Second pass: Extract `OrchestrationConfig` fields
- Both structs can have overlapping field names (which is why table names must match struct fields)

4. **Backward Compatibility Bridge** (lines 490-578):
```rust
pub fn as_tasker_config(&self) -> Option<TaskerConfig> {
    match self.context {
        ConfigContext::Orchestration => {
            if let Some((common, orch)) = self.config.downcast_ref::<(CommonConfig, OrchestrationConfig)>() {
                let mut config = TaskerConfig {
                    database: common.database.clone(),
                    queues: common.queues.clone(),
                    backoff: orch.backoff.clone(),
                    orchestration: orch.orchestration_system.clone(),
                    circuit_breakers: common.circuit_breakers.clone(),
                    ..TaskerConfig::default()  // MISSING FIELDS USE DEFAULTS!
                };

                // Copy nested fields manually
                config.execution.environment = common.environment.clone();
                config.mpsc_channels.shared = common.shared_channels.clone();
                config.mpsc_channels.orchestration = orch.mpsc_channels.clone();
                config.event_systems.orchestration = orch.orchestration_events.clone();
                config.event_systems.task_readiness = orch.task_readiness_events.clone();

                Some(config)
            } else {
                None
            }
        }
        // ... similar for Worker, Combined
    }
}
```

**Critical Problem**: This manual reconstruction:
- Uses `..TaskerConfig::default()` which fills missing fields with defaults
- Manually copies nested fields (error-prone, incomplete)
- Creates TaskerConfig with invalid state (telemetry, system, execution all default values!)

#### Environment Variable Substitution (Duplicated)

**Lines 305-378**: ConfigManager duplicates the env var substitution logic from UnifiedConfigLoader!

```rust
fn substitute_env_vars_in_value(value: &mut toml::Value) -> ConfigResult<()> {
    // Same regex pattern as UnifiedConfigLoader
    // Same expansion logic
    // Code duplication!
}

fn expand_env_vars(input: &str) -> ConfigResult<String> {
    // Exact duplicate of UnifiedConfigLoader::expand_env_vars()
}
```

**Why**: So `load_from_single_file()` can work without UnifiedConfigLoader dependency.

## Transformation Flow Diagrams

### Path 1: Legacy Monolithic Loading (Runtime)

```
┌─────────────────────────────────────────────────────────────┐
│ ConfigManager::load_from_env("test")                        │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ UnifiedConfigLoader::new("test")                            │
│ - Sets root: config/tasker                                  │
│ - Sets environment: test                                    │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ UnifiedConfigLoader::load_tasker_config()                   │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ├─> load_context_toml("common")
                 │   ├─> Load base/common.toml + env vars
                 │   └─> Merge environments/test/common.toml (SHALLOW)
                 │
                 ├─> load_context_toml("orchestration")
                 │   ├─> Load base/orchestration.toml + env vars
                 │   └─> Merge environments/test/orchestration.toml (SHALLOW)
                 │
                 └─> load_context_toml("worker")
                     ├─> Load base/worker.toml + env vars
                     └─> Merge environments/test/worker.toml (SHALLOW)

                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Merge all contexts together (SHALLOW!)                      │
│ - merged.insert("database", common.database)                │
│ - merged.insert("queues", common.queues)                    │
│ - merged.insert("backoff", orch.backoff)                    │
│ - merged.insert("mpsc_channels", worker.mpsc_channels) ← REPLACES common's mpsc_channels!
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Deserialize merged TOML into TaskerConfig                   │
│ ❌ ERROR: "missing field 'telemetry'"                       │
└─────────────────────────────────────────────────────────────┘
```

**Root Cause**: Shallow merge replaces nested tables + missing fields!

### Path 2: Context-Specific from Directory (TAS-50 Phase 2)

```
┌─────────────────────────────────────────────────────────────┐
│ ConfigManager::load_context_direct(Orchestration)           │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ UnifiedConfigLoader::load_common_config()                   │
│ - load_context_toml("common")                               │
│ - Deserialize into CommonConfig struct                      │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ UnifiedConfigLoader::load_orchestration_config()            │
│ - load_context_toml("common")  ← LOADS AGAIN!               │
│ - load_context_toml("orchestration")                        │
│ - Merge together (SHALLOW)                                  │
│ - Deserialize into OrchestrationConfig struct               │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ ContextConfigManager                                         │
│ - Holds Box<(CommonConfig, OrchestrationConfig)>            │
│ - Provides common() and orchestration() accessors            │
└─────────────────────────────────────────────────────────────┘
```

**Inefficiency**: Loads common.toml twice!

### Path 3: Context-Specific from Single File (TAS-50 Phase 3)

```
┌─────────────────────────────────────────────────────────────┐
│ ConfigManager::load_from_single_file(                       │
│   "config/tasker/complete-test.toml",                       │
│   ConfigContext::Orchestration                              │
│ )                                                            │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Read complete-test.toml (flat merged TOML)                  │
│ - All tables at root level                                  │
│ - environment, database, queues, backoff, etc.              │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Perform env var substitution                                │
│ - ${DATABASE_URL:-...} → actual value                       │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Deserialize SAME TOML into TWO structs                      │
│ - CommonConfig::deserialize(toml)                           │
│   ✓ Extracts: database, queues, circuit_breakers, etc.      │
│ - OrchestrationConfig::deserialize(toml)                    │
│   ✓ Extracts: backoff, orchestration_system, etc.           │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ ContextConfigManager                                         │
│ - Holds Box<(CommonConfig, OrchestrationConfig)>            │
└─────────────────────────────────────────────────────────────┘
```

**Key Insight**: This works because **both structs deserialize from the same flat TOML**, each extracting their respective fields.

### Path 4: CLI Config Generation

```
┌─────────────────────────────────────────────────────────────┐
│ tasker-cli config generate -c complete -e test              │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ ConfigMerger::new("config/tasker", "test")                  │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ ConfigMerger::merge_context("complete")                     │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ├─> loader.load_context_toml("common")
                 │   ├─> base/common.toml + env vars
                 │   └─> environments/test/common.toml (SHALLOW)
                 │
                 ├─> loader.load_context_toml("orchestration")
                 │   ├─> base/orchestration.toml + env vars
                 │   └─> environments/test/orchestration.toml (SHALLOW)
                 │
                 └─> loader.load_context_toml("worker")
                     ├─> base/worker.toml + env vars
                     └─> environments/test/worker.toml (SHALLOW)

                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Deep merge all contexts (CORRECT!)                          │
│ - deep_merge_tables(base, common)                           │
│ - deep_merge_tables(base, orchestration)                    │
│ - deep_merge_tables(base, worker)                           │
│ ✓ mpsc_channels.shared preserved                            │
│ ✓ mpsc_channels.orchestration added                         │
│ ✓ mpsc_channels.worker added                                │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Strip _docs sections                                         │
└────────────────┬────────────────────────────────────────────┘
                 │
                 v
┌─────────────────────────────────────────────────────────────┐
│ Serialize to TOML string                                     │
│ ❌ Still missing telemetry, system, execution fields!       │
└─────────────────────────────────────────────────────────────┘
```

**Problem**: Even with deep merge, still produces incomplete TOML!

## Configuration Sprawl Analysis

### TaskReadinessConfig: A Case Study

**Location**: `tasker-shared/src/config/task_readiness.rs`
**Lines**: 574 lines
**Structs**: 15 nested structs
**Depth**: 4 levels

```rust
pub struct TaskReadinessConfig {
    pub enabled: bool,
    pub event_system: Option<TaskReadinessEventSystemConfig>,  // ← From event_systems.toml
    pub enhanced_settings: EnhancedCoordinatorSettings,        // ← ???
    pub notification: TaskReadinessNotificationConfig,         // ← PostgreSQL LISTEN/NOTIFY
    pub fallback_polling: ReadinessFallbackConfig,            // ← Polling fallback
    pub event_channel: EventChannelConfig,                    // ← TAS-51: buffer moved to mpsc_channels
    pub coordinator: TaskReadinessCoordinatorConfig,          // ← Coordinator settings
    pub error_handling: ErrorHandlingConfig,                  // ← Error thresholds
}
```

**Issues**:

1. **Dual Representation**: `event_system` field duplicates configuration that's also in `EventSystemsConfig.task_readiness`!

2. **Moved Fields (TAS-51)**: `event_channel.buffer_size` was moved to `mpsc_channels.task_readiness.event_channel.buffer_size`, but old struct remains!

3. **TOML Mismatch**: Base TOML files only have `[task_readiness_events]` (the event system portion), not the full TaskReadinessConfig!

4. **Unused Complexity**: 8 subsections defined in struct, but only 1 populated from TOML.

**User's Insight**: "Task readiness is actually now modeled in code within the event system framework" - so most of TaskReadinessConfig may be legacy!

### OrchestrationConfig: Legacy vs Context-Specific

**Two different structs with same name!**

1. **TaskerConfig.orchestration** (orchestration/mod.rs lines 16-51):
```rust
pub struct OrchestrationConfig {
    pub mode: String,
    pub enable_performance_logging: bool,
    pub web: WebConfig,
}
```

**TOML**: `[orchestration_system]`

2. **contexts::OrchestrationConfig** (contexts/orchestration.rs lines 26-62):
```rust
pub struct OrchestrationConfig {
    pub backoff: BackoffConfig,
    pub orchestration_system: LegacyOrchestrationConfig,  // ← The above struct!
    pub orchestration_events: OrchestrationEventSystemConfig,
    pub task_readiness_events: TaskReadinessEventSystemConfig,
    pub mpsc_channels: OrchestrationChannelsConfig,
    pub staleness_detection: StalenessDetectionConfig,  // TAS-49
    pub dlq: DlqConfig,  // TAS-49
    pub archive: ArchiveConfig,  // TAS-49
    pub environment: String,
}
```

**TOML**: Multiple tables merged together

**This is the "context extraction" pattern** - but it's confusing to have two structs with same name!

### Missing Configuration Analysis

**Fields in TaskerConfig struct** (14 required + 1 optional):
1. ✅ database - Present in common.toml
2. ❌ telemetry - **NOT IN ANY TOML FILE**
3. ❌ task_templates - **NOT IN ANY TOML FILE**
4. ❌ system - **NOT IN ANY TOML FILE**
5. ✅ backoff - Present in orchestration.toml
6. ❌ execution - **NOT IN ANY TOML FILE**
7. ✅ queues - Present in common.toml
8. ✅ orchestration - Present as `orchestration_system` in orchestration.toml
9. ✅ circuit_breakers - Present in common.toml
10. ⚠️ task_readiness - Partial (only `task_readiness_events` portion)
11. ✅ event_systems - Split across files as `orchestration_events`, `task_readiness_events`, `worker_events`
12. ✅ mpsc_channels - Split across files with subsystem prefixes
13. ✅ decision_points - Present in decision_points.toml (but not merged by ConfigMerger!)
14. ✅ worker - Present as `worker_system` in worker.toml

**5 fields completely missing**, **1 field partially missing**, **1 field not merged**.

### Extraneous TOML Fields

**Fields in TOML that DON'T exist in structs**:

1. `database.encoding` - Not in DatabaseConfig
2. `database.host` - Not in DatabaseConfig
3. `database.checkout_timeout` - Not in DatabaseConfig
4. `database.reaping_frequency` - Not in DatabaseConfig
5. `queues.default_namespace` - Not in QueuesConfig

**Impact**: Silently ignored by Serde during deserialization.

**Question**: Are these fields used elsewhere? Or are they truly dead code?

## Key Insights for Redesign

### 1. Shallow Merge is the Root Cause

UnifiedConfigLoader's `merge_toml()` performs **shallow merge**, causing nested tables to be replaced instead of merged. ConfigMerger has the correct `deep_merge_tables()` implementation, but it's only used by the CLI, not runtime loading!

**Fix**: UnifiedConfigLoader should use ConfigMerger's deep merge logic.

### 2. Multiple Loading Paths Create Inconsistency

Four different loading paths with different merge strategies:
- Legacy: Shallow merge (broken)
- Context from directory: Shallow merge (broken)
- Context from single file: No merge (works because of dual deserialization)
- CLI generation: Deep merge (correct)

**Fix**: Consolidate to single loading path with correct merging.

### 3. Dual Struct Deserialization is Clever but Fragile

`load_from_single_file()` deserializes the same TOML into multiple structs:
```rust
let common: CommonConfig = toml::from_str(&toml)?;  // Extracts common fields
let orch: OrchestrationConfig = toml::from_str(&toml)?;  // Extracts orch fields
```

This works but requires:
- TOML table names match struct field names exactly
- No overlapping field names between structs
- All required fields present in flat TOML

**Alternative**: Use proper TOML hierarchy with `[common]`, `[orchestration]`, `[worker]` top-level tables.

### 4. Backward Compatibility Bridge is Incomplete

`as_tasker_config()` manually reconstructs TaskerConfig from context configs:
- Uses `..TaskerConfig::default()` for missing fields (wrong!)
- Manually copies nested fields (error-prone)
- Creates invalid TaskerConfig with default telemetry, system, execution

**Fix**: Either complete the reconstruction OR stop using TaskerConfig for contexts.

### 5. Configuration Sprawl is Unsustainable

TaskReadinessConfig has 8 subsections but only 1 is populated from TOML:
- `event_system` - Populated from `task_readiness_events` in TOML
- `enhanced_settings` - Defaults only
- `notification` - Defaults only
- `fallback_polling` - Defaults only
- `event_channel` - Defaults only (TAS-51 moved to mpsc_channels)
- `coordinator` - Defaults only
- `error_handling` - Defaults only

**Question**: Are these 7 subsections actually used in code? Or can they be removed?

### 6. Naming Inconsistencies Create Confusion

**TOML Table** → **Rust Struct Field**:
- `orchestration_system` → `TaskerConfig.orchestration`
- `orchestration_events` → `EventSystemsConfig.orchestration`
- `task_readiness_events` → `EventSystemsConfig.task_readiness`
- `worker_system` → `TaskerConfig.worker`
- `worker_events` → `EventSystemsConfig.worker`

**Fix**: Use consistent naming (TOML should match struct).

### 7. Missing Fields Cannot Be Defaulted

TaskerConfig requires:
- `telemetry: TelemetryConfig` - Observability settings
- `system: SystemConfig` - System version, limits
- `execution: ExecutionConfig` - **Contains TASKER_ENV!**
- `task_templates: TaskTemplatesConfig` - Template discovery paths
- `decision_points: DecisionPointsConfig` - Decision point limits

These cannot use `Default::default()` because they contain critical runtime configuration (especially `execution.environment`!).

**Fix**: Add these sections to appropriate TOML files.

## Proposed Ideal State

Based on user's insight: `{ common: ..., orchestration: ..., worker: ... }`

### Simplified Structure

```rust
pub struct TaskerConfig {
    /// Shared infrastructure (drives SystemContext)
    pub common: CommonConfig,

    /// Orchestration-specific (optional - only when orchestration deployed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<OrchestrationConfig>,

    /// Worker-specific (optional - only when worker deployed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<WorkerConfig>,
}
```

### TOML Representation

```toml
[common]
environment = "production"

[common.database]
url = "${DATABASE_URL}"
# ...

[common.telemetry]
enabled = true
# ...

[orchestration]
# Only present in orchestration config files

[orchestration.backoff]
# ...

[worker]
# Only present in worker config files

[worker.step_processing]
# ...
```

### Benefits

1. **Clear Separation**: What's common vs context-specific
2. **Optional Contexts**: Orchestration and worker are truly optional
3. **No Dual Structs**: Single OrchestrationConfig, single WorkerConfig
4. **No Manual Reconstruction**: Direct deserialization
5. **Semantic TOML**: Hierarchy matches struct hierarchy

## Recommendations

### Immediate Fixes (TAS-60)

1. **Fix shallow merge** - Copy `deep_merge_tables()` from ConfigMerger to UnifiedConfigLoader
2. **Add missing fields** - Add telemetry, system, execution, task_templates to common.toml
3. **Merge decision_points** - Include decision_points.toml in ConfigMerger
4. **Remove extraneous fields** - Clean up database.encoding, database.host, etc.

### Medium-Term Refactoring (TAS-61?)

1. **Consolidate merge logic** - Single implementation used by all loading paths
2. **Audit configuration usage** - Find unused TaskReadinessConfig subsections
3. **Simplify context loading** - Remove duplicate common.toml loading
4. **Fix naming inconsistencies** - Align TOML table names with struct fields

### Long-Term Redesign (TAS-62?)

1. **Adopt `{ common, orchestration, worker }` structure**
2. **Make contexts truly optional** - `Option<OrchestrationConfig>`, `Option<WorkerConfig>`
3. **Eliminate dual structs** - Single OrchestrationConfig/WorkerConfig
4. **Remove backward compatibility bridge** - Stop reconstructing TaskerConfig from contexts
5. **Prune configuration sprawl** - Remove unused subsections

## Next Steps

For TAS-60, proceed with Task 4 (intersection analysis) but with focus on:
1. Mapping TOML paths to struct hydration points
2. **Identifying unused configuration** (especially TaskReadinessConfig subsections)
3. Proposing semantically-near TOML representation that aligns with ideal state

The goal is not just "fix the TOML to match the struct" but "redesign both for long-term maintainability".
