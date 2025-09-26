# Ruby Worker Local Development Guide

Comprehensive guide for setting up, running, and testing the Tasker Core Ruby Worker locally.

## Quick Start

### Option 1: Docker-based Development (Recommended)
```bash
# 1. Start all services
cd /path/to/tasker-core
docker compose -f docker/docker-compose.test.yml up --build -d

# 2. Run Ruby tests
cd workers/ruby
bundle install
bundle exec rspec --format documentation

# 3. Verify integration
curl http://localhost:8082/health  # Ruby worker
curl http://localhost:8080/health  # Orchestration service
```

### Option 2: Local Services with Docker Database
```bash
# 1. Start only PostgreSQL
cd /path/to/tasker-core
docker compose -f docker/docker-compose.test.yml up postgres -d

# 2. Setup Ruby environment
cd workers/ruby
bundle install
bundle exec rake compile

# 3. Setup Rust environment
cd ../..
cargo build --all-features

# 4. Run tests locally
cd workers/ruby
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
bundle exec rspec --format documentation
```

### Option 3: Fully Native Development
```bash
# 1. Setup PostgreSQL with PGMQ
createdb tasker_rust_test
psql tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq;"

# 2. Run migrations
cd /path/to/tasker-core
export DATABASE_URL="postgresql://localhost/tasker_rust_test"
cargo sqlx migrate run

# 3. Setup Ruby
cd workers/ruby
bundle install
bundle exec rake compile

# 4. Run services manually (see Manual Service Startup section)
```

## Environment Setup

### System Requirements
- **Ruby**: 3.0+ with development headers (`ruby-dev` or `ruby-devel`)
- **Rust**: 1.70+ with standard toolchain
- **PostgreSQL**: 12+ with PGMQ extension
- **Docker**: For containerized development (optional)

### Ruby Dependencies
```bash
cd workers/ruby
bundle install

# Key dependencies installed:
# - ffi: Foreign function interface
# - rspec: Testing framework
# - magnus: Rust-Ruby FFI bindings (compiled extension)
```

### Rust Dependencies
```bash
cd /path/to/tasker-core
cargo build --all-features

# Key components built:
# - tasker-orchestration: Core orchestration logic
# - tasker-worker: Worker implementation
# - tasker-shared: Shared types and utilities
# - pgmq-notify: PostgreSQL message queue wrapper
```

## Configuration

### Environment Variables

#### Database Configuration
```bash
# Required for all setups
export DATABASE_URL="postgresql://user:password@localhost/database_name"

# Docker setup (default)
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"

# Local PostgreSQL setup
export DATABASE_URL="postgresql://localhost/tasker_rust_dev"
```

#### Ruby Worker Configuration
```bash
# Ruby handler discovery
export TASK_TEMPLATE_PATH="/path/to/ruby/templates"
export RUBY_HANDLER_PATH="/path/to/ruby/handlers"

# Docker setup (automatic)
export TASK_TEMPLATE_PATH="/app/ruby_templates"
export RUBY_HANDLER_PATH="/app/ruby_handlers"

# Local setup
export TASK_TEMPLATE_PATH="$(pwd)/spec/fixtures/templates"
export RUBY_HANDLER_PATH="$(pwd)/spec/handlers/examples"
```

#### Service URLs (for integration tests)
```bash
# Default Docker setup
export TASKER_TEST_ORCHESTRATION_URL="http://localhost:8080"
export TASKER_TEST_RUBY_WORKER_URL="http://localhost:8082"

# Custom ports
export TASKER_TEST_ORCHESTRATION_URL="http://localhost:9080"
export TASKER_TEST_RUBY_WORKER_URL="http://localhost:9082"

# Skip health checks (useful for CI or rapid testing)
export TASKER_TEST_SKIP_HEALTH_CHECK="true"
```

#### Rust Configuration
```bash
# Environment detection
export TASKER_ENV="development"  # or test, production

# Logging
export RUST_LOG="debug"
export RUST_LOG="tasker_orchestration=debug,tasker_worker=debug"

# FFI debugging
export TASKER_FFI_DEBUG="true"
export MAGNUS_DEBUG="true"
```

## Testing

### Test Categories

#### Unit Tests (Ruby)
```bash
cd workers/ruby

# Run all unit tests
bundle exec rspec spec/ffi/ spec/types/ spec/worker/ --format documentation

# Run specific test files
bundle exec rspec spec/ffi/ffi_spec.rb --format documentation
bundle exec rspec spec/types/task_request_spec.rb --format documentation
bundle exec rspec spec/worker/bootstrap_spec.rb --format documentation

# Run with coverage
bundle exec rspec spec/ffi/ spec/types/ spec/worker/ --format documentation --require simplecov
```

#### Integration Tests (Docker-based)
```bash
cd workers/ruby

# Prerequisites: Start Docker services first
docker compose -f ../../docker/docker-compose.test.yml up --build -d

# Run all integration tests
bundle exec rspec spec/integration/ --format documentation

# Run specific workflow tests
bundle exec rspec spec/integration/diamond_workflow_docker_integration_spec.rb
bundle exec rspec spec/integration/linear_workflow_docker_integration_spec.rb
bundle exec rspec spec/integration/tree_workflow_docker_integration_spec.rb
bundle exec rspec spec/integration/mixed_dag_workflow_docker_integration_spec.rb
bundle exec rspec spec/integration/order_fulfillment_docker_integration_spec.rb

# Run smoke tests and health checks
bundle exec rspec spec/integration/docker_integration_runner_spec.rb
```

#### Rust Tests
```bash
cd /path/to/tasker-core

# Run all Rust tests
cargo test --all-features

# Run specific package tests
cargo test --all-features --package tasker-worker
cargo test --all-features --package tasker-orchestration

# Run with logging
RUST_LOG=debug cargo test --all-features -- --nocapture
```

### Test Configuration Files

#### RSpec Configuration (`.rspec`)
```
--require spec_helper
--format documentation
--color
--order random
```

#### SimpleCov Configuration (`spec/spec_helper.rb`)
```ruby
require 'simplecov'
SimpleCov.start do
  add_filter '/spec/'
  add_group 'FFI', 'lib/tasker_core/ffi'
  add_group 'Types', 'lib/tasker_core/types'
  add_group 'Worker', 'lib/tasker_core/worker'
end
```

## Manual Service Startup

For debugging and development, you may want to run services manually:

### 1. Start PostgreSQL and Run Migrations
```bash
# Option A: Docker PostgreSQL
docker compose -f docker/docker-compose.test.yml up postgres -d

# Option B: Local PostgreSQL
createdb tasker_rust_dev
psql tasker_rust_dev -c "CREATE EXTENSION IF NOT EXISTS pgmq;"

# Run migrations
export DATABASE_URL="postgresql://localhost/tasker_rust_dev"
cargo sqlx migrate run
```

### 2. Start Orchestration Service
```bash
cd /path/to/tasker-core
export DATABASE_URL="postgresql://localhost/tasker_rust_dev"
export TASKER_ENV="development"
export RUST_LOG="debug"

# Build and run orchestration service
cargo build --all-features --bin tasker-server
./target/debug/tasker-server

# Service will be available at http://localhost:8080
```

### 3. Start Ruby Worker Service
```bash
cd workers/ruby
export DATABASE_URL="postgresql://localhost/tasker_rust_dev"
export TASK_TEMPLATE_PATH="$(pwd)/spec/fixtures/templates"
export RUBY_HANDLER_PATH="$(pwd)/spec/handlers/examples"

# Compile Ruby extension
bundle exec rake compile

# Start Ruby worker
bundle exec ruby -r ./lib/tasker_core -e "
  TaskerCore::Worker::Bootstrap.start!
  puts 'Ruby worker started - press Ctrl+C to stop'
  sleep
"

# Service will be available at http://localhost:8081
```

### 4. Verify Services
```bash
# Check orchestration service
curl http://localhost:8080/health
curl http://localhost:8080/api/v1/health

# Check Ruby worker service
curl http://localhost:8081/health
curl http://localhost:8081/worker/status
curl http://localhost:8081/worker/handlers
```

## Development Workflows

### Adding New Ruby Handlers

1. **Create Handler Class**:
   ```ruby
   # spec/handlers/examples/my_workflow/step_handlers/my_step_handler.rb
   module MyWorkflow
     module StepHandlers
       class MyStepHandler < TaskerCore::Worker::BaseStepHandler
         def execute(context)
           # Your handler logic here
           { result: "Step completed", data: context }
         end
       end
     end
   end
   ```

2. **Create YAML Template**:
   ```yaml
   # spec/fixtures/templates/my_workflow/my_step.yaml
   namespace: my_workflow
   name: my_step
   version: "1.0.0"
   handler_class: "MyWorkflow::StepHandlers::MyStepHandler"
   description: "My custom step handler"
   ```

3. **Create Integration Test**:
   ```ruby
   # spec/integration/my_workflow_docker_integration_spec.rb
   RSpec.describe 'My Workflow Docker Integration', type: :integration do
     include RubyIntegrationTestHelpers
     let(:manager) { RubyWorkerIntegrationManager.setup }

     it 'executes my custom workflow' do
       task_request = create_task_request('my_workflow', 'my_step', { test: true })
       task_response = manager.orchestration_client.create_task(task_request)
       # ... test implementation
     end
   end
   ```

4. **Test the Handler**:
   ```bash
   # Start services
   docker compose -f docker/docker-compose.test.yml up --build -d

   # Run the test
   bundle exec rspec spec/integration/my_workflow_docker_integration_spec.rb
   ```

### Debugging Common Issues

#### FFI Compilation Issues
```bash
# Check Rust toolchain
rustc --version
cargo --version

# Check Ruby development headers
ruby -v
gem list ffi

# Clean and rebuild
cd workers/ruby
bundle exec rake clean
bundle exec rake compile

# Debug compilation
bundle exec rake compile --trace
```

#### Service Connection Issues
```bash
# Check Docker services
docker compose -f docker/docker-compose.test.yml ps

# Check service logs
docker compose -f docker/docker-compose.test.yml logs orchestration
docker compose -f docker/docker-compose.test.yml logs ruby-worker

# Check network connectivity
curl -v http://localhost:8080/health
curl -v http://localhost:8082/health

# Check PostgreSQL connectivity
psql $DATABASE_URL -c "SELECT 1;"
```

#### Handler Discovery Issues
```bash
# Check template files
ls -la spec/fixtures/templates/

# Check handler files
ls -la spec/handlers/examples/

# Debug handler registration
bundle exec ruby -r ./lib/tasker_core -e "
  puts 'Template path: ' + ENV['TASK_TEMPLATE_PATH'].to_s
  puts 'Handler path: ' + ENV['RUBY_HANDLER_PATH'].to_s

  registry = TaskerCore::Registry::HandlerRegistry.instance
  puts 'Registered handlers: ' + registry.registered_handlers.to_s
"
```

#### Database Issues
```bash
# Check PGMQ extension
psql $DATABASE_URL -c "SELECT * FROM pgmq.meta;"

# Check migrations
psql $DATABASE_URL -c "\\dt" | grep tasker

# Reset database
dropdb tasker_rust_test
createdb tasker_rust_test
psql tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq;"
cargo sqlx migrate run
```

## Performance Monitoring

### Local Performance Testing
```bash
# Run performance tests
bundle exec rspec spec/performance/ --format documentation

# Profile Ruby code
bundle exec ruby-prof spec/integration/diamond_workflow_docker_integration_spec.rb

# Monitor service resources
docker stats tasker-core_orchestration_1 tasker-core_ruby-worker_1
```

### Benchmarking
```bash
# Ruby FFI overhead benchmarking
bundle exec ruby benchmarks/ffi_overhead_benchmark.rb

# End-to-end workflow performance
bundle exec ruby benchmarks/workflow_performance_benchmark.rb
```

## CI/CD Integration

### Local CI Simulation
```bash
# Run the same commands as CI
cd workers/ruby

# Install dependencies (like CI)
bundle install

# Compile extension (like CI)
bundle exec rake compile

# Run unit tests with JUnit output (like CI)
bundle exec rspec spec/ffi/ spec/types/ spec/worker/ \
  --format RspecJunitFormatter \
  --out ../../target/ruby-unit-results.xml

# Start Docker services (like CI)
docker compose -f ../../docker/docker-compose.test.yml up --build -d

# Run integration tests with JUnit output (like CI)
bundle exec rspec spec/integration/ \
  --format RspecJunitFormatter \
  --out ../../target/ruby-integration-results.xml
```

### GitHub Actions Workflows
- `.github/workflows/test-ruby-unit.yml` - Ruby unit tests
- `.github/workflows/test-ruby-integration.yml` - Ruby integration tests
- `.github/workflows/ci.yml` - Main CI pipeline with Ruby tests

## Troubleshooting Guide

### Common Error Messages

#### "Magnus gem not found" or "FFI compilation failed"
```bash
# Solution: Ensure Rust toolchain is installed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable

# Reinstall gems
bundle install
bundle exec rake compile
```

#### "Connection refused" to localhost:8080 or 8082
```bash
# Solution: Start Docker services
docker compose -f docker/docker-compose.test.yml up --build -d

# Check service health
curl http://localhost:8080/health
curl http://localhost:8082/health
```

#### "PGMQ extension not found"
```bash
# Solution: Ensure PostgreSQL has PGMQ extension
psql $DATABASE_URL -c "CREATE EXTENSION IF NOT EXISTS pgmq;"

# Or use Docker PostgreSQL (includes PGMQ)
docker compose -f docker/docker-compose.test.yml up postgres -d
```

#### "Handler not found" or "Template discovery failed"
```bash
# Solution: Check environment variables
echo $TASK_TEMPLATE_PATH
echo $RUBY_HANDLER_PATH

# Set correctly for your setup
export TASK_TEMPLATE_PATH="$(pwd)/spec/fixtures/templates"
export RUBY_HANDLER_PATH="$(pwd)/spec/handlers/examples"
```

#### "Ruby worker FFI bootstrap failed"
```bash
# Solution: Check Ruby extension compilation
cd workers/ruby
bundle exec rake clean
bundle exec rake compile

# Check for compilation errors
bundle exec rake compile --trace
```

### Development Best Practices

1. **Always start with Docker setup** - it's the most reliable
2. **Use environment variables** for configuration instead of hardcoding paths
3. **Run unit tests first** before integration tests
4. **Check service logs** when debugging integration issues
5. **Use `--format documentation`** for readable test output
6. **Set `RUST_LOG=debug`** for detailed Rust service logging

### Getting Help

- Check service logs: `docker compose logs [service-name]`
- Run tests with detailed output: `--format documentation`
- Enable debug logging: `RUST_LOG=debug`
- Check the integration test README: `spec/integration/README.md`
- Review CI workflows: `.github/workflows/`

---

**Happy developing! 🚀**