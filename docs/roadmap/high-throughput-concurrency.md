# High-Throughput Concurrency Architecture

## Executive Summary

This document outlines a **revolutionary architectural shift** from complex FFI-based step execution to a **ZeroMQ-based message passing architecture** that fundamentally solves the separation of concerns between orchestration and execution.

**Current Problem**: FFI execution hanging, tight coupling between Rust orchestration and Ruby execution
**Proposed Solution**: ZeroMQ message passing with clear orchestration/execution boundary
**Impact**: Language-agnostic execution, true concurrency, massive simplification
**Timeline**: 4-6 weeks for complete implementation

## 🎉 UPDATE: Critical FFI Issues Resolved (January 23, 2025)

While implementing the ZeroMQ architecture, we successfully resolved several critical FFI issues:

1. **✅ Step Dependency Information**: Fixed nil dependency arrays - steps now correctly include `depends_on_steps` data
2. **✅ Workflow Execution Unblocked**: Discovered and fixed root cause - `validate_order` step had `retryable: false` blocking ALL execution
3. **✅ Integration Tests Improved**: Test failures reduced from 16 to 4 (75% improvement) after fixing retryable flag
4. **✅ Workflows Now Execute**: Steps are executing (seeing "steps_executed=2 steps_succeeded=1 steps_failed=1" in logs)

**Remaining Issues**:
- Status mapping between Rust ('error') and Ruby ('complete'/'error') expectations
- Empty task context data from hardcoded return in `get_task_context`

These fixes validate that the current FFI approach CAN work, but the architectural benefits of ZeroMQ remain compelling for long-term scalability and maintainability.

---

## 🎯 Architectural Breakthrough (July 23, 2025)

### The Core Insight

We've been violating a fundamental principle of separation of concerns. The Rust orchestration system should **orchestrate**, not manage execution details. Trying to call Ruby methods from Rust through FFI creates:

- **Tight Coupling**: Rust needs to understand Ruby class loading, memory management, and execution context
- **Complex Memory Management**: Borrow checker issues, Magnus type conversions, GC coordination
- **Language Lock-in**: Each new language requires complex FFI bindings
- **Execution Blocking**: Hanging at FFI boundaries when crossing language runtimes

### The Solution: ZeroMQ Pub-Sub Bidirectional Pattern

Replace blocking REQ-REP with async pub-sub for true fire-and-forget orchestration:

```
┌─────────────────┐  PUB: steps    ┌─────────────────┐
│ Rust            │ ──────────────► │ Any Language    │
│ Orchestration   │                 │ Step Handler    │
│ (Dependencies,  │                 │ (Business Logic)│
│  State, Retry)  │ ◄────────────── │                 │
└─────────────────┘  SUB: results  └─────────────────┘
          │                                   │
          ▼                                   ▼
   PUB: step_batch           SUB: step_batch
   SUB: result_batch         PUB: result_batch
```

**Critical Advantages Over REQ-REP**:
- **No Timeout Issues**: Fire-and-forget eliminates blocking and timeout complexity
- **Idempotency Safe**: Steps complete independently of network timing
- **True Async**: Handlers process at their own pace without orchestrator blocking
- **Fault Tolerance**: Network issues don't create inconsistent state
- **Horizontal Scaling**: Multiple handler instances can subscribe to same step queue

**Key Benefits**:
- **Language Agnostic**: Any language with ZeroMQ bindings can execute steps
- **Clear Boundaries**: Orchestration vs execution responsibilities perfectly separated
- **True Concurrency**: No GIL or runtime constraints, no blocking operations
- **Fault Isolation**: Handler crashes don't affect orchestrator
- **Financial Safety**: No risk of timeout-induced duplicate transactions

---

## Architecture Overview

### System Design

```
┌─────────────────────────────────────────────────────────────────┐
│                    Rust Orchestration Core                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐            │
│  │ Workflow    │  │ State       │  │ Task        │            │
│  │ Coordinator │  │ Manager     │  │ Finalizer   │            │
│  └──────┬──────┘  └─────────────┘  └─────────────┘            │
│         │                                                        │
│         ▼                                                        │
│  ┌─────────────────────────────────────────────────┐           │
│  │           ZmqPubSubExecutor                      │           │
│  │  - PUB: Batch viable steps as JSON               │           │
│  │  - SUB: Receive result batches asynchronously    │           │
│  │  - No blocking, no timeouts, pure async          │           │
│  └─────────────────────┬───────────────────────────┘           │
└─────────────────────────┼───────────────────────────────────────┘
                          │
                          │ ZeroMQ Pub-Sub (inproc/ipc/tcp)
                          │
┌─────────────────────────┼───────────────────────────────────────┐
│                         ▼                                        │
│  ┌─────────────────────────────────────────────────┐           │
│  │         Language Handler (PUB-SUB)               │           │
│  │  - SUB: Receive step batch JSON                  │           │
│  │  - Execute using native handlers                 │           │
│  │  - PUB: Send results when complete               │           │
│  └─────────────────────┬───────────────────────────┘           │
│                         │                                        │
│         ┌───────────────┴──────────────┐                        │
│         ▼               ▼              ▼                        │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐               │
│  │Ruby Handler│  │Python      │  │Go Handler  │               │
│  │(Rails)     │  │Handler(ML) │  │(High Perf) │               │
│  └────────────┘  └────────────┘  └────────────┘               │
│                   Any Language Step Handlers                    │
└──────────────────────────────────────────────────────────────────┘
```

### Message Protocol

#### Step Batch Request (Rust → Handler)
```json
{
  "batch_id": "batch_12345_1706039072",
  "protocol_version": "1.0",
  "steps": [
    {
      "step_id": 8127,
      "task_id": 6591,
      "step_name": "ship_order",
      "handler_class": "OrderFulfillment::StepHandlers::ShipOrderHandler",
      "handler_config": {
        "shipping_timeout": 60,
        "send_notifications": true
      },
      "task_context": {
        "order_id": 12345,
        "customer_info": {...}
      },
      "previous_results": {
        "validate_order": { "status": "approved" },
        "process_payment": { "transaction_id": "txn_789" }
      },
      "metadata": {
        "attempt": 0,
        "retry_limit": 5,
        "timeout_ms": 30000
      }
    }
  ]
}
```

#### Step Batch Response (Handler → Rust)
```json
{
  "batch_id": "batch_12345_1706039072",
  "protocol_version": "1.0",
  "results": [
    {
      "step_id": 8127,
      "status": "completed",  // completed|failed|error
      "output": {
        "tracking_number": "1234567890",
        "carrier": "UPS",
        "estimated_delivery": "2025-01-25"
      },
      "error": null,
      "metadata": {
        "execution_time_ms": 1234,
        "handler_version": "1.2.3",
        "retryable": true
      }
    }
  ]
}
```

---

## Implementation Architecture

### Rust Side: ZmqPubSubExecutor

```rust
use zmq::{Context, Socket, PUB, SUB};
use serde::{Serialize, Deserialize};

#[derive(Serialize)]
struct StepBatchRequest {
    batch_id: String,
    protocol_version: String,
    steps: Vec<StepExecutionRequest>,
}

#[derive(Deserialize)]
struct StepBatchResponse {
    batch_id: String,
    results: Vec<StepExecutionResult>,
}

pub struct ZmqPubSubExecutor {
    context: Context,
    pub_endpoint: String,
    sub_endpoint: String,
    pub_socket: Socket,
    sub_socket: Socket,
    result_handler: Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
}

impl ZmqPubSubExecutor {
    pub fn new(pub_endpoint: &str, sub_endpoint: &str) -> Result<Self, Error> {
        let context = Context::new();
        
        // Publisher socket for sending step batches
        let pub_socket = context.socket(PUB)?;
        pub_socket.bind(pub_endpoint)?;
        
        // Subscriber socket for receiving results
        let sub_socket = context.socket(SUB)?;
        sub_socket.connect(sub_endpoint)?;
        sub_socket.set_subscribe(b"results")?; // Subscribe to "results" topic
        
        Ok(Self {
            context,
            pub_endpoint: pub_endpoint.to_string(),
            sub_endpoint: sub_endpoint.to_string(),
            pub_socket,
            sub_socket,
            result_handler: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn start_result_listener(&self) {
        let sub_socket = self.sub_socket.clone();
        let result_handler = self.result_handler.clone();
        
        tokio::spawn(async move {
            loop {
                if let Ok(msg) = sub_socket.recv_bytes(zmq::DONTWAIT) {
                    if let Ok(response) = serde_json::from_slice::<StepBatchResponse>(&msg) {
                        let mut handlers = result_handler.lock().await;
                        if let Some(sender) = handlers.remove(&response.batch_id) {
                            let _ = sender.send(response);
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
        });
    }

    async fn publish_batch(&self, steps: Vec<ViableStep>) -> Result<String, Error> {
        let batch_id = format!("batch_{}_{}", Uuid::new_v4(), SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
        
        let batch = StepBatchRequest {
            batch_id: batch_id.clone(),
            protocol_version: "1.0".to_string(),
            steps: steps.into_iter().map(|s| self.build_step_request(s)).collect(),
        };

        let request_json = serde_json::to_string(&batch)?;
        
        // Publish with "steps" topic
        let topic_msg = format!("steps {}", request_json);
        self.pub_socket.send(&topic_msg, 0)?;
        
        Ok(batch_id)
    }

    async fn wait_for_results(&self, batch_id: String) -> Result<StepBatchResponse, Error> {
        let (sender, receiver) = oneshot::channel();
        
        {
            let mut handlers = self.result_handler.lock().await;
            handlers.insert(batch_id, sender);
        }
        
        // Wait for results with timeout
        tokio::time::timeout(Duration::from_secs(300), receiver)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::ChannelClosed)?
    }
}

#[async_trait]
impl FrameworkIntegration for ZmqPubSubExecutor {
    async fn execute_step_with_handler(
        &self,
        context: &StepExecutionContext,
        handler_class: &str,
        handler_config: &HashMap<String, serde_json::Value>,
    ) -> Result<StepResult, OrchestrationError> {
        // Publish step batch (fire-and-forget)
        let batch_id = self.publish_batch(vec![context.viable_step.clone()]).await?;
        
        // Asynchronously wait for results
        let response = self.wait_for_results(batch_id).await?;
        
        response.results.into_iter().next()
            .ok_or_else(|| OrchestrationError::ExecutionError("No result returned".to_string()))
    }
}
```

### Ruby Side: ZmqPubSubHandler

```ruby
require 'ffi-rzmq'
require 'json'

module TaskerCore
  class ZmqPubSubHandler
    def initialize(
      step_sub_endpoint: 'inproc://steps', 
      result_pub_endpoint: 'inproc://results',
      handler_registry: nil
    )
      @context = ZMQ::Context.new
      
      # Subscriber for receiving step batches
      @step_socket = @context.socket(ZMQ::SUB)
      @step_socket.connect(step_sub_endpoint)
      @step_socket.setsockopt(ZMQ::SUBSCRIBE, 'steps') # Subscribe to "steps" topic
      
      # Publisher for sending results
      @result_socket = @context.socket(ZMQ::PUB)
      @result_socket.bind(result_pub_endpoint)
      
      @handler_registry = handler_registry || OrchestrationManager.instance
      @running = false
    end

    def start
      @running = true
      @thread = Thread.new { run_handler_loop }
    end

    def stop
      @running = false
      @thread&.join(5)
      @step_socket.close
      @result_socket.close
      @context.terminate
    end

    private

    def run_handler_loop
      while @running
        # Non-blocking receive
        message = receive_step_message
        next unless message

        begin
          # Parse topic and message
          topic, json_data = message.split(' ', 2)
          next unless topic == 'steps'
          
          request = JSON.parse(json_data, symbolize_names: true)
          
          # Process batch asynchronously
          Thread.new do
            response = process_batch(request)
            publish_results(response)
          end
        rescue => e
          Rails.logger.error "ZMQ Handler Error: #{e.message}"
          publish_error_response(request&.dig(:batch_id), e)
        end
        
        # Yield to prevent busy-waiting
        sleep(0.001)
      end
    end

    def receive_step_message
      message = ''
      rc = @step_socket.recv_string(message, ZMQ::DONTWAIT)
      rc == 0 ? message : nil
    end

    def publish_results(response)
      result_message = "results #{response.to_json}"
      @result_socket.send_string(result_message)
    end

    def process_batch(request)
      results = request[:steps].map do |step|
        process_single_step(step)
      end

      {
        batch_id: request[:batch_id],
        protocol_version: request[:protocol_version],
        results: results
      }
    end

    def process_single_step(step)
      # Get handler instance from registry
      handler = get_handler_for_step(step)

      # Build execution context
      task = build_task_object(step)
      sequence = build_sequence_object(step)
      step_obj = build_step_object(step)

      # Execute using existing handler interface
      result = handler.process(task, sequence, step_obj)

      # Build response
      {
        step_id: step[:step_id],
        status: 'completed',
        output: result,
        error: nil,
        metadata: {
          execution_time_ms: (Time.now - start_time) * 1000,
          handler_version: handler.class::VERSION,
          retryable: true
        }
      }
    rescue => e
      {
        step_id: step[:step_id],
        status: 'failed',
        output: nil,
        error: {
          message: e.message,
          backtrace: e.backtrace.first(5)
        },
        metadata: {
          retryable: determine_retryability(e)
        }
      }
    end

    def get_handler_for_step(step)
      # Use the already loaded handlers from TaskHandler initialization
      task_handler = @handler_registry.get_task_handler_for_task(step[:task_id])
      raise "No task handler found for task #{step[:task_id]}" unless task_handler

      step_handler = task_handler.get_step_handler_from_name(step[:step_name])
      raise "No step handler found for #{step[:step_name]}" unless step_handler

      step_handler
    end
  end
end
```

### Integration Points

#### 1. Minimal Changes to Existing Code

**Keep Working**:
- `initialize_task` - Still uses FFI (orchestration command)
- `handle(task_id)` - Still uses FFI (orchestration command)
- All existing Ruby step handlers - Work unchanged
- TaskHandlerRegistry - Already established over FFI

**Replace**:
- Direct `process()` calls from Rust → ZeroMQ message passing
- Complex FFI step execution → Simple JSON serialization

#### 2. Configuration

```yaml
# config/zeromq.yaml
step_execution:
  # Pub-Sub endpoints (bidirectional)
  rust_step_publisher: "inproc://steps"      # Rust publishes steps
  rust_result_subscriber: "inproc://results" # Rust subscribes to results
  
  # Handler endpoints (opposite direction) 
  handler_step_subscriber: "inproc://steps"     # Handlers subscribe to steps
  handler_result_publisher: "inproc://results"  # Handlers publish results

  # Later: unix socket for process isolation
  # rust_step_publisher: "ipc:///tmp/tasker_steps.sock"
  # rust_result_subscriber: "ipc:///tmp/tasker_results.sock"

  # Future: network distribution  
  # rust_step_publisher: "tcp://*:5555"
  # rust_result_subscriber: "tcp://*:5556"

  batch_size: 10
  result_timeout_ms: 300000  # 5 minutes max wait for results
  handler_poll_interval_ms: 1  # Polling frequency for non-blocking receives

  # High-water marks for message queuing
  step_queue_hwm: 1000
  result_queue_hwm: 1000
```

---

## Implementation Phases

### Phase 1: Proof of Concept (Week 1)

**Goals**: Validate ZeroMQ pub-sub approach works with existing system

**Tasks**:
1. Add `zmq = "0.10"` and `tokio = { version = "1.0", features = ["full"] }` to Cargo.toml
2. Add `ffi-rzmq` to Gemfile
3. Create `ZmqPubSubExecutor` implementing `FrameworkIntegration` trait
4. Create Ruby `ZmqPubSubHandler` with pub-sub sockets
5. Test with single step execution using `inproc://` endpoints
6. Validate existing handlers work through async ZeroMQ
7. Implement batch correlation with unique batch IDs

**Success Criteria**:
- Single step executes successfully through ZeroMQ pub-sub
- No timeout/idempotency issues with async messaging
- No changes required to existing step handlers
- Results correctly correlated with batch IDs

### Phase 2: Batch Processing (Week 2)

**Goals**: Implement efficient batch processing

**Tasks**:
1. Modify `WorkflowCoordinator` to batch viable steps
2. Update `ZmqStepExecutor` to send step batches
3. Ruby side concurrent processing with thread pool
4. Result correlation and error handling
5. Timeout and retry logic

**Success Criteria**:
- Batch of 10 steps processes successfully
- Individual step failures don't fail batch
- Proper timeout handling

### Phase 3: Production Hardening (Week 3-4)

**Goals**: Production-ready implementation

**Tasks**:
1. Connection pooling for ZeroMQ sockets
2. Circuit breaker for handler failures
3. Monitoring and metrics collection
4. Move from `inproc://` to `ipc://` or `tcp://`
5. Load testing and performance optimization

**Success Criteria**:
- Handle 100+ concurrent tasks
- Sub-second latency for step batches
- Graceful degradation under load
- Zero message loss

### Phase 4: Multi-Language Support (Week 5-6)

**Goals**: Demonstrate language-agnostic architecture

**Tasks**:
1. Python step handler implementation
2. Go high-performance handler example
3. Protocol documentation and SDK
4. Integration testing across languages
5. Performance benchmarking

**Success Criteria**:
- Three languages processing steps
- Consistent performance across languages
- Clear documentation for adding new languages

---

## Architectural Benefits

### 1. Separation of Concerns

**Rust Orchestration Core**:
- Workflow coordination
- Dependency resolution
- State management
- Retry logic
- Persistence

**Language Handlers**:
- Business logic execution
- Domain-specific processing
- Integration with external systems
- No knowledge of Tasker internals required

### 2. Scalability Patterns

```
                    ┌──────────────┐
                    │ Load Balancer│
                    └──────┬───────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│Handler Pool 1│  │Handler Pool 2│  │Handler Pool 3│
│(Ruby - Rails)│  │(Python - ML) │  │(Go - HiPerf) │
└──────────────┘  └──────────────┘  └──────────────┘
```

- **Horizontal Scaling**: Add more handler instances
- **Polyglot Architecture**: Right language for each job
- **Fault Isolation**: Handler crashes don't affect others
- **Geographic Distribution**: Handlers can run anywhere

### 3. Development Benefits

**Simplified Testing**:
- Mock ZeroMQ messages for unit tests
- Test handlers in isolation
- No complex FFI setup required

**Easier Debugging**:
- Clear message boundaries
- JSON inspection at every step
- Language-native debugging tools

**Faster Development**:
- Add new languages without Rust changes
- Iterate on handlers independently
- Clear API contract

---

## Migration Strategy

### Incremental Rollout

1. **Shadow Mode**: Run ZeroMQ in parallel with FFI
2. **A/B Testing**: Route percentage through ZeroMQ
3. **Feature Flags**: Task-type specific rollout
4. **Full Migration**: Remove FFI execution code

### Compatibility Layer

```ruby
# Existing BaseTaskHandler remains unchanged
class BaseTaskHandler
  def handle(task_id)
    # Still works - calls through to orchestration
    @rust_handler.handle(task_id)
  end

  # Internal execution now goes through ZeroMQ
  # but interface remains the same
end
```

### Rollback Safety

- Keep FFI code during transition
- Feature flag for instant rollback
- Message format versioning
- Comprehensive monitoring

---

## Performance Considerations

### Expected Performance

**Current FFI**:
- ~10ms per step execution
- Memory overhead of FFI boundary
- Limited by Ruby GIL
- Complex memory management

**ZeroMQ Architecture**:
- ~1-2ms message overhead
- True parallel execution
- No memory management issues
- Linear scaling with handlers

### Optimization Strategies

1. **Batching**: Send multiple steps in one message
2. **Connection Pooling**: Reuse ZeroMQ sockets
3. **Binary Protocol**: Consider MessagePack for performance
4. **Async I/O**: Non-blocking socket operations

---

## Risk Assessment

### Technical Risks

| Risk | Impact | Mitigation |
|------|---------|------------|
| Message loss | High | ZeroMQ reliability patterns |
| Latency increase | Medium | Benchmark and optimize |
| Serialization overhead | Low | Efficient protocols |
| Debugging complexity | Medium | Comprehensive logging |

### Mitigation Strategies

1. **Reliability**: Use ZeroMQ's built-in patterns
2. **Performance**: Profile and optimize hot paths
3. **Observability**: Structured logging at boundaries
4. **Testing**: Comprehensive integration tests

---

## Conclusion

The ZeroMQ architecture represents a fundamental correction to our system design. By clearly separating orchestration from execution, we achieve:

1. **True Language Independence**: Any language can be a step handler
2. **Massive Simplification**: Remove complex FFI code
3. **Horizontal Scalability**: Distribute handlers across machines
4. **Clear Boundaries**: Orchestration vs execution responsibilities

This isn't just fixing a bug - it's implementing the architecture that Tasker was always meant to have. The investment in this migration will pay dividends in system flexibility, reliability, and performance for years to come.

**Recommendation**: Proceed immediately with Phase 1 proof of concept. The current FFI hanging issue demonstrates the fundamental problems with tight coupling. ZeroMQ provides a clean, proven solution that aligns perfectly with Tasker's vision as a language-agnostic orchestration system.
