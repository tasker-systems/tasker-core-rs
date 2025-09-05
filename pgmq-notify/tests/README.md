# PGMQ-Notify Integration Tests

This directory contains comprehensive integration tests for the TAS-41 event-driven notification system. These tests verify that the PGMQ wrapper functions correctly send messages and issue notifications to the appropriate channels.

## Test Structure

### Common Module (`common/mod.rs`)
Provides shared test utilities including:
- `TestDb`: Database connection and test isolation utilities
- Helper functions for message creation and namespace testing
- Automatic queue cleanup after tests complete

### Test Files

#### `basic_wrapper_test.rs`
Tests core PGMQ wrapper function functionality:
- Single message sending with `pgmq_send_with_notify`
- Batch message sending with `pgmq_send_batch_with_notify`
- Queue pattern recognition and namespace extraction
- Message content integrity verification

#### `complete_wrapper_flow_test.rs` 
Tests the complete message flow from queue creation to notification:
- End-to-end workflow verification
- Notification channel mapping validation
- Delayed message functionality
- Error handling for non-existent queues

#### `comprehensive_integration_test.rs`
The most comprehensive test covering the full TAS-41 integration:
- Orchestration namespace message handling
- Worker namespace message patterns
- Batch processing with correct notifications
- Listener channel configuration validation
- Namespace extraction edge cases
- Concurrent notification testing

## Running Tests

### Prerequisites
1. PostgreSQL database running locally
2. Database with our TAS-41 migration applied (wrapper functions created)
3. Appropriate DATABASE_URL environment variable set

### Environment Setup
```bash
# Set database URL (example)
export DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test"

# Or source the .env file
source .env
```

### Running Individual Test Files
```bash
# Run basic wrapper tests
cargo test --test basic_wrapper_test

# Run complete flow tests  
cargo test --test complete_wrapper_flow_test

# Run comprehensive integration tests
cargo test --test comprehensive_integration_test

# Run with output
cargo test --test comprehensive_integration_test -- --nocapture
```

### Running All Integration Tests
```bash
cargo test --tests
```

## Test Coverage

These tests verify:
- ✅ Wrapper functions send messages to PGMQ queues correctly
- ✅ pg_notify notifications are sent to correct namespace channels
- ✅ Global notifications are sent to `pgmq_message_ready` channel
- ✅ Namespace extraction works for all queue patterns
- ✅ Batch operations maintain atomicity
- ✅ Message content integrity is preserved
- ✅ Error conditions are handled gracefully
- ✅ Concurrent operations work correctly
- ✅ Edge cases in queue naming are handled

## Queue Pattern Testing

The tests verify these namespace extraction patterns:
- `worker_rust_queue` → `rust`
- `worker_linear_workflow_queue` → `linear_workflow`
- `order_fulfillment_queue` → `order_fulfillment`
- `orchestration` → `orchestration`
- `orchestration_priority` → `orchestration`

## Expected Notification Channels

### Orchestration Listeners
Should subscribe to:
- `pgmq_message_ready.orchestration`
- `pgmq_message_ready` (global)

### Worker Listeners
Should subscribe to:
- `pgmq_message_ready.rust`
- `pgmq_message_ready.linear_workflow`
- `pgmq_message_ready.diamond_workflow`
- `pgmq_message_ready.tree_workflow`
- `pgmq_message_ready.mixed_dag_workflow`
- `pgmq_message_ready.order_fulfillment`

## Performance Expectations

The TAS-41 system provides:
- **<10ms latency** from message send to notification delivery
- **90% reduction** in database polling load
- **Atomic operations** ensuring message send + notification consistency

## Test Database Requirements

Tests require a PostgreSQL database with:
1. PGMQ extension installed
2. TAS-41 migration applied (wrapper functions created)
3. Test isolation through unique queue names per test run

## Cleanup

The test suite automatically:
- Creates unique queue names per test run to avoid conflicts
- Cleans up created queues after tests complete
- Provides proper test isolation
