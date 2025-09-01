# Docker Integration Tests

This directory contains Docker-based integration tests using the **shared service pattern** for efficient testing.

## Quick Start

```rust
use tasker_core::test_helpers::{DockerTestClient, create_test_task_request};

#[tokio::test]
async fn my_integration_test() -> Result<()> {
    // Lightweight client - services start automatically if needed
    let test_client = DockerTestClient::new("my_test").await?;
    
    let context = create_mathematical_test_context(6);
    let request = create_test_task_request(/* ... */);
    
    // Run test against shared Docker services
    let result = test_client.run_integration_test(request, 30).await?;
    
    assert_eq!(result.status, "completed");
    Ok(())
}
```

## Architecture

### Shared Service Pattern
- **One Docker stack** serves all integration tests
- **Fast test execution** - no per-test container startup
- **Concurrent testing** - multiple tests can run simultaneously
- **Resource efficient** - single PostgreSQL, orchestration, and worker containers

### Components
- `DockerTestSuiteManager` - Singleton managing Docker service lifecycle
- `DockerTestClient` - Lightweight per-test client interface
- Shared PostgreSQL with PGMQ + UUID v7 extensions
- Shared orchestration and worker services

## Running Tests

### Individual Test
```bash
cargo test --test docker_integration_tests test_linear_workflow_basic
```

### All Integration Tests  
```bash
cargo test --test docker_integration_tests
```

### With Logging
```bash
RUST_LOG=info cargo test --test docker_integration_tests -- --nocapture
```

## Test Development

### Creating New Tests

1. **Use DockerTestClient for lightweight setup:**
   ```rust
   let test_client = DockerTestClient::new("unique_test_name").await?;
   ```

2. **Create test requests with helpers:**
   ```rust
   let context = create_mathematical_test_context(even_number);
   let request = create_test_task_request(namespace, name, version, context, description);
   ```

3. **Execute tests against shared services:**
   ```rust
   let result = test_client.run_integration_test(request, timeout_seconds).await?;
   ```

4. **Assert on results:**
   ```rust
   assert_eq!(result.status, "completed");
   assert_eq!(result.failed_steps, 0);
   ```

### Test Naming Convention

Use descriptive test names that include:
- Workflow type: `test_linear_workflow_*`, `test_diamond_workflow_*`
- Test scenario: `*_basic_execution`, `*_error_handling`, `*_concurrent`
- Unique identifier for logging clarity

## Performance

### Startup Time
- **First test**: ~60 seconds (Docker service startup)
- **Subsequent tests**: <1 second (shared services)

### Resource Usage
- **Memory**: ~500MB total (vs ~2GB+ for per-test containers)
- **Disk**: ~1GB (vs ~5GB+ for per-test containers)

### Parallelization
Tests can run concurrently against shared services:
```rust
let tasks = vec![
    test_client_1.run_integration_test(request_1, 30),
    test_client_2.run_integration_test(request_2, 30), 
    test_client_3.run_integration_test(request_3, 30),
];

let results = futures::future::try_join_all(tasks).await?;
```

## Debugging

### Service Health
```rust
let suite_manager = DockerTestSuiteManager::get_or_start().await?;
assert!(suite_manager.are_services_healthy());
println!("Uptime: {:?}", suite_manager.uptime());
```

### Service Logs
```rust
let logs = test_client.get_service_logs("orchestration").await?;
println!("Orchestration logs:\n{}", logs);
```

### Docker Commands
```bash
# Check running containers
docker-compose -f docker/docker-compose.integration.yml ps

# View logs
docker-compose -f docker/docker-compose.integration.yml logs orchestration
docker-compose -f docker/docker-compose.integration.yml logs worker
docker-compose -f docker/docker-compose.integration.yml logs postgres

# Manual cleanup (if needed)
docker-compose -f docker/docker-compose.integration.yml down -v
```

## File Structure

```
tests/docker_integration/
├── README.md                              # This file
├── linear_workflow_integration_test.rs    # Linear workflow specific tests
└── all_workflows_test.rs                 # Comprehensive workflow tests
    ├── Linear Workflow Tests              # Sequential step execution (input^8)
    ├── Diamond Workflow Tests             # Converging/diverging patterns
    ├── Tree Workflow Tests                # Hierarchical branching
    ├── Mixed DAG Workflow Tests           # Complex dependency graphs
    ├── Order Fulfillment Tests            # Business workflow patterns
    └── Performance Tests                  # Concurrent execution & throughput
```

## Workflow Patterns Tested

### Linear Workflow
- **Pattern**: Sequential step execution (Step1 → Step2 → Step3 → Step4)
- **Example**: Mathematical sequence where input^8 is calculated
- **Tests**: Complete sequence, multiple inputs, error handling

### Diamond Workflow  
- **Pattern**: Split → Parallel branches → Converge
- **Example**: Initial step splits to branch_a and branch_b, then converges
- **Tests**: Convergence patterns, parallel branch processing

### Tree Workflow
- **Pattern**: Hierarchical branching (root → level1 → level2 leaves)
- **Example**: Data aggregation and hierarchical processing
- **Tests**: Branching logic, result aggregation

### Mixed DAG Workflow
- **Pattern**: Complex directed acyclic graph with multiple paths
- **Example**: Complex dependency resolution with parallel and sequential parts
- **Tests**: Complex dependencies, parallel execution paths

### Order Fulfillment
- **Pattern**: Business process workflow
- **Example**: Order validation → inventory → payment → shipping → notification
- **Tests**: Complete flow, inventory checking, payment processing

## Best Practices

1. **Unique test names** - helps with log correlation
2. **Reasonable timeouts** - allow for service warmup on first test
3. **Descriptive assertions** - include context in assertion messages
4. **Error handling** - use `?` for clean error propagation
5. **Test isolation** - use unique test data/namespaces when needed

## Simple, Clean API

The integration testing API is designed to be simple and efficient:
- `DockerTestClient::new()` - Create lightweight test client
- `create_test_task_request()` - Build task requests  
- `create_mathematical_test_context()` - Create test contexts
- `run_integration_test()` - Execute complete test workflow

## Troubleshooting

### Tests Hanging
- Check Docker service health: `docker ps`
- Verify ports aren't in use: `lsof -i :8080,8081,5432`
- Check Docker Compose logs for errors

### Resource Issues
- Ensure Docker has sufficient memory allocated (4GB+ recommended)
- Close other applications using significant resources
- Check disk space in Docker directory

### Network Issues
- Verify Docker network connectivity
- Check firewall settings for localhost ports
- Ensure no VPN interference with Docker networking

### Service Startup Failures
- Check Docker Compose file path in source code
- Verify PostgreSQL extensions are properly built
- Check for missing workspace dependencies