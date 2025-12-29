# Conditional Workflows and Decision Points

**Last Updated**: 2025-10-27
**Audience**: Developers, Architects
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [Use Cases & Patterns](use-cases-and-patterns.md) | [States and Lifecycles](states-and-lifecycles.md)

← Back to [Documentation Hub](README.md)

---

## Overview

Conditional workflows enable **runtime decision-making** that dynamically determines which workflow steps to execute based on business logic. Unlike static DAG workflows where all steps are predefined, conditional workflows use **decision point steps** to create steps on-demand based on runtime conditions.

**TAS-53 Dynamic Workflow Decision Points** introduces this capability through:
- **Decision Point Steps**: Special step type that evaluates business logic and returns step names to create
- **Deferred Steps**: Step type with dynamic dependency resolution using intersection semantics
- **Type-Safe Integration**: Ruby and Rust helpers ensuring clean serialization between languages

---

## Table of Contents

1. [When to Use Conditional Workflows](#when-to-use-conditional-workflows)
2. [Logical Pattern](#logical-pattern)
3. [Architecture and Implementation](#architecture-and-implementation)
4. [YAML Configuration](#yaml-configuration)
5. [Simple Example: Approval Routing](#simple-example-approval-routing)
6. [Complex Example: Multi-Tier Approval](#complex-example-multi-tier-approval)
7. [Ruby Implementation Guide](#ruby-implementation-guide)
8. [Rust Implementation Guide](#rust-implementation-guide)
9. [Best Practices](#best-practices)
10. [Limitations and Constraints](#limitations-and-constraints)

---

## When to Use Conditional Workflows

### ✅ Use Conditional Workflows When:

**1. Business Logic Determines Execution Path**
- Approval workflows with amount-based routing (small/medium/large)
- Risk-based processing (low/medium/high risk paths)
- Tiered customer service (bronze/silver/gold/platinum)
- Regulatory compliance with jurisdictional variations

**2. Step Requirements Are Unknown Until Runtime**
- Dynamic validation checks based on request type
- Multi-stage approvals where approval count depends on amount
- Conditional enrichment steps based on data completeness
- Parallel processing with variable worker count

**3. Workflow Complexity Varies By Input**
- Simple cases skip expensive steps
- Complex cases trigger additional validation
- Emergency processing bypasses normal checks
- VIP customers get expedited handling

### ❌ Don't Use Conditional Workflows When:

**1. Static DAG is Sufficient**
- All possible execution paths known at design time
- Complexity overhead not justified
- Simple if/else can be handled in handler code

**2. Purely Sequential Logic**
- No parallelism or branching needed
- Handler code can make decisions directly

**3. Real-Time Sub-Second Decisions**
- Decision overhead (~10-20ms) not acceptable
- In-memory processing required

---

## Logical Pattern

### Core Concepts

```
Task Initialization
       ↓
Regular Step(s)
       ↓
Decision Point Step ← Evaluates business logic
       ↓
   [Decision Made]
       ↓
   ┌───┴───┐
   ↓       ↓
Path A  Path B  ← Steps created dynamically
   ↓       ↓
   └───┬───┘
       ↓
Convergence Step ← Deferred dependencies resolve via intersection
       ↓
Task Complete
```

### Decision Point Pattern

1. **Evaluation Phase**: Decision point step executes handler
2. **Decision Output**: Handler returns list of step names to create
3. **Dynamic Creation**: Orchestration creates specified steps with proper dependencies
4. **Execution**: Created steps execute like normal steps
5. **Convergence**: Deferred steps wait for intersection of declared dependencies + created steps

### Intersection Semantics for Deferred Steps

**Declared Dependencies** (in template):
```yaml
- step_a
- step_b
- step_c
```

**Actually Created Steps** (by decision point):
```
Only step_a and step_c were created
```

**Effective Dependencies** (intersection):
```
step_a AND step_c  (step_b ignored since not created)
```

This enables convergence steps that work regardless of which path was taken.

---

## Architecture and Implementation

### Step Type: Decision Point

Decision point steps are regular steps with a special handler that returns a `DecisionPointOutcome`:

```rust
pub enum DecisionPointOutcome {
    NoBranches,               // No additional steps needed
    CreateSteps {             // Dynamically create these steps
        step_names: Vec<String>,
    },
}
```

**Key Characteristics**:
- Executes like a normal step
- Result includes `decision_point_outcome` field
- Orchestration detects outcome and creates steps
- Created steps depend on the decision point step
- Fully atomic - either all steps created or none

### Step Type: Deferred

Deferred steps use intersection semantics for dependency resolution:

```yaml
type: deferred  # Special step type
dependencies:
  - routing_decision  # Must wait for decision point
  - step_a           # Might be created
  - step_b           # Might be created
  - step_c           # Might be created
```

**Resolution Logic**:
1. Wait for decision point to complete
2. Check which declared dependencies actually exist
3. Wait only for intersection of declared + created
4. Execute when all existing dependencies complete

### Orchestration Flow

```
┌─────────────────────────────────────────┐
│ Step Result Processor                   │
│                                         │
│ 1. Check if result has                  │
│    decision_point_outcome field         │
│                                         │
│ 2. If CreateSteps:                      │
│    - Validate step names exist          │
│    - Create WorkflowStep records        │
│    - Set dependencies                   │
│    - Enqueue for execution              │
│                                         │
│ 3. If NoBranches:                       │
│    - Continue normally                  │
│                                         │
│ 4. Metrics and telemetry:               │
│    - Track steps_created count          │
│    - Log decision outcome               │
│    - Warn if depth limit approached     │
└─────────────────────────────────────────┘
```

### Configuration

Decision point behavior is configured per environment:

```toml
# config/tasker/base/orchestration.toml
[orchestration.decision_points]
enabled = true
max_depth = 3           # Prevent infinite recursion
warn_threshold = 2      # Warn when nearing limit
```

---

## YAML Configuration

### Task Template Structure

**Actual Implementation** (from `tests/fixtures/task_templates/ruby/conditional_approval_handler.yaml`):

```yaml
---
name: approval_routing
namespace_name: conditional_approval
version: 1.0.0
description: >
  Ruby implementation of conditional approval workflow demonstrating TAS-53 dynamic decision points.
  Routes approval requests through different paths based on amount thresholds.
task_handler:
  callable: tasker_worker_ruby::TaskHandler
  initialization: {}
steps:
  - name: validate_request
    type: standard
    dependencies: []
    handler:
      callable: ConditionalApproval::StepHandlers::ValidateRequestHandler
      initialization: {}

  - name: routing_decision
    type: decision  # DECISION POINT
    dependencies:
      - validate_request
    handler:
      callable: ConditionalApproval::StepHandlers::RoutingDecisionHandler
      initialization: {}

  - name: finalize_approval
    type: deferred  # DEFERRED - uses intersection semantics
    dependencies:
      - auto_approve       # ALL possible dependencies listed
      - manager_approval   # System computes intersection at runtime
      - finance_review
    handler:
      callable: ConditionalApproval::StepHandlers::FinalizeApprovalHandler
      initialization: {}

  # Possible dynamic branches (created by decision point)
  - name: auto_approve
    type: standard
    dependencies:
      - routing_decision
    handler:
      callable: ConditionalApproval::StepHandlers::AutoApproveHandler
      initialization: {}

  - name: manager_approval
    type: standard
    dependencies:
      - routing_decision
    handler:
      callable: ConditionalApproval::StepHandlers::ManagerApprovalHandler
      initialization: {}

  - name: finance_review
    type: standard
    dependencies:
      - routing_decision
    handler:
      callable: ConditionalApproval::StepHandlers::FinanceReviewHandler
      initialization: {}
```

**Key Points**:
- `type: decision` marks the decision point step
- `type: deferred` enables intersection semantics for convergence
- ALL possible dependencies listed in deferred step
- Orchestration computes: declared deps ∩ actually created steps

---

## Simple Example: Approval Routing

### Business Requirement

Route approval requests based on amount:
- **< $1,000**: Auto-approve (no human intervention)
- **$1,000 - $4,999**: Manager approval required
- **≥ $5,000**: Manager + Finance approval required

### Template Configuration

```yaml
namespace: approval_workflows
name: simple_routing
version: "1.0"

steps:
  - name: validate_request
    handler: validate_request

  - name: routing_decision
    handler: routing_decision
    type: decision_point
    dependencies:
      - validate_request

  - name: auto_approve
    handler: auto_approve
    dependencies:
      - routing_decision

  - name: manager_approval
    handler: manager_approval
    dependencies:
      - routing_decision

  - name: finance_review
    handler: finance_review
    dependencies:
      - routing_decision

  - name: finalize_approval
    handler: finalize_approval
    type: deferred
    dependencies:
      - routing_decision
      - auto_approve
      - manager_approval
      - finance_review
```

### Ruby Handler Implementation

**Actual Implementation** (from `workers/ruby/spec/handlers/examples/conditional_approval/step_handlers/routing_decision_handler.rb`):

```ruby
# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Routing Decision: DECISION POINT that routes approval based on amount
    #
    # Uses TaskerCore::StepHandler::Decision base class for clean, type-safe
    # decision outcome serialization consistent with Rust expectations.
    class RoutingDecisionHandler < TaskerCore::StepHandler::Decision
      SMALL_AMOUNT_THRESHOLD = 1_000
      LARGE_AMOUNT_THRESHOLD = 5_000

      def call(task, _sequence, _step)
        # Get amount from validated request
        amount = task.context['amount']
        raise 'Amount is required for routing decision' unless amount

        # Make routing decision based on amount
        route = determine_route(amount)

        # TAS-53: Use Decision base class helper for clean outcome serialization
        decision_success(
          steps: route[:steps],
          result_data: {
            route_type: route[:type],
            reasoning: route[:reasoning],
            amount: amount
          },
          metadata: {
            operation: 'routing_decision',
            route_thresholds: {
              small: SMALL_AMOUNT_THRESHOLD,
              large: LARGE_AMOUNT_THRESHOLD
            }
          }
        )
      end

      private

      def determine_route(amount)
        if amount < SMALL_AMOUNT_THRESHOLD
          {
            type: 'auto_approval',
            steps: ['auto_approve'],
            reasoning: "Amount $#{amount} below threshold - auto-approval"
          }
        elsif amount < LARGE_AMOUNT_THRESHOLD
          {
            type: 'manager_only',
            steps: ['manager_approval'],
            reasoning: "Amount $#{amount} requires manager approval"
          }
        else
          {
            type: 'dual_approval',
            steps: %w[manager_approval finance_review],
            reasoning: "Amount $#{amount} >= $#{LARGE_AMOUNT_THRESHOLD} - dual approval required"
          }
        end
      end
    end
  end
end
```

**Key Ruby Patterns**:
- **Inherit from** `TaskerCore::StepHandler::Decision` - Specialized base class for decision points
- **Use helper method** `decision_success(steps:, result_data:, metadata:)` - Clean API for decision outcomes
- Helper automatically creates `DecisionPointOutcome` and embeds it correctly
- No manual serialization needed - base class handles Rust compatibility
- For no-branch scenarios, use `decision_no_branches(result_data:, metadata:)`

### Execution Flow Examples

**Example 1: Small Amount ($500)**
```
1. validate_request → Complete
2. routing_decision → Complete (creates: auto_approve)
3. auto_approve     → Complete
4. finalize_approval → Complete
   (waits for: routing_decision ∩ {auto_approve} = auto_approve)

Total Steps Created: 4
Execution Time: ~500ms
```

**Example 2: Medium Amount ($2,500)**
```
1. validate_request  → Complete
2. routing_decision  → Complete (creates: manager_approval)
3. manager_approval  → Complete
4. finalize_approval → Complete
   (waits for: routing_decision ∩ {manager_approval} = manager_approval)

Total Steps Created: 4
Execution Time: ~2s (human approval delay)
```

**Example 3: Large Amount ($10,000)**
```
1. validate_request  → Complete
2. routing_decision  → Complete (creates: manager_approval, finance_review)
3. manager_approval  → Complete (parallel)
3. finance_review    → Complete (parallel)
4. finalize_approval → Complete
   (waits for: routing_decision ∩ {manager_approval, finance_review})

Total Steps Created: 5
Execution Time: ~3s (parallel approvals)
```

---

## Complex Example: Multi-Tier Approval

### Business Requirement

Implement sophisticated approval routing with:
- Risk assessment step
- Tiered approval requirements
- Emergency override path
- Compliance checks based on jurisdiction

### Template Configuration

```yaml
namespace: approval_workflows
name: multi_tier_approval
version: "1.0"

steps:
  # Phase 1: Initial validation and risk assessment
  - name: validate_request
    handler: validate_request

  - name: assess_risk
    handler: assess_risk
    dependencies:
      - validate_request

  # Phase 2: Primary routing decision
  - name: primary_routing
    handler: primary_routing
    type: decision_point
    dependencies:
      - assess_risk

  # Phase 3: Conditional approval paths
  - name: emergency_approval
    handler: emergency_approval
    dependencies:
      - primary_routing

  - name: standard_manager_approval
    handler: standard_manager_approval
    dependencies:
      - primary_routing

  - name: senior_manager_approval
    handler: senior_manager_approval
    dependencies:
      - primary_routing

  # Phase 4: Secondary routing for high-risk cases
  - name: compliance_routing
    handler: compliance_routing
    type: decision_point
    dependencies:
      - primary_routing
      - senior_manager_approval  # Only if created

  # Phase 5: Compliance paths
  - name: legal_review
    handler: legal_review
    dependencies:
      - compliance_routing

  - name: fraud_investigation
    handler: fraud_investigation
    dependencies:
      - compliance_routing

  - name: jurisdictional_check
    handler: jurisdictional_check
    dependencies:
      - compliance_routing

  # Phase 6: Convergence
  - name: finalize_approval
    handler: finalize_approval
    type: deferred
    dependencies:
      - primary_routing
      - emergency_approval
      - standard_manager_approval
      - senior_manager_approval
      - compliance_routing
      - legal_review
      - fraud_investigation
      - jurisdictional_check
```

### Ruby Handler: Primary Routing

```ruby
class PrimaryRoutingHandler < TaskerCore::StepHandler::Decision
  def call(task, sequence, _step)
    amount = task.context['amount']
    risk_score = sequence.get_results('assess_risk')['risk_score']
    is_emergency = task.context['emergency'] == true

    steps_to_create = if is_emergency && amount < 10_000
      # Emergency override path
      ['emergency_approval']
    elsif risk_score < 30 && amount < 5_000
      # Low risk, standard approval
      ['standard_manager_approval']
    else
      # High risk or large amount - senior approval + compliance routing
      ['senior_manager_approval', 'compliance_routing']
    end

    decision_success(
      steps: steps_to_create,
      result_data: {
        route_type: determine_route_type(is_emergency, risk_score, amount),
        risk_score: risk_score,
        amount: amount,
        emergency: is_emergency
      }
    )
  end
end
```

### Ruby Handler: Compliance Routing (Nested Decision)

```ruby
class ComplianceRoutingHandler < TaskerCore::StepHandler::Decision
  def call(task, sequence, _step)
    amount = task.context['amount']
    risk_score = sequence.get_results('assess_risk')['risk_score']
    jurisdiction = task.context['jurisdiction']

    steps_to_create = []

    # Large amounts always need legal review
    steps_to_create << 'legal_review' if amount >= 50_000

    # High risk triggers fraud investigation
    steps_to_create << 'fraud_investigation' if risk_score >= 70

    # Certain jurisdictions need special checks
    steps_to_create << 'jurisdictional_check' if high_regulation_jurisdiction?(jurisdiction)

    if steps_to_create.empty?
      # No additional compliance steps needed
      decision_no_branches(
        result_data: { reason: 'no_compliance_requirements' }
      )
    else
      decision_success(
        steps: steps_to_create,
        result_data: {
          compliance_level: 'enhanced',
          checks_required: steps_to_create
        }
      )
    end
  end

  private

  def high_regulation_jurisdiction?(jurisdiction)
    %w[EU UK APAC].include?(jurisdiction)
  end
end
```

### Execution Scenarios

**Scenario 1: Emergency Low-Risk Request ($5,000)**
```
Path: validate → assess_risk → primary_routing → emergency_approval → finalize
Steps Created: 5
Decision Points: 1 (primary_routing creates emergency_approval)
Complexity: Low
```

**Scenario 2: Standard Medium-Risk Request ($3,000, Risk 25)**
```
Path: validate → assess_risk → primary_routing → standard_manager_approval → finalize
Steps Created: 5
Decision Points: 1 (primary_routing creates standard_manager_approval)
Complexity: Low
```

**Scenario 3: High-Risk Large Amount ($75,000, Risk 80, EU)**
```
Path: validate → assess_risk → primary_routing → senior_manager_approval + compliance_routing
      → legal_review + fraud_investigation + jurisdictional_check → finalize
Steps Created: 9
Decision Points: 2 (primary_routing → compliance_routing)
Complexity: High (nested decisions)
```

---

## Ruby Implementation Guide

### Using the Decision Base Class

The `TaskerCore::StepHandler::Decision` base class provides type-safe helpers:

```ruby
class MyDecisionHandler < TaskerCore::StepHandler::Decision
  def call(context)
    # Your business logic here
    amount = context.get_task_field('amount')

    if amount < 1000
      # Create single step
      decision_success(
        steps: 'auto_approve',  # Can pass string or array
        result_data: { route: 'auto' }
      )
    elsif amount < 5000
      # Create multiple steps
      decision_success(
        steps: ['manager_approval', 'risk_check'],
        result_data: { route: 'standard' }
      )
    else
      # No additional steps needed
      decision_no_branches(
        result_data: { route: 'none', reason: 'manual_review_required' }
      )
    end
  end
end
```

### Helper Methods

**`decision_success(steps:, result_data: {}, metadata: {})`**
- Creates steps dynamically
- `steps`: String or Array of step names
- `result_data`: Additional data to store in step results
- `metadata`: Observability metadata

**`decision_no_branches(result_data: {}, metadata: {})`**
- No additional steps created
- Workflow proceeds to next static step

**`decision_with_custom_outcome(outcome:, result_data: {}, metadata: {})`**
- Advanced: Full control over outcome structure
- Most handlers should use `decision_success` or `decision_no_branches`

**`validate_decision_outcome!(outcome)`**
- Validates custom outcome structure
- Raises error if invalid

### Type Definitions

```ruby
# workers/ruby/lib/tasker_core/types/decision_point_outcome.rb

module TaskerCore
  module Types
    module DecisionPointOutcome
      # Factory methods
      def self.no_branches
        NoBranches.new
      end

      def self.create_steps(step_names)
        CreateSteps.new(step_names: step_names)
      end

      # Serialization format (matches Rust)
      class NoBranches
        def to_h
          { type: 'no_branches' }
        end
      end

      class CreateSteps
        def to_h
          { type: 'create_steps', step_names: step_names }
        end
      end
    end
  end
end
```

---

## Rust Implementation Guide

### Decision Handler Implementation

**Actual Implementation** (from `workers/rust/src/step_handlers/conditional_approval_rust.rs`):

```rust
use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::{DecisionPointOutcome, StepExecutionResult};
use tasker_shared::types::TaskSequenceStep;

const SMALL_AMOUNT_THRESHOLD: i64 = 1000;
const LARGE_AMOUNT_THRESHOLD: i64 = 5000;

pub struct RoutingDecisionHandler {
    #[allow(dead_code)]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for RoutingDecisionHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract amount from task context
        let amount: i64 = step_data.get_context_field("amount")?;

        // Business logic: determine routing
        let (route_type, steps, reasoning) = if amount < SMALL_AMOUNT_THRESHOLD {
            (
                "auto_approval",
                vec!["auto_approve"],
                format!("Amount ${} under threshold", amount)
            )
        } else if amount < LARGE_AMOUNT_THRESHOLD {
            (
                "manager_only",
                vec!["manager_approval"],
                format!("Amount ${} requires manager approval", amount)
            )
        } else {
            (
                "dual_approval",
                vec!["manager_approval", "finance_review"],
                format!("Amount ${} requires dual approval", amount)
            )
        };

        // Create decision point outcome
        let outcome = DecisionPointOutcome::create_steps(
            steps.iter().map(|s| s.to_string()).collect()
        );

        // Build result with embedded outcome
        let result_data = json!({
            "route_type": route_type,
            "reasoning": reasoning,
            "amount": amount,
            "decision_point_outcome": outcome.to_value()  // Embedded outcome
        });

        let metadata = HashMap::from([
            ("route_type".to_string(), json!(route_type)),
            ("steps_to_create".to_string(), json!(steps)),
        ]);

        Ok(success_result(
            step_uuid,
            result_data,
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "routing_decision"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
```

### DecisionPointOutcome Type

**Type Definition** (from `tasker-shared/src/messaging/execution_types.rs`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionPointOutcome {
    NoBranches,
    CreateSteps {
        step_names: Vec<String>,
    },
}

impl DecisionPointOutcome {
    /// Create outcome that creates specific steps
    pub fn create_steps(step_names: Vec<String>) -> Self {
        Self::CreateSteps { step_names }
    }

    /// Create outcome with no additional steps
    pub fn no_branches() -> Self {
        Self::NoBranches
    }

    /// Convert to JSON value for embedding in StepExecutionResult
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("DecisionPointOutcome serialization should not fail")
    }

    /// Extract decision outcome from step execution result
    pub fn from_step_result(result: &serde_json::Value) -> Option<Self> {
        result
            .as_object()?
            .get("decision_point_outcome")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}
```

**Key Rust Patterns**:
- `DecisionPointOutcome::create_steps(vec![...])` - Type-safe factory
- `outcome.to_value()` - Serializes to JSON matching Ruby format
- Embedded in result JSON as `decision_point_outcome` field
- Serde handles serialization: `{ "type": "create_steps", "step_names": [...] }`

---

## Best Practices

### 1. Keep Decision Logic Deterministic

```ruby
# ✅ Good: Deterministic decision based on input
def call(context)
  amount = context.get_task_field('amount')

  steps = if amount < 1000
    ['auto_approve']
  else
    ['manager_approval']
  end

  decision_success(steps: steps)
end

# ❌ Bad: Non-deterministic (time-based, random)
def call(context)
  # Decision changes based on when it runs
  steps = if Time.now.hour < 9
    ['emergency_approval']
  else
    ['standard_approval']
  end

  decision_success(steps: steps)
end
```

### 2. Validate Step Names

Ensure all step names in decision outcomes exist in template:

```ruby
VALID_STEPS = %w[auto_approve manager_approval finance_review].freeze

def call(context)
  steps_to_create = determine_steps(context)

  # Validate step names
  invalid = steps_to_create - VALID_STEPS
  unless invalid.empty?
    raise "Invalid step names: #{invalid.join(', ')}"
  end

  decision_success(steps: steps_to_create)
end
```

### 3. Use Deferred Type for Convergence

Any step that might depend on dynamically created steps should be `type: deferred`:

```yaml
# ✅ Correct
- name: finalize
  type: deferred  # Uses intersection semantics
  dependencies:
    - routing_decision
    - auto_approve
    - manager_approval

# ❌ Wrong - will fail if dependencies don't all exist
- name: finalize
  dependencies:
    - routing_decision
    - auto_approve
    - manager_approval
```

### 4. Limit Decision Depth

Prevent infinite recursion:

```toml
[orchestration.decision_points]
max_depth = 3  # Maximum nesting level
warn_threshold = 2  # Warn when approaching limit
```

```ruby
# ✅ Good: Linear decision chain (depth 1-2)
validate → routing_decision → compliance_check → finalize

# ⚠️ Be Careful: Deep nesting (depth 3)
validate → routing_1 → routing_2 → routing_3 → finalize

# ❌ Bad: Circular or unbounded nesting
routing_decision creates steps that create more routing decisions...
```

### 5. Handle No-Branch Cases

Explicitly return `no_branches` when no steps needed:

```ruby
def call(context)
  amount = context.get_task_field('amount')

  if context.get_task_field('skip_approval')
    # No additional steps needed
    decision_no_branches(
      result_data: { reason: 'approval_skipped' }
    )
  else
    decision_success(steps: determine_steps(amount))
  end
end
```

### 6. Meaningful Result Data

Include context for debugging and audit trails:

```ruby
decision_success(
  steps: ['manager_approval', 'finance_review'],
  result_data: {
    route_type: 'dual_approval',
    reasoning: "Amount $#{amount} >= $5,000 threshold",
    amount: amount,
    thresholds_applied: {
      small: 1_000,
      large: 5_000
    }
  },
  metadata: {
    decision_time_ms: elapsed_ms,
    steps_created_count: 2
  }
)
```

---

## Limitations and Constraints

### Technical Limits

**1. Maximum Decision Depth**
- Default: 3 levels of nested decision points
- Configurable via `orchestration.decision_points.max_depth`
- Prevents infinite recursion

**2. Step Names Must Exist in Template**
- All step names in `CreateSteps` must be defined in template
- Orchestration validates before creating steps
- Invalid names cause permanent failure

**3. Decision Logic is Non-Retryable by Default**
- Decision steps should be deterministic
- Retry disabled by default (`max_attempts: 1`)
- External API calls should be in separate steps

**4. Created Steps Cannot Modify Template**
- Decision points create instances of template steps
- Cannot dynamically define new step types
- All possible steps must be in template

### Performance Considerations

**1. Decision Overhead**
- Each decision point adds ~10-20ms overhead
- Includes: handler execution + step creation + dependency resolution
- Factor into SLA planning

**2. Database Impact**
- Each created step = 1 WorkflowStep record + edges
- Large branch counts increase database operations
- Monitor `workflow_steps` table growth

**3. Observability**
- Decision outcomes logged with telemetry
- Metrics track: `decision_points.steps_created`, `decision_points.depth`
- Use structured logging for audit trails

### Semantic Constraints

**1. Deferred Dependencies Must Include Decision Point**
```yaml
# ✅ Correct
- name: finalize
  type: deferred
  dependencies:
    - routing_decision  # Must list the decision point
    - auto_approve
    - manager_approval

# ❌ Wrong - missing decision point
- name: finalize
  type: deferred
  dependencies:
    - auto_approve
    - manager_approval
```

**2. Decision Points Cannot Be Circular**
```
# ❌ Not allowed - circular dependency
routing_a creates routing_b
routing_b creates routing_a
```

**3. No Dynamic Template Modification**
- Cannot add new handler types at runtime
- Cannot modify step configurations
- All possibilities must be predefined

---

## Testing Decision Point Workflows

### E2E Test Structure

Both Ruby and Rust implementations include comprehensive E2E tests covering all routing scenarios:

**Test Locations**:
- Ruby: `tests/e2e/ruby/conditional_approval_test.rs`
- Rust: `tests/e2e/rust/conditional_approval_rust.rs`

**Test Scenarios**:

1. **Small Amount ($500)** - Auto-approval only
   ```
   validate_request → routing_decision → auto_approve → finalize_approval
   Expected: 4 steps created, only auto_approve path taken
   ```

2. **Medium Amount ($3,000)** - Manager approval only
   ```
   validate_request → routing_decision → manager_approval → finalize_approval
   Expected: 4 steps created, only manager path taken
   ```

3. **Large Amount ($10,000)** - Dual approval
   ```
   validate_request → routing_decision → manager_approval + finance_review → finalize_approval
   Expected: 5 steps created, both approval paths taken (parallel)
   ```

4. **API Validation** - Initial step count verification
   ```
   Expected: 2 steps at initialization (validate_request, routing_decision)
   Reason: finalize_approval is transitive descendant of decision point
   ```

### Running Tests

```bash
# Run all E2E tests
cargo test --test e2e_tests

# Run Ruby conditional approval tests only
cargo test --test e2e_tests e2e::ruby::conditional_approval

# Run Rust conditional approval tests only
cargo test --test e2e_tests e2e::rust::conditional_approval_rust

# Run with output for debugging
cargo test --test e2e_tests -- --nocapture
```

### Test Fixtures

**Ruby Template**: `tests/fixtures/task_templates/ruby/conditional_approval_handler.yaml`
**Rust Template**: `tests/fixtures/task_templates/rust/conditional_approval_rust.yaml`

Both templates demonstrate:
- Decision point step configuration (`type: decision`)
- Deferred convergence step (`type: deferred`)
- Dynamic step dependencies
- Namespace isolation between Ruby/Rust

### Validation Checklist

When implementing decision point workflows, ensure:

- ✅ Decision point step has `type: decision`
- ✅ Deferred convergence step has `type: deferred`
- ✅ All possible dependencies listed in deferred step
- ✅ Handler embeds `decision_point_outcome` in result
- ✅ Step names in outcome match template definitions
- ✅ E2E tests cover all routing scenarios
- ✅ Tests validate step creation and completion
- ✅ Namespace isolated if multiple implementations exist

---

## Related Documentation

- **[Use Cases & Patterns](use-cases-and-patterns.md)** - More workflow examples
- **[States and Lifecycles](states-and-lifecycles.md)** - State machine details
- **[Task and Step Readiness](task-and-step-readiness-and-execution.md)** - Dependency resolution logic
- **[Quick Start](quick-start.md)** - Getting started guide
- **[Crate Architecture](crate-architecture.md)** - System architecture overview
- **[Decision Point E2E Tests](testing/decision-point-e2e-tests.md)** - Detailed test documentation
- **[TAS-53](https://linear.app/tasker-systems/issue/TAS-53)** - Implementation details and learnings

---

← Back to [Documentation Hub](README.md)
