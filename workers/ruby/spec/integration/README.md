# Ruby Worker Docker Integration Tests

This directory contains Docker-based integration tests that validate Ruby handler execution through FFI with containerized Rust services.

## Architecture Overview

### New Docker-based Approach
- **Orchestration Service**: Running in Docker container with HTTP API
- **Ruby Worker Service**: Ruby process that bootstraps Rust foundation via FFI
- **Test Communication**: HTTP clients communicate with running services
- **Handler Execution**: Ruby worker discovers and executes Ruby handlers for steps
- **FFI Integration**: Ruby bootstrap system manages Rust foundation lifecycle
- **Service Isolation**: Clean boundaries, realistic deployment environment

### Previous Embedded Approach (Deprecated)
- **SharedTestLoop**: Embedded Ruby components directly instantiated
- **Manual Loading**: Tests manually require_relative handler files
- **Direct FFI**: Tests call TaskerCore::initialize_task_embedded() directly
- **No Isolation**: All components running in same process

## Running Docker Integration Tests

### Prerequisites

1. **Start Docker Services**:
   ```bash
   cd /path/to/tasker-core
   docker-compose -f docker/docker-compose.test.yml up --build -d
   ```

2. **Verify Services Are Running**:
   ```bash
   curl http://localhost:8080/health  # Orchestration service
   curl http://localhost:8082/health  # Ruby worker service (port 8082)
   ```

3. **Install Ruby Dependencies**:
   ```bash
   cd workers/ruby
   bundle install
   ```

### Running Tests

#### Run All Docker Integration Tests
```bash
cd workers/ruby
bundle exec rspec spec/integration/*docker*_spec.rb --format documentation
```

#### Run Specific Workflow Tests
```bash
# Diamond workflow with Docker services
bundle exec rspec spec/integration/diamond_workflow_docker_integration_spec.rb

# Linear workflow with Docker services
bundle exec rspec spec/integration/linear_workflow_docker_integration_spec.rb

# Integration test runner (health checks and smoke tests)
bundle exec rspec spec/integration/docker_integration_runner_spec.rb
```

#### Run With Different Service URLs
```bash
# Custom service URLs
export TASKER_TEST_ORCHESTRATION_URL="http://localhost:9080"
export TASKER_TEST_RUBY_WORKER_URL="http://localhost:9082"
bundle exec rspec spec/integration/diamond_workflow_docker_integration_spec.rb

# Skip health checks (useful for CI)
export TASKER_TEST_SKIP_HEALTH_CHECK="true"
bundle exec rspec spec/integration/docker_integration_runner_spec.rb
```

## Available Test Files

### Docker-based Integration Tests (New)
- `diamond_workflow_docker_integration_spec.rb` - Diamond pattern with parallel branches
- `linear_workflow_docker_integration_spec.rb` - Sequential step processing
- `docker_integration_runner_spec.rb` - Comprehensive smoke tests and health checks
- `test_helpers/ruby_integration_manager.rb` - HTTP client utilities and service management

### Legacy Embedded Tests (Deprecated)
- `diamond_workflow_integration_spec.rb` - Original embedded approach
- `linear_workflow_integration_spec.rb` - Original embedded approach
- `test_helpers/shared_test_loop.rb` - Original embedded test utilities

## Configuration

### Environment Variables
```bash
# Service endpoints
TASKER_TEST_ORCHESTRATION_URL="http://localhost:8080"  # Orchestration API
TASKER_TEST_RUBY_WORKER_URL="http://localhost:8082"    # Ruby worker API

# Health check configuration
TASKER_TEST_SKIP_HEALTH_CHECK="false"     # Skip service health checks
TASKER_TEST_HEALTH_TIMEOUT="30"           # Health check timeout (seconds)
TASKER_TEST_HEALTH_RETRY_INTERVAL="2"     # Health check retry interval (seconds)

# Template and handler discovery
TASKER_TEMPLATE_PATH="/app/ruby_templates"   # Ruby handler template discovery
RUBY_HANDLER_PATH="/app/ruby_handlers"     # Ruby handler source files
```

### Docker Service Configuration
The Ruby worker service is configured in `docker/docker-compose.test.yml`:

```yaml
ruby-worker:
  build:
    dockerfile: docker/build/ruby-worker.test.Dockerfile
  ports:
    - "8082:8081"  # External port 8082, internal port 8081
  volumes:
    - ./workers/ruby/spec/handlers/examples:/app/ruby_handlers:ro
    - ./workers/ruby/spec/fixtures/templates:/app/ruby_templates:ro
  environment:
    TASKER_TEMPLATE_PATH: /app/ruby_templates
    RUBY_WORKER_ENABLED: "true"
```

**Key Architecture Notes:**
- Ruby process runs `TaskerCore::Worker::Bootstrap.start!`
- Ruby bootstrap calls `TaskerCore::FFI.bootstrap_worker` to start Rust foundation
- Rust worker provides HTTP API endpoints (health, status) on port 8081
- Ruby handlers are discovered from mounted YAML templates
- Ruby executes handlers when called by Rust via FFI

## Handler Discovery

Ruby handlers are discovered through:

1. **YAML Templates**: Mounted from `spec/fixtures/templates/` to `/app/ruby_templates`
2. **Handler Files**: Mounted from `spec/handlers/examples/` to `/app/ruby_handlers`
3. **Template Discovery**: Ruby `TemplateDiscovery` system crawls YAML files
4. **FFI Registration**: Handlers registered via FFI for Rust worker to call

## Test Flow

1. **Service Setup**: RubyWorkerIntegrationManager connects to Docker services
2. **Health Checks**: Verify all services are running and healthy
3. **Task Creation**: Create workflow task via orchestration HTTP API
4. **Execution**: Rust worker discovers ready steps and calls Ruby handlers via FFI
5. **Monitoring**: Poll task status via HTTP API until completion
6. **Validation**: Verify step results and workflow completion

## Troubleshooting

### Service Not Ready
```bash
# Check Docker services
docker-compose -f docker/docker-compose.test.yml ps

# Check service logs
docker-compose -f docker/docker-compose.test.yml logs orchestration
docker-compose -f docker/docker-compose.test.yml logs ruby-worker

# Restart services
docker-compose -f docker/docker-compose.test.yml down
docker-compose -f docker/docker-compose.test.yml up --build -d
```

### Handler Discovery Issues
```bash
# Check mounted volumes
docker exec -it tasker-core_ruby-worker_1 ls -la /app/ruby_templates
docker exec -it tasker-core_ruby-worker_1 ls -la /app/ruby_handlers

# Check Ruby handler registration
docker exec -it tasker-core_ruby-worker_1 bundle exec ruby -e "
require_relative 'ruby_worker/lib/tasker_core'
puts TaskerCore::Registry::HandlerRegistry.instance.registered_handlers
"
```

### FFI Integration Issues
```bash
# Check Rust worker FFI integration
docker exec -it tasker-core_ruby-worker_1 ./rust-worker --help

# Check Ruby FFI extensions
docker exec -it tasker-core_ruby-worker_1 bundle exec ruby -e "
require_relative 'ruby_worker/lib/tasker_core'
puts TaskerCore::FFI.worker_status
"
```

## Migration Guide

### From Embedded to Docker Tests

1. **Replace SharedTestLoop**:
   ```ruby
   # Before (embedded)
   let(:shared_loop) { SharedTestLoop.new }
   task = shared_loop.run(task_request: task_request, namespace: namespace)

   # After (Docker)
   let(:manager) { RubyWorkerIntegrationManager.setup }
   task_response = manager.orchestration_client.create_task(task_request)
   task = wait_for_task_completion(manager, task_response[:task_uuid])
   ```

2. **Remove Manual Handler Loading**:
   ```ruby
   # Before (embedded)
   require_relative '../handlers/examples/diamond_workflow/step_handlers/diamond_start_handler'

   # After (Docker)
   # Handlers discovered automatically via mounted YAML templates
   ```

3. **Update Test Structure**:
   ```ruby
   # Before (embedded)
   RSpec.describe 'Diamond Workflow Integration', type: :integration do
     let(:shared_loop) { SharedTestLoop.new }

   # After (Docker)
   RSpec.describe 'Diamond Workflow Docker Integration', type: :integration do
     include RubyIntegrationTestHelpers
     let(:manager) { RubyWorkerIntegrationManager.setup }
   ```

## Benefits

### Docker-based Approach
- ✅ **Service Isolation**: True separation of concerns
- ✅ **Realistic Environment**: Matches production deployment
- ✅ **Scalable Testing**: Can test multiple workers, load balancing
- ✅ **CI/CD Compatible**: Easy integration with containerized CI
- ✅ **Debugging**: Clear service boundaries and logging
- ✅ **Performance**: Can measure real service communication overhead

### Previous Embedded Approach
- ❌ **Tight Coupling**: All components in same process
- ❌ **Unrealistic**: Doesn't match production environment
- ❌ **Thread Issues**: Complex cleanup and race conditions
- ❌ **Limited Scope**: Can't test distributed scenarios
- ❌ **Debugging Difficulty**: Mixed logging and state
