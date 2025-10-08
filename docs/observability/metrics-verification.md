# Metrics Verification Guide

**Purpose**: Verify that documented metrics queries work with actual system data
**Test Task**: `mathematical_sequence`
**Correlation ID**: `0199c3e0-ccdb-7581-87ab-3f67daeaa4a5`
**Task ID**: `0199c3e0-ccea-70f0-b6ae-3086b2f68280`
**Trace ID**: `d640f82572e231322edba0a5ef6e1405`

## How to Use This Guide

1. Open Grafana at http://localhost:3000
2. Navigate to **Explore** (compass icon in sidebar)
3. Select **Prometheus** as the data source
4. Copy each query below into the query editor
5. Record the actual output
6. Mark ✅ if query works, ❌ if it fails, or ⚠️ if partial data

---

## Orchestration Metrics Verification

### 1. Task Requests Counter

**Metric**: `tasker.tasks.requests.total`

**Query 1**: Basic counter
```promql
tasker_tasks_requests_total
```

**Expected**: At least 1 (for our test task)
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Filtered by correlation_id
```promql
tasker_tasks_requests_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected**: Exactly 1
**Actual Result**: _____________
**Labels Present**: [ ] correlation_id [ ] task_type [ ] namespace
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: Sum by namespace
```promql
sum by (namespace) (tasker_tasks_requests_total)
```

**Expected**: 1 for namespace "rust_e2e_linear"
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 2. Task Completions Counter

**Metric**: `tasker.tasks.completions.total`

**Query 1**: Basic counter
```promql
tasker_tasks_completions_total
```

**Expected**: At least 1 (if task completed successfully)
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Filtered by correlation_id
```promql
tasker_tasks_completions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected**: 1
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: Completion rate over 5 minutes
```promql
rate(tasker_tasks_completions_total[5m])
```

**Expected**: Some positive rate value
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 3. Steps Enqueued Counter

**Metric**: `tasker.steps.enqueued.total`

**Query 1**: Total steps enqueued for our task
```promql
tasker_steps_enqueued_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected**: Number of steps in mathematical_sequence workflow (likely 3-4 steps)
**Actual Result**: _____________
**Step Names Visible**: [ ] Yes [ ] No
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Sum by step name
```promql
sum by (step_name) (tasker_steps_enqueued_total)
```

**Expected**: Breakdown by step name
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 4. Step Results Processed Counter

**Metric**: `tasker.step_results.processed.total`

**Query 1**: Results processed for our task
```promql
tasker_step_results_processed_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected**: Same as steps enqueued
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Sum by result type
```promql
sum by (result_type) (tasker_step_results_processed_total)
```

**Expected**: Breakdown showing "success" results
**Actual Result**: _____________
**Result Types Visible**: [ ] success [ ] error [ ] timeout [ ] cancelled [ ] skipped
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 5. Task Initialization Duration Histogram

**Metric**: `tasker.task.initialization.duration`

**Query 1**: Check if histogram has data
```promql
tasker_task_initialization_duration_count
```

**Expected**: At least 1
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Average initialization time
```promql
rate(tasker_task_initialization_duration_sum[5m]) /
rate(tasker_task_initialization_duration_count[5m])
```

**Expected**: Some millisecond value (probably < 100ms)
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: P95 latency
```promql
histogram_quantile(0.95, rate(tasker_task_initialization_duration_bucket[5m]))
```

**Expected**: P95 millisecond value
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 4**: P99 latency
```promql
histogram_quantile(0.99, rate(tasker_task_initialization_duration_bucket[5m]))
```

**Expected**: P99 millisecond value
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 6. Task Finalization Duration Histogram

**Metric**: `tasker.task.finalization.duration`

**Query 1**: Check count
```promql
tasker_task_finalization_duration_count
```

**Expected**: At least 1
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Average finalization time
```promql
rate(tasker_task_finalization_duration_sum[5m]) /
rate(tasker_task_finalization_duration_count[5m])
```

**Expected**: Some millisecond value
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: P95 by final_state
```promql
histogram_quantile(0.95,
  sum by (final_state, le) (
    rate(tasker_task_finalization_duration_bucket[5m])
  )
)
```

**Expected**: P95 value grouped by final_state (likely "complete")
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 7. Step Result Processing Duration Histogram

**Metric**: `tasker.step_result.processing.duration`

**Query 1**: Check count
```promql
tasker_step_result_processing_duration_count
```

**Expected**: Number of steps processed
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Average processing time
```promql
rate(tasker_step_result_processing_duration_sum[5m]) /
rate(tasker_step_result_processing_duration_count[5m])
```

**Expected**: Millisecond value for result processing
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

## Worker Metrics Verification

### 8. Step Executions Counter

**Metric**: `tasker.steps.executions.total`

**Query 1**: Total executions
```promql
tasker_steps_executions_total
```

**Expected**: Number of steps in workflow
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: For specific task
```promql
tasker_steps_executions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected**: Number of steps executed
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: Execution rate
```promql
rate(tasker_steps_executions_total[5m])
```

**Expected**: Positive rate
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 9. Step Successes Counter

**Metric**: `tasker.steps.successes.total`

**Query 1**: Total successes
```promql
tasker_steps_successes_total
```

**Expected**: Should equal executions if all succeeded
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: By namespace
```promql
sum by (namespace) (tasker_steps_successes_total)
```

**Expected**: Successes grouped by namespace
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: Success rate
```promql
rate(tasker_steps_successes_total[5m]) / rate(tasker_steps_executions_total[5m])
```

**Expected**: ~1.0 (100%) if all steps succeeded
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 10. Step Failures Counter

**Metric**: `tasker.steps.failures.total`

**Query 1**: Total failures
```promql
tasker_steps_failures_total
```

**Expected**: 0 if all steps succeeded
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: By error type
```promql
sum by (error_type) (tasker_steps_failures_total)
```

**Expected**: No results if no failures, or breakdown by error type
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 11. Steps Claimed Counter

**Metric**: `tasker.steps.claimed.total`

**Query 1**: Total claims
```promql
tasker_steps_claimed_total
```

**Expected**: Number of steps claimed (should match executions)
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: By claim method
```promql
sum by (claim_method) (tasker_steps_claimed_total)
```

**Expected**: Breakdown by "event" or "poll"
**Actual Result**: _____________
**Claim Methods Visible**: [ ] event [ ] poll
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 12. Step Results Submitted Counter

**Metric**: `tasker.steps.results_submitted.total`

**Query 1**: Total submissions
```promql
tasker_steps_results_submitted_total
```

**Expected**: Number of steps that submitted results
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: For specific task
```promql
tasker_steps_results_submitted_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected**: Number of step results submitted
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 13. Step Execution Duration Histogram

**Metric**: `tasker.step.execution.duration`

**Query 1**: Check count
```promql
tasker_step_execution_duration_count
```

**Expected**: Number of step executions
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Average execution time
```promql
rate(tasker_step_execution_duration_sum[5m]) /
rate(tasker_step_execution_duration_count[5m])
```

**Expected**: Average milliseconds per step
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: P95 latency by namespace
```promql
histogram_quantile(0.95,
  sum by (namespace, le) (
    rate(tasker_step_execution_duration_bucket[5m])
  )
)
```

**Expected**: P95 latency for rust_e2e_linear namespace
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 4**: P99 latency
```promql
histogram_quantile(0.99, rate(tasker_step_execution_duration_bucket[5m]))
```

**Expected**: P99 latency value
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 14. Step Claim Duration Histogram

**Metric**: `tasker.step.claim.duration`

**Query 1**: Check count
```promql
tasker_step_claim_duration_count
```

**Expected**: Number of claims
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Average claim time
```promql
rate(tasker_step_claim_duration_sum[5m]) /
rate(tasker_step_claim_duration_count[5m])
```

**Expected**: Average milliseconds to claim
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: P95 by claim method
```promql
histogram_quantile(0.95,
  sum by (claim_method, le) (
    rate(tasker_step_claim_duration_bucket[5m])
  )
)
```

**Expected**: P95 claim latency by method
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

### 15. Step Result Submission Duration Histogram

**Metric**: `tasker.step_result.submission.duration`

**Query 1**: Check count
```promql
tasker_step_result_submission_duration_count
```

**Expected**: Number of result submissions
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 2**: Average submission time
```promql
rate(tasker_step_result_submission_duration_sum[5m]) /
rate(tasker_step_result_submission_duration_count[5m])
```

**Expected**: Average milliseconds to submit
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

**Query 3**: P95 submission latency
```promql
histogram_quantile(0.95, rate(tasker_step_result_submission_duration_bucket[5m]))
```

**Expected**: P95 submission latency
**Actual Result**: _____________
**Status**: [ ] ✅ [ ] ❌ [ ] ⚠️

---

## Complete Execution Flow Verification

**Purpose**: Verify the full task lifecycle is visible in metrics

**Query**: Complete flow for correlation_id
```promql
# Run each query and record the value

# 1. Task created
tasker_tasks_requests_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________

# 2. Steps enqueued
tasker_steps_enqueued_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________

# 3. Steps claimed
tasker_steps_claimed_total
Result: _____________

# 4. Steps executed
tasker_steps_executions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________

# 5. Steps succeeded
tasker_steps_successes_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________

# 6. Results submitted
tasker_steps_results_submitted_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________

# 7. Results processed
tasker_step_results_processed_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________

# 8. Task completed
tasker_tasks_completions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
Result: _____________
```

**Expected Pattern**: 1 → N → N → N → N → N → N → 1
**Actual Pattern**: _____________ → _____ → _____ → _____ → _____ → _____ → _____ → _____

**Analysis**:
- Do the numbers make sense for your workflow? [ ] Yes [ ] No
- Are any steps missing? _____________
- Do counts match expectations? [ ] Yes [ ] No

---

## Issues Found

Document any issues discovered during verification:

### Issue 1
**Query**: _____________
**Expected**: _____________
**Actual**: _____________
**Problem**: _____________
**Fix Required**: [ ] Yes [ ] No

### Issue 2
**Query**: _____________
**Expected**: _____________
**Actual**: _____________
**Problem**: _____________
**Fix Required**: [ ] Yes [ ] No

---

## Summary

**Total Queries Tested**: _____________
**Successful**: _____________ ✅
**Failed**: _____________ ❌
**Partial**: _____________ ⚠️

**Overall Status**: [ ] All Working [ ] Some Issues [ ] Major Problems

**Ready for Production**: [ ] Yes [ ] No [ ] Needs Work

---

**Verification Date**: _____________
**Verified By**: _____________
**Grafana Version**: _____________
**OpenTelemetry Version**: 0.26
