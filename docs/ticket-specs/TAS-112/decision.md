# TAS-112 Phase 4: Decision Handler Pattern Analysis

**Phase**: 4 of 9  
**Created**: 2025-12-27  
**Status**: Analysis Complete  
**Prerequisites**: Phase 3 (API Handler Pattern)

---

## Executive Summary

This analysis compares decision point handler patterns for conditional workflow routing across all four languages. **Finding**: Three languages have mature, well-aligned implementations with excellent consistency. Rust has the **shared type definition** (DecisionPointOutcome enum) but **no decision handler helper class** - handlers must manually construct outcomes.

**Key Findings**:
1. **Rust Gap**: Type exists, but no decision handler helper class to simplify usage
2. **Excellent Alignment**: Ruby, Python, TypeScript have nearly identical APIs
3. **Outcome Structure**: Highly consistent serialization format across all languages
4. **Helper Methods**: All three have `decision_success()`, `skip_branches()`/`no_branches()` patterns
5. **Architectural Pattern**: All use **subclass inheritance** (same anti-pattern as API handlers!)

**Guiding Principle** (Zen of Python): *"Explicit is better than implicit."*

**Project Context**: Pre-alpha greenfield. Breaking changes encouraged to achieve composition-over-inheritance architecture.

---

## Comparison Matrix

### Handler Class Definition

| Language | Class Name | Inheritance | File Location |
|----------|-----------|-------------|---------------|
| **Rust** | ❌ **No helper class** | N/A (type only) | - |
| **Ruby** | `Decision` | Subclass of `Base` | `lib/tasker_core/step_handler/decision.rb` |
| **Python** | `DecisionHandler` | Subclass of `StepHandler` | `python/tasker_core/step_handler/decision.py` |
| **TypeScript** | `DecisionHandler` | Subclass of `StepHandler` | `src/handler/decision.ts` |

**Analysis**: 
- ❌ **All use subclass inheritance** - violates composition-over-inheritance goal
- ✅ Three implementations are remarkably consistent
- ❌ Rust has type but no helper class - gap in developer ergonomics

**Recommendation**:
- ✅ **Create Rust decision handler trait** (not subclass!)
- ✅ **Migrate Ruby/Python/TypeScript to mixin/trait pattern**
- 📝 **Zen Alignment**: "Flat is better than nested" - composition over inheritance

---

## DecisionPointOutcome Type

### Rust: Enum (Shared Type)

**Location**: `tasker-shared/src/messaging/execution_types.rs`

**Definition**:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionPointOutcome {
    /// No branches should be created
    NoBranches,
    
    /// Create and execute specific steps by name
    CreateSteps {
        step_names: Vec<String>,
    },
}
```

**Factory Methods**:
```rust
impl DecisionPointOutcome {
    pub fn no_branches() -> Self { ... }
    pub fn create_steps(step_names: Vec<String>) -> Self { ... }
    pub fn requires_step_creation(&self) -> bool { ... }
    pub fn step_names(&self) -> Vec<String> { ... }
    pub fn from_step_result(result: &StepExecutionResult) -> Option<Self> { ... }
}
```

**Serialization**:
```json
// NoBranches
{ "type": "no_branches" }

// CreateSteps
{ "type": "create_steps", "step_names": ["step_a", "step_b"] }
```

**Analysis**:
- ✅ **Excellent type safety** - Rust enum pattern
- ✅ **Factory methods** for ergonomic construction
- ✅ **Serde integration** for automatic serialization
- ⚠️ **No handler helper** - must manually embed in result

### Ruby: Class Hierarchy (Dry-Struct)

**Location**: `lib/tasker_core/types/decision_point_outcome.rb`

**Definition**:
```ruby
module DecisionPointOutcome
  class NoBranches < Dry::Struct
    attribute :type, Types::String.default('no_branches')
    
    def to_h
      { type: 'no_branches' }
    end
    
    def requires_step_creation?
      false
    end
    
    def step_names
      []
    end
  end

  class CreateSteps < Dry::Struct
    attribute :type, Types::String.default('create_steps')
    attribute :step_names, Types::Array.of(Types::Strict::String).constrained(min_size: 1)
    
    def to_h
      { type: 'create_steps', step_names: step_names }
    end
    
    def requires_step_creation?
      true
    end
  end
end
```

**Factory Methods**:
```ruby
DecisionPointOutcome.no_branches
DecisionPointOutcome.create_steps(['step1', 'step2'])
DecisionPointOutcome.from_hash({ type: 'create_steps', step_names: [...] })
```

**Analysis**:
- ✅ **Dry-Struct validation** - type safety
- ✅ **Factory methods** match Rust pattern
- ✅ **Serialization** via `to_h` method
- ✅ **Helper methods** for checking outcome type

### Python: Pydantic Model

**Location**: `python/tasker_core/types.py`

**Definition**:
```python
class DecisionType(str, Enum):
    CREATE_STEPS = "create_steps"
    NO_BRANCHES = "no_branches"

class DecisionPointOutcome(BaseModel):
    decision_type: DecisionType
    next_step_names: list[str] = Field(default_factory=list)
    dynamic_steps: list[dict[str, Any]] | None = None
    reason: str | None = None
    routing_context: dict[str, Any] = Field(default_factory=dict)
```

**Factory Methods**:
```python
@classmethod
def create_steps(cls, step_names: list[str], 
                 dynamic_steps=None, 
                 routing_context=None) -> DecisionPointOutcome:
    return cls(
        decision_type=DecisionType.CREATE_STEPS,
        next_step_names=step_names,
        dynamic_steps=dynamic_steps,
        routing_context=routing_context or {}
    )

@classmethod
def no_branches(cls, reason: str, 
                routing_context=None) -> DecisionPointOutcome:
    return cls(
        decision_type=DecisionType.NO_BRANCHES,
        reason=reason,
        routing_context=routing_context or {}
    )
```

**Analysis**:
- ✅ **Pydantic validation** - type safety
- ✅ **Factory methods** for ergonomic construction
- ✅ **Extra fields**: `routing_context`, `dynamic_steps`, `reason`
- ⚠️ **Field naming differs**: `next_step_names` vs `step_names`

### TypeScript: Interface + Enum

**Location**: `src/handler/decision.ts`

**Definition**:
```typescript
export enum DecisionType {
  CREATE_STEPS = 'create_steps',
  NO_BRANCHES = 'no_branches',
}

export interface DecisionPointOutcome {
  decisionType: DecisionType;
  nextStepNames: string[];
  dynamicSteps?: Array<Record<string, unknown>>;
  reason?: string;
  routingContext: Record<string, unknown>;
}
```

**Usage**:
```typescript
const outcome: DecisionPointOutcome = {
  decisionType: DecisionType.CREATE_STEPS,
  nextStepNames: ['step1', 'step2'],
  routingContext: {}
};
```

**Analysis**:
- ✅ **TypeScript type safety** via interface
- ✅ **Enum for decision type**
- ✅ **Matches Python structure** closely (camelCase vs snake_case)
- ⚠️ **No factory methods** - must construct manually

**Cross-Language Comparison**:

| Feature | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| **Type System** | Enum | Dry::Struct classes | Pydantic BaseModel | Interface + Enum |
| **Factory: no_branches()** | ✅ | ✅ | ✅ | ❌ (manual) |
| **Factory: create_steps()** | ✅ | ✅ | ✅ | ❌ (manual) |
| **Validation** | Serde | Dry-Struct | Pydantic | TypeScript compiler |
| **Serialization** | Automatic | `to_h` | Pydantic | Manual |
| **routing_context** | ❌ | ❌ | ✅ | ✅ |
| **dynamic_steps** | ❌ | ❌ | ✅ | ✅ |
| **reason field** | ❌ | ❌ | ✅ | ✅ |

**Recommendation**:
- ✅ **Add fields to Rust/Ruby**: `routing_context`, `reason`, `dynamic_steps`
- ✅ **Add factory methods to TypeScript**: Align with other languages
- ✅ **Standardize field names**: Use `step_names` everywhere (not `next_step_names`)
- 📝 **Zen Alignment**: "There should be one obvious way" - unified factories

---

## Decision Handler Helper Methods

### Ruby Decision Handler

**Location**: `lib/tasker_core/step_handler/decision.rb`

**Primary Methods**:
```ruby
# Main API - simplified success helper
def decision_success(steps:, result_data: {}, metadata: {})
  step_names = Array(steps)
  validate_step_names!(step_names)
  
  outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(step_names)
  
  result = result_data.merge(
    decision_point_outcome: outcome.to_h
  )
  
  TaskerCore::Types::StepHandlerCallResult.success(
    result: result,
    metadata: build_decision_metadata(metadata, outcome)
  )
end

# No branches helper
def decision_no_branches(result_data: {}, metadata: {})
  outcome = TaskerCore::Types::DecisionPointOutcome.no_branches
  
  result = result_data.merge(
    decision_point_outcome: outcome.to_h
  )
  
  TaskerCore::Types::StepHandlerCallResult.success(
    result: result,
    metadata: build_decision_metadata(metadata, outcome)
  )
end

# Advanced: custom outcome
def decision_with_custom_outcome(outcome:, result_data: {}, metadata: {})
  validated_outcome = validate_decision_outcome!(outcome)
  
  result = result_data.merge(
    decision_point_outcome: validated_outcome
  )
  
  TaskerCore::Types::StepHandlerCallResult.success(
    result: result,
    metadata: build_decision_metadata(metadata, outcome)
  )
end
```

**Validation**:
```ruby
def validate_step_names!(step_names)
  unless step_names.is_a?(Array) && !step_names.empty?
    raise_invalid_outcome!('step_names must be non-empty array')
  end
  
  unless step_names.all? { |name| name.is_a?(String) && !name.empty? }
    raise_invalid_outcome!('All step names must be non-empty strings')
  end
end

def validate_decision_outcome!(outcome)
  # Validates outcome structure, type field, step_names
  # Raises PermanentError if invalid
end
```

**Capabilities**:
```ruby
def capabilities
  super + %w[decision_point dynamic_workflow step_creation]
end
```

**Analysis**:
- ✅ **Keyword arguments** (`steps:`, `result_data:`)
- ✅ **Validation** built-in
- ✅ **Metadata builder** adds decision context
- ✅ **Custom outcome support** for advanced cases
- ⚠️ **No failure helper** (unlike Python/TypeScript)

### Python DecisionHandler

**Location**: `python/tasker_core/step_handler/decision.py`

**Primary Methods**:
```python
def decision_success(
    self,
    steps: list[str],
    routing_context: dict[str, Any] | None = None,
    metadata: dict[str, Any] | None = None,
) -> StepHandlerResult:
    """Simplified decision success helper (cross-language standard API)."""
    outcome = DecisionPointOutcome.create_steps(steps, routing_context=routing_context)
    return self.decision_success_with_outcome(outcome, metadata=metadata)

def decision_success_with_outcome(
    self,
    outcome: DecisionPointOutcome,
    metadata: dict[str, Any] | None = None,
) -> StepHandlerResult:
    """Create success result with DecisionPointOutcome."""
    decision_point_outcome: dict[str, Any] = {
        "type": outcome.decision_type.value,
        "step_names": outcome.next_step_names,
    }
    
    result: dict[str, Any] = {
        "decision_point_outcome": decision_point_outcome,
    }
    
    if outcome.dynamic_steps:
        result["dynamic_steps"] = outcome.dynamic_steps
    
    if outcome.routing_context:
        result["routing_context"] = outcome.routing_context
    
    combined_metadata = metadata or {}
    combined_metadata["decision_handler"] = self.name
    combined_metadata["decision_version"] = self.version
    
    return self.success(result, metadata=combined_metadata)

def decision_no_branches(
    self,
    outcome: DecisionPointOutcome,
    metadata: dict[str, Any] | None = None,
) -> StepHandlerResult:
    """Create success result for decision with no branches."""
    decision_point_outcome: dict[str, Any] = {
        "type": outcome.decision_type.value,
    }
    
    result: dict[str, Any] = {
        "decision_point_outcome": decision_point_outcome,
        "reason": outcome.reason,
    }
    
    if outcome.routing_context:
        result["routing_context"] = outcome.routing_context
    
    combined_metadata = metadata or {}
    combined_metadata["decision_handler"] = self.name
    combined_metadata["decision_version"] = self.version
    
    return self.success(result, metadata=combined_metadata)

def skip_branches(
    self,
    reason: str,
    routing_context: dict[str, Any] | None = None,
    metadata: dict[str, Any] | None = None,
) -> StepHandlerResult:
    """Convenience method to skip all branches."""
    outcome = DecisionPointOutcome.no_branches(
        reason=reason,
        routing_context=routing_context,
    )
    return self.decision_no_branches(outcome, metadata=metadata)

def decision_failure(
    self,
    message: str,
    error_type: str = "decision_error",
    retryable: bool = False,
    metadata: dict[str, Any] | None = None,
) -> StepHandlerResult:
    """Create failure result for decision that could not be made."""
    combined_metadata = metadata or {}
    combined_metadata["decision_handler"] = self.name
    combined_metadata["decision_version"] = self.version
    
    return self.failure(
        message=message,
        error_type=error_type,
        retryable=retryable,
        metadata=combined_metadata,
    )

# Alias for backward compatibility
def route_to_steps(self, step_names, routing_context=None, metadata=None):
    return self.decision_success(step_names, routing_context, metadata)
```

**Capabilities**:
```python
@property
def capabilities(self) -> list[str]:
    return ["process", "decision", "routing"]
```

**Analysis**:
- ✅ **Positional + named arguments**
- ✅ **Two-tier API**: Simple (`decision_success`) + advanced (`decision_success_with_outcome`)
- ✅ **Convenience methods**: `skip_branches()` is cleaner than manual outcome
- ✅ **Failure helper**: `decision_failure()` for error cases
- ✅ **Routing context** explicitly supported
- ✅ **Backward compat**: `route_to_steps()` alias

### TypeScript DecisionHandler

**Location**: `src/handler/decision.ts`

**Primary Methods**:
```typescript
protected decisionSuccess(
  steps: string[],
  routingContext?: Record<string, unknown>,
  metadata?: Record<string, unknown>
): StepHandlerResult {
  const outcome: DecisionPointOutcome = {
    decisionType: DecisionType.CREATE_STEPS,
    nextStepNames: steps,
    routingContext: routingContext || {},
  };
  
  return this.decisionSuccessWithOutcome(outcome, metadata);
}

protected decisionSuccessWithOutcome(
  outcome: DecisionPointOutcome,
  metadata?: Record<string, unknown>
): StepHandlerResult {
  const decisionPointOutcome: Record<string, unknown> = {
    type: outcome.decisionType,
    step_names: outcome.nextStepNames,
  };
  
  const result: Record<string, unknown> = {
    decision_point_outcome: decisionPointOutcome,
  };
  
  if (outcome.dynamicSteps) {
    result.dynamic_steps = outcome.dynamicSteps;
  }
  
  if (outcome.routingContext && Object.keys(outcome.routingContext).length > 0) {
    result.routing_context = outcome.routingContext;
  }
  
  const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
  combinedMetadata.decision_handler = this.name;
  combinedMetadata.decision_version = this.version;
  
  return this.success(result, combinedMetadata);
}

protected decisionNoBranches(
  outcome: DecisionPointOutcome,
  metadata?: Record<string, unknown>
): StepHandlerResult {
  const decisionPointOutcome: Record<string, unknown> = {
    type: outcome.decisionType,
  };
  
  const result: Record<string, unknown> = {
    decision_point_outcome: decisionPointOutcome,
  };
  
  if (outcome.reason) {
    result.reason = outcome.reason;
  }
  
  if (outcome.routingContext && Object.keys(outcome.routingContext).length > 0) {
    result.routing_context = outcome.routingContext;
  }
  
  const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
  combinedMetadata.decision_handler = this.name;
  combinedMetadata.decision_version = this.version;
  
  return this.success(result, combinedMetadata);
}

protected skipBranches(
  reason: string,
  routingContext?: Record<string, unknown>,
  metadata?: Record<string, unknown>
): StepHandlerResult {
  const outcome: DecisionPointOutcome = {
    decisionType: DecisionType.NO_BRANCHES,
    nextStepNames: [],
    reason,
    routingContext: routingContext || {},
  };
  
  return this.decisionNoBranches(outcome, metadata);
}

protected decisionFailure(
  message: string,
  errorType = 'decision_error',
  retryable = false,
  metadata?: Record<string, unknown>
): StepHandlerResult {
  const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
  combinedMetadata.decision_handler = this.name;
  combinedMetadata.decision_version = this.version;
  
  return this.failure(message, errorType, retryable, combinedMetadata);
}
```

**Capabilities**:
```typescript
get capabilities(): string[] {
  return ['process', 'decision', 'routing'];
}
```

**Analysis**:
- ✅ **Optional parameters** pattern
- ✅ **Two-tier API** matches Python exactly
- ✅ **Convenience methods**: `skipBranches()` 
- ✅ **Failure helper**: `decisionFailure()` 
- ✅ **Routing context** explicitly supported
- ✅ **Nearly identical to Python** (naming convention differences only)

### Rust: Missing Handler Class

**Gap**: Rust has `DecisionPointOutcome` type but no handler helper class

**Current Pattern** (manual):
```rust
// Handler must manually construct outcome and embed in result
let outcome = DecisionPointOutcome::create_steps(vec!["step1".to_string()]);
let result = json!({
    "decision_point_outcome": outcome.to_value(),
    "routing_context": json!({"reason": "high_value"})
});

success_result(
    step_uuid,
    result,
    elapsed_ms,
    Some(json!({"decision_handler": "my_handler"}))
)
```

**Problem**: 
- ❌ **Boilerplate** - developers must remember structure
- ❌ **No validation** - easy to construct invalid results
- ❌ **Inconsistent** - every handler does it differently
- ❌ **No metadata helpers** - decision context not standardized

---

## API Comparison Matrix

### Method Availability

| Method | Ruby | Python | TypeScript | Rust |
|--------|------|--------|------------|------|
| **decision_success(steps)** | ✅ `decision_success(steps:)` | ✅ `decision_success(steps)` | ✅ `decisionSuccess(steps)` | ❌ Missing |
| **decision_success_with_outcome** | ✅ `decision_with_custom_outcome` | ✅ `decision_success_with_outcome` | ✅ `decisionSuccessWithOutcome` | ❌ Missing |
| **decision_no_branches** | ✅ `decision_no_branches()` | ✅ `decision_no_branches(outcome)` | ✅ `decisionNoBranches(outcome)` | ❌ Missing |
| **skip_branches** | ❌ Missing | ✅ `skip_branches(reason)` | ✅ `skipBranches(reason)` | ❌ Missing |
| **decision_failure** | ❌ Missing | ✅ `decision_failure(msg)` | ✅ `decisionFailure(msg)` | ❌ Missing |
| **validate_outcome** | ✅ `validate_decision_outcome!` | ❌ (implicit via Pydantic) | ❌ (implicit via TypeScript) | ❌ Missing |

**Gap Analysis**:
- ❌ **Ruby missing**: `skip_branches()`, `decision_failure()`
- ❌ **Rust missing**: Entire handler helper class
- ✅ **Python/TypeScript**: Feature complete and aligned

**Recommendation**:
- ✅ **Add to Ruby**: `skip_branches()` and `decision_failure()` helpers
- ✅ **Add to Rust**: Full decision handler trait with all helpers
- 📝 **Zen Alignment**: "If the implementation is easy to explain, it may be a good idea"

---

## Result Serialization Format

### Expected Structure (All Languages)

**CreateSteps Outcome**:
```json
{
  "result": {
    "decision_point_outcome": {
      "type": "create_steps",
      "step_names": ["step_a", "step_b"]
    },
    "routing_context": {
      "customer_tier": "premium",
      "amount": 5000
    },
    "dynamic_steps": [
      {
        "name": "custom_step",
        "handler_name": "custom_handler"
      }
    ]
  },
  "metadata": {
    "decision_handler": "my_decision_handler",
    "decision_version": "1.0.0"
  }
}
```

**NoBranches Outcome**:
```json
{
  "result": {
    "decision_point_outcome": {
      "type": "no_branches"
    },
    "reason": "Amount below threshold",
    "routing_context": {
      "amount": 100
    }
  },
  "metadata": {
    "decision_handler": "my_decision_handler",
    "decision_version": "1.0.0"
  }
}
```

**Consistency Analysis**:

| Field | Ruby | Python | TypeScript | Rust (Expected) |
|-------|------|--------|------------|-----------------|
| **decision_point_outcome.type** | ✅ `create_steps` / `no_branches` | ✅ `create_steps` / `no_branches` | ✅ `create_steps` / `no_branches` | ✅ `create_steps` / `no_branches` |
| **decision_point_outcome.step_names** | ✅ | ✅ | ✅ | ✅ |
| **routing_context** | ❌ Not in type | ✅ | ✅ | ❌ Not in type |
| **dynamic_steps** | ❌ Not in type | ✅ | ✅ | ❌ Not in type |
| **reason** | ❌ Not in type | ✅ | ✅ | ❌ Not in type |
| **metadata.decision_handler** | ✅ (via helper) | ✅ | ✅ | ❌ Manual |
| **metadata.decision_version** | ✅ (via helper) | ✅ | ✅ | ❌ Manual |

**Recommendation**:
- ✅ **Extend Ruby DecisionPointOutcome**: Add `routing_context`, `reason`, `dynamic_steps`
- ✅ **Extend Rust DecisionPointOutcome**: Add same fields
- ✅ **Standardize metadata**: All handlers should add `decision_handler` and `decision_version`
- 📝 **Zen Alignment**: "Explicit is better than implicit" - all context should be captured

---

## Usage Examples

### Ruby Example

```ruby
class OrderRoutingDecision < TaskerCore::StepHandler::Decision
  def call(context)
    amount = context.get_task_field('amount')
    customer_tier = context.get_task_field('customer_tier')
    
    if amount < 1000
      # Auto-approve
      decision_success(
        steps: ['auto_approve'],
        result_data: { route_type: 'auto', amount: amount }
      )
    elsif customer_tier == 'enterprise'
      # Enterprise fast-track
      decision_success(
        steps: ['enterprise_approval'],
        result_data: { route_type: 'enterprise', amount: amount }
      )
    elsif amount > 10000
      # Dual approval for large amounts
      decision_success(
        steps: ['manager_approval', 'finance_review'],
        result_data: { route_type: 'dual', amount: amount }
      )
    else
      # Standard approval
      decision_success(
        steps: ['standard_approval'],
        result_data: { route_type: 'standard', amount: amount }
      )
    end
  end
end
```

### Python Example

```python
class OrderRoutingDecision(DecisionHandler):
    handler_name = "order_routing_decision"
    handler_version = "1.0.0"
    
    def call(self, context: StepContext) -> StepHandlerResult:
        amount = context.get_task_field('amount')
        customer_tier = context.get_task_field('customer_tier')
        
        if amount < 1000:
            # Auto-approve
            return self.decision_success(
                ['auto_approve'],
                routing_context={'route_type': 'auto', 'amount': amount}
            )
        elif customer_tier == 'enterprise':
            # Enterprise fast-track
            return self.decision_success(
                ['enterprise_approval'],
                routing_context={'route_type': 'enterprise', 'amount': amount}
            )
        elif amount > 10000:
            # Dual approval for large amounts
            return self.decision_success(
                ['manager_approval', 'finance_review'],
                routing_context={'route_type': 'dual', 'amount': amount}
            )
        else:
            # Standard approval
            return self.decision_success(
                ['standard_approval'],
                routing_context={'route_type': 'standard', 'amount': amount}
            )
```

### TypeScript Example

```typescript
class OrderRoutingDecision extends DecisionHandler {
  static handlerName = 'order_routing_decision';
  static handlerVersion = '1.0.0';
  
  async call(context: StepContext): Promise<StepHandlerResult> {
    const amount = context.getTaskField<number>('amount');
    const customerTier = context.getTaskField<string>('customer_tier');
    
    if (amount < 1000) {
      // Auto-approve
      return this.decisionSuccess(
        ['auto_approve'],
        { route_type: 'auto', amount }
      );
    } else if (customerTier === 'enterprise') {
      // Enterprise fast-track
      return this.decisionSuccess(
        ['enterprise_approval'],
        { route_type: 'enterprise', amount }
      );
    } else if (amount > 10000) {
      // Dual approval for large amounts
      return this.decisionSuccess(
        ['manager_approval', 'finance_review'],
        { route_type: 'dual', amount }
      );
    } else {
      // Standard approval
      return this.decisionSuccess(
        ['standard_approval'],
        { route_type: 'standard', amount }
      );
    }
  }
}
```

### Rust Example (Proposed)

```rust
pub struct OrderRoutingDecision;

#[async_trait]
impl DecisionCapable for OrderRoutingDecision {
    async fn call(&self, context: StepContext) -> StepExecutionResult {
        let amount = context.get_task_field::<f64>("amount")?;
        let customer_tier = context.get_task_field::<String>("customer_tier")?;
        
        if amount < 1000.0 {
            // Auto-approve
            self.decision_success(
                vec!["auto_approve".to_string()],
                Some(json!({"route_type": "auto", "amount": amount}))
            )
        } else if customer_tier == "enterprise" {
            // Enterprise fast-track
            self.decision_success(
                vec!["enterprise_approval".to_string()],
                Some(json!({"route_type": "enterprise", "amount": amount}))
            )
        } else if amount > 10000.0 {
            // Dual approval for large amounts
            self.decision_success(
                vec!["manager_approval".to_string(), "finance_review".to_string()],
                Some(json!({"route_type": "dual", "amount": amount}))
            )
        } else {
            // Standard approval
            self.decision_success(
                vec!["standard_approval".to_string()],
                Some(json!({"route_type": "standard", "amount": amount}))
            )
        }
    }
}
```

**Analysis**:
- ✅ **Consistent logic** across all languages
- ✅ **Similar API surface** - minor naming differences only
- ⚠️ **Rust more verbose** without trait helpers
- 📝 **Zen Alignment**: "Readability counts" - all are readable despite language differences

---

## Architecture Anti-Pattern

### All Use Subclass Inheritance

**Ruby**:
```ruby
class Decision < Base  # ❌ Inheritance
  # ...
end
```

**Python**:
```python
class DecisionHandler(StepHandler):  # ❌ Inheritance
  # ...
```

**TypeScript**:
```typescript
export abstract class DecisionHandler extends StepHandler {  # ❌ Inheritance
  // ...
}
```

**Problem**: Same as API handlers - violates composition-over-inheritance goal!

**Impact**:
- Cannot combine decision + API features easily
- Forces single inheritance hierarchy
- Handlers must choose: Decision OR API, not both
- Tight coupling

**Recommendation** (Composition Pattern):

**Ruby (Mixin)**:
```ruby
module StepHandler
  module DecisionCapabilities
    def decision_success(steps:, result_data: {}, metadata: {})
      # Implementation
    end
    
    def decision_no_branches(result_data: {}, metadata: {})
      # Implementation
    end
    
    def skip_branches(reason:, result_data: {}, metadata: {})
      # Implementation
    end
  end
end

class MyDecisionHandler < Base
  include StepHandler::DecisionCapabilities  # ✅ Composition
end
```

**Python (Mixin)**:
```python
class DecisionMixin:
    """Provides decision routing capabilities via composition."""
    
    def decision_success(self, steps, routing_context=None, metadata=None):
        # Implementation
    
    def skip_branches(self, reason, routing_context=None, metadata=None):
        # Implementation

class MyDecisionHandler(StepHandler, DecisionMixin):  # ✅ Composition
    pass
```

**TypeScript (Mixin/Composition)**:
```typescript
// Helper function approach
function withDecisionCapabilities<T extends StepHandler>(handler: T) {
  return {
    ...handler,
    decisionSuccess(steps: string[], routingContext?: Record<string, unknown>) {
      // Implementation
    },
    skipBranches(reason: string) {
      // Implementation
    }
  };
}
```

**Rust (Trait)**:
```rust
#[async_trait]
pub trait DecisionCapable {
    fn decision_success(
        &self,
        step_names: Vec<String>,
        routing_context: Option<Value>
    ) -> StepExecutionResult;
    
    fn skip_branches(
        &self,
        reason: &str,
        routing_context: Option<Value>
    ) -> StepExecutionResult;
}

// Implement for any handler
impl DecisionCapable for MyHandler {
    // Implementation
}
```

**Recommendation**:
- ✅ **Adopt Now**: Migrate all languages to composition pattern
- ✅ **Breaking Change Acceptable**: Pre-alpha status
- ✅ **Enables Combining**: Handler can be both API + Decision capable
- 📝 **Zen Alignment**: "Flat is better than nested" - composition over inheritance

---

## Functional Gaps

### Rust (Critical Gaps)
1. ❌ **No decision handler helper trait** - needs full implementation
2. ❌ **No helper methods** - manual outcome construction
3. ❌ **Missing fields in DecisionPointOutcome**: `routing_context`, `reason`, `dynamic_steps`
4. ❌ **No validation** - easy to construct invalid outcomes
5. ❌ **No metadata helpers** - must manually add decision context

### Ruby (Minor Gaps)
1. ⚠️ **Missing `skip_branches()` convenience method** - less ergonomic than Python/TypeScript
2. ⚠️ **Missing `decision_failure()` helper** - no standardized error handling
3. ⚠️ **DecisionPointOutcome lacks fields**: `routing_context`, `reason`, `dynamic_steps`
4. ✅ **Has validation** - `validate_decision_outcome!` is unique

### Python (Complete)
1. ✅ **Fully featured** - reference implementation
2. ✅ **All helper methods** including convenience methods
3. ✅ **Complete outcome type** with all fields
4. ✅ **Failure helper** for error cases

### TypeScript (Complete)
1. ✅ **Fully featured** - matches Python
2. ✅ **All helper methods** including convenience methods
3. ✅ **Complete outcome type** with all fields
4. ✅ **Failure helper** for error cases

---

## Recommendations Summary

### Critical Changes (Implement Now)

#### 1. Rust Decision Handler Trait Implementation
- ✅ Create `DecisionCapable` trait (not subclass!)
- ✅ Add helper methods: `decision_success()`, `skip_branches()`, `decision_failure()`
- ✅ Extend `DecisionPointOutcome` enum with fields:
  - `routing_context: Option<Value>`
  - `reason: Option<String>` (for NoBranches)
  - `dynamic_steps: Option<Vec<Value>>` (for CreateSteps)
- ✅ Add validation helpers
- ✅ Add metadata builders (add `decision_handler`, `decision_version`)

**Implementation Priority**: **HIGH** - critical ergonomics gap

#### 2. Migration to Composition Pattern
All languages must migrate from subclass inheritance to composition:

**Ruby**:
- ✅ Create `StepHandler::DecisionCapabilities` module
- ✅ Extract decision methods to mixin
- ✅ Deprecate `Decision` subclass pattern
- ✅ Update documentation

**Python**:
- ✅ Refactor `DecisionHandler` to `DecisionMixin`
- ✅ Use multiple inheritance: `class MyHandler(StepHandler, DecisionMixin)`
- ✅ Update documentation

**TypeScript**:
- ✅ Extract decision functionality to composable helper
- ✅ Consider decorator pattern or mixin utilities
- ✅ Update documentation

**Rust**:
- ✅ Implement as trait from the start (already composition-friendly)

#### 3. Ruby Enhancements
- ✅ Add `skip_branches(reason:, result_data:, metadata:)` helper
- ✅ Add `decision_failure(message:, error_type:, retryable:, metadata:)` helper
- ✅ Extend `DecisionPointOutcome` classes with `routing_context`, `reason`, `dynamic_steps`

#### 4. Cross-Language Standardization

**DecisionPointOutcome Structure**:
- ✅ Add `routing_context` field to Ruby and Rust
- ✅ Add `reason` field to Ruby and Rust (for NoBranches)
- ✅ Add `dynamic_steps` field to Ruby and Rust (for CreateSteps)
- ✅ Standardize field names: use `step_names` everywhere (not `next_step_names`)

**Helper Method Names**:
- ✅ Primary: `decision_success(steps, routing_context, metadata)`
- ✅ Advanced: `decision_success_with_outcome(outcome, metadata)`
- ✅ Convenience: `skip_branches(reason, routing_context, metadata)`
- ✅ Error: `decision_failure(message, error_type, retryable, metadata)`

**Metadata Standards**:
- ✅ All decision results should include:
  - `decision_handler`: handler name
  - `decision_version`: handler version
  - `outcome_type`: `create_steps` or `no_branches`
  - `branches_created`: count of steps (for CreateSteps)

### Documentation Requirements

1. **Decision Handler Reference Guide**:
   - Helper method signatures per language
   - DecisionPointOutcome structure
   - Serialization format examples
   - Common routing patterns

2. **Composition Pattern Guide**:
   - How to use decision capabilities via composition
   - Migration examples from inheritance
   - Combining decision + API capabilities

3. **Conditional Workflow Tutorial**:
   - End-to-end decision handler examples
   - Dynamic step creation patterns
   - Routing context best practices

---

## Implementation Checklist

### Rust Decision Handler (New Implementation)
- [ ] Extend `DecisionPointOutcome` enum in `tasker-shared`:
  - [ ] Add `routing_context: Option<Value>` field
  - [ ] Add `reason: Option<String>` to NoBranches
  - [ ] Add `dynamic_steps: Option<Vec<Value>>` to CreateSteps
- [ ] Create `DecisionCapable` trait in `workers/rust`:
  - [ ] `decision_success()` method
  - [ ] `decision_success_with_outcome()` method
  - [ ] `skip_branches()` convenience method
  - [ ] `decision_failure()` error helper
- [ ] Add validation helpers
- [ ] Add metadata builder utilities
- [ ] Add example handler using DecisionCapable
- [ ] Add integration tests
- [ ] Update documentation

### Ruby Composition Migration
- [ ] Extend `DecisionPointOutcome` types:
  - [ ] Add `routing_context` attribute
  - [ ] Add `reason` attribute (NoBranches)
  - [ ] Add `dynamic_steps` attribute (CreateSteps)
- [ ] Create `StepHandler::DecisionCapabilities` module
- [ ] Move decision methods to module
- [ ] Add `skip_branches()` convenience method
- [ ] Add `decision_failure()` error helper
- [ ] Deprecate `Decision` subclass
- [ ] Update all examples to use mixin
- [ ] Add migration guide

### Python Composition Migration
- [ ] Rename `DecisionHandler` → `DecisionMixin`
- [ ] Keep class functional (backward compat in pre-alpha)
- [ ] Update examples to show composition
- [ ] Document mixin usage
- [ ] Add tests for composition pattern

### TypeScript Composition Migration
- [ ] Extract decision functionality to composable helper
- [ ] Implement decorator or mixin pattern
- [ ] Update examples
- [ ] Document composition approach

### Documentation
- [ ] Create `docs/decision-handlers-reference.md`
- [ ] Create `docs/composition-patterns.md` (combine with API handler guidance)
- [ ] Update `docs/conditional-workflows.md` with new API patterns
- [ ] Add decision handler examples to `docs/worker-crates/*.md`
- [ ] Create decision outcome serialization reference

---

## Next Phase

**Phase 5: Batchable Handler Pattern** will analyze batch processing handlers for parallel step creation. Key questions:
- Do all languages have batch analyzer + batch worker patterns?
- Is the `CursorConfig` structure consistent?
- Are there helpers for batch outcome construction?
- Same inheritance anti-pattern?

---

## Appendix: Rust Decision Handler Trait Sketch

**Trait Definition**:
```rust
use async_trait::async_trait;
use serde_json::{json, Value};
use tasker_shared::messaging::{DecisionPointOutcome, StepExecutionResult};

#[async_trait]
pub trait DecisionCapable {
    /// Get the step UUID for result construction
    fn step_uuid(&self) -> Uuid;
    
    /// Get the handler name for metadata
    fn handler_name(&self) -> &str;
    
    /// Get the handler version for metadata
    fn handler_version(&self) -> &str;
    
    /// Get elapsed milliseconds for result
    fn elapsed_ms(&self) -> u64;
    
    /// Simple decision success - route to specified steps
    fn decision_success(
        &self,
        step_names: Vec<String>,
        routing_context: Option<Value>,
    ) -> StepExecutionResult {
        let outcome = DecisionPointOutcome::CreateSteps {
            step_names,
            routing_context: routing_context.clone(),
            dynamic_steps: None,
        };
        
        self.decision_success_with_outcome(outcome, routing_context)
    }
    
    /// Advanced decision success - full outcome control
    fn decision_success_with_outcome(
        &self,
        outcome: DecisionPointOutcome,
        routing_context: Option<Value>,
    ) -> StepExecutionResult {
        let mut result = json!({
            "decision_point_outcome": outcome.to_value()
        });
        
        if let Some(ctx) = routing_context {
            result.as_object_mut().unwrap().insert("routing_context".to_string(), ctx);
        }
        
        if let DecisionPointOutcome::CreateSteps { dynamic_steps: Some(steps), .. } = &outcome {
            result.as_object_mut().unwrap().insert(
                "dynamic_steps".to_string(),
                json!(steps)
            );
        }
        
        let metadata = json!({
            "decision_handler": self.handler_name(),
            "decision_version": self.handler_version(),
            "outcome_type": match outcome {
                DecisionPointOutcome::NoBranches { .. } => "no_branches",
                DecisionPointOutcome::CreateSteps { .. } => "create_steps",
            },
            "branches_created": outcome.step_names().len()
        });
        
        success_result(
            self.step_uuid(),
            result,
            self.elapsed_ms(),
            Some(metadata)
        )
    }
    
    /// Convenience method to skip all branches
    fn skip_branches(
        &self,
        reason: &str,
        routing_context: Option<Value>,
    ) -> StepExecutionResult {
        let outcome = DecisionPointOutcome::NoBranches {
            reason: Some(reason.to_string()),
            routing_context: routing_context.clone(),
        };
        
        let mut result = json!({
            "decision_point_outcome": outcome.to_value(),
            "reason": reason
        });
        
        if let Some(ctx) = routing_context {
            result.as_object_mut().unwrap().insert("routing_context".to_string(), ctx);
        }
        
        let metadata = json!({
            "decision_handler": self.handler_name(),
            "decision_version": self.handler_version(),
            "outcome_type": "no_branches"
        });
        
        success_result(
            self.step_uuid(),
            result,
            self.elapsed_ms(),
            Some(metadata)
        )
    }
    
    /// Create a failure result for a decision that could not be made
    fn decision_failure(
        &self,
        message: &str,
        error_type: &str,
        retryable: bool,
    ) -> StepExecutionResult {
        let metadata = json!({
            "decision_handler": self.handler_name(),
            "decision_version": self.handler_version()
        });
        
        failure_result(
            self.step_uuid(),
            message.to_string(),
            error_type.to_string(),
            retryable,
            Some(metadata)
        )
    }
}
```

**Extended DecisionPointOutcome** (in `tasker-shared`):
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionPointOutcome {
    /// No branches should be created
    NoBranches {
        /// Optional reason for no branches
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        
        /// Optional routing context
        #[serde(skip_serializing_if = "Option::is_none")]
        routing_context: Option<Value>,
    },
    
    /// Create and execute specific steps by name
    CreateSteps {
        /// Names of steps to create
        step_names: Vec<String>,
        
        /// Optional dynamically generated step definitions
        #[serde(skip_serializing_if = "Option::is_none")]
        dynamic_steps: Option<Vec<Value>>,
        
        /// Optional routing context
        #[serde(skip_serializing_if = "Option::is_none")]
        routing_context: Option<Value>,
    },
}
```

---

## Metadata

**Document Version**: 1.0  
**Analysis Date**: 2025-12-27  
**Reviewers**: TBD  
**Approval Status**: Draft
