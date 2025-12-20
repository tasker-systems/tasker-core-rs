# Use Cases and Patterns

**Last Updated**: 2025-10-10
**Audience**: Developers, Architects, Product Managers
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [Quick Start](quick-start.md) | [Crate Architecture](crate-architecture.md)

← Back to [Documentation Hub](README.md)

---

## Overview

This guide provides practical examples of when and how to use Tasker Core for workflow orchestration. Each use case includes architectural patterns, example workflows, and implementation guidance based on real-world scenarios.

---

## Table of Contents

1. [E-Commerce Order Fulfillment](#e-commerce-order-fulfillment)
2. [Payment Processing Pipeline](#payment-processing-pipeline)
3. [Data Transformation ETL](#data-transformation-etl)
4. [Microservices Orchestration](#microservices-orchestration)
5. [Scheduled Job Coordination](#scheduled-job-coordination)
6. [Conditional Workflows and Decision Points](#conditional-workflows-and-decision-points)
7. [Anti-Patterns](#anti-patterns)

---

## E-Commerce Order Fulfillment

### Problem Statement

An e-commerce platform needs to coordinate multiple steps when processing orders:
- Validate order details and inventory
- Reserve inventory and process payment (parallel)
- Ship order after both payment and inventory confirmed
- Send confirmation emails
- Handle failures gracefully with retries

### Why Tasker Core?

- **Complex Dependencies**: Steps have clear dependency relationships
- **Parallel Execution**: Payment and inventory can happen simultaneously
- **Retry Logic**: External API calls need retry with backoff
- **Audit Trail**: Complete history needed for compliance
- **Idempotency**: Steps must handle duplicate executions safely

### Workflow Structure

```yaml
Task: order_fulfillment_#{order_id}
  Priority: Based on order value and customer tier
  Namespace: fulfillment

  Steps:
    1. validate_order
       - Handler: ValidateOrderHandler
       - Dependencies: None (root step)
       - Retry: retryable=true, max_attempts=3
       - Validates order data, checks fraud

    2. check_inventory
       - Handler: InventoryCheckHandler
       - Dependencies: validate_order (must complete)
       - Retry: retryable=true, max_attempts=5
       - Queries inventory system

    3. reserve_inventory
       - Handler: InventoryReservationHandler
       - Dependencies: check_inventory
       - Retry: retryable=true, max_attempts=3
       - Reserves stock with timeout

    4. process_payment
       - Handler: PaymentProcessingHandler
       - Dependencies: validate_order
       - Retry: retryable=true, max_attempts=3
       - Charges customer payment method
       - **Runs in parallel with reserve_inventory**

    5. ship_order
       - Handler: ShippingHandler
       - Dependencies: reserve_inventory AND process_payment
       - Retry: retryable=false, max_attempts=1
       - Creates shipping label, schedules pickup

    6. send_confirmation
       - Handler: EmailNotificationHandler
       - Dependencies: ship_order
       - Retry: retryable=true, max_attempts=10
       - Sends confirmation email to customer
```

### Implementation Pattern

**Task Template** (YAML configuration):
```yaml
namespace: fulfillment
name: order_fulfillment
version: "1.0"

steps:
  - name: validate_order
    handler: validate_order
    retry:
      retryable: true
      max_attempts: 3
      backoff: exponential
      backoff_base_ms: 1000

  - name: check_inventory
    handler: check_inventory
    dependencies:
      - validate_order
    retry:
      retryable: true
      max_attempts: 5
      backoff: exponential
      backoff_base_ms: 2000

  # ... remaining steps
```

**Step Handler** (Rust implementation):
```rust
pub struct ValidateOrderHandler;

#[async_trait]
impl StepHandler for ValidateOrderHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        // Extract order data from context
        let order_id: String = context.configuration.get("order_id")?;
        let customer_id: String = context.configuration.get("customer_id")?;

        // Validate order
        let order = validate_order_data(&order_id).await?;

        // Check fraud detection
        if check_fraud_risk(&customer_id, &order).await? {
            return Ok(StepResult::permanent_failure(
                "fraud_detected",
                json!({"reason": "High fraud risk"})
            ));
        }

        // Success - pass data to next steps
        Ok(StepResult::success(json!({
            "order_id": order_id,
            "validated_at": Utc::now(),
            "total_amount": order.total
        })))
    }
}
```

**Ruby Handler Alternative**:
```ruby
class ProcessPaymentHandler < TaskerCore::StepHandler
  def execute(context)
    order_id = context.configuration['order_id']
    amount = context.configuration['amount']

    # Process payment via payment gateway
    result = PaymentGateway.charge(
      amount: amount,
      idempotency_key: context.step_uuid
    )

    if result.success?
      { success: true, transaction_id: result.transaction_id }
    else
      # Retryable failure with backoff
      { success: false, retryable: true, error: result.error }
    end
  rescue PaymentGateway::NetworkError => e
    # Transient error, retry
    { success: false, retryable: true, error: e.message }
  rescue PaymentGateway::CardDeclined => e
    # Permanent failure, don't retry
    { success: false, retryable: false, error: e.message }
  end
end
```

### Key Patterns

**1. Parallel Execution**
- `reserve_inventory` and `process_payment` both depend only on earlier steps
- Tasker automatically executes them in parallel
- `ship_order` waits for both to complete

**2. Idempotent Handlers**
- Use `step_uuid` as idempotency key for external APIs
- Check if operation already completed before retrying
- Handle duplicate executions gracefully

**3. Smart Retry Logic**
- Network errors → retryable with exponential backoff
- Business logic failures → permanent, no retry
- Configure max_attempts based on criticality

**4. Data Flow**
- Early steps provide data to later steps via results
- Access parent results: `context.parent_results["validate_order"]`
- Build context as workflow progresses

### Observability

Monitor these metrics for order fulfillment:
```rust
// Track order processing stages
metrics::counter!("orders.validated").increment(1);
metrics::counter!("orders.payment_processed").increment(1);
metrics::counter!("orders.shipped").increment(1);

// Track failures by reason
metrics::counter!("orders.failed", "reason" => "fraud").increment(1);
metrics::counter!("orders.failed", "reason" => "inventory").increment(1);

// Track timing
metrics::histogram!("order.fulfillment_time_ms").record(elapsed_ms);
```

---

## Payment Processing Pipeline

### Problem Statement

A fintech platform needs to process payments with strict requirements:
- Multiple payment methods (card, bank transfer, wallet)
- Regulatory compliance and audit trails
- Automatic retry for transient failures
- Reconciliation with accounting system
- Webhook notifications to customers

### Why Tasker Core?

- **Compliance**: Complete audit trail with state transitions
- **Reliability**: Automatic retry with configurable limits
- **Observability**: Detailed metrics for financial operations
- **Idempotency**: Prevent duplicate charges
- **Flexibility**: Support multiple payment flows

### Workflow Structure

```yaml
Task: payment_processing_#{payment_id}
  Namespace: payments
  Priority: High (financial operations)

  Steps:
    1. validate_payment_request
       - Verify payment details
       - Check account status
       - Validate payment method

    2. check_fraud
       - Run fraud detection
       - Verify transaction limits
       - Check velocity rules

    3. authorize_payment
       - Contact payment gateway
       - Reserve funds (authorization hold)
       - Return authorization code

    4. capture_payment (depends on authorize_payment)
       - Capture authorized funds
       - Handle settlement
       - Generate receipt

    5. record_transaction (depends on capture_payment)
       - Write to accounting ledger
       - Update customer balance
       - Create audit records

    6. send_notification (depends on record_transaction)
       - Send webhook to merchant
       - Send receipt to customer
       - Update payment status
```

### Implementation Highlights

**Retry Strategy for Payment Gateway**:
```rust
impl StepHandler for AuthorizePaymentHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        let payment_id = context.configuration.get("payment_id")?;

        match gateway.authorize(payment_id, &context.step_uuid).await {
            Ok(auth) => {
                Ok(StepResult::success(json!({
                    "authorization_code": auth.code,
                    "authorized_at": Utc::now(),
                    "gateway_transaction_id": auth.transaction_id
                })))
            }

            Err(GatewayError::NetworkTimeout) => {
                // Transient - retry with backoff
                Ok(StepResult::retryable_failure(
                    "network_timeout",
                    json!({"retry_recommended": true})
                ))
            }

            Err(GatewayError::InsufficientFunds) => {
                // Permanent - don't retry
                Ok(StepResult::permanent_failure(
                    "insufficient_funds",
                    json!({"requires_manual_intervention": false})
                ))
            }

            Err(GatewayError::InvalidCard) => {
                // Permanent - don't retry
                Ok(StepResult::permanent_failure(
                    "invalid_card",
                    json!({"requires_manual_intervention": true})
                ))
            }
        }
    }
}
```

**Idempotency Pattern**:
```rust
async fn capture_payment(context: &StepContext) -> Result<StepResult> {
    let idempotency_key = context.step_uuid.to_string();

    // Check if we already captured this payment
    if let Some(existing) = check_existing_capture(&idempotency_key).await? {
        return Ok(StepResult::success(json!({
            "already_captured": true,
            "transaction_id": existing.transaction_id,
            "note": "Idempotent duplicate detected"
        })));
    }

    // Proceed with capture
    let result = gateway.capture(&idempotency_key).await?;

    // Store idempotency record
    store_capture_record(&idempotency_key, &result).await?;

    Ok(StepResult::success(json!(result)))
}
```

### Key Patterns

**1. Two-Phase Commit**
- Authorize (reserve) → Capture (settle)
- Allows cancellation between phases
- Common in payment processing

**2. Audit Trail**
- Every state transition recorded
- Regulatory compliance built-in
- Forensic investigation support

**3. Circuit Breaking**
- Protect against payment gateway failures
- Automatic backoff when gateway degraded
- Fallback to alternate gateways

---

## Data Transformation ETL

### Problem Statement

A data analytics platform needs to process data through multiple transformation stages:
- Extract data from multiple sources (APIs, databases, files)
- Transform data (clean, enrich, aggregate)
- Load to data warehouse
- Handle large datasets with partitioning
- Retry transient failures, skip corrupted data

### Why Tasker Core?

- **DAG Execution**: Complex transformation pipelines
- **Parallel Processing**: Independent partitions processed concurrently
- **Error Handling**: Skip corrupted records, retry transient failures
- **Observability**: Track data quality and processing metrics
- **Scheduling**: Integrate with cron/scheduler for periodic runs

### Workflow Structure

```yaml
Task: etl_customer_data_#{date}
  Namespace: data_pipeline

  Steps:
    1. extract_customer_profiles
       - Fetch from customer database
       - Partition by customer_id ranges
       - Creates multiple output partitions

    2. extract_transaction_history
       - Fetch from transactions database
       - Runs in parallel with extract_customer_profiles
       - Time-based partitioning

    3. enrich_customer_data (depends on extract_customer_profiles)
       - Add demographic data from external API
       - Process partitions in parallel
       - Each partition is independent

    4. join_transactions (depends on enrich_customer_data, extract_transaction_history)
       - Join enriched profiles with transactions
       - Aggregate metrics per customer
       - Parallel processing per partition

    5. load_to_warehouse (depends on join_transactions)
       - Bulk load to data warehouse
       - Verify data quality
       - Update metadata tables

    6. generate_summary_report (depends on load_to_warehouse)
       - Generate processing statistics
       - Send notification with summary
       - Archive source files
```

### Implementation Pattern

**Partition-Based Processing**:
```rust
pub struct ExtractCustomerProfilesHandler;

#[async_trait]
impl StepHandler for ExtractCustomerProfilesHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        let date: String = context.configuration.get("processing_date")?;

        // Determine partitions (e.g., by customer_id ranges)
        let partitions = calculate_partitions(1000000, 100000)?; // 10 partitions

        // Extract data for each partition
        let mut partition_files = Vec::new();
        for partition in partitions {
            let filename = extract_partition(&date, partition).await?;
            partition_files.push(filename);
        }

        // Return partition info for downstream steps
        Ok(StepResult::success(json!({
            "partitions": partition_files,
            "total_records": 1000000,
            "extracted_at": Utc::now()
        })))
    }
}
```

**Error Handling for Data Quality**:
```rust
async fn enrich_customer_data(context: &StepContext) -> Result<StepResult> {
    let partition_file: String = context.configuration.get("partition_file")?;

    let mut processed = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();

    for record in read_partition(&partition_file).await? {
        match enrich_record(record).await {
            Ok(enriched) => {
                write_enriched(enriched).await?;
                processed += 1;
            }
            Err(EnrichmentError::MalformedData(e)) => {
                // Skip corrupted record, continue processing
                skipped += 1;
                errors.push(format!("Skipped record: {}", e));
            }
            Err(EnrichmentError::ApiTimeout(e)) => {
                // Transient failure, retry entire step
                return Ok(StepResult::retryable_failure(
                    "api_timeout",
                    json!({"error": e.to_string()})
                ));
            }
        }
    }

    if skipped as f64 / processed as f64 > 0.1 {
        // Too many skipped records
        return Ok(StepResult::permanent_failure(
            "data_quality_issue",
            json!({
                "processed": processed,
                "skipped": skipped,
                "error_rate": skipped as f64 / processed as f64
            })
        ));
    }

    Ok(StepResult::success(json!({
        "processed": processed,
        "skipped": skipped,
        "errors": errors
    })))
}
```

### Key Patterns

**1. Partition-Based Parallelism**
- Split large datasets into partitions
- Process partitions independently
- Aggregate results in final step

**2. Graceful Degradation**
- Skip corrupted individual records
- Continue processing remaining data
- Report data quality issues

**3. Monitoring Data Quality**
- Track record counts through pipeline
- Alert on unexpected error rates
- Validate schema at boundaries

---

## Microservices Orchestration

### Problem Statement

Coordinate operations across multiple microservices:
- User registration flow (auth, profile, notifications, analytics)
- Distributed transactions with compensation
- Service dependency management
- Timeout and circuit breaking

### Why Tasker Core?

- **Service Coordination**: Orchestrate distributed operations
- **Saga Pattern**: Implement compensation for failures
- **Resilience**: Circuit breakers and timeouts
- **Observability**: End-to-end tracing with correlation IDs
- **Flexibility**: Handle heterogeneous service protocols

### Workflow Structure (User Registration Example)

```yaml
Task: user_registration_#{user_id}
  Namespace: user_onboarding

  Steps:
    1. create_auth_account
       - Call auth service to create account
       - Generate user credentials
       - Store authentication tokens

    2. create_user_profile (depends on create_auth_account)
       - Call profile service
       - Initialize user preferences
       - Set default settings

    3. setup_notification_preferences (depends on create_user_profile)
       - Call notification service
       - Configure email preferences
       - Set up push notifications

    4. track_user_signup (depends on create_user_profile)
       - Call analytics service
       - Record signup event
       - Runs in parallel with setup_notification_preferences

    5. send_welcome_email (depends on setup_notification_preferences)
       - Send welcome email
       - Provide onboarding links
       - Track email delivery

  Compensation Steps (on failure):
    - If create_user_profile fails → delete_auth_account
    - If any step fails after profile → deactivate_user
```

### Implementation Pattern (Saga with Compensation)

```rust
pub struct CreateUserProfileHandler;

#[async_trait]
impl StepHandler for CreateUserProfileHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        let user_id: String = context.configuration.get("user_id")?;
        let email: String = context.configuration.get("email")?;

        // Get auth details from previous step
        let auth_result = context.parent_results.get("create_auth_account")
            .ok_or("Missing auth result")?;
        let auth_token: String = auth_result.get("auth_token")?;

        // Call profile service
        match profile_service.create_profile(&user_id, &email, &auth_token).await {
            Ok(profile) => {
                Ok(StepResult::success(json!({
                    "profile_id": profile.id,
                    "created_at": profile.created_at,
                    "user_id": user_id
                })))
            }

            Err(ProfileServiceError::DuplicateEmail) => {
                // Permanent failure - email already exists
                // Trigger compensation
                Ok(StepResult::permanent_failure_with_compensation(
                    "duplicate_email",
                    json!({"email": email}),
                    vec!["delete_auth_account"] // Compensation steps
                ))
            }

            Err(ProfileServiceError::ServiceUnavailable) => {
                // Transient - retry
                Ok(StepResult::retryable_failure(
                    "service_unavailable",
                    json!({"retry_recommended": true})
                ))
            }
        }
    }
}
```

**Compensation Handler**:
```rust
pub struct DeleteAuthAccountHandler;

#[async_trait]
impl StepHandler for DeleteAuthAccountHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        let user_id: String = context.configuration.get("user_id")?;

        // Best-effort deletion
        match auth_service.delete_account(&user_id).await {
            Ok(_) => {
                Ok(StepResult::success(json!({
                    "compensated": true,
                    "user_id": user_id
                })))
            }
            Err(e) => {
                // Log error but don't fail - compensation is best-effort
                warn!("Compensation failed for user {}: {}", user_id, e);
                Ok(StepResult::success(json!({
                    "compensated": false,
                    "error": e.to_string(),
                    "requires_manual_cleanup": true
                })))
            }
        }
    }
}
```

### Key Patterns

**1. Correlation IDs**
- Pass correlation_id through all services
- Enable end-to-end tracing
- Simplify debugging distributed issues

**2. Compensation (Saga Pattern)**
- Define compensation steps for cleanup
- Execute on permanent failures
- Best-effort execution, log failures

**3. Service Circuit Breakers**
- Wrap service calls in circuit breakers
- Fail fast when services degraded
- Automatic recovery detection

---

## Scheduled Job Coordination

### Problem Statement

Run periodic jobs with dependencies:
- Daily report generation (depends on data refresh)
- Scheduled data backups (depends on maintenance window)
- Cleanup jobs (depends on retention policies)

### Why Tasker Core?

- **Dependency Management**: Jobs run in correct order
- **Failure Handling**: Automatic retry of failed jobs
- **Observability**: Track job execution history
- **Flexibility**: Dynamic scheduling based on results

### Implementation Pattern

```rust
// External scheduler (cron, Kubernetes CronJob, etc.) creates tasks
pub async fn schedule_daily_reports() -> Result<Uuid> {
    let client = OrchestrationClient::new("http://orchestration:8080").await?;

    let task_request = TaskRequest {
        template_name: "daily_reporting".to_string(),
        namespace: "scheduled_jobs".to_string(),
        configuration: json!({
            "report_date": Utc::now().format("%Y-%m-%d").to_string(),
            "report_types": ["sales", "inventory", "customer_activity"]
        }),
        priority: 5, // Normal priority
    };

    let response = client.create_task(task_request).await?;
    Ok(response.task_uuid)
}
```

---

## Conditional Workflows and Decision Points

### Problem Statement

Many workflows require **runtime decision-making** where the execution path depends on business logic evaluated at runtime:
- Approval routing based on request amount or risk level
- Tiered processing based on customer status
- Compliance checks varying by jurisdiction
- Dynamic resource allocation based on workload

### Why Use Decision Points?

**Traditional Approach (Static DAG)**:
```yaml
# Must define ALL possible paths upfront
Steps:
  - validate
  - route_A  # Always created
  - route_B  # Always created
  - route_C  # Always created
  - converge # Must handle all paths
```

**Decision Point Approach (Dynamic DAG)**:
```yaml
# Create ONLY the needed path at runtime
Steps:
  - validate
  - routing_decision  # Decides which path
  - route_A           # Created dynamically if needed
  - route_B           # Created dynamically if needed
  - route_C           # Created dynamically if needed
  - converge          # Uses intersection semantics
```

### Benefits

- **Efficiency**: Only execute steps actually needed
- **Clarity**: Workflow reflects actual business logic
- **Cost Savings**: Reduce API calls, processing time, and resource usage
- **Flexibility**: Add new paths without changing core logic

### Core Pattern

```yaml
Task: conditional_approval
  Steps:
    1. validate_request       # Regular step
    2. routing_decision       # Decision point (type: decision_point)
       → Evaluates business logic
       → Returns: CreateSteps(['manager_approval']) or NoBranches
    3. auto_approve          # Might be created
    4. manager_approval      # Might be created
    5. finance_review        # Might be created
    6. finalize_approval     # Convergence (type: deferred)
       → Waits for intersection of dependencies
```

### Example: Amount-Based Approval Routing

```ruby
class RoutingDecisionHandler < TaskerCore::StepHandler::Decision
  def call(context)
    amount = context.get_task_field('amount')

    # Business logic determines which steps to create
    steps = if amount < 1_000
      ['auto_approve']
    elsif amount < 5_000
      ['manager_approval']
    else
      ['manager_approval', 'finance_review']
    end

    # Return decision outcome
    decision_success(
      steps: steps,
      result_data: {
        route_type: determine_route_type(amount),
        amount: amount
      }
    )
  end
end
```

### Real-World Scenarios

**1. E-Commerce Returns Processing**
- Low-value returns: Auto-approve
- Medium-value: Manager review
- High-value or suspicious: Fraud investigation + manager review

**2. Financial Risk Assessment**
- Low-risk transactions: Standard processing
- Medium-risk: Additional verification
- High-risk: Manual review + compliance checks + legal review

**3. Healthcare Prior Authorization**
- Standard procedures: Auto-approve
- Specialized care: Medical director review
- Experimental treatments: Medical director + insurance review + compliance

**4. Customer Support Escalation**
- Simple issues: Tier 1 resolution
- Complex issues: Tier 2 specialist
- VIP customers: Immediate senior support + account manager notification

### Key Features

**Decision Point Steps** (TAS-53):
- Special step type that returns `DecisionPointOutcome`
- Can return `NoBranches` (no additional steps) or `CreateSteps` (list of step names)
- Fully atomic - either all steps created or none
- Supports nested decisions (configurable depth limit)

**Deferred Steps**:
- Use intersection semantics for dependencies
- Wait for: (declared dependencies) ∩ (actually created steps)
- Enable convergence regardless of path taken

**Type-Safe Implementation**:
- Ruby: `TaskerCore::StepHandler::Decision` base class
- Rust: `DecisionPointOutcome` enum with serde support
- Automatic validation and serialization

### Implementation

See the complete guide: **[Conditional Workflows and Decision Points](conditional-workflows.md)**

Covers:
- When to use conditional workflows
- YAML configuration
- Ruby and Rust implementation patterns
- Simple and complex examples
- Best practices and limitations

---

## Anti-Patterns

### ❌ Don't Use Tasker Core For:

**1. Simple Cron Jobs**
```yaml
# ❌ Anti-pattern: Single-step scheduled job
Task: send_daily_email
  Steps:
    - send_email  # No dependencies, no retry needed
```
**Why**: Overhead not justified. Use native cron or systemd timers.

**2. Real-Time Sub-Millisecond Operations**
```yaml
# ❌ Anti-pattern: High-frequency trading
Task: execute_trade_#{microseconds}
  Steps:
    - check_price   # Needs <1ms latency
    - execute_order
```
**Why**: Architectural overhead (~10-20ms) too high. Use in-memory queues or direct service calls.

**3. Pure Fan-Out**
```yaml
# ❌ Anti-pattern: Simple message broadcasting
Task: broadcast_notification
  Steps:
    - send_to_user_1
    - send_to_user_2
    - send_to_user_3
    # ... 1000s of independent steps
```
**Why**: Use message bus (Kafka, RabbitMQ) for pub/sub patterns. Tasker is for orchestration, not broadcasting.

**4. Stateless Single Operations**
```yaml
# ❌ Anti-pattern: Single API call with no retry
Task: fetch_user_data
  Steps:
    - call_api  # No dependencies, no state management needed
```
**Why**: Direct API call with client-side retry is simpler.

---

## Pattern Selection Guide

| Characteristic | Use Tasker Core? | Alternative |
|----------------|------------------|-------------|
| Multiple dependent steps | ✅ Yes | N/A |
| Parallel execution needed | ✅ Yes | Thread pools for simple cases |
| Retry logic required | ✅ Yes | Client-side retry libraries |
| Audit trail needed | ✅ Yes | Append-only logs |
| Single step, no retry | ❌ No | Direct function call |
| Sub-second latency required | ❌ No | In-memory queues |
| Pure broadcast/fan-out | ❌ No | Message bus (Kafka, etc.) |
| Simple scheduled job | ❌ No | Cron, systemd timers |

---

## Related Documentation

- **[Quick Start](quick-start.md)** - Get your first workflow running
- **[Conditional Workflows](conditional-workflows.md)** - Runtime decision-making and dynamic step creation
- **[Crate Architecture](crate-architecture.md)** - Understand the codebase
- **[Deployment Patterns](deployment-patterns.md)** - Deploy to production
- **[States and Lifecycles](states-and-lifecycles.md)** - State machine deep dive
- **[Events and Commands](events-and-commands.md)** - Event-driven patterns

---

← Back to [Documentation Hub](README.md)
