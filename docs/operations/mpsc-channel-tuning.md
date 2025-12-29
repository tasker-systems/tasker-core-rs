# MPSC Channel Tuning - Operational Runbook

**Last Updated**: 2025-12-10
**Owner**: Platform Engineering
**Related**: TAS-51, TAS-75 | [ADR: Bounded MPSC Channels](../decisions/TAS-51-bounded-mpsc-channels.md) | [Circuit Breakers](../architecture/circuit-breakers.md) | [Backpressure Architecture](../architecture/backpressure-architecture.md)

## Overview

This runbook provides operational guidance for monitoring, diagnosing, and tuning MPSC channel buffer sizes in the tasker-core system. All channels are bounded with configurable capacities to prevent unbounded memory growth.

## Quick Reference

### Configuration Files

| File | Purpose | When to Edit |
|------|---------|--------------|
| `config/tasker/base/mpsc_channels.toml` | Base configuration | Default values |
| `config/tasker/environments/test/mpsc_channels.toml` | Test overrides | Test environment tuning |
| `config/tasker/environments/development/mpsc_channels.toml` | Dev overrides | Local development tuning |
| `config/tasker/environments/production/mpsc_channels.toml` | Prod overrides | Production capacity planning |

### Key Metrics

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `mpsc_channel_usage_percent` | Current fill percentage | > 80% |
| `mpsc_channel_capacity` | Configured buffer size | N/A (informational) |
| `mpsc_channel_full_events_total` | Overflow events counter | > 0 (indicates backpressure) |

### Default Buffer Sizes

| Channel | Base | Test | Development | Production |
|---------|------|------|-------------|------------|
| Orchestration command | 1000 | 100 | 1000 | 5000 |
| PGMQ notifications | 10000 | 10000 | 10000 | 50000 |
| Task readiness | 1000 | 100 | 500 | 5000 |
| Worker command | 1000 | 1000 | 1000 | 2000 |
| Event publisher | 5000 | 5000 | 5000 | 10000 |
| Ruby FFI | 1000 | 1000 | 500 | 2000 |

## Monitoring and Alerting

### Recommended Alerts

**Critical: Channel Saturation**
```promql
# Alert when any channel is >90% full for 5 minutes
mpsc_channel_usage_percent > 90
```
**Action**: Immediate capacity increase or identify bottleneck

**Warning: Channel High Usage**
```promql
# Alert when any channel is >80% full for 15 minutes
mpsc_channel_usage_percent > 80
```
**Action**: Plan capacity increase, investigate throughput

**Info: Channel Overflow Events**
```promql
# Alert on any overflow events
rate(mpsc_channel_full_events_total[5m]) > 0
```
**Action**: Review backpressure handling, consider capacity increase

### Grafana Queries

**Channel Usage by Component**
```promql
max by (channel, component) (mpsc_channel_usage_percent)
```

**Channel Capacity Configuration**
```promql
max by (channel, component) (mpsc_channel_capacity)
```

**Overflow Event Rate**
```promql
rate(mpsc_channel_full_events_total[5m])
```

### Log Patterns

**Saturation Warning (80% full)**
```
WARN mpsc_channel_saturation channel=orchestration_command usage_percent=82.5
```

**Overflow Event (channel full)**
```
ERROR mpsc_channel_full channel=event_publisher action=dropped
```

**Backpressure Applied**
```
ERROR Ruby FFI event channel full - backpressure applied
```

## Common Issues and Solutions

### Issue 1: High Channel Saturation

**Symptoms:**
- `mpsc_channel_usage_percent` consistently > 80%
- Slow message processing
- Increased latency

**Diagnosis:**
1. Check which channel is saturated:
   ```bash
   # Grep logs for saturation warnings
   grep "mpsc_channel_saturation" logs/tasker.log | tail -20
   ```

2. Check metrics for specific channel:
   ```promql
   mpsc_channel_usage_percent{channel="orchestration_command"}
   ```

**Solutions:**

**Short-term (Immediate Relief):**
```toml
# Edit appropriate environment file
# Example: config/tasker/environments/production/mpsc_channels.toml

[mpsc_channels.orchestration.command_processor]
command_buffer_size = 10000  # Increase from 5000
```

**Long-term:**
- Investigate message producer rate
- Optimize message consumer processing
- Consider horizontal scaling

### Issue 2: PGMQ Notification Bursts

**Symptoms:**
- Spike in `mpsc_channel_usage_percent{channel="pgmq_notifications"}`
- During bulk task creation (1000+ tasks)
- Temporary saturation followed by recovery

**Diagnosis:**
1. Correlate with bulk task operations:
   ```bash
   # Check for bulk task creation in logs
   grep "Bulk task creation" logs/tasker.log
   ```

2. Verify buffer size configuration:
   ```bash
   # Check current production configuration
   cat config/tasker/environments/production/mpsc_channels.toml | \
     grep -A 2 "event_listeners"
   ```

**Solutions:**

**If production buffer < 50000:**
```toml
# config/tasker/environments/production/mpsc_channels.toml
[mpsc_channels.orchestration.event_listeners]
pgmq_event_buffer_size = 50000  # Recommended for production
```

**If already at 50000 and still saturating:**
- Consider notification coalescing (future feature)
- Implement batch notification processing
- Scale orchestration services horizontally

### Issue 3: Ruby FFI Backpressure

**Symptoms:**
- Errors: "Ruby FFI event channel full - backpressure applied"
- Ruby handler slowness
- Increased Rust-side latency

**Diagnosis:**
1. Check Ruby handler processing time:
   ```ruby
   # Add timing to Ruby handlers
   time_start = Time.now
   result = handler.execute(step)
   duration = Time.now - time_start
   logger.warn("Slow handler: #{duration}s") if duration > 1.0
   ```

2. Check FFI channel saturation:
   ```promql
   mpsc_channel_usage_percent{channel="ruby_ffi"}
   ```

**Solutions:**

**If Ruby handlers are slow:**
- Optimize Ruby handler code
- Consider async Ruby processing
- Profile Ruby handler performance

**If FFI buffer too small:**
```toml
# config/tasker/environments/production/mpsc_channels.toml
[mpsc_channels.shared.ffi]
ruby_event_buffer_size = 2000  # Increase from 1000
```

**If Rust-side producing too fast:**
- Add rate limiting to Rust event production
- Batch events before FFI crossing

### Issue 4: Event Publisher Drops

**Symptoms:**
- Counter increasing: `mpsc_channel_full_events_total{channel="event_publisher"}`
- Log warnings: "Event channel full, dropping event"

**Diagnosis:**
1. Check drop rate:
   ```promql
   rate(mpsc_channel_full_events_total{channel="event_publisher"}[5m])
   ```

2. Identify event types being dropped:
   ```bash
   grep "dropping event" logs/tasker.log | awk '{print $NF}' | sort | uniq -c
   ```

**Solutions:**

**If drops are rare (< 1/min):**
- Acceptable for non-critical events
- Monitor but no action needed

**If drops are frequent (> 10/min):**
```toml
# config/tasker/environments/production/mpsc_channels.toml
[mpsc_channels.shared.event_publisher]
event_queue_buffer_size = 20000  # Increase from 10000
```

**If drops are continuous:**
- Investigate event storm cause
- Consider event sampling/filtering
- Review event subscriber performance

## Capacity Planning

### Sizing Formula

**General guideline:**
```
buffer_size = (peak_message_rate_per_sec * avg_processing_time_sec) * safety_factor
```

Where:
- `peak_message_rate_per_sec`: Expected peak throughput
- `avg_processing_time_sec`: Average consumer processing time
- `safety_factor`: 2-5x for bursts

**Example calculation:**
```
# Orchestration command channel
peak_rate = 500 messages/sec
processing_time = 0.01 sec (10ms)
safety_factor = 2x

buffer_size = (500 * 0.01) * 2 = 10 messages minimum
# Use 1000 for burst handling
```

### Environment-Specific Guidelines

**Test Environment:**
- Use **small buffers** (100-500)
- Exposes backpressure issues early
- Forces proper error handling

**Development Environment:**
- Use **moderate buffers** (500-1000)
- Balances local resource usage
- Mimics test environment behavior

**Production Environment:**
- Use **large buffers** (2000-50000)
- Handles real-world burst traffic
- Prioritizes availability over memory

### When to Increase Buffer Sizes

**Increase if:**
- ✅ Saturation > 80% for extended periods
- ✅ Overflow events occur regularly
- ✅ Latency increases during peak load
- ✅ Known traffic increase incoming

**Don't increase if:**
- ❌ Consumer is bottleneck (fix consumer instead)
- ❌ Saturation is brief and recovers quickly
- ❌ Would mask underlying performance issue

## Configuration Change Procedure

### 1. Identify Need

Review metrics and logs to determine which channel needs adjustment.

### 2. Calculate New Size

Use sizing formula or apply percentage increase:
```
new_size = current_size * (100 / (100 - target_usage_percent))

# Example: Currently 90% full, target 70%
new_size = 5000 * (100 / (100 - 70)) = 5000 * 3.33 = 16,650
# Round up: 20,000
```

### 3. Update Configuration

**Important**: Environment overrides MUST use full `[mpsc_channels.*]` prefix!

```toml
# ✅ CORRECT
[mpsc_channels.orchestration.command_processor]
command_buffer_size = 20000

# ❌ WRONG - creates conflicting top-level key
[orchestration.command_processor]
command_buffer_size = 20000
```

### 4. Deploy

**Local/Development:**
```bash
# Restart service - picks up new config automatically
cargo run --package tasker-orchestration --bin tasker-server --features web-api
```

**Production:**
```bash
# Standard deployment process
# Configuration is loaded at service startup
kubectl rollout restart deployment/tasker-orchestration
```

### 5. Monitor

Watch metrics for 1-2 hours post-change:
- Channel usage percentage should decrease
- Overflow events should stop
- Latency should improve

### 6. Document

Update this runbook with:
- Why change was made
- New values
- Observed impact

## Troubleshooting Checklist

```
□ Check metric: mpsc_channel_usage_percent for affected channel
□ Review logs for saturation warnings in last 24 hours
□ Verify configuration file has correct [mpsc_channels] prefix
□ Confirm environment variable TASKER_ENV matches intended environment
□ Check if issue correlates with specific operations (bulk tasks, etc.)
□ Verify consumer processing time hasn't increased
□ Check for resource constraints (CPU, memory)
□ Review recent code changes that might affect throughput
□ Consider if horizontal scaling is needed vs buffer increase
```

## Emergency Response

### Critical Saturation (>95%)

**Immediate Actions:**
1. Increase buffer size by 2-5x in production config
2. Deploy immediately via rolling restart
3. Page on-call if service degradation visible

**Example:**
```bash
# Edit config
vim config/tasker/environments/production/mpsc_channels.toml

# Deploy
kubectl rollout restart deployment/tasker-orchestration

# Monitor
watch -n 5 'curl -s localhost:9090/api/v1/query?query=mpsc_channel_usage_percent | jq'
```

### Service Unresponsive Due to Backpressure

**Symptoms:**
- All channels showing 100% usage
- No message processing
- Health checks failing

**Actions:**
1. Check for downstream bottleneck (database, queue service)
2. Scale out consumer services
3. Temporarily increase all buffer sizes
4. Check circuit breaker states (`/health/detailed` endpoint) - if circuit breakers are open, address underlying database/service issues first

> **Note**: MPSC channels and circuit breakers are complementary resilience mechanisms. Channel saturation indicates internal backpressure, while circuit breaker state indicates external service health. See [Circuit Breakers](../circuit-breakers.md) for operational guidance.

## Best Practices

1. **Monitor Proactively**: Don't wait for alerts - review metrics weekly
2. **Test Changes in Dev**: Validate buffer changes in development first
3. **Document Rationale**: Note why each production override exists
4. **Gradual Increases**: Prefer 2x increases over 10x jumps
5. **Review Quarterly**: Adjust defaults based on production patterns
6. **Alert on Changes**: Get notified of configuration file commits

## Related Documentation

**Architecture**:
- [Backpressure Architecture](../architecture/backpressure-architecture.md) - How MPSC channels fit into the broader resilience strategy
- [Circuit Breakers](../architecture/circuit-breakers.md) - Fault isolation working alongside bounded channels
- [ADR: Bounded MPSC Channels](../decisions/TAS-51-bounded-mpsc-channels.md) - Design decisions

**Development**:
- [Developer Guidelines](../development/mpsc-channel-guidelines.md) - Creating and using MPSC channels
- [TAS-51](https://linear.app/tasker-systems/issue/TAS-51) - Original implementation ticket

**Operations**:
- [Backpressure Monitoring](backpressure-monitoring.md) - Unified alerting and incident response

## Support

**Questions?** Ask in `#platform-engineering` Slack channel
**Issues?** File ticket with label `infrastructure/channels`
**Escalation?** Page on-call via PagerDuty
