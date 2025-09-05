# Phase 2 Implementation Plan: Event System Architecture Improvements (Revised)

## Overview

Based on the Phase 2 analysis and the system's database-centric design philosophy, this implementation plan focuses on actual performance and operational improvements while respecting the existing architecture's strengths.

## Key Architectural Understanding

The Tasker system is intentionally designed with resilience to event ordering and delivery issues:

- **Database as Source of Truth**: All state decisions are made based on database state, not event order
- **Atomic Claiming**: `claim_ready_tasks()` and `claim_task_for_finalization()` prevent duplicate execution
- **State Machine Guards**: Invalid transitions are rejected regardless of event arrival order
- **Built-in Resilience**: System assumes PG_NOTIFY can lose messages, hence Hybrid mode exists

## Implementation Areas

### 1. Statistics Collection Optimization

**Problem**: Extensive statistics collection creates measurable latency overhead without proportional value.

**Current Impact**:
- Every event triggers 3-5 database writes for statistics
- Statistics tables grow unbounded (no archival)
- Synchronous writes block event processing
- 10-15% of processing time spent on metrics

**Solution**:

```rust
// tasker-shared/src/event_system/sampled_statistics.rs
pub struct SampledStatisticsCollector {
    sampling_rate: f64,           // Configurable: 0.01 to 1.0
    async_buffer: Arc<RingBuffer<StatsSample>>,
    background_writer: JoinHandle<()>,
}

impl SampledStatisticsCollector {
    pub fn record_if_sampled(&self, event: &Event, latency: Duration) {
        // Only sample based on configuration
        if !self.should_sample(event.priority()) {
            return;
        }
        
        // Non-blocking write to ring buffer
        let sample = StatsSample {
            event_type: event.type_name(),
            latency_ms: latency.as_millis() as f64,
            timestamp: Instant::now(),
        };
        
        // Fire and forget - no waiting
        let _ = self.async_buffer.try_push(sample);
    }
    
    fn should_sample(&self, priority: Priority) -> bool {
        match priority {
            Priority::Urgent => true,  // Always sample urgent
            Priority::High => fastrand::f64() < 0.5,
            Priority::Normal => fastrand::f64() < 0.1,
            Priority::Low => fastrand::f64() < 0.01,
        }
    }
}
```

**Implementation Steps**:
1. Audit all statistics collection points
2. Remove non-essential metrics
3. Implement sampling based on event priority
4. Move remaining stats to async background writer
5. Add statistics archival/rotation

**Acceptance Criteria**:
- [ ] < 1% processing time on statistics
- [ ] No blocking database writes for stats
- [ ] Configurable sampling rates
- [ ] Stats older than 30 days auto-archived

### 2. Container Deployment Strategy

**Problem**: Deployment mode strategy not well documented for production use.

**Solution**: Document and test multi-container deployment patterns.

```yaml
# docker-compose.production.yml
services:
  # High-throughput event-driven processors
  orchestrator-event-driven:
    image: tasker-orchestrator:latest
    environment:
      DEPLOYMENT_MODE: EventDrivenOnly
      INSTANCE_ID: "orch-event-${HOSTNAME}"
    deploy:
      replicas: 5  # Scale for throughput
    
  # Reliability-focused polling instances
  orchestrator-polling:
    image: tasker-orchestrator:latest
    environment:
      DEPLOYMENT_MODE: PollingOnly
      INSTANCE_ID: "orch-poll-${HOSTNAME}"
      POLL_INTERVAL_MS: 5000  # Less aggressive polling
    deploy:
      replicas: 2  # Just enough for reliability
  
  # Worker pools by namespace
  worker-fulfillment:
    image: tasker-worker:latest
    environment:
      DEPLOYMENT_MODE: EventDrivenOnly
      NAMESPACE: fulfillment
    deploy:
      replicas: 10
      
  # Fallback polling worker
  worker-polling:
    image: tasker-worker:latest
    environment:
      DEPLOYMENT_MODE: PollingOnly
      POLL_INTERVAL_MS: 10000
    deploy:
      replicas: 1  # Single instance for catch-all
```

**Documentation Needs**:
- Deployment patterns guide
- Scaling recommendations per mode
- Resource requirements per deployment mode
- Monitoring setup for mixed deployments

### 3. PG_NOTIFY Monitoring and Alerting

**Problem**: No visibility into PG_NOTIFY delivery success rate.

**Solution**: Add comprehensive monitoring without trying to "fix" the at-most-once delivery.

```rust
// tasker-shared/src/monitoring/pg_notify_monitor.rs
pub struct PgNotifyMonitor {
    sent_counter: Counter,
    discovered_by_polling: Counter,
    lag_histogram: Histogram,
}

impl PgNotifyMonitor {
    pub fn record_polling_discovery(&self, task_uuid: Uuid, ready_since: Instant) {
        let lag = ready_since.elapsed();
        
        if lag > Duration::from_secs(30) {
            warn!(
                "Task {} was ready for {:?} before polling discovered it - likely PG_NOTIFY miss",
                task_uuid, lag
            );
            self.discovered_by_polling.increment();
            self.lag_histogram.record(lag.as_secs_f64());
        }
    }
}
```

**Metrics to Track**:
- PG_NOTIFY events sent
- Work discovered by polling that should have been notified
- Lag between readiness and discovery
- Polling vs event-driven processing ratio

**Alerting Rules**:
```yaml
alerts:
  - name: HighPgNotifyMissRate
    expr: rate(tasks_discovered_by_polling[5m]) / rate(tasks_ready[5m]) > 0.05
    severity: warning
    annotations:
      summary: "{{ $value | humanizePercentage }} of tasks missed by PG_NOTIFY"
      action: "Consider decreasing polling interval or investigating database load"
```

### 4. Testing Multi-Mode Deployments

**Test Scenarios**:

```rust
#[tokio::test]
async fn test_mixed_mode_deployment_handles_notify_failure() {
    // Setup: 
    // - 2 EventDrivenOnly orchestrators
    // - 1 PollingOnly orchestrator
    // - Simulate PG_NOTIFY connection loss
    
    // Assert:
    // - All tasks eventually processed
    // - Polling instance picks up missed work
    // - No duplicate execution
}

#[tokio::test]
async fn test_polling_instance_performance_characteristics() {
    // Setup: PollingOnly with 5 second interval
    
    // Measure:
    // - Database query load
    // - Discovery latency distribution
    // - Resource consumption vs EventDriven
}
```

## Implementation Priority

| Priority | Task | Impact | Effort |
|----------|------|--------|--------|
| **P0** | Profile and reduce statistics overhead | High - Direct latency improvement | Medium |
| **P1** | Implement SampledStatisticsCollector | High - Sustained performance | Medium |
| **P1** | Document multi-container deployment | High - Production readiness | Low |
| **P2** | Add PG_NOTIFY monitoring | Medium - Operational visibility | Low |
| **P2** | Test mixed-mode deployments | Medium - Confidence | Medium |

## Success Metrics

1. **Performance**: 50%+ reduction in statistics-related latency
2. **Reliability**: Zero unprocessed tasks in mixed-mode deployment
3. **Visibility**: 100% of PG_NOTIFY misses detected and alerted
4. **Documentation**: Clear deployment patterns for production use

## What We're NOT Doing (and Why)

1. **Event Sequencing**: Database claiming already prevents issues
2. **Reliable Notifier**: Polling fallback already provides reliability
3. **Runtime Mode Switching**: Deployment modes are per-container, not dynamic
4. **Event Ordering**: State machines and guards handle any order

## Next Steps

1. **Immediate**: Profile current statistics overhead with production workload
2. **Week 1**: Implement SampledStatisticsCollector
3. **Week 2**: Document and test multi-container deployments
4. **Week 3**: Deploy monitoring and establish baselines