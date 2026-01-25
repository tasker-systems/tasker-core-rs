# TAS-73: Cluster Configuration Findings

## Summary

During multi-instance cluster testing, we discovered a configuration architecture issue that prevents e2e tests from passing in cluster mode.

## Root Cause

### Problem: Environment Variable Inheritance

The cluster start scripts (`start-cluster.sh`) inherit environment from the root `.env` file, which is configured for orchestration, not workers.

**Root `.env` has:**
```bash
TASKER_CONFIG_PATH=/Users/.../config/tasker/generated/complete-test.toml
TASKER_TEMPLATE_PATH=/Users/.../config/tasks
```

**But rust workers need:**
```bash
TASKER_CONFIG_PATH=/Users/.../config/tasker/generated/worker-test.toml
TASKER_TEMPLATE_PATH=/Users/.../tests/fixtures/task_templates/rust
```

### Impact

1. **Wrong template path**: Workers load templates from `/config/tasks` which only has `integration`, `payments`, `test` namespaces
2. **E2E tests use different namespaces**: Test fixtures in `/tests/fixtures/task_templates/rust/` use namespaces like `rust_e2e_linear`, `diamond_workflow`, etc.
3. **Result**: Workers poll the wrong queues, steps get enqueued but never claimed

### Evidence

Worker logs show polling only:
- `worker_integration_queue` (namespace=integration)
- `worker_payments_queue` (namespace=payments)
- `worker_test_queue` (namespace=test)

But test tasks use namespace `rust_e2e_linear` which maps to `worker_rust_e2e_linear_queue` - not subscribed.

## Observed Behavior

1. Task created successfully via orchestration API
2. Initial steps marked as `enqueued` in database
3. Steps never transition to `in_progress` (no worker claims them)
4. Task times out in `steps_in_process` state

Database snapshot:
```
task_state        | step_name     | step_state | in_process | processed
------------------+---------------+------------+------------+-----------
steps_in_process  | linear_step_1 | enqueued   | f          | f
steps_in_process  | linear_step_2 | pending    | f          | f
steps_in_process  | linear_step_3 | pending    | f          | f
steps_in_process  | linear_step_4 | pending    | f          | f
```

## Proposed Solutions

### Option 1: Service-Specific Root Environment Files

Create separate root `.env` generation for orchestration vs workers:

```bash
cargo make setup-env-orchestration-cluster  # Creates .env for orchestration
cargo make setup-env-workers-cluster        # Creates workers/*.env with correct paths
```

### Option 2: Override Env Vars in Cluster Start Scripts

Update `start-cluster.sh` to export service-specific environment:

```bash
case "$SERVICE_TYPE" in
    worker-rust)
        export TASKER_CONFIG_PATH="${PROJECT_ROOT}/config/tasker/generated/worker-test.toml"
        export TASKER_TEMPLATE_PATH="${PROJECT_ROOT}/tests/fixtures/task_templates/rust"
        ;;
esac
```

### Option 3: Unified Test Templates

Move/symlink test fixtures into `/config/tasks/` so all templates are in one location. This would require namespace restructuring.

## Recommendation

**Option 2** is the most surgical fix. Update `start-cluster.sh` to set service-appropriate environment variables before starting each service type.

This preserves the existing setup-env architecture while ensuring cluster workers get the right configuration.

## Additional Findings

### .env Loading in Tests

Tests running via `cargo test` don't automatically load `.env` files. The workaround is:

```bash
set -a; source .env; set +a; cargo test ...
```

The cargo-make `test-rust` task includes this, but direct `cargo test` does not.

**Future consideration**: Add a test harness bootstrap that uses `dotenvy` to auto-load environment.

### Test URL Configuration

Tests use singular env vars (`TASKER_TEST_WORKER_URL`) while cluster config uses plural (`TASKER_TEST_WORKER_URLS`). We added singular vars to `cluster.env` for backward compatibility.

## Next Steps

1. Implement Option 2 fix in `start-cluster.sh`
2. Restart cluster with correct configuration
3. Verify e2e tests pass against multi-instance cluster
4. Investigate any remaining race conditions (the actual TAS-73 goal)

## Files Involved

- `cargo-make/scripts/multi-deploy/start-cluster.sh` - needs service-specific env overrides
- `config/dotenv/cluster.env` - cluster-specific settings
- `config/dotenv/rust-worker.env` - has correct paths, but not used by cluster scripts
- `.env` (root) - currently orchestration-focused, used by cluster scripts
