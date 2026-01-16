# Quick Start Guide

**Last Updated**: 2025-10-10
**Audience**: Developers
**Status**: Active
**Time to Complete**: 5 minutes
**Related Docs**: [Documentation Hub](README.md) | [Use Cases](use-cases-and-patterns.md) | [Crate Architecture](crate-architecture.md)

← Back to [Documentation Hub](README.md)

---

## Get Tasker Core Running in 5 Minutes

This guide will get you from zero to running your first workflow in under 5 minutes using Docker Compose.

### Prerequisites

Before starting, ensure you have:
- **Docker** and **Docker Compose** installed
- **Git** to clone the repository
- **curl** for testing (or any HTTP client)

That's it! Docker Compose handles all the complexity.

---

## Step 1: Clone and Start Services (2 minutes)

```bash
# Clone the repository
git clone https://github.com/tasker-systems/tasker-core
cd tasker-core

# Start PostgreSQL (includes PGMQ extension for default messaging)
docker-compose up -d postgres

# Wait for PostgreSQL to be ready (about 10 seconds)
docker-compose logs -f postgres
# Press Ctrl+C when you see "database system is ready to accept connections"

# Run database migrations
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
docker-compose exec postgres psql -U tasker -d tasker_rust_test -c "SELECT 1"  # Verify connection

# Start orchestration server and workers
docker-compose --profile server up -d

# Verify all services are healthy
docker-compose ps
```

You should see:
```
NAME                     STATUS              PORTS
tasker-postgres          Up (healthy)        5432
tasker-orchestration     Up (healthy)        0.0.0.0:8080->8080/tcp
tasker-worker            Up (healthy)        0.0.0.0:8081->8081/tcp
tasker-ruby-worker       Up (healthy)        0.0.0.0:8082->8082/tcp
```

---

## Step 2: Verify Services (30 seconds)

Check that all services are responding:

```bash
# Check orchestration health
curl http://localhost:8080/health

# Expected response:
# {
#   "status": "healthy",
#   "database": "connected",
#   "message_queue": "operational"
# }

# Check Rust worker health
curl http://localhost:8081/health

# Check Ruby worker health (if started)
curl http://localhost:8082/health
```

---

## Step 3: Create Your First Task (1 minute)

Now let's create a simple linear workflow with 4 steps:

```bash
# Create a task using the linear_workflow template
curl -X POST http://localhost:8080/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "template_name": "linear_workflow",
    "namespace": "rust_e2e_linear",
    "configuration": {
      "test_value": "hello_world"
    }
  }'
```

**Response**:
```json
{
  "task_uuid": "01234567-89ab-cdef-0123-456789abcdef",
  "status": "pending",
  "namespace": "rust_e2e_linear",
  "created_at": "2025-10-10T12:00:00Z"
}
```

**Save the `task_uuid` from the response!** You'll need it to check the task status.

---

## Step 4: Monitor Task Execution (1 minute)

Watch your workflow execute in real-time:

```bash
# Replace {task_uuid} with your actual task UUID
TASK_UUID="01234567-89ab-cdef-0123-456789abcdef"

# Check task status
curl http://localhost:8080/v1/tasks/${TASK_UUID}
```

**Initial Response** (task just created):
```json
{
  "task_uuid": "01234567-89ab-cdef-0123-456789abcdef",
  "current_state": "initializing",
  "total_steps": 4,
  "completed_steps": 0,
  "namespace": "rust_e2e_linear"
}
```

**Wait a few seconds** and check again:

```bash
# Check again after a few seconds
curl http://localhost:8080/v1/tasks/${TASK_UUID}
```

**Final Response** (task completed):
```json
{
  "task_uuid": "01234567-89ab-cdef-0123-456789abcdef",
  "current_state": "complete",
  "total_steps": 4,
  "completed_steps": 4,
  "namespace": "rust_e2e_linear",
  "completed_at": "2025-10-10T12:00:05Z",
  "duration_ms": 134
}
```

**Congratulations!** 🎉 You've just executed your first workflow with Tasker Core!

---

## What Just Happened?

Let's break down what happened in those ~100-150ms:

```
1. Orchestration received task creation request
   ↓
2. Task initialized with "linear_workflow" template
   ↓
3. 4 workflow steps created with dependencies:
   - mathematical_add (no dependencies)
   - mathematical_multiply (depends on add)
   - mathematical_subtract (depends on multiply)
   - mathematical_divide (depends on subtract)
   ↓
4. Orchestration discovered step 1 was ready
   ↓
5. Step 1 enqueued to "rust_e2e_linear" namespace queue
   ↓
6. Worker claimed and executed step 1
   ↓
7. Worker sent result back to orchestration
   ↓
8. Orchestration processed result, discovered step 2
   ↓
9. Steps 2, 3, 4 executed sequentially (due to dependencies)
   ↓
10. All steps complete → Task marked "complete"
```

**Key Observations**:
- Each step executed by autonomous workers
- Steps executed in dependency order automatically
- Complete workflow: ~130-150ms (including all coordination)
- All state changes recorded in audit trail

---

## View Detailed Task Information

Get complete task execution details:

```bash
# Get full task details including steps
curl http://localhost:8080/v1/tasks/${TASK_UUID}/details
```

**Response includes**:
```json
{
  "task": {
    "task_uuid": "...",
    "current_state": "complete",
    "namespace": "rust_e2e_linear"
  },
  "steps": [
    {
      "name": "mathematical_add",
      "current_state": "complete",
      "result": {"value": 15},
      "duration_ms": 12
    },
    {
      "name": "mathematical_multiply",
      "current_state": "complete",
      "result": {"value": 30},
      "duration_ms": 8
    },
    // ... remaining steps
  ],
  "state_transitions": [
    {
      "from_state": null,
      "to_state": "pending",
      "timestamp": "2025-10-10T12:00:00.000Z"
    },
    {
      "from_state": "pending",
      "to_state": "initializing",
      "timestamp": "2025-10-10T12:00:00.050Z"
    },
    // ... complete transition history
  ]
}
```

---

## Try a More Complex Workflow

Now try the diamond workflow pattern (parallel execution):

```bash
curl -X POST http://localhost:8080/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "template_name": "diamond_workflow",
    "namespace": "rust_e2e_diamond",
    "configuration": {
      "test_value": "parallel_test"
    }
  }'
```

**Diamond pattern**:
```
        step_1 (root)
       /            \
   step_2          step_3    ← Execute in PARALLEL
       \            /
        step_4 (join)
```

Steps 2 and 3 execute simultaneously because they both depend only on step 1!

---

## View Logs

See what's happening inside the services:

```bash
# Orchestration logs
docker-compose logs -f orchestration

# Worker logs
docker-compose logs -f worker

# All logs
docker-compose logs -f
```

**Key log patterns to look for**:
- `Task initialized: task_uuid=...` - Task created
- `Step enqueued: step_uuid=...` - Step sent to worker
- `Step claimed: step_uuid=...` - Worker picked up step
- `Step completed: step_uuid=...` - Step finished successfully
- `Task finalized: task_uuid=...` - Workflow complete

---

## Explore the API

### List All Tasks

```bash
curl http://localhost:8080/v1/tasks
```

### Get Task Execution Context

```bash
curl http://localhost:8080/v1/tasks/${TASK_UUID}/context
```

### View Available Templates

```bash
curl http://localhost:8080/v1/templates
```

### Check System Health

```bash
curl http://localhost:8080/health/detailed
```

---

## Next Steps

### 1. **Understand What You Just Built**

Read about the architecture:
- **[Crate Architecture](crate-architecture.md)** - How the workspace is organized
- **[Events and Commands](events-and-commands.md)** - How orchestration and workers coordinate
- **[States and Lifecycles](states-and-lifecycles.md)** - Task and step state machines

### 2. **See Real-World Examples**

Explore practical use cases:
- **[Use Cases and Patterns](use-cases-and-patterns.md)** - E-commerce, payments, ETL, microservices
- See example templates in: `tests/fixtures/task_templates/`

### 3. **Create Your Own Workflow**

#### Option A: Rust Handler (Native Performance)

```rust
// workers/rust/src/handlers/my_handler.rs
pub struct MyCustomHandler;

#[async_trait]
impl StepHandler for MyCustomHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        // Your business logic here
        let input: String = context.configuration.get("input")?;

        let result = process_data(&input).await?;

        Ok(StepResult::success(json!({
            "output": result
        })))
    }
}
```

#### Option B: Ruby Handler (via FFI)

```ruby
# workers/ruby/app/tasker/tasks/templates/my_workflow/handlers/my_handler.rb
class MyHandler < TaskerCore::StepHandler
  def execute(context)
    input = context.configuration['input']

    result = process_data(input)

    { success: true, output: result }
  end
end
```

#### Define Your Workflow Template

```yaml
# tests/fixtures/task_templates/rust/my_workflow.yaml
namespace: my_namespace
name: my_workflow
version: "1.0"

steps:
  - name: my_step
    handler: my_handler
    dependencies: []
    retry:
      retryable: true
      max_attempts: 3
      backoff: exponential
      backoff_base_ms: 1000
```

### 4. **Deploy to Production**

Learn about deployment:
- **[Deployment Patterns](deployment-patterns.md)** - Hybrid, EventDriven, PollingOnly modes
- **[Observability](observability/README.md)** - Metrics, logging, monitoring
- **[Benchmarks](benchmarks/README.md)** - Performance validation

### 5. **Run Tests Locally**

```bash
# Build the workspace
cargo build --all-features

# Run all tests
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test" \
cargo test --all-features

# Run benchmarks
cargo bench --all-features
```

---

## Troubleshooting

### Services Won't Start

```bash
# Check Docker service status
docker-compose ps

# View service logs
docker-compose logs postgres
docker-compose logs orchestration

# Restart services
docker-compose restart

# Clean restart
docker-compose down
docker-compose up -d
```

### Task Stays in "pending" or "initializing"

**Possible causes**:
1. **Template not found** - Check available templates: `curl http://localhost:8080/v1/templates`
2. **Worker not running** - Check worker status: `curl http://localhost:8081/health`
3. **Database connection issue** - Check logs: `docker-compose logs postgres`

**Solution**:
```bash
# Verify template exists
curl http://localhost:8080/v1/templates | jq '.[] | select(.name == "linear_workflow")'

# Restart workers
docker-compose restart worker

# Check orchestration logs for errors
docker-compose logs orchestration | grep ERROR
```

### "Connection refused" Errors

**Cause**: Services not fully started yet

**Solution**: Wait 10-15 seconds after `docker-compose up`, then check health:
```bash
curl http://localhost:8080/health
```

### PostgreSQL Connection Issues

```bash
# Verify PostgreSQL is running
docker-compose ps postgres

# Test connection
docker-compose exec postgres psql -U tasker -d tasker_rust_test -c "SELECT 1"

# View PostgreSQL logs
docker-compose logs postgres | tail -50
```

---

## Cleanup

When you're done exploring:

```bash
# Stop all services
docker-compose down

# Stop and remove volumes (cleans database)
docker-compose down -v

# Remove all Docker resources (complete cleanup)
docker-compose down -v
docker system prune -f
```

---

## Summary

You've successfully:
- ✅ Started Tasker Core services with Docker Compose
- ✅ Created and executed a linear workflow
- ✅ Monitored task execution in real-time
- ✅ Viewed detailed task and step information
- ✅ Explored the REST API

**Total time**: ~5 minutes from zero to working workflow! 🚀

---

## Getting Help

- **Documentation Issues**: Open an issue on GitHub
- **Architecture Questions**: See [Crate Architecture](crate-architecture.md)
- **Use Case Examples**: See [Use Cases and Patterns](use-cases-and-patterns.md)
- **Deployment Help**: See [Deployment Patterns](deployment-patterns.md)

---

← Back to [Documentation Hub](README.md)

**Next**: [Use Cases and Patterns](use-cases-and-patterns.md) | [Crate Architecture](crate-architecture.md)
