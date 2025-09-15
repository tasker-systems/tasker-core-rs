# TaskerCore Ruby Bindings Configuration

## Gemfile-Root Based Configuration (TAS-34 Phase 2)

TaskerCore Ruby bindings now use a **Gemfile-root based configuration system** that ensures complete isolation from the tasker-core repository. This enables independent deployment of Ruby workers while maintaining coordination through PostgreSQL and pgmq.

## Configuration Architecture

### Directory Structure
```
your-ruby-app/
├── Gemfile                      # Application root marker (REQUIRED)
├── config/
│   └── tasker/                  # Component-based configuration
│       ├── auth.yaml           # Authentication & authorization
│       ├── database.yaml       # Database connection settings
│       ├── engine.yaml         # Rails engine configuration
│       ├── pgmq.yaml          # PostgreSQL message queue settings
│       ├── system.yaml        # System settings & dependent systems
│       ├── telemetry.yaml     # Monitoring & observability
│       └── environments/      # Environment-specific overrides
│           ├── test/
│           ├── development/
│           └── production/
└── app/                        # Your application code
```

### Configuration Discovery

1. **PathResolver** looks for `Gemfile` to determine application root
2. **Config** loads from `{gemfile_root}/config/tasker/`
3. **Component-based** loading with environment overrides
4. **NO fallback** to repo-level configuration

## Quick Start

### Generate Configuration
```ruby
# In your Ruby application directory (where Gemfile exists)
require 'tasker_core'

# Generate configuration for Rails app
TaskerCore.generate_config!(app_type: 'rails')

# Generate configuration for Sinatra app
TaskerCore.generate_config!(app_type: 'sinatra')

# Generate configuration for standalone Ruby app
TaskerCore.generate_config!(app_type: 'standalone')
```

### Manual Setup
```bash
# Create directory structure
mkdir -p config/tasker/environments/{test,development,production}

# Copy templates from workers/ruby/templates/
cp -r templates/config/tasker/* config/tasker/
```

## Component Configuration

### Ruby-Specific Components

The Ruby bindings care about these components:

| Component | Purpose | Key Settings |
|-----------|---------|--------------|
| `auth.yaml` | JWT, session, API key authentication | JWT secret, session timeout, GraphQL permissions |
| `database.yaml` | PostgreSQL ActiveRecord settings | Connection pool, host, credentials |
| `engine.yaml` | Rails engine configuration | Event system, registries, health monitoring |
| `pgmq.yaml` | Queue worker settings | Poll interval, batch size, worker pool |
| `system.yaml` | System configuration | Default dependent system, resource limits |
| `telemetry.yaml` | Observability settings | Metrics, tracing, health checks |

### Environment Overrides

Environment-specific settings override base components:

```yaml
# config/tasker/environments/production/pgmq.yaml
pgmq:
  poll_interval_ms: 500      # Less aggressive than development
  batch_size: 10             # Larger batches for efficiency
  max_workers: 20            # More workers for production load
```

## Configuration Loading

### Automatic Discovery
```ruby
# TaskerCore automatically discovers configuration
require 'tasker_core'

# Boot sequence finds Gemfile root and loads config
TaskerCore::Boot.boot!
# Logs: "Configuration mode: component-based (Gemfile-root)"
```

### Programmatic Access
```ruby
# Access configuration
config = TaskerCore::Config.instance
puts config.environment              # => "production"
puts config.config_directory         # => "/app/config"
puts config.effective_config         # => Merged configuration hash
```

## Environment Variables

Use environment variable substitution in YAML:

```yaml
database:
  host: "${DATABASE_HOST:-localhost}"
  password: "${DATABASE_PASSWORD}"
  pool: "${DATABASE_POOL:-25}"
```

## Migration from Repo-Level Config

### Before (Repo-Level)
```ruby
# Config loaded from tasker-core/config/
# Tight coupling to repository structure
# PathResolver looked for Cargo.toml
```

### After (Gemfile-Root)
```ruby
# Config loaded from your-app/config/tasker/
# Complete isolation from repository
# PathResolver ONLY looks for Gemfile
# No fallback to repo-level paths
```

## Verification

Run the verification script to ensure proper configuration:

```bash
ruby scripts/verify_gemfile_root_config.rb
```

This verifies:
- PathResolver finds Gemfile only
- Config directory is under Gemfile root
- No references to repo-level paths
- Component discovery works correctly
- Boot sequence uses correct configuration

## Deployment

The Ruby bindings with Gemfile-root configuration can be deployed:

1. **Independently** - Deploy Ruby workers separately from orchestration core
2. **Containerized** - Include `config/tasker/` in your Docker image
3. **Cloud Native** - Use ConfigMaps or environment variables for settings
4. **Multi-Environment** - Same codebase with different environment configs

## Troubleshooting

### No Gemfile Found Error
```
❌ PROJECT_ROOT: No Gemfile found!
```
**Solution**: Ensure you're running from a Ruby application directory with a Gemfile.

### Configuration Not Found
```
Configuration file not found. Looked for: /app/config/tasker-config.yaml
```
**Solution**: Generate configuration using `TaskerCore.generate_config!` or create component files manually.

### Wrong Configuration Mode
```
Configuration mode: monolithic (Gemfile-root)
```
**Solution**: This is fine for backward compatibility. To use component-based, ensure `config/tasker/*.yaml` files exist.

## Best Practices

1. **Use Component Configuration** - Organize settings by concern
2. **Environment Overrides** - Keep base components generic, override per environment
3. **Environment Variables** - Use `${VAR:-default}` for deployment flexibility
4. **Version Control** - Commit base configs, use env vars for secrets
5. **Independent Deployment** - Deploy workers independently from orchestration core
