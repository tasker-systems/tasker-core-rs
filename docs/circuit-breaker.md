# Circuit Breaker Pattern in TaskerCore

## Overview

Circuit breakers are a fault tolerance pattern that prevents cascade failures by temporarily stopping calls to services that are experiencing problems. Like electrical circuit breakers that protect your home's wiring, software circuit breakers protect your system by "opening" when they detect repeated failures, allowing the failing service time to recover.

## How Circuit Breakers Work

### The Three States

1. **Closed (Normal Operation)**
   - All requests pass through to the service
   - Monitor for failures and track success/failure ratio
   - Transition to Open state when failure threshold is exceeded

2. **Open (Failing Fast)**
   - All requests are immediately failed without calling the service
   - This prevents further stress on the failing service
   - After timeout period, transition to Half-Open state

3. **Half-Open (Testing Recovery)**
   - Allow a limited number of requests through to test service health
   - If requests succeed → transition back to Closed
   - If requests fail → transition back to Open

### State Transition Flow

```
Closed → Open → Half-Open → Closed
  ↑                           ↓
  └─────── (on failure) ──────┘
```

## Where Circuit Breakers Are Applied in TaskerCore

### Message Queue Operations (`pgmq` component)

**Why**: Queue operations are high-frequency and failures can cause message buildup, memory issues, and processing delays. Failed queue operations often indicate infrastructure problems that need immediate attention.

**Configuration**:
- `failure_threshold: 3` - Fail fast to prevent message buildup
- `timeout_seconds: 15` - Queue recovery is usually quick (infrastructure-level issues)
- `success_threshold: 2` - Fewer successful operations needed to restore confidence

**Benefits**:
- Prevents message queue from backing up
- Stops workers from attempting to process messages they can't complete
- Allows queue infrastructure time to recover
- Prevents memory exhaustion from retry loops

## Circuit Breaker Benefits

### 1. **Cascade Failure Prevention**
Without circuit breakers, when one service fails, every service that depends on it also starts failing, creating a cascade effect throughout the system.

### 2. **Resource Conservation**
By failing fast, circuit breakers prevent:
- Thread pool exhaustion
- Memory leaks from accumulated retry attempts
- Network connection exhaustion
- CPU waste on doomed operations

### 3. **Graceful Degradation**
Circuit breakers allow systems to:
- Continue operating with reduced functionality
- Provide fallback responses
- Preserve system stability during partial outages

### 4. **Recovery Facilitation**
By reducing load on failing services, circuit breakers:
- Give services time to recover naturally
- Prevent "retry storms" that keep services down
- Allow gradual load restoration through half-open testing

## Configuration Guidelines

### Environment-Specific Tuning

**Development**:
- Higher failure thresholds (more forgiving)
- Longer timeouts (debugging-friendly)
- Logging enabled for learning

**Testing**:
- Lower failure thresholds (fail fast for quick feedback)
- Shorter timeouts (fast test execution)
- Detailed metrics collection

**Production**:
- Balanced thresholds (stability vs. availability)
- Monitored timeouts (based on SLA requirements)
- Comprehensive metrics and alerting

### Service-Specific Considerations

**Message Queues**:
- Aggressive failure detection (prevent message buildup)
- Quick recovery testing (infrastructure issues resolve quickly)
- Fast fail recovery (queues either work or they don't)

## Implementation Details

### Configuration Structure

```yaml
circuit_breakers:
  enabled: true
  global_settings:
    max_circuit_breakers: 50          # Total circuit breaker limit
    metrics_collection_interval_seconds: 30  # Monitoring frequency
    auto_create_enabled: true         # Create breakers automatically
    min_state_transition_interval_seconds: 1  # Prevent oscillation
  
  default_config:
    failure_threshold: 5              # Failures before opening
    timeout_seconds: 30               # Open state duration
    success_threshold: 2              # Successes to close

  component_configs:
    pgmq:
      failure_threshold: 3
      timeout_seconds: 15
      success_threshold: 2
```

### Usage Example

```rust
use crate::config::CircuitBreakerConfig;
use crate::resilience::CircuitBreakerManager;

// Create manager from YAML configuration
let config = load_yaml_config(); // Your config loading
let manager = CircuitBreakerManager::from_yaml_config(&config.circuit_breakers);

// Circuit breaker is automatically applied to protected clients
let protected_db = ProtectedPgmqClient::new_with_yaml_config(
    database_url,
    &config.circuit_breakers,
).await?;

// Operations are automatically protected
match protected_db.send_message("my_queue", &message).await {
    Ok(msg_id) => println!("Message sent: {}", msg_id),
    Err(CircuitBreakerError::CircuitOpen) => {
        // Handle circuit breaker open - service is down
        handle_service_unavailable();
    },
    Err(CircuitBreakerError::ServiceError(e)) => {
        // Handle actual service error
        handle_service_error(e);
    },
}
```

## Monitoring and Alerting

### Key Metrics

1. **Circuit State Changes**
   - Track when circuits open/close
   - Alert on frequent state changes (flapping)

2. **Failure Rates**
   - Monitor failure thresholds approaching
   - Track patterns in failure types

3. **Recovery Times**
   - Measure how long circuits stay open
   - Identify services with slow recovery

4. **System Impact**
   - Monitor overall system health during circuit events
   - Track user experience impact

### Recommended Alerts

- **Warning**: Queue circuit breaker open (processing degraded)
- **Info**: Circuit breaker state changes (operational awareness)

## Best Practices

### 1. **Set Appropriate Timeouts**
- Too short: Services don't get time to recover
- Too long: System remains degraded unnecessarily

### 2. **Monitor and Tune**
- Start with conservative settings
- Monitor actual failure patterns
- Adjust thresholds based on real-world behavior

### 3. **Plan for Circuit Open States**
- Always have fallback strategies
- Consider graceful degradation options
- Plan operational responses

### 4. **Test Circuit Breaker Behavior**
- Include circuit breaker scenarios in testing
- Verify fallback mechanisms work
- Test recovery procedures

### 5. **Coordinate with Operations**
- Circuit breaker events should trigger alerts
- Document operational procedures for each component
- Train operators on circuit breaker concepts

## Anti-Patterns to Avoid

### ❌ FFI Circuit Breakers
Circuit breaking memory access between language runtimes doesn't make sense:
- FFI is just shared memory access
- Memory allocation failures should hard fail immediately
- No network or service dependency to protect against

### ❌ Overly Aggressive Settings
- Setting failure thresholds too low causes unnecessary service interruptions
- Too short timeouts prevent proper service recovery

### ❌ No Fallback Strategy
- Circuit breakers without fallback plans just move the failure point
- Always plan what happens when circuits are open

### ❌ Ignoring Circuit Breaker State
- Applications should respond appropriately to circuit breaker errors
- Monitor and alert on circuit breaker events

## Conclusion

Circuit breakers are a crucial pattern for building resilient distributed systems. In TaskerCore, they protect against database and queue failures that could otherwise bring down the entire orchestration system. By failing fast when services are unhealthy, circuit breakers preserve system resources and allow failing services time to recover naturally.

Remember: Circuit breakers are not a silver bullet - they're one tool in a comprehensive resilience strategy that should include monitoring, alerting, graceful degradation, and operational procedures for handling service failures.