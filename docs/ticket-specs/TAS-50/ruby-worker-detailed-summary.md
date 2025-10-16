# Ruby Worker Configuration Usage Analysis

## Summary Statistics
- **Total Direct Config Access**: Minimal - Ruby worker delegates to Rust via FFI
- **Primary Config Mechanism**: Environment variables
- **Secondary Config Mechanism**: Task templates from filesystem/database
- **FFI Delegation**: Rust handles all database/queue/orchestration configuration

---

## Configuration Sources Actually Used

### 1. Environment Variables (`ENV[...]`)
**Files**:
- `workers/ruby/bin/server.rb`
- `workers/ruby/lib/tasker_core/test_environment.rb`
- `workers/ruby/lib/tasker_core/registry/handler_registry.rb`
- `workers/ruby/lib/tasker_core/template_discovery.rb`

**Environment Variables Used**:
```ruby
ENV['TASKER_ENV']                       # Environment detection (test/development/production)
ENV['RAILS_ENV']                        # Rails environment fallback
ENV['DATABASE_URL']                     # Database connection (passed to Rust)
ENV['TASKER_TEMPLATE_PATH']            # Template discovery path
ENV['TASKER_FORCE_EXAMPLE_HANDLERS']   # Test mode handler loading
ENV['LOG_LEVEL']                       # Ruby log level control
ENV['RUST_LOG']                        # Rust log level control (Rust consumes)
ENV['RUBY_GC_HEAP_GROWTH_FACTOR']     # Production GC optimization
ENV['WORKSPACE_PATH']                  # Template discovery fallback
```

**Usage Pattern**: Ruby worker reads environment variables directly, delegates to Rust via FFI

**Required**: ✅ YES - Environment variables are primary configuration mechanism

**Context**: Ruby-worker-specific (environment variable based, not TaskerConfig)

---

### 2. Task Template Configuration (Filesystem/Database)
**Files**:
- `workers/ruby/lib/tasker_core/registry/handler_registry.rb`
- `workers/ruby/lib/tasker_core/template_discovery.rb`

**Template Discovery Paths**:
```ruby
# Priority order:
1. ENV['TASKER_TEMPLATE_PATH']                    # Explicit override
2. ENV['WORKSPACE_PATH']/config/tasks            # Workspace-based
3. TaskerCore::TemplateDiscovery::TemplatePath.find_template_config_directory  # Auto-discovery
```

**Usage Pattern**: Ruby discovers and loads YAML task templates from filesystem or database

**Required**: ✅ YES - Templates define handlers and workflow definitions

**Context**: Ruby-worker-specific (template metadata, not system config)

---

### 3. Handler-Specific Configuration (Task Context)
**Files**:
- `workers/ruby/lib/tasker_core/step_handler/api.rb`
- `workers/ruby/lib/tasker_core/step_handler/base.rb`
- `workers/ruby/lib/tasker_core/task_handler/base.rb`

**Handler Config Patterns**:
```ruby
@config.dig(:pagination, :page_size)              # API handler pagination
self.config[:timeout]                            # API handler timeout
self.config[:open_timeout]                       # API handler open timeout
auth_config[:token]                              # API handler auth token
auth_config[:username], auth_config[:password]   # API handler basic auth
ssl_config                                       # API handler SSL configuration
```

**Usage Pattern**: Handlers receive configuration from task context, not system config

**Required**: ✅ YES - Handlers need task-specific configuration

**Context**: Task-specific (passed via task context, not system config)

---

### 4. FFI-Delegated Configuration (Rust TaskerConfig)
**Files**:
- Implicit via FFI in `workers/ruby/lib/tasker_core/ffi.rb`

**Configuration Delegated to Rust**:
```
- Database connection pooling (Rust manages via DATABASE_URL)
- Queue configuration (PGMQ managed by Rust)
- Event system configuration (Rust manages event coordination)
- MPSC channel configuration (Rust internal)
- Circuit breakers (Rust internal)
- Backoff/retry logic (Rust handles orchestration)
```

**Usage Pattern**: Ruby calls Rust FFI functions, Rust uses its own TaskerConfig

**Required**: ✅ YES - All infrastructure managed by Rust

**Context**: Common (Rust TaskerConfig used by worker FFI layer)

---

## Configuration Fields NOT Used by Ruby

### Not Used Directly (Delegated to Rust)
- `database.*` - Rust reads from DATABASE_URL environment variable
- `queues.*` - Rust manages PGMQ configuration
- `event_systems.*` - Rust manages event coordination
- `mpsc_channels.*` - Rust internal communication
- `circuit_breakers.*` - Rust resilience layer
- `backoff.*` - Rust retry logic
- `telemetry.*` - Not used
- `orchestration.*` - Rust orchestration system
- `worker.web.*` - No Ruby web API (Rust worker has web API)

### Ruby Has No Configuration File
Ruby worker does NOT have a configuration file equivalent to TaskerConfig. It relies on:
1. Environment variables for system settings
2. Task templates for workflow definitions
3. Task context for handler-specific config
4. Rust FFI for infrastructure

---

## Key Insights

### 1. Ruby Worker Configuration is Minimal

**Environment Variable Based**:
- No TOML configuration files in Ruby
- All system config via environment variables
- Template discovery via filesystem paths
- Handler config via task context

**FFI Delegation Pattern**:
- Rust handles all infrastructure (database, queues, events)
- Ruby focuses on business logic (handlers)
- Clean separation of concerns

### 2. Configuration Access Patterns

**Primary Pattern**: Environment variables for system settings
```ruby
environment = ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || 'development'
database_url = ENV['DATABASE_URL']
template_path = ENV['TASKER_TEMPLATE_PATH']
```

**Secondary Pattern**: Task templates from filesystem/database
```ruby
template_path = TaskerCore::TemplateDiscovery::TemplatePath.find_template_config_directory
templates = TaskTemplateManager.load_templates(template_path)
```

**Tertiary Pattern**: Handler config from task context
```ruby
handler_config = task.context.dig(:handler_config) || {}
timeout = handler_config[:timeout] || 30000
```

### 3. Ruby vs Rust Worker Configuration

**Ruby Worker Configuration**:
- Environment variables: TASKER_ENV, DATABASE_URL, TASKER_TEMPLATE_PATH
- Task templates: YAML files or database records
- Handler config: Task context (passed per-execution)
- Infrastructure: Delegated to Rust via FFI

**Rust Worker Configuration (from Step 1.2)**:
- TaskerConfig TOML: worker.*, event_systems.worker.*, queues.*, database.*
- SystemContext: Database pool, circuit breakers, event publisher
- Bootstrap config: Web API, orchestration API location

**Implication**: Ruby and Rust workers have COMPLETELY DIFFERENT configuration systems
- Ruby: Environment variables + templates + task context
- Rust: TOML files + TaskerConfig struct + SystemContext

### 4. Shared Configuration Between Ruby and Rust

**Environment Variables Shared**:
- `TASKER_ENV` / `RAILS_ENV` - Environment detection
- `DATABASE_URL` - Database connection string
- `RUST_LOG` - Logging level (Rust consumes)

**Configuration Rust Must Provide to Ruby via FFI**:
- Task execution events (what to execute)
- Task context (handler config)
- Completion acknowledgment (results received)

**Configuration Ruby Must Provide to Rust via FFI**:
- Completion events (success/failure/retryable)
- Error details (error_message, error_class, retryable flag)
- Result data (handler output)

---

## Recommendations for Context-Specific Config

### Ruby Worker Configuration (Environment Variable Based)
```bash
# Required environment variables
TASKER_ENV=development                          # Environment
DATABASE_URL=postgresql://localhost/tasker_dev  # Database connection
TASKER_TEMPLATE_PATH=/path/to/templates        # Template discovery

# Optional environment variables
RUST_LOG=info                                  # Rust log level
LOG_LEVEL=debug                                # Ruby log level
RUBY_GC_HEAP_GROWTH_FACTOR=1.1                # Production GC tuning
WORKSPACE_PATH=/app                            # Workspace root for discovery
TASKER_FORCE_EXAMPLE_HANDLERS=true             # Test mode (load examples)
```

### Ruby Worker Does NOT Need TaskerConfig
Ruby worker should NOT load TaskerConfig TOML files because:
1. It delegates all infrastructure to Rust via FFI
2. It uses environment variables for system settings
3. It uses task templates for workflow definitions
4. It uses task context for handler-specific config

### Rust Worker Configuration (TOML Based)
From Step 1.2 analysis, Rust worker needs:
```toml
# Common configuration (shared with orchestration)
[database]
# ... database pool config

[queues]
# ... PGMQ namespace config

# Worker-specific configuration
[worker.web]
enabled = true
bind_address = "0.0.0.0:8081"
# ... web API config

[event_systems.worker]
deployment_mode = "Hybrid"
# ... event system config
```

---

## Critical Discovery: Ruby vs Rust Worker Configuration Dichotomy

**Ruby Worker (Environment Variable Based)**:
- ZERO TaskerConfig TOML files needed
- Environment variables for ALL system config
- FFI delegates infrastructure to Rust
- Focus: Business logic handlers

**Rust Worker (TOML Based)**:
- TaskerConfig TOML files required
- Structured configuration via config crate
- Direct infrastructure management
- Focus: High-performance step execution

**Implication for TAS-50**:
The CLI configuration tool must support TWO different worker deployment patterns:

1. **Ruby Worker Deployment**: Environment variable generator
   - Generate shell script or .env file
   - No TOML configuration needed
   - Focus on DATABASE_URL, TASKER_ENV, TASKER_TEMPLATE_PATH

2. **Rust Worker Deployment**: TOML configuration generator
   - Generate tasker.toml with Common + Worker sections
   - Include worker.web.*, event_systems.worker.*, queues.*
   - Exclude orchestration-specific config

**Interactive CLI Must Ask**:
```
? Which worker type are you deploying?
  > Ruby Worker (FFI-based, environment variables)
  > Rust Worker (Native, TOML configuration)
  > Both (generate both config types)
```

---

## Next Steps

1. ✅ Analyze tasker-orchestration configuration usage (Step 1.1)
2. ✅ Analyze tasker-worker (Rust) configuration usage (Step 1.2)
3. ✅ Analyze Ruby worker configuration usage (Step 1.3)
4. ⏳ Create comprehensive usage matrix (Step 1.4)
5. Design configuration contexts:
   - CommonConfig (Rust TOML)
   - OrchestrationConfig (Rust TOML)
   - RustWorkerConfig (Rust TOML)
   - RubyWorkerConfig (Environment variables)
6. Build CLI tools:
   - TOML generator for Rust deployments (orchestration + rust worker)
   - Environment variable generator for Ruby worker deployments
   - Validator for both config types

---

**Analysis Date**: 2025-10-14
**Analyst**: TAS-50 Configuration Analysis
