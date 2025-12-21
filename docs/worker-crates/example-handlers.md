# Example Handlers - Cross-Language Reference

**Last Updated**: 2025-12-21
**Status**: Active
**Related Tickets**: TAS-92, TAS-98

<- Back to [Worker Crates Overview](README.md)

---

## Overview

This document provides side-by-side handler examples across Ruby, Python, and Rust. These examples demonstrate the aligned APIs that enable consistent patterns across all worker implementations.

---

## Simple Step Handler

### Ruby

```ruby
class ProcessOrderHandler < TaskerCore::StepHandler::Base
  def call(context)
    order_id = context.get_task_field('order_id')
    amount = context.get_task_field('amount')

    result = process_order(order_id, amount)

    success(
      result: {
        order_id: order_id,
        status: 'processed',
        total: result[:total]
      },
      metadata: { processed_at: Time.now.iso8601 }
    )
  rescue StandardError => e
    failure(
      message: e.message,
      error_type: 'UnexpectedError',
      retryable: true,
      metadata: { order_id: order_id }
    )
  end

  private

  def process_order(order_id, amount)
    # Business logic here
    { total: amount * 1.08 }
  end
end
```

### Python

```python
from tasker_core import BaseStepHandler, StepContext, StepHandlerResult


class ProcessOrderHandler(BaseStepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        try:
            order_id = context.get_task_field("order_id")
            amount = context.get_task_field("amount")

            result = self.process_order(order_id, amount)

            return self.success(
                result={
                    "order_id": order_id,
                    "status": "processed",
                    "total": result["total"],
                },
                metadata={"processed_at": datetime.now().isoformat()},
            )
        except Exception as e:
            return self.failure(
                message=str(e),
                error_type="handler_error",
                retryable=True,
                metadata={"order_id": order_id},
            )

    def process_order(self, order_id: str, amount: float) -> dict:
        # Business logic here
        return {"total": amount * 1.08}
```

### Rust

```rust
use tasker_shared::types::{TaskSequenceStep, StepExecutionResult};

pub struct ProcessOrderHandler;

impl ProcessOrderHandler {
    pub async fn call(&self, step_data: &TaskSequenceStep) -> StepExecutionResult {
        let order_id = step_data.task.context.get("order_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let amount = step_data.task.context.get("amount")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        match self.process_order(order_id, amount).await {
            Ok(result) => StepExecutionResult::success(
                serde_json::json!({
                    "order_id": order_id,
                    "status": "processed",
                    "total": result.total,
                }),
                Some(serde_json::json!({
                    "processed_at": chrono::Utc::now().to_rfc3339(),
                })),
            ),
            Err(e) => StepExecutionResult::failure(
                &e.to_string(),
                "handler_error",
                true, // retryable
            ),
        }
    }

    async fn process_order(&self, _order_id: &str, amount: f64) -> Result<OrderResult, Error> {
        Ok(OrderResult { total: amount * 1.08 })
    }
}
```

---

## Handler with Dependencies

### Ruby

```ruby
class ShipOrderHandler < TaskerCore::StepHandler::Base
  def call(context)
    # Get results from dependent steps
    validation = context.get_dependency_result('validate_order')
    payment = context.get_dependency_result('process_payment')

    unless validation && validation['valid']
      return failure(
        message: 'Order validation failed',
        error_type: 'ValidationError',
        retryable: false
      )
    end

    unless payment && payment['status'] == 'completed'
      return failure(
        message: 'Payment not completed',
        error_type: 'PermanentError',
        retryable: false
      )
    end

    # Access task context
    order_id = context.get_task_field('order_id')
    shipping_address = context.get_task_field('shipping_address')

    tracking_number = create_shipment(order_id, shipping_address)

    success(result: {
      order_id: order_id,
      tracking_number: tracking_number,
      shipped_at: Time.now.iso8601
    })
  end
end
```

### Python

```python
class ShipOrderHandler(BaseStepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        # Get results from dependent steps
        validation = context.get_dependency_result("validate_order")
        payment = context.get_dependency_result("process_payment")

        if not validation or not validation.get("valid"):
            return self.failure(
                message="Order validation failed",
                error_type="validation_error",
                retryable=False,
            )

        if not payment or payment.get("status") != "completed":
            return self.failure(
                message="Payment not completed",
                error_type="permanent_error",
                retryable=False,
            )

        # Access task context
        order_id = context.get_task_field("order_id")
        shipping_address = context.get_task_field("shipping_address")

        tracking_number = self.create_shipment(order_id, shipping_address)

        return self.success(
            result={
                "order_id": order_id,
                "tracking_number": tracking_number,
                "shipped_at": datetime.now().isoformat(),
            }
        )
```

---

## Decision Handler

### Ruby

```ruby
class ApprovalRoutingHandler < TaskerCore::StepHandler::Decision
  THRESHOLDS = {
    auto_approve: 1000,
    manager_only: 5000
  }.freeze

  def call(context)
    amount = context.get_task_field('amount').to_f
    department = context.get_task_field('department')

    if amount < THRESHOLDS[:auto_approve]
      decision_success(
        steps: ['auto_approve'],
        result_data: {
          route_type: 'automatic',
          amount: amount,
          reason: 'Below threshold'
        }
      )
    elsif amount < THRESHOLDS[:manager_only]
      decision_success(
        steps: ['manager_approval'],
        result_data: {
          route_type: 'manager',
          amount: amount,
          approver: find_manager(department)
        }
      )
    else
      decision_success(
        steps: ['manager_approval', 'finance_review'],
        result_data: {
          route_type: 'dual_approval',
          amount: amount,
          requires_cfo: amount > 50_000
        }
      )
    end
  end

  private

  def find_manager(department)
    # Lookup logic
    "manager@example.com"
  end
end
```

### Python

```python
class ApprovalRoutingHandler(DecisionHandler):
    THRESHOLDS = {
        "auto_approve": 1000,
        "manager_only": 5000,
    }

    def call(self, context: StepContext) -> StepHandlerResult:
        amount = float(context.get_task_field("amount") or 0)
        department = context.get_task_field("department")

        if amount < self.THRESHOLDS["auto_approve"]:
            return self.decision_success(
                steps=["auto_approve"],
                routing_context={
                    "route_type": "automatic",
                    "amount": amount,
                    "reason": "Below threshold",
                },
            )
        elif amount < self.THRESHOLDS["manager_only"]:
            return self.decision_success(
                steps=["manager_approval"],
                routing_context={
                    "route_type": "manager",
                    "amount": amount,
                    "approver": self.find_manager(department),
                },
            )
        else:
            return self.decision_success(
                steps=["manager_approval", "finance_review"],
                routing_context={
                    "route_type": "dual_approval",
                    "amount": amount,
                    "requires_cfo": amount > 50000,
                },
            )

    def find_manager(self, department: str) -> str:
        return "manager@example.com"
```

---

## Batch Processing Handler

### Ruby (Analyzer)

```ruby
class CsvAnalyzerHandler < TaskerCore::StepHandler::Batchable
  BATCH_SIZE = 100

  def call(context)
    file_path = context.get_task_field('csv_file_path')
    total_rows = count_csv_rows(file_path)

    if total_rows <= BATCH_SIZE
      # Small file - process inline, no batches needed
      outcome = TaskerCore::Types::BatchProcessingOutcome.no_batches

      success(
        result: {
          batch_processing_outcome: outcome.to_h,
          total_rows: total_rows,
          processing_mode: 'inline'
        }
      )
    else
      # Large file - create batch workers
      cursor_configs = calculate_batches(total_rows, BATCH_SIZE)
      outcome = TaskerCore::Types::BatchProcessingOutcome.create_batches(
        worker_template_name: 'process_csv_batch',
        worker_count: cursor_configs.size,
        cursor_configs: cursor_configs,
        total_items: total_rows
      )

      success(
        result: {
          batch_processing_outcome: outcome.to_h,
          total_rows: total_rows,
          batch_count: cursor_configs.size
        }
      )
    end
  end

  private

  def calculate_batches(total, batch_size)
    (0...total).step(batch_size).map.with_index do |start, idx|
      {
        'batch_id' => format('%03d', idx),
        'start_cursor' => start,
        'end_cursor' => [start + batch_size, total].min,
        'batch_size' => [batch_size, total - start].min
      }
    end
  end
end
```

### Ruby (Batch Worker)

```ruby
class CsvBatchWorkerHandler < TaskerCore::StepHandler::Batchable
  def call(context)
    batch_ctx = get_batch_context(context)

    # Handle placeholder batches
    no_op_result = handle_no_op_worker(batch_ctx)
    return no_op_result if no_op_result

    # Get file path from analyzer step
    analyzer_result = context.get_dependency_result('analyze_csv')
    file_path = analyzer_result&.dig('csv_file_path')

    # Process this batch
    records = read_csv_range(file_path, batch_ctx.start_cursor, batch_ctx.batch_size)
    processed = records.map { |row| transform_row(row) }

    batch_worker_complete(
      processed_count: processed.size,
      result_data: {
        batch_id: batch_ctx.batch_id,
        records_processed: processed.size,
        summary: calculate_summary(processed)
      }
    )
  end
end
```

### Python (Batch Worker)

```python
class CsvBatchWorkerHandler(BatchableHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        batch_ctx = self.get_batch_context(context)

        # Handle placeholder batches
        no_op_result = self.handle_no_op_worker(batch_ctx)
        if no_op_result:
            return no_op_result

        # Get file path from analyzer step
        analyzer_result = context.get_dependency_result("analyze_csv")
        file_path = analyzer_result.get("csv_file_path") if analyzer_result else None

        # Process this batch
        records = self.read_csv_range(
            file_path, batch_ctx.start_cursor, batch_ctx.batch_size
        )
        processed = [self.transform_row(row) for row in records]

        return self.batch_worker_complete(
            processed_count=len(processed),
            result_data={
                "batch_id": batch_ctx.batch_id,
                "records_processed": len(processed),
                "summary": self.calculate_summary(processed),
            },
        )
```

---

## API Handler

### Ruby

```ruby
class FetchUserHandler < TaskerCore::StepHandler::Api
  def call(context)
    user_id = context.get_task_field('user_id')

    # Automatic error classification (429 -> retryable, 404 -> permanent)
    response = connection.get("/users/#{user_id}")
    process_response(response)

    success(result: {
      user_id: user_id,
      email: response.body['email'],
      name: response.body['name']
    })
  end

  def base_url
    'https://api.example.com'
  end

  def configure_connection
    Faraday.new(base_url) do |conn|
      conn.request :json
      conn.response :json
      conn.options.timeout = 30
    end
  end
end
```

### Python

```python
class FetchUserHandler(ApiStepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        user_id = context.get_task_field("user_id")

        # Automatic error classification
        response = self.get(f"/users/{user_id}")

        return self.success(
            result={
                "user_id": user_id,
                "email": response["email"],
                "name": response["name"],
            }
        )

    @property
    def base_url(self) -> str:
        return "https://api.example.com"

    def configure_session(self, session):
        session.headers["Authorization"] = f"Bearer {self.get_token()}"
        session.timeout = 30
```

---

## Error Handling Patterns

### Ruby - Raising Exceptions

```ruby
class ValidateOrderHandler < TaskerCore::StepHandler::Base
  def call(context)
    order = context.task.context

    # Permanent error - will not retry
    if order['amount'].to_f <= 0
      raise TaskerCore::Errors::PermanentError.new(
        'Order amount must be positive',
        error_code: 'INVALID_AMOUNT',
        context: { amount: order['amount'] }
      )
    end

    # Retryable error - will retry with backoff
    if external_service_unavailable?
      raise TaskerCore::Errors::RetryableError.new(
        'External service temporarily unavailable',
        retry_after: 30,
        context: { service: 'payment_gateway' }
      )
    end

    success(result: { valid: true })
  end
end
```

### Python - Returning Failures

```python
class ValidateOrderHandler(BaseStepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        order = context.task.context

        # Permanent error - will not retry
        amount = float(order.get("amount", 0))
        if amount <= 0:
            return self.failure(
                message="Order amount must be positive",
                error_type="validation_error",
                error_code="INVALID_AMOUNT",
                retryable=False,
                metadata={"amount": amount},
            )

        # Retryable error - will retry with backoff
        if self.external_service_unavailable():
            return self.failure(
                message="External service temporarily unavailable",
                error_type="retryable_error",
                retryable=True,
                metadata={"service": "payment_gateway"},
            )

        return self.success(result={"valid": True})
```

---

## See Also

- [API Convergence Matrix](api-convergence-matrix.md) - Quick reference tables
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Ruby Worker](ruby.md) - Ruby implementation details
- [Python Worker](python.md) - Python implementation details
- [Rust Worker](rust.md) - Rust implementation details
