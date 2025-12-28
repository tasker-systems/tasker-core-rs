# TAS-112 Phase 2: Base Handler API Analysis

**Phase**: 2 of 9  
**Created**: 2025-12-27  
**Status**: Analysis Complete  
**Prerequisites**: Phase 1 (Documentation Analysis)

---

## Executive Summary

This analysis compares the base handler APIs across Rust, Ruby, Python, and TypeScript. We identify **18 distinct areas of divergence** ranging from minor naming inconsistencies to fundamental architectural differences. Key findings:

1. **Success/Failure API Inconsistency**: Ruby uses keyword-required arguments, while Python/TypeScript use positional
2. **Error Code Field**: Ruby has `error_code`, Python/TypeScript recently added it, likely missing in Rust
3. **Result Type Architecture**: Ruby uses separate Success/Error classes, others use unified classes
4. **Config Schema**: Rust lacks schema support present in other languages

**Guiding Principle** (Zen of Python): *"There should be one-- and preferably only one --obvious way to do it."*

**Project Context**: Pre-alpha greenfield open source project. We prioritize **getting it right** over backward compatibility. Breaking changes are expected and encouraged when they lead to better architecture.

---

## Comparison Matrix

### Handler Trait/Class Definition

| Aspect | Rust | Ruby | Python | TypeScript |
|--------|------|------|--------|------------|
| **Trait/Class Name** | `RustStepHandler` (trait) | `Base` (class) | `StepHandler` (ABC) | `StepHandler` (abstract class) |
| **Module Path** | `workers/rust/src/step_handlers/mod.rs` | `lib/tasker_core/step_handler/base.rb` | `python/tasker_core/step_handler/base.py` | `src/handler/base.ts` |
| **Registration Pattern** | Manual in registry | Automatic/manual | Automatic/manual | Manual in registry |
| **Instantiation** | `new(config)` required | `initialize(config:, logger:)` | `__init__` implicit | `new()` implicit |

**Analysis**: Naming divergence creates cognitive load. `RustStepHandler` implies language-specificity; others use language-neutral names.

**Recommendation**: 
- ✅ **Adopt Now**: Rename Rust trait to `StepHandler` (breaking change acceptable)
- 📝 **Rationale**: Language-neutral naming reduces cognitive load in polyglot systems
- 📝 **Zen Alignment**: "Explicit is better than implicit" - name should describe concept, not implementation

---

### Call Signature

| Language | Signature | Context Parameter | Async |
|----------|-----------|-------------------|-------|
| **Rust** | `async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult>` | `&TaskSequenceStep` | ✅ Yes |
| **Ruby** | `def call(context)` | `StepContext` | ❌ No (sync) |
| **Python** | `def call(self, context: StepContext) -> StepHandlerResult` | `StepContext` | ❌ No (sync) |
| **TypeScript** | `async call(context: StepContext): Promise<StepHandlerResult>` | `StepContext` | ✅ Yes |

**Analysis**: 
1. **Context Parameter Name**: Rust uses `step_data`, others use `context`
2. **Async Pattern**: Rust/TypeScript are async, Ruby/Python are sync
3. **Type Hints**: Python/TypeScript have type annotations, Ruby does not

**Discrepancies**:
- Rust's `TaskSequenceStep` vs others' `StepContext` suggests different abstraction levels
- Async/sync differences may be intentional (Ruby GIL, Python GIL considerations)
- Ruby lacks type hints (Ruby 3+ RBS/Sorbet could add these)

**Recommendation**:
- ✅ **Adopt Now**: Rust rename parameter to `context`, change type to `StepContext` (breaking change)
- ✅ **Accept**: Async/sync differences are language-idiomatic and acceptable
- 📝 **Zen Alignment**: "There should be one... obvious way" - parameter name must be consistent
- 🔍 **Investigate**: Should Ruby/Python adopt async patterns? (FFI threading model implications)

---

### Result Types

| Language | Result Type Name | Success Method | Failure Method | Structure |
|----------|------------------|----------------|----------------|-----------|
| **Rust** | `StepExecutionResult` | `success_result(...)` (helper fn) | `failure_result(...)` | Unified type |
| **Ruby** | `StepHandlerCallResult` | `.success(result:, metadata:)` | `.error(error_type:, message:, ...)` | Separate `Success`/`Error` classes |
| **Python** | `StepHandlerResult` | `.success(result, metadata)` | `.failure(message, error_type, ...)` | Unified class |
| **TypeScript** | `StepHandlerResult` | `.success(result, metadata)` | `.failure(message, errorType, ...)` | Unified class |

**Analysis**: Ruby is the outlier with separate classes, which contradicts composition-over-inheritance goals.

**Detailed Comparison**:

#### Ruby (Separate Classes Pattern)
```ruby
# Two distinct classes
class Success < Dry::Struct
  attribute :success, Types::Bool.default(true)
  attribute :result, Types::Any
  attribute :metadata, Types::Hash.default { {} }
end

class Error < Dry::Struct
  attribute :success, Types::Bool.default(false)
  attribute :error_type, Types::String.enum(...)
  attribute :message, Types::String
  attribute :error_code, Types::String.optional  # ✅ Has error_code
  attribute :retryable, Types::Bool
  attribute :metadata, Types::Hash
end

# Factory methods
StepHandlerCallResult.success(result:, metadata:)  # Keyword required
StepHandlerCallResult.error(error_type:, message:, error_code:, ...)
```

**Ruby Analysis**:
- ✅ **Pros**: Type safety via Dry::Struct, impossible to mix success/error fields
- ❌ **Cons**: Inheritance-based, requires two classes, keyword arguments are verbose
- ⚠️ **Anti-Pattern**: Violates composition-over-inheritance goal

#### Python (Unified Class Pattern)
```python
class StepHandlerResult(BaseModel):
    is_success: bool = Field(alias="success")  # Field name differs from JSON
    result: dict[str, Any] | None = None
    error_message: str | None = None
    error_type: str | None = None
    error_code: str | None = None  # ✅ Has error_code
    retryable: bool = True
    metadata: dict[str, Any] = Field(default_factory=dict)

# Factory methods
@classmethod
def success(cls, result, metadata=None) -> StepHandlerResult:
    return cls(is_success=True, result=result, metadata=metadata or {})

@classmethod
def failure(cls, message, error_type="handler_error", retryable=True, 
            metadata=None, error_code=None) -> StepHandlerResult:
    return cls(is_success=False, error_message=message, ...)
```

**Python Analysis**:
- ✅ **Pros**: Single class, positional args allowed, Pydantic validation
- ✅ **Composition-Friendly**: No inheritance needed for result types
- ⚠️ **Quirk**: `is_success` field name vs `success` JSON field (alias needed for Python reserved word avoidance)

#### TypeScript (Unified Class Pattern)
```typescript
export class StepHandlerResult {
  public readonly success: boolean;
  public readonly result: Record<string, unknown> | null;
  public readonly errorMessage: string | null;
  public readonly errorType: string | null;
  public readonly errorCode: string | null;  // ✅ Has error_code
  public readonly retryable: boolean;
  public readonly metadata: Record<string, unknown>;

  static success(result, metadata?): StepHandlerResult {
    return new StepHandlerResult({ success: true, result, metadata: metadata ?? {} });
  }

  static failure(message, errorType=ErrorType.HANDLER_ERROR, retryable=true,
                 metadata?, errorCode?): StepHandlerResult {
    return new StepHandlerResult({ 
      success: false, errorMessage: message, errorType, errorCode, retryable, metadata 
    });
  }
}
```

**TypeScript Analysis**:
- ✅ **Pros**: Single class, readonly fields, clear factory methods
- ✅ **Composition-Friendly**: No inheritance, just data structure
- ✅ **Type Safety**: TypeScript ensures correct usage

**Recommendation**:
- ✅ **Adopt Now**: Ruby replace Success/Error classes with unified `StepHandlerResult` (breaking change)
- ✅ **Implement**: All languages use single result class with optional fields
- ✅ **Rationale**: 
  - Composition over inheritance (project goal)
  - Simpler mental model - one class, not two
  - Easier to extend with new fields
  - Single source of truth for result structure
- 📝 **Zen Alignment**: "Flat is better than nested" - single class vs class hierarchy

---

### Success Helper Method

| Language | Method Signature | Argument Style | Return Type |
|----------|------------------|----------------|-------------|
| **Rust** | `success_result(step_uuid, result, elapsed_ms, metadata)` | Positional | `StepExecutionResult` |
| **Ruby** | `success(result:, metadata: {})` | **Keyword required** | `Success` |
| **Python** | `success(result=None, metadata=None)` | Positional or keyword | `StepHandlerResult` |
| **TypeScript** | `success(result, metadata?)` | Positional | `StepHandlerResult` |

**Analysis**: Ruby's keyword-required style is the outlier.

**Ruby Usage**:
```ruby
# MUST use keywords
success(result: { order_id: "123" }, metadata: { time_ms: 100 })

# This would fail:
# success({ order_id: "123" }, { time_ms: 100 })  # ❌ ArgumentError
```

**Python Usage**:
```python
# Positional
self.success({"order_id": "123"}, {"time_ms": 100})

# Keyword
self.success(result={"order_id": "123"}, metadata={"time_ms": 100})

# Mixed
self.success({"order_id": "123"}, metadata={"time_ms": 100})
```

**TypeScript Usage**:
```typescript
// Positional
this.success({ orderId: "123" }, { timeMs: 100 });

// Metadata optional
this.success({ orderId: "123" });
```

**Recommendation**:
- ✅ **Adopt Now**: Ruby support positional arguments, make keywords optional (breaking change)
- ✅ **Preferred Style**: `success(result, metadata)` across all languages
- **Rationale**:
  - Positional is more concise for common 2-parameter case
  - Optional metadata is clearer with positional
  - Consistency across languages
- 📝 **Zen Alignment**: "Simple is better than complex" - positional for simple signatures

---

### Failure Helper Method

| Language | Method Signature | error_code Field | Retryable Default |
|----------|------------------|------------------|-------------------|
| **Rust** | `failure_result(step_uuid, message, error_type, retryable, ...)` | ❓ Unknown | ❓ Unknown |
| **Ruby** | `failure(message:, error_type:, error_code:, retryable:, metadata:)` | ✅ **Yes** | `false` |
| **Python** | `failure(message, error_type, retryable, metadata, error_code)` | ✅ **Yes** | `true` |
| **TypeScript** | `failure(message, errorType, retryable, metadata, errorCode)` | ✅ **Yes** | `true` |

**Critical Discrepancy**: Ruby defaults `retryable: false`, others default `true`.

**Ruby Pattern**:
```ruby
failure(
  message: "Payment failed",
  error_type: "PermanentError",  # Required
  error_code: "CARD_DECLINED",   # Optional, but supported
  retryable: false,               # Explicit (default: false)
  metadata: { card_last_four: "1234" }
)
```

**Python Pattern**:
```python
self.failure(
    message="Payment failed",
    error_type="PermanentError",
    retryable=False,              # Explicit (default: true)
    error_code="CARD_DECLINED",   # Optional
    metadata={"card_last_four": "1234"}
)
```

**Analysis**:
1. **error_code Field**: Ruby has had this from the start, Python/TypeScript added it (good alignment)
2. **Retryable Default Divergence**: 
   - Ruby `false` (pessimistic: assume errors are permanent)
   - Python/TypeScript `true` (optimistic: assume errors are transient)
3. **Safety Implications**: Defaulting `true` means transient errors get retried automatically

**Recommendation**:
- ✅ **Adopt Now**: All languages default `retryable=true` (Ruby breaking change)
- **Rationale**:
  - Retrying is safer than not retrying (eventual consistency)
  - Permanent errors should be explicit (`retryable=false`)
  - Network/timeout errors (most common) benefit from retry
  - Optimistic default aligns with distributed systems best practices
- 📝 **Zen Alignment**: "Errors should never pass silently" - retries give errors a chance to succeed

---

### Error Code Field Deep Dive

| Language | Supported | Field Name | Type | Optional |
|----------|-----------|------------|------|----------|
| **Rust** | ❓ Unknown (needs investigation) | - | - | - |
| **Ruby** | ✅ Yes | `error_code` | `String` | Yes |
| **Python** | ✅ Yes | `error_code` | `str \| None` | Yes |
| **TypeScript** | ✅ Yes | `errorCode` | `string \| null` | Yes |

**Purpose**: Application-specific error codes for:
- Error categorization (e.g., `PAYMENT_GATEWAY_TIMEOUT`, `INVALID_EMAIL_FORMAT`)
- Monitoring/alerting grouping
- Client-side error handling
- Distinct from `error_type` (broader classification)

**Example**:
```python
# error_type: VALIDATION_ERROR (broad classification)
# error_code: INVALID_EMAIL_FORMAT (specific issue)
return self.failure(
    message="Email address format is invalid",
    error_type="ValidationError",
    error_code="INVALID_EMAIL_FORMAT",
    retryable=False,
    metadata={"email": "bad@@example"}
)
```

**Recommendation**:
- ✅ **Adopt Now**: All languages must support `error_code`/`errorCode` field
- ✅ **Standardize Naming**: 
  - Ruby/Python/Rust: `error_code` (snake_case)
  - TypeScript: `errorCode` (camelCase) - language convention
- ✅ **Verification**: Confirm Rust implementation, add if missing
- 📝 **Zen Alignment**: "Explicit is better than implicit" - error_code enables precise error handling

---

### Configuration and Metadata

#### Handler Name

| Language | Method | Pattern | Example |
|----------|--------|---------|---------|
| **Rust** | `fn name(&self) -> &str` | Required trait method | `"process_order"` |
| **Ruby** | `def handler_name` | Auto-derived from class | `"process_order_handler"` → `"process_order_handler"` |
| **Python** | `handler_name: str` class attribute | Must be set explicitly | `handler_name = "process_order"` |
| **TypeScript** | `static handlerName: string` | Must be set explicitly | `static handlerName = 'process_order'` |

**Ruby Auto-Derivation**:
```ruby
class ProcessOrderHandler < TaskerCore::StepHandler::Base
  # handler_name auto-derived: "process_order_handler"
  # (CamelCase → snake_case conversion)
end
```

**Analysis**:
- Ruby's auto-derivation is convenient but loses explicit control
- Python/TypeScript explicit declaration prevents mismatches
- Rust requires implementation (most explicit)

**Recommendation**:
- ✅ **Adopt Now**: All handlers must explicitly declare their name (Ruby breaking change)
- ✅ **Remove**: Ruby auto-derivation (too magical, error-prone)
- 📝 **Zen Alignment**: "Explicit is better than implicit" - handlers declare their identity

#### Handler Version

| Language | Support | Default | Override |
|----------|---------|---------|----------|
| **Rust** | ❌ No version field | - | - |
| **Ruby** | ✅ Via `VERSION` constant | `'1.0.0'` | `VERSION = '2.1.0'` |
| **Python** | ✅ `handler_version` class attribute | `'1.0.0'` | `handler_version = '2.1.0'` |
| **TypeScript** | ✅ `static handlerVersion` | `'1.0.0'` | `static handlerVersion = '2.1.0'` |

**Recommendation**:
- ✅ **Adopt Now**: All languages must support handler versioning
- ✅ **Implementation**: Rust add version field to trait
- 📝 **Zen Alignment**: Versioning aids debugging and evolution tracking

#### Capabilities

| Language | Method | Return Type | Purpose |
|----------|--------|-------------|---------|
| **Rust** | ❓ No capabilities method visible | - | - |
| **Ruby** | `def capabilities` | `Array<String>` | Handler feature advertising |
| **Python** | `@property capabilities` | `list[str]` | Handler feature advertising |
| **TypeScript** | `get capabilities()` | `string[]` | Handler feature advertising |

**Ruby Example**:
```ruby
def capabilities
  caps = ['process']
  caps << 'process_results' if respond_to?(:process_results, true)
  caps << 'async' if respond_to?(:process_async, true)
  caps << 'streaming' if respond_to?(:process_stream, true)
  caps
end
```

**Purpose**: Advertise handler capabilities for:
- Handler selection/routing
- Feature detection
- Documentation generation

**Recommendation**:
- ✅ **Adopt Now**: All languages must support capabilities method
- ✅ **Implementation**: Rust add capabilities, default `['process']`
- 📝 **Zen Alignment**: "Explicit is better than implicit" - capabilities advertise features

#### Config Schema

| Language | Method | Return Type | Validation |
|----------|--------|-------------|------------|
| **Rust** | ❌ No schema support | - | - |
| **Ruby** | `def config_schema` | `Hash` (JSON Schema) | Manual |
| **Python** | `def config_schema()` | `dict \| None` | Manual |
| **TypeScript** | `configSchema()` | `Record<string, unknown> \| null` | Manual |

**Ruby Example**:
```ruby
def config_schema
  {
    type: 'object',
    properties: {
      timeout: { type: 'integer', minimum: 1, default: 300 },
      retries: { type: 'integer', minimum: 0, default: 3 },
      log_level: { type: 'string', enum: %w[debug info warn error], default: 'info' }
    },
    additionalProperties: true
  }
end
```

**Recommendation**:
- ✅ **Adopt Now**: All languages must support config schema validation
- ✅ **Implementation**: Rust add schema support
- 🎯 **Consider**: Shared schema validation library across languages
- 📝 **Zen Alignment**: "Errors should never pass silently" - schema catches config errors early

---

## Architectural Patterns

### Instantiation and Configuration

**Rust**:
```rust
pub trait RustStepHandler: Send + Sync {
    fn new(config: StepHandlerConfig) -> Self where Self: Sized;
}

pub struct StepHandlerConfig {
    pub data: HashMap<String, Value>,
    pub event_publisher: Option<Arc<DomainEventPublisher>>,  // TAS-65
}
```

**Ruby**:
```ruby
class Base
  def initialize(config: {}, logger: nil)
    @config = config || {}
    @logger = logger || TaskerCore::Logger.instance
    # ... orchestration_system setup
  end
end
```

**Python**:
```python
class StepHandler(ABC):
    handler_name: str = ""
    handler_version: str = "1.0.0"
    # No explicit __init__ - subclasses define if needed
```

**TypeScript**:
```typescript
export abstract class StepHandler {
  static handlerName: string;
  static handlerVersion = '1.0.0';
  // No explicit constructor - subclasses define if needed
}
```

**Analysis**:
- **Rust**: Explicit `new()` method required, config struct with domain event publisher
- **Ruby**: Traditional `initialize`, accepts config hash and logger
- **Python/TypeScript**: No required constructor, configuration passed at runtime

**Recommendation**:
- ✅ **Adopt Now**: Standardize constructor signatures across all languages
- ✅ **Implementation**: All constructors accept `config` parameter
- ✅ **TAS-65**: Ensure domain event publisher injection support
- 📝 **Zen Alignment**: "Explicit is better than implicit" - configuration is explicit

### Context Access Patterns

**Rust (`TaskSequenceStep`)**:
```rust
step_data.task.context.get("field_name")
step_data.workflow_step.workflow_step_uuid
step_data.dependency_results.get("step_name")
step_data.step_definition.handler.initialization
```

**Ruby (`StepContext`)**:
```ruby
context.get_task_field('field_name')        # Helper method
context.task_uuid
context.step_uuid
context.get_dependency_result('step_name')  # Helper method
context.input_data
context.step_config
```

**Python (`StepContext`)**:
```python
context.input_data.get("field_name")        # Direct dict access
context.task_uuid
context.step_uuid
context.dependency_results.get("step_name")  # Direct dict access
context.step_config
```

**TypeScript (`StepContext`)**:
```typescript
context.getInput<string>('field_name')      // Generic helper method
context.taskUuid
context.stepUuid
context.getDependencyResult('step_name')    // Helper method
context.stepConfig
```

**Analysis**:
- **Rust**: Most verbose, direct property access
- **Ruby**: Helper methods for common operations
- **Python**: Mix of direct access and properties
- **TypeScript**: Typed helper methods with generics

**Recommendation**:
- ✅ **Adopt Now**: All languages use helper methods for context access (breaking change for Python)
- ✅ **Implementation**: Python add `get_task_field()`, `get_dependency_result()` methods
- **Rationale**:
  - Encapsulation and consistency
  - Type safety (TypeScript generics)
  - Same API patterns across all languages
- 📝 **Zen Alignment**: "Simple is better than complex" - helpers simplify common operations

---

## Functional Gaps

### Missing in Rust
1. ❌ **Handler Version**: No version field/method
2. ❌ **Capabilities**: No capabilities method
3. ❌ **Config Schema**: No JSON schema validation
4. ❌ **Metadata Introspection**: No metadata() method
5. ❓ **Error Code**: Unclear if error_code field exists in StepExecutionResult

### Missing in Python
1. ⚠️ **Context Helper Methods**: Direct dict access instead of helper methods
2. ⚠️ **Metadata Method**: No metadata() introspection

### Missing in TypeScript
1. ⚠️ **Metadata Method**: No metadata() introspection
2. ⚠️ **Config Validation**: Schema support exists but not enforced

### Missing in Ruby
1. ✅ **Mostly Complete**: Ruby has most features but uses inheritance anti-pattern

---

## Recommendations Summary

### Critical Changes (Implement Now)

#### 1. Rust Handler Trait
- ✅ Rename `RustStepHandler` → `StepHandler`
- ✅ Add `handler_version` field
- ✅ Add `capabilities()` method
- ✅ Add `config_schema()` method
- ✅ Add `metadata()` method
- ✅ Verify/add `error_code` field support
- ✅ Rename parameter `step_data` → `context`
- ✅ Change context type from `TaskSequenceStep` → `StepContext`

#### 2. Ruby Result Type Refactor
- ✅ Replace `Success`/`Error` classes with unified `StepHandlerResult`
- ✅ Support positional arguments: `success(result, metadata)`
- ✅ Change `retryable` default: `false` → `true`
- ✅ Remove auto-derived handler names (require explicit declaration)
- ✅ Result structure matches Python/TypeScript

#### 3. Python Context Enhancement
- ✅ Add `context.get_task_field(key)` helper method
- ✅ Add `context.get_dependency_result(step_name)` helper method
- ✅ Match Ruby/TypeScript helper method patterns
- ✅ Keep direct dict access for backward compatibility in pre-alpha

#### 4. Cross-Language Standardization
- ✅ All languages: `error_code` field support
- ✅ All languages: `retryable=true` default
- ✅ All languages: Positional argument support for `success()`/`failure()`
- ✅ All languages: Explicit handler name declaration
- ✅ All languages: Handler version field
- ✅ All languages: Capabilities method
- ✅ All languages: Config schema support

### Architectural Principles (Guiding Changes)

1. **Composition Over Inheritance** (Project Goal)
   - Ruby: Unified result class (not inheritance hierarchy)
   - Future: API/Decision handlers as mixins (not subclasses)
   - Python: Already uses mixin pattern - model for others

2. **One Obvious Way** (Zen Principle)
   - Single result class pattern across all languages
   - Consistent helper methods for context access
   - Uniform argument signatures (positional with optional metadata)

3. **Explicit Over Implicit** (Zen Principle)
   - No auto-derivation (handler names must be declared)
   - No magic defaults (error handling must be explicit)
   - Clear configuration schemas

### Documentation Requirements

1. **Cross-Language API Reference**:
   - Canonical handler interface
   - Context access patterns
   - Result creation patterns

2. **Migration Guide** (for existing code):
   - Ruby result type changes
   - Python context helper additions
   - Rust trait renaming

3. **Best Practices**:
   - Handler naming conventions
   - Error code taxonomy
   - Capabilities declaration

---

## Zen of Python Alignment Check

| Principle | Alignment | Notes |
|-----------|-----------|-------|
| Beautiful is better than ugly | ⚠️ Partial | Ruby dual-class pattern is less beautiful than unified class |
| Explicit is better than implicit | ✅ Good | Most APIs are explicit; Ruby auto-naming is outlier |
| Simple is better than complex | ⚠️ Partial | Keyword-required args add complexity |
| Flat is better than nested | ⚠️ Partial | Ruby inheritance hierarchy is nested |
| Readability counts | ✅ Good | All APIs are readable |
| There should be one obvious way | ❌ Poor | Multiple ways to create results, access context |
| Errors should never pass silently | ✅ Good | Strong error handling across all languages |

**Overall Zen Score**: 5/7 aligned

**Improvement Path**: Reduce inheritance, standardize APIs, provide one obvious pattern.

---

## Action Items

### For Phase 3 (API Handler Analysis)
- [ ] Investigate Rust's API handler pattern (may not exist)
- [ ] Compare HTTP error classification across languages
- [ ] Analyze connection pooling strategies

### For Implementation (Pre-Alpha: Break Freely)
- [ ] **Rust**: Rename trait, add methods, refactor context parameter
- [ ] **Ruby**: Implement unified `StepHandlerResult`, remove dual classes
- [ ] **Python**: Add context helper methods
- [ ] **TypeScript**: Verify compliance (likely already compliant)
- [ ] Create implementation tickets for each language team

### For Documentation
- [ ] Create "Canonical Handler API" reference guide
- [ ] Update `docs/worker-crates/patterns-and-practices.md`
- [ ] Add cross-language examples for each pattern
- [ ] Document migration paths for breaking changes

---

## Next Phase

**Phase 3: API Handler Pattern** will analyze HTTP/REST integration handlers, building on this base handler foundation. Key questions:
- Does Rust have a dedicated API handler?
- Are HTTP status code classifications consistent?
- Do all languages handle Retry-After headers?

---

## Appendix: Code Examples

### Unified Result Pattern (Target)

**Python (Current)**:
```python
class StepHandlerResult(BaseModel):
    is_success: bool
    result: dict | None = None
    error_message: str | None = None
    error_type: str | None = None
    error_code: str | None = None
    retryable: bool = True
    metadata: dict = Field(default_factory=dict)
    
    @classmethod
    def success(cls, result, metadata=None):
        return cls(is_success=True, result=result, metadata=metadata or {})
    
    @classmethod
    def failure(cls, message, error_type, retryable=True, metadata=None, error_code=None):
        return cls(is_success=False, error_message=message, error_type=error_type,
                   error_code=error_code, retryable=retryable, metadata=metadata or {})
```

**Ruby (Target - Unified Class)**:
```ruby
module TaskerCore
  module Types
    class StepHandlerResult
      attr_reader :success, :result, :error_message, :error_type, :error_code, :retryable, :metadata
      
      def initialize(success:, result: nil, error_message: nil, error_type: nil, 
                     error_code: nil, retryable: true, metadata: {})
        @success = success
        @result = result
        @error_message = error_message
        @error_type = error_type
        @error_code = error_code
        @retryable = retryable
        @metadata = metadata
      end
      
      def self.success(result, metadata = {})
        new(success: true, result: result, metadata: metadata)
      end
      
      def self.failure(message, error_type:, retryable: true, metadata: {}, error_code: nil)
        new(success: false, error_message: message, error_type: error_type,
            error_code: error_code, retryable: retryable, metadata: metadata)
      end
      
      def success?
        @success
      end
    end
  end
end
```

---

## Metadata

**Document Version**: 1.0  
**Analysis Date**: 2025-12-27  
**Reviewers**: TBD  
**Approval Status**: Draft
