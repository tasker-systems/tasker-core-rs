# TAS-36: TaskTemplate Redesign - Self-Describing Workflow Configuration

## Executive Summary

This specification details a comprehensive redesign of the TaskTemplate structure to create a more self-describing, loosely coupled, and intuitive workflow configuration system. The redesign eliminates rigid handler class requirements, removes redundant naming conventions, and introduces structured configuration patterns while maintaining all essential functionality.

## Problem Statement

The current TaskTemplate structure has several limitations:

1. **Tight Handler Coupling**: The `handler_class` field assumes class-based handlers, excluding procs, lambdas, and other callables
2. **Unclear Concepts**: `dependent_system` is poorly documented despite being crucial for single responsibility
3. **Unstructured Configuration**: Open-ended `handler_config` lacks intuitive structure
4. **Redundant Prefixes**: Fields like `default_retryable` and `default_retry_limit` have unnecessary prefixes
5. **Misplaced Features**: `skippable` belongs in event subscribers, not workflow steps
6. **Incomplete Event Integration**: Domain events are partially implemented but not well integrated

## Design Goals

1. **Flexibility**: Support any callable with `.call(task, sequence, step)` signature
2. **Clarity**: Self-documenting field names and structures
3. **Extensibility**: Structured sub-objects for future enhancements
4. **Single Responsibility**: Clear system dependency declarations
5. **Event-First**: First-class support for domain events
6. **Environment Awareness**: Improved environment-specific overrides

## Proposed Solution

### New TaskTemplate YAML Structure

```yaml
# Example: Credit Card Payment Processing
name: credit_card_payment
namespace_name: payments
version: "1.0.0"
description: "Process credit card payments with validation and fraud detection"

# Metadata for documentation and discovery
metadata:
  author: "payments-team"
  tags: ["payment", "critical", "financial"]
  documentation_url: "https://docs.example.com/payments"
  created_at: "2024-01-01T00:00:00Z"
  updated_at: "2024-01-15T00:00:00Z"

# Task-level handler configuration
task_handler:
  callable: "PaymentProcessing::CreditCardPaymentHandler"  # Class, proc, or lambda reference
  initialization:  # Structured initialization parameters
    timeout_ms: 30000
    retry_strategy: "exponential_backoff"
    max_concurrency: 10
    circuit_breaker:
      failure_threshold: 5
      reset_timeout_ms: 60000

# External system dependencies
system_dependencies:
  primary: "payment_gateway"  # Main system this task interacts with
  secondary:  # Additional systems
    - "fraud_detection_api"
    - "customer_database"
    - "notification_service"
  
# Domain events this task publishes
domain_events:
  - name: "payment.authorized"
    description: "Payment successfully authorized by gateway"
    schema:  # JSON Schema for event payload validation
      type: object
      required: ["order_id", "authorization_id", "amount", "currency"]
      properties:
        order_id: 
          type: "integer"
          description: "Unique order identifier"
        authorization_id: 
          type: "string"
          description: "Gateway authorization code"
        amount: 
          type: "number"
          minimum: 0.01
        currency:
          type: "string"
          enum: ["USD", "EUR", "GBP"]
  
  - name: "payment.declined"
    description: "Payment was declined"
    schema:
      type: object
      required: ["order_id", "reason", "decline_code"]
      properties:
        order_id: { type: "integer" }
        reason: { type: "string" }
        decline_code: { type: "string" }

# Input validation schema (JSON Schema)
input_schema:
  type: object
  required: ["card_number", "cvv", "amount", "currency", "customer_id"]
  properties:
    card_number: 
      type: "string"
      pattern: "^[0-9]{13,19}$"
      description: "Credit card number"
    cvv: 
      type: "string"
      pattern: "^[0-9]{3,4}$"
      description: "Card verification value"
    amount: 
      type: "number"
      minimum: 0.01
      maximum: 1000000
    currency: 
      type: "string"
      enum: ["USD", "EUR", "GBP"]
    customer_id:
      type: "integer"

# Workflow step definitions
steps:
  - name: validate_payment
    description: "Validate payment information and card status"
    handler:
      callable: "PaymentProcessing::ValidationHandler"
      initialization:
        validation_rules: 
          - "check_card_expiry"
          - "validate_cvv"
          - "check_amount_limits"
        max_amount: 10000
        min_amount: 0.01
    system_dependency: "payment_gateway"  # Which system this step interacts with
    retry:
      retryable: true
      limit: 3
      backoff: "exponential"
      backoff_base_ms: 1000
      max_backoff_ms: 30000
    timeout_seconds: 30

  - name: check_fraud
    description: "Run fraud detection algorithms"
    dependencies: ["validate_payment"]  # Unified dependency field
    handler:
      callable: "PaymentProcessing::FraudCheckHandler"
      initialization:
        risk_threshold: 0.8
        ml_model_version: "2.1.0"
        feature_flags:
          use_ml_model: true
          use_rule_engine: true
    system_dependency: "fraud_detection_api"
    retry:
      retryable: true
      limit: 2
      backoff: "linear"
      backoff_base_ms: 2000
    timeout_seconds: 60
    publishes_events:  # Events this step can publish
      - "fraud.check_completed"
      - "fraud.high_risk_detected"

  - name: authorize_payment
    description: "Authorize payment with gateway"
    dependencies: ["validate_payment", "check_fraud"]  # Multiple dependencies
    handler:
      callable: "PaymentProcessing::AuthorizationHandler"
      initialization:
        gateway_endpoint: "https://api.gateway.com/v2/authorize"
        api_version: "2023-11"
        timeout_ms: 30000
    system_dependency: "payment_gateway"
    retry:
      retryable: true
      limit: 3
      backoff: "exponential"
      backoff_base_ms: 1000
    timeout_seconds: 120
    publishes_events:
      - "payment.authorized"
      - "payment.declined"

  - name: capture_payment
    description: "Capture the authorized payment"
    dependencies: ["authorize_payment"]
    handler:
      callable: "PaymentProcessing::CaptureHandler"
      initialization:
        auto_capture: true
        capture_delay_seconds: 0
    system_dependency: "payment_gateway"
    retry:
      retryable: true
      limit: 5
      backoff: "fibonacci"
      backoff_base_ms: 1000
    timeout_seconds: 120
    publishes_events:
      - "payment.captured"
      - "payment.capture_failed"

  - name: send_confirmation
    description: "Send payment confirmation to customer"
    dependencies: ["capture_payment"]
    handler:
      callable: "PaymentProcessing::NotificationHandler"
      initialization:
        template_id: "payment_confirmation_v2"
        channels: ["email", "sms"]
    system_dependency: "notification_service"
    retry:
      retryable: true
      limit: 3
      backoff: "exponential"
    timeout_seconds: 30
    publishes_events:
      - "notification.sent"
      - "notification.failed"

# Environment-specific overrides
environments:
  development:
    task_handler:
      initialization:
        debug_mode: true
        timeout_ms: 60000
        log_level: "debug"
    steps:
      - name: check_fraud
        handler:
          initialization:
            risk_threshold: 0.5  # Lower threshold in dev
            use_ml_model: false  # Skip ML in dev
            use_rule_engine: true
      - name: authorize_payment
        handler:
          initialization:
            gateway_endpoint: "https://sandbox.gateway.com/v2/authorize"
            use_test_credentials: true
    
  test:
    task_handler:
      initialization:
        timeout_ms: 5000  # Faster timeouts for tests
    steps:
      - name: ALL  # Special keyword to apply to all steps
        timeout_seconds: 10
        retry:
          limit: 1
          backoff: "none"
  
  staging:
    task_handler:
      initialization:
        max_concurrency: 50
        enable_metrics: true
    steps:
      - name: check_fraud
        handler:
          initialization:
            risk_threshold: 0.7
            ml_model_version: "2.1.0-staging"
  
  production:
    task_handler:
      initialization:
        max_concurrency: 100
        enable_metrics: true
        enable_tracing: true
    steps:
      - name: check_fraud
        handler:
          initialization:
            risk_threshold: 0.9  # Strictest in production
            ml_model_version: "2.1.0-prod"
            feature_flags:
              use_ml_model: true
              use_rule_engine: true
              use_graph_analysis: true
```

### Rust Model Implementation

```rust
// src/models/core/task_template.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Complete task template with all workflow configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    /// Unique task name within namespace
    pub name: String,
    
    /// Namespace for organization
    pub namespace_name: String,
    
    /// Semantic version
    pub version: String,
    
    /// Human-readable description
    pub description: Option<String>,
    
    /// Template metadata for documentation
    pub metadata: Option<TemplateMetadata>,
    
    /// Task-level handler configuration
    pub task_handler: Option<HandlerDefinition>,
    
    /// External system dependencies
    pub system_dependencies: SystemDependencies,
    
    /// Domain events this task can publish
    pub domain_events: Vec<DomainEventDefinition>,
    
    /// JSON Schema for input validation
    pub input_schema: Option<Value>,
    
    /// Workflow step definitions
    pub steps: Vec<StepDefinition>,
    
    /// Environment-specific overrides
    pub environments: HashMap<String, EnvironmentOverride>,
}

/// Template metadata for documentation and discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub documentation_url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Handler definition with callable and initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerDefinition {
    /// Callable reference (class, proc, lambda)
    pub callable: String,
    
    /// Initialization parameters
    pub initialization: HashMap<String, Value>,
}

/// External system dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDependencies {
    /// Primary system interaction
    #[serde(default = "default_system")]
    pub primary: String,
    
    /// Secondary systems
    #[serde(default)]
    pub secondary: Vec<String>,
}

fn default_system() -> String {
    "default".to_string()
}

/// Domain event definition with schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEventDefinition {
    pub name: String,
    pub description: Option<String>,
    pub schema: Option<Value>,  // JSON Schema
}

/// Individual workflow step definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDefinition {
    pub name: String,
    pub description: Option<String>,
    
    /// Handler for this step
    pub handler: HandlerDefinition,
    
    /// System this step interacts with
    pub system_dependency: Option<String>,
    
    /// Dependencies on other steps
    #[serde(default)]
    pub dependencies: Vec<String>,
    
    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfiguration,
    
    /// Step timeout
    pub timeout_seconds: Option<u32>,
    
    /// Events this step publishes
    #[serde(default)]
    pub publishes_events: Vec<String>,
}

/// Retry configuration with backoff strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfiguration {
    #[serde(default = "default_retryable")]
    pub retryable: bool,
    
    #[serde(default = "default_retry_limit")]
    pub limit: u32,
    
    #[serde(default)]
    pub backoff: BackoffStrategy,
    
    pub backoff_base_ms: Option<u64>,
    pub max_backoff_ms: Option<u64>,
}

impl Default for RetryConfiguration {
    fn default() -> Self {
        Self {
            retryable: true,
            limit: 3,
            backoff: BackoffStrategy::Exponential,
            backoff_base_ms: Some(1000),
            max_backoff_ms: Some(30000),
        }
    }
}

fn default_retryable() -> bool { true }
fn default_retry_limit() -> u32 { 3 }

/// Backoff strategies for retries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    None,
    Linear,
    #[default]
    Exponential,
    Fibonacci,
}

/// Environment-specific overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentOverride {
    pub task_handler: Option<HandlerOverride>,
    pub steps: Vec<StepOverride>,
}

/// Handler override for environments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerOverride {
    pub initialization: Option<HashMap<String, Value>>,
}

/// Step override for environments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOverride {
    /// Step name or "ALL" for all steps
    pub name: String,
    pub handler: Option<HandlerOverride>,
    pub timeout_seconds: Option<u32>,
    pub retry: Option<RetryConfiguration>,
}

impl TaskTemplate {
    /// Create from YAML string
    pub fn from_yaml(yaml_str: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml_str)
    }
    
    /// Create from YAML file
    pub fn from_yaml_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        Ok(Self::from_yaml(&contents)?)
    }
    
    /// Resolve template for specific environment
    pub fn resolve_for_environment(&self, environment: &str) -> ResolvedTaskTemplate {
        let mut resolved = self.clone();
        
        if let Some(env_override) = self.environments.get(environment) {
            // Apply task handler overrides
            if let Some(handler_override) = &env_override.task_handler {
                if let Some(task_handler) = &mut resolved.task_handler {
                    if let Some(init_override) = &handler_override.initialization {
                        task_handler.initialization.extend(init_override.clone());
                    }
                }
            }
            
            // Apply step overrides
            for step_override in &env_override.steps {
                if step_override.name == "ALL" {
                    // Apply to all steps
                    for step in &mut resolved.steps {
                        apply_step_override(step, step_override);
                    }
                } else {
                    // Apply to specific step
                    if let Some(step) = resolved.steps.iter_mut().find(|s| s.name == step_override.name) {
                        apply_step_override(step, step_override);
                    }
                }
            }
        }
        
        ResolvedTaskTemplate {
            template: resolved,
            environment: environment.to_string(),
            resolved_at: Utc::now(),
        }
    }
    
    /// Extract all callable references
    pub fn all_callables(&self) -> Vec<String> {
        let mut callables = Vec::new();
        
        if let Some(handler) = &self.task_handler {
            callables.push(handler.callable.clone());
        }
        
        for step in &self.steps {
            callables.push(step.handler.callable.clone());
        }
        
        callables
    }
    
    /// Validate template structure
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        
        // Validate version format
        if !self.version.chars().filter(|c| *c == '.').count() == 2 {
            errors.push("Version must be in semver format (x.y.z)".to_string());
        }
        
        // Validate step dependencies exist
        let step_names: Vec<_> = self.steps.iter().map(|s| &s.name).collect();
        for step in &self.steps {
            for dep in &step.dependencies {
                if !step_names.contains(&&dep.as_str()) {
                    errors.push(format!("Step '{}' depends on non-existent step '{}'", step.name, dep));
                }
            }
        }
        
        // Validate no circular dependencies
        if let Err(e) = self.validate_no_circular_dependencies() {
            errors.push(e);
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    fn validate_no_circular_dependencies(&self) -> Result<(), String> {
        // Build dependency graph and check for cycles
        // Implementation details omitted for brevity
        Ok(())
    }
}

fn apply_step_override(step: &mut StepDefinition, override_def: &StepOverride) {
    if let Some(handler_override) = &override_def.handler {
        if let Some(init_override) = &handler_override.initialization {
            step.handler.initialization.extend(init_override.clone());
        }
    }
    
    if let Some(timeout) = override_def.timeout_seconds {
        step.timeout_seconds = Some(timeout);
    }
    
    if let Some(retry) = &override_def.retry {
        step.retry = retry.clone();
    }
}

/// Resolved template for a specific environment
#[derive(Debug, Clone)]
pub struct ResolvedTaskTemplate {
    pub template: TaskTemplate,
    pub environment: String,
    pub resolved_at: DateTime<Utc>,
}
```

### Ruby Type Implementation

```ruby
# bindings/ruby/lib/tasker_core/types/task_template.rb

require 'dry-types'
require 'dry-struct'

module TaskerCore
  module Types
    # Include dry-types
    include Dry.Types()

    # Template metadata for documentation
    class TemplateMetadata < Dry::Struct
      attribute? :author, Types::String.optional
      attribute :tags, Types::Array.of(Types::String).default([].freeze)
      attribute? :documentation_url, Types::String.optional
      attribute? :created_at, Types::String.optional
      attribute? :updated_at, Types::String.optional
    end

    # Handler definition with callable and initialization
    class HandlerDefinition < Dry::Struct
      attribute :callable, Types::Strict::String
      attribute :initialization, Types::Hash.default({}.freeze)
      
      def to_h
        {
          callable: callable,
          initialization: initialization
        }
      end
    end

    # System dependencies
    class SystemDependencies < Dry::Struct
      attribute :primary, Types::Strict::String.default('default')
      attribute :secondary, Types::Array.of(Types::String).default([].freeze)
      
      def all_systems
        [primary] + secondary
      end
    end

    # Domain event definition
    class DomainEventDefinition < Dry::Struct
      attribute :name, Types::Strict::String
      attribute? :description, Types::String.optional
      attribute? :schema, Types::Hash.optional
      
      def validate_payload(payload)
        return true unless schema
        # JSON Schema validation logic here
        true
      end
    end

    # Retry configuration
    class RetryConfiguration < Dry::Struct
      BACKOFF_STRATEGIES = %w[none linear exponential fibonacci].freeze
      
      attribute :retryable, Types::Bool.default(true)
      attribute :limit, Types::Integer.default(3)
      attribute :backoff, Types::String.enum(*BACKOFF_STRATEGIES).default('exponential')
      attribute? :backoff_base_ms, Types::Integer.optional.default(1000)
      attribute? :max_backoff_ms, Types::Integer.optional.default(30000)
      
      def calculate_backoff(attempt)
        return 0 if backoff == 'none'
        
        base = backoff_base_ms || 1000
        
        case backoff
        when 'linear'
          base * attempt
        when 'exponential'
          base * (2 ** (attempt - 1))
        when 'fibonacci'
          fib(attempt) * base
        else
          base
        end.tap do |delay|
          if max_backoff_ms && delay > max_backoff_ms
            return max_backoff_ms
          end
        end
      end
      
      private
      
      def fib(n)
        return 1 if n <= 2
        fib(n - 1) + fib(n - 2)
      end
    end

    # Step definition
    class StepDefinition < Dry::Struct
      attribute :name, Types::Strict::String
      attribute? :description, Types::String.optional
      attribute :handler, HandlerDefinition
      attribute? :system_dependency, Types::String.optional.default('default')
      attribute :dependencies, Types::Array.of(Types::String).default([].freeze)
      attribute :retry, RetryConfiguration.default { RetryConfiguration.new }
      attribute? :timeout_seconds, Types::Integer.optional.default(30)
      attribute :publishes_events, Types::Array.of(Types::String).default([].freeze)
      
      def depends_on?(other_step_name)
        dependencies.include?(other_step_name)
      end
      
      def can_retry?(attempt_count)
        retry.retryable && attempt_count < retry.limit
      end
    end

    # Handler override for environments
    class HandlerOverride < Dry::Struct
      attribute? :initialization, Types::Hash.optional
    end

    # Step override for environments
    class StepOverride < Dry::Struct
      attribute :name, Types::Strict::String
      attribute? :handler, HandlerOverride.optional
      attribute? :timeout_seconds, Types::Integer.optional
      attribute? :retry, RetryConfiguration.optional
    end

    # Environment override
    class EnvironmentOverride < Dry::Struct
      attribute? :task_handler, HandlerOverride.optional
      attribute :steps, Types::Array.of(StepOverride).default([].freeze)
    end

    # Main TaskTemplate structure
    class TaskTemplate < Dry::Struct
      VERSION_PATTERN = /\A\d+\.\d+\.\d+\z/

      attribute :name, Types::Strict::String
      attribute :namespace_name, Types::Strict::String
      attribute :version, Types::String.constrained(format: VERSION_PATTERN).default('1.0.0')
      attribute? :description, Types::String.optional
      
      attribute? :metadata, TemplateMetadata.optional
      attribute? :task_handler, HandlerDefinition.optional
      attribute :system_dependencies, SystemDependencies.default { SystemDependencies.new }
      attribute :domain_events, Types::Array.of(DomainEventDefinition).default([].freeze)
      attribute? :input_schema, Types::Hash.optional
      attribute :steps, Types::Array.of(StepDefinition).default([].freeze)
      attribute :environments, Types::Hash.map(
        Types::String, 
        EnvironmentOverride
      ).default({}.freeze)
      
      # Generate unique template key
      def template_key
        "#{namespace_name}/#{name}:#{version}"
      end
      
      # Extract all callables
      def all_callables
        callables = []
        callables << task_handler.callable if task_handler
        steps.each { |step| callables << step.handler.callable }
        callables.uniq
      end
      
      # Check if valid for registration
      def valid_for_registration?
        return false if name.empty? || namespace_name.empty?
        return false unless version.match?(VERSION_PATTERN)
        
        # All steps must have handlers
        steps.all? { |step| !step.handler.callable.empty? }
      end
      
      # Resolve for environment
      def resolve_for_environment(environment)
        resolved = self.class.new(attributes.deep_dup)
        
        if env_override = environments[environment.to_s]
          # Apply task handler overrides
          if env_override.task_handler && resolved.task_handler
            resolved = resolved.class.new(
              resolved.attributes.merge(
                task_handler: merge_handler(resolved.task_handler, env_override.task_handler)
              )
            )
          end
          
          # Apply step overrides
          resolved_steps = resolved.steps.map do |step|
            step_override = env_override.steps.find { |so| so.name == step.name || so.name == 'ALL' }
            step_override ? merge_step(step, step_override) : step
          end
          
          resolved = resolved.class.new(
            resolved.attributes.merge(steps: resolved_steps)
          )
        end
        
        resolved
      end
      
      private
      
      def merge_handler(original, override)
        return original unless override.initialization
        
        HandlerDefinition.new(
          callable: original.callable,
          initialization: original.initialization.merge(override.initialization)
        )
      end
      
      def merge_step(original, override)
        attrs = original.attributes.dup
        
        if override.handler
          attrs[:handler] = merge_handler(original.handler, override.handler)
        end
        
        attrs[:timeout_seconds] = override.timeout_seconds if override.timeout_seconds
        attrs[:retry] = override.retry if override.retry
        
        StepDefinition.new(attrs)
      end
    end
  end
end
```

## Implementation Plan

### Phase 1: Core Model Updates (Days 1-3)

#### Day 1: Rust Model Implementation
- [ ] Update `src/models/core/task_template.rs` with new structure
- [ ] Add validation methods for circular dependencies
- [ ] Implement environment resolution logic
- [ ] Add serialization/deserialization tests

#### Day 2: Ruby Type Implementation  
- [ ] Update `bindings/ruby/lib/tasker_core/types/task_template.rb`
- [ ] Implement retry backoff calculations
- [ ] Add environment resolution methods
- [ ] Create dry-struct validations

#### Day 3: Migration Utilities
- [ ] Create YAML migration script for existing templates
- [ ] Build validation tool for new format
- [ ] Document migration process

### Phase 2: Registry Updates (Days 4-6)

#### Day 4: Rust Registry Updates
- [ ] Update `src/registry/task_handler_registry.rs` for callable support
- [ ] Modify `get_task_template` to handle new structure
- [ ] Update `resolve_handler` for new handler definition
- [ ] Add callable type detection

#### Day 5: Ruby Registry Updates
- [ ] Update `bindings/ruby/lib/tasker_core/registry/task_template_registry.rb`
- [ ] Modify `build_database_configuration` for new structure
- [ ] Update `register_task_template` validation
- [ ] Enhance environment override application

#### Day 6: Step Handler Resolver
- [ ] Update `bindings/ruby/lib/tasker_core/registry/step_handler_resolver.rb`
- [ ] Implement `resolve_callable` to replace `resolve_handler_class`
- [ ] Add support for proc and lambda detection
- [ ] Update handler instantiation logic

### Phase 3: Database & Persistence (Days 7-8)

#### Day 7: Database Schema Updates
- [ ] Update `named_task` configuration storage format
- [ ] Modify JSON serialization for new structure
- [ ] Add indexes for new fields if needed
- [ ] Update migration scripts

#### Day 8: Configuration Persistence
- [ ] Update `TaskNamespace` to store system dependencies
- [ ] Modify configuration retrieval queries
- [ ] Add domain event storage support
- [ ] Test configuration round-trip

### Phase 4: Configuration Migration (Days 9-11)

#### Day 9: Example Templates
- [ ] Convert `mathematical_sequence.yaml`
- [ ] Convert `credit_card_payment.yaml`
- [ ] Convert `linear_workflow_handler.yaml`
- [ ] Convert all test fixtures

#### Day 10: Documentation Updates
- [ ] Update TaskTemplate documentation
- [ ] Create migration guide
- [ ] Document new fields and concepts
- [ ] Add best practices guide

#### Day 11: Test Data Migration
- [ ] Update all test YAML files
- [ ] Modify integration test expectations
- [ ] Update example configurations
- [ ] Verify all tests pass

### Phase 5: Testing & Validation (Days 12-14)

#### Day 12: Unit Tests
- [ ] Test new model structures
- [ ] Validate serialization/deserialization
- [ ] Test environment resolution
- [ ] Verify callable resolution

#### Day 13: Integration Tests
- [ ] End-to-end workflow tests with new structure
- [ ] Test all callable types (class, proc, lambda)
- [ ] Verify environment overrides work correctly
- [ ] Test domain event publishing

#### Day 14: Performance & Edge Cases
- [ ] Benchmark callable resolution performance
- [ ] Test circular dependency detection
- [ ] Validate large template handling
- [ ] Test error scenarios

## Migration Guide

### For Existing Templates

#### 1. Handler Class → Callable
```yaml
# OLD
task_handler_class: "PaymentHandler"
handler_class: "StepHandler"

# NEW
task_handler:
  callable: "PaymentHandler"
handler:
  callable: "StepHandler"
```

#### 2. Handler Config → Initialization
```yaml
# OLD
handler_config:
  timeout: 30
  retries: 3

# NEW
handler:
  initialization:
    timeout: 30
    retries: 3
```

#### 3. Dependencies Unification
```yaml
# OLD
depends_on_step: "step1"
depends_on_steps: ["step2", "step3"]

# NEW
dependencies: ["step1", "step2", "step3"]
```

#### 4. Retry Configuration
```yaml
# OLD
default_retryable: true
default_retry_limit: 3

# NEW
retry:
  retryable: true
  limit: 3
  backoff: "exponential"
```

####

## Schema Validation & IDE Support

### JSON Schema for TaskTemplate YAML

#### Schema Header for TaskTemplate YAML Files
Every TaskTemplate YAML file should begin with the following metadata header for IDE support and validation:

```yaml
# yaml-language-server: $schema=https://tasker.systems/schemas/v1/task-template.json
# tasker-schema-version: 1.0.0
# tasker-template-version: 1.0.0
---
name: credit_card_payment
namespace_name: payments
# ... rest of template
```

#### TaskTemplate JSON Schema Definition
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://tasker.systems/schemas/v1/task-template.json",
  "title": "TaskTemplate",
  "description": "Schema for Tasker workflow task templates",
  "type": "object",
  "required": ["name", "namespace_name", "version", "steps"],
  "properties": {
    "name": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9_]*$",
      "description": "Unique task template name within namespace"
    },
    "namespace_name": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9_]*$",
      "description": "Namespace for logical grouping"
    },
    "version": {
      "type": "string",
      "pattern": "^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)(?:-((?:0|[1-9]\\d*|\\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\\.(?:0|[1-9]\\d*|\\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\\+([0-9a-zA-Z-]+(?:\\.[0-9a-zA-Z-]+)*))?$",
      "description": "Semantic version of the template"
    },
    "description": {
      "type": "string",
      "description": "Human-readable description of the task"
    },
    "metadata": {
      "$ref": "#/definitions/TemplateMetadata"
    },
    "task_handler": {
      "$ref": "#/definitions/HandlerDefinition"
    },
    "system_dependencies": {
      "$ref": "#/definitions/SystemDependencies"
    },
    "domain_events": {
      "type": "array",
      "items": {
        "$ref": "#/definitions/DomainEventDefinition"
      }
    },
    "input_schema": {
      "$ref": "#/definitions/JSONSchema"
    },
    "steps": {
      "type": "array",
      "minItems": 1,
      "items": {
        "$ref": "#/definitions/StepDefinition"
      }
    },
    "environments": {
      "type": "object",
      "additionalProperties": {
        "$ref": "#/definitions/EnvironmentOverride"
      }
    }
  },
  "definitions": {
    "TemplateMetadata": {
      "type": "object",
      "properties": {
        "author": {
          "type": "string"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          }
        },
        "documentation_url": {
          "type": "string",
          "format": "uri"
        },
        "created_at": {
          "type": "string",
          "format": "date-time"
        },
        "updated_at": {
          "type": "string",
          "format": "date-time"
        }
      }
    },
    "HandlerDefinition": {
      "type": "object",
      "required": ["callable"],
      "properties": {
        "callable": {
          "type": "string",
          "pattern": "^[A-Z][A-Za-z0-9]*(::[A-Z][A-Za-z0-9]*)*$",
          "description": "Fully qualified class or module path"
        },
        "initialization": {
          "type": "object",
          "description": "Handler-specific initialization parameters"
        }
      }
    },
    "SystemDependencies": {
      "type": "object",
      "properties": {
        "primary": {
          "type": "string",
          "enum": ["default", "payments", "inventory", "notifications", "analytics"],
          "default": "default"
        },
        "secondary": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": ["default", "payments", "inventory", "notifications", "analytics"]
          },
          "uniqueItems": true
        }
      }
    },
    "DomainEventDefinition": {
      "type": "object",
      "required": ["name"],
      "properties": {
        "name": {
          "type": "string",
          "pattern": "^[a-z][a-z0-9_]*$"
        },
        "description": {
          "type": "string"
        },
        "schema": {
          "$ref": "#/definitions/JSONSchema"
        }
      }
    },
    "StepDefinition": {
      "type": "object",
      "required": ["name", "handler"],
      "properties": {
        "name": {
          "type": "string",
          "pattern": "^[a-z][a-z0-9_]*$"
        },
        "description": {
          "type": "string"
        },
        "handler": {
          "$ref": "#/definitions/HandlerDefinition"
        },
        "system_dependency": {
          "type": "string",
          "enum": ["default", "payments", "inventory", "notifications", "analytics"]
        },
        "dependencies": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "uniqueItems": true
        },
        "retry": {
          "$ref": "#/definitions/RetryConfiguration"
        },
        "timeout_seconds": {
          "type": "integer",
          "minimum": 1,
          "maximum": 3600
        },
        "publishes_events": {
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      }
    },
    "RetryConfiguration": {
      "type": "object",
      "properties": {
        "retryable": {
          "type": "boolean",
          "default": true
        },
        "limit": {
          "type": "integer",
          "minimum": 0,
          "maximum": 10,
          "default": 3
        },
        "backoff": {
          "type": "string",
          "enum": ["none", "linear", "exponential", "fibonacci"],
          "default": "exponential"
        },
        "backoff_base_ms": {
          "type": "integer",
          "minimum": 100,
          "default": 1000
        },
        "max_backoff_ms": {
          "type": "integer",
          "minimum": 1000,
          "default": 30000
        }
      }
    },
    "EnvironmentOverride": {
      "type": "object",
      "properties": {
        "task_handler": {
          "$ref": "#/definitions/HandlerOverride"
        },
        "steps": {
          "type": "array",
          "items": {
            "$ref": "#/definitions/StepOverride"
          }
        }
      }
    },
    "HandlerOverride": {
      "type": "object",
      "properties": {
        "initialization": {
          "type": "object"
        }
      }
    },
    "StepOverride": {
      "type": "object",
      "required": ["name"],
      "properties": {
        "name": {
          "type": "string",
          "pattern": "^([a-z][a-z0-9_]*|ALL)$"
        },
        "handler": {
          "$ref": "#/definitions/HandlerOverride"
        },
        "timeout_seconds": {
          "type": "integer",
          "minimum": 1
        },
        "retry": {
          "$ref": "#/definitions/RetryConfiguration"
        }
      }
    },
    "JSONSchema": {
      "type": "object",
      "properties": {
        "type": {
          "type": "string",
          "enum": ["object", "array", "string", "number", "integer", "boolean", "null"]
        },
        "properties": {
          "type": "object"
        },
        "required": {
          "type": "array",
          "items": {
            "type": "string"
          }
        },
        "items": {
          "type": "object"
        },
        "enum": {
          "type": "array"
        },
        "pattern": {
          "type": "string"
        },
        "minimum": {
          "type": "number"
        },
        "maximum": {
          "type": "number"
        },
        "minLength": {
          "type": "integer"
        },
        "maxLength": {
          "type": "integer"
        },
        "description": {
          "type": "string"
        }
      }
    }
  }
}
```

### OpenAPI 3.0 Specification

#### OpenAPI Descriptor for TaskTemplate Management API
```yaml
openapi: 3.0.3
info:
  title: Tasker TaskTemplate API
  description: API for managing and executing Tasker workflow templates
  version: 1.0.0
  contact:
    name: Tasker Team
    email: support@tasker.systems
servers:
  - url: https://api.tasker.systems/v1
    description: Production API
  - url: https://staging-api.tasker.systems/v1
    description: Staging API
tags:
  - name: templates
    description: TaskTemplate management operations
  - name: execution
    description: Task execution operations
  - name: validation
    description: Template validation operations

paths:
  /templates:
    get:
      tags:
        - templates
      summary: List all task templates
      operationId: listTemplates
      parameters:
        - name: namespace
          in: query
          schema:
            type: string
          description: Filter by namespace
        - name: tag
          in: query
          schema:
            type: array
            items:
              type: string
          description: Filter by tags
        - name: page
          in: query
          schema:
            type: integer
            default: 1
        - name: limit
          in: query
          schema:
            type: integer
            default: 20
            maximum: 100
      responses:
        '200':
          description: Successful response
          content:
            application/json:
              schema:
                type: object
                properties:
                  templates:
                    type: array
                    items:
                      $ref: '#/components/schemas/TaskTemplateSummary'
                  pagination:
                    $ref: '#/components/schemas/Pagination'

    post:
      tags:
        - templates
      summary: Create a new task template
      operationId: createTemplate
      requestBody:
        required: true
        content:
          application/yaml:
            schema:
              $ref: '#/components/schemas/TaskTemplate'
          application/json:
            schema:
              $ref: '#/components/schemas/TaskTemplate'
      responses:
        '201':
          description: Template created successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/TaskTemplateResponse'
        '400':
          description: Invalid template format
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ValidationError'

  /templates/{namespace}/{name}:
    get:
      tags:
        - templates
      summary: Get a specific task template
      operationId: getTemplate
      parameters:
        - name: namespace
          in: path
          required: true
          schema:
            type: string
        - name: name
          in: path
          required: true
          schema:
            type: string
        - name: version
          in: query
          schema:
            type: string
          description: Specific version (defaults to latest)
        - name: environment
          in: query
          schema:
            type: string
            enum: [development, test, staging, production]
          description: Resolve template for specific environment
      responses:
        '200':
          description: Template found
          content:
            application/yaml:
              schema:
                $ref: '#/components/schemas/TaskTemplate'
            application/json:
              schema:
                $ref: '#/components/schemas/TaskTemplate'
        '404':
          description: Template not found

    put:
      tags:
        - templates
      summary: Update an existing task template
      operationId: updateTemplate
      parameters:
        - name: namespace
          in: path
          required: true
          schema:
            type: string
        - name: name
          in: path
          required: true
          schema:
            type: string
      requestBody:
        required: true
        content:
          application/yaml:
            schema:
              $ref: '#/components/schemas/TaskTemplate'
          application/json:
            schema:
              $ref: '#/components/schemas/TaskTemplate'
      responses:
        '200':
          description: Template updated successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/TaskTemplateResponse'

    delete:
      tags:
        - templates
      summary: Delete a task template
      operationId: deleteTemplate
      parameters:
        - name: namespace
          in: path
          required: true
          schema:
            type: string
        - name: name
          in: path
          required: true
          schema:
            type: string
        - name: version
          in: query
          schema:
            type: string
          description: Specific version to delete
      responses:
        '204':
          description: Template deleted successfully
        '404':
          description: Template not found

  /templates/validate:
    post:
      tags:
        - validation
      summary: Validate a task template
      operationId: validateTemplate
      requestBody:
        required: true
        content:
          application/yaml:
            schema:
              $ref: '#/components/schemas/TaskTemplate'
          application/json:
            schema:
              $ref: '#/components/schemas/TaskTemplate'
      responses:
        '200':
          description: Validation result
          content:
            application/json:
              schema:
                type: object
                properties:
                  valid:
                    type: boolean
                  errors:
                    type: array
                    items:
                      $ref: '#/components/schemas/ValidationError'
                  warnings:
                    type: array
                    items:
                      type: string

  /templates/{namespace}/{name}/execute:
    post:
      tags:
        - execution
      summary: Execute a task from template
      operationId: executeTask
      parameters:
        - name: namespace
          in: path
          required: true
          schema:
            type: string
        - name: name
          in: path
          required: true
          schema:
            type: string
        - name: environment
          in: query
          schema:
            type: string
            enum: [development, test, staging, production]
            default: production
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                input:
                  type: object
                  description: Input data matching the template's input_schema
                metadata:
                  type: object
                  description: Additional execution metadata
      responses:
        '202':
          description: Task execution started
          content:
            application/json:
              schema:
                type: object
                properties:
                  task_id:
                    type: string
                    format: uuid
                  status:
                    type: string
                    enum: [pending, running]
                  created_at:
                    type: string
                    format: date-time

components:
  schemas:
    TaskTemplate:
      type: object
      required: [name, namespace_name, version, steps]
      properties:
        name:
          type: string
          pattern: '^[a-z][a-z0-9_]*$'
        namespace_name:
          type: string
          pattern: '^[a-z][a-z0-9_]*$'
        version:
          type: string
          pattern: '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$'
        description:
          type: string
        metadata:
          $ref: '#/components/schemas/TemplateMetadata'
        task_handler:
          $ref: '#/components/schemas/HandlerDefinition'
        system_dependencies:
          $ref: '#/components/schemas/SystemDependencies'
        domain_events:
          type: array
          items:
            $ref: '#/components/schemas/DomainEventDefinition'
        input_schema:
          type: object
        steps:
          type: array
          items:
            $ref: '#/components/schemas/StepDefinition'
        environments:
          type: object
          additionalProperties:
            $ref: '#/components/schemas/EnvironmentOverride'

    TaskTemplateSummary:
      type: object
      properties:
        name:
          type: string
        namespace_name:
          type: string
        version:
          type: string
        description:
          type: string
        tags:
          type: array
          items:
            type: string
        created_at:
          type: string
          format: date-time
        updated_at:
          type: string
          format: date-time

    TaskTemplateResponse:
      type: object
      properties:
        template:
          $ref: '#/components/schemas/TaskTemplate'
        validation:
          type: object
          properties:
            valid:
              type: boolean
            warnings:
              type: array
              items:
                type: string

    TemplateMetadata:
      type: object
      properties:
        author:
          type: string
        tags:
          type: array
          items:
            type: string
        documentation_url:
          type: string
          format: uri
        created_at:
          type: string
          format: date-time
        updated_at:
          type: string
          format: date-time

    HandlerDefinition:
      type: object
      required: [callable]
      properties:
        callable:
          type: string
        initialization:
          type: object

    SystemDependencies:
      type: object
      properties:
        primary:
          type: string
          enum: [default, payments, inventory, notifications, analytics]
        secondary:
          type: array
          items:
            type: string

    DomainEventDefinition:
      type: object
      required: [name]
      properties:
        name:
          type: string
        description:
          type: string
        schema:
          type: object

    StepDefinition:
      type: object
      required: [name, handler]
      properties:
        name:
          type: string
        description:
          type: string
        handler:
          $ref: '#/components/schemas/HandlerDefinition'
        system_dependency:
          type: string
        dependencies:
          type: array
          items:
            type: string
        retry:
          $ref: '#/components/schemas/RetryConfiguration'
        timeout_seconds:
          type: integer
        publishes_events:
          type: array
          items:
            type: string

    RetryConfiguration:
      type: object
      properties:
        retryable:
          type: boolean
        limit:
          type: integer
        backoff:
          type: string
          enum: [none, linear, exponential, fibonacci]
        backoff_base_ms:
          type: integer
        max_backoff_ms:
          type: integer

    EnvironmentOverride:
      type: object
      properties:
        task_handler:
          type: object
          properties:
            initialization:
              type: object
        steps:
          type: array
          items:
            type: object
            properties:
              name:
                type: string
              handler:
                type: object
              timeout_seconds:
                type: integer
              retry:
                $ref: '#/components/schemas/RetryConfiguration'

    ValidationError:
      type: object
      properties:
        field:
          type: string
        message:
          type: string
        code:
          type: string
          enum: [required, invalid_format, circular_dependency, unknown_reference]

    Pagination:
      type: object
      properties:
        page:
          type: integer
        limit:
          type: integer
        total:
          type: integer
        total_pages:
          type: integer
```

### Implementation Tooling

#### Schema Validation Tools

##### Rust Implementation
```rust
// src/template/validator.rs
use jsonschema::{Draft, JSONSchema};
use serde_json::Value;
use std::path::Path;

pub struct TemplateValidator {
    schema: JSONSchema,
}

impl TemplateValidator {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let schema_str = include_str!("../../schemas/task-template.json");
        let schema: Value = serde_json::from_str(schema_str)?;
        let compiled = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema)?;
        
        Ok(Self { schema: compiled })
    }
    
    pub fn validate_yaml(&self, yaml_content: &str) -> Result<(), Vec<String>> {
        let value: Value = serde_yaml::from_str(yaml_content)
            .map_err(|e| vec![format!("YAML parse error: {}", e)])?;
        
        self.validate_json(&value)
    }
    
    pub fn validate_json(&self, value: &Value) -> Result<(), Vec<String>> {
        let result = self.schema.validate(value);
        
        if let Err(errors) = result {
            let error_messages: Vec<String> = errors
                .map(|e| format!("{}: {}", e.instance_path, e))
                .collect();
            return Err(error_messages);
        }
        
        Ok(())
    }
}
```

##### Ruby Implementation
```ruby
# lib/tasker_core/template/validator.rb
require 'json_schemer'
require 'yaml'

module TaskerCore
  module Template
    class Validator
      SCHEMA_PATH = File.join(__dir__, '../../../schemas/task-template.json')
      
      def initialize
        schema_content = File.read(SCHEMA_PATH)
        @schema = JSONSchemer.schema(JSON.parse(schema_content))
      end
      
      def validate_yaml(yaml_content)
        data = YAML.safe_load(yaml_content, permitted_classes: [Symbol, Date, Time])
        validate_data(data)
      rescue Psych::SyntaxError => e
        { valid: false, errors: ["YAML syntax error: #{e.message}"] }
      end
      
      def validate_file(path)
        yaml_content = File.read(path)
        validate_yaml(yaml_content)
      end
      
      def validate_data(data)
        errors = @schema.validate(data).map do |error|
          "#{error['data_pointer']}: #{error['error']}"
        end
        
        {
          valid: errors.empty?,
          errors: errors,
          warnings: check_warnings(data)
        }
      end
      
      private
      
      def check_warnings(data)
        warnings = []
        
        # Check for deprecated patterns
        if data['task_handler_class']
          warnings << "Field 'task_handler_class' is deprecated, use 'task_handler.callable'"
        end
        
        # Check for missing recommended fields
        unless data.dig('metadata', 'documentation_url')
          warnings << "Consider adding 'metadata.documentation_url' for better documentation"
        end
        
        warnings
      end
    end
  end
end
```

### CLI Tools for Validation

#### Rust CLI Tool
```rust
// src/bin/validate-template.rs
use clap::Parser;
use std::path::PathBuf;
use tasker_core::template::TemplateValidator;

#[derive(Parser)]
#[clap(name = "validate-template")]
#[clap(about = "Validates TaskTemplate YAML files against the schema")]
struct Args {
    /// Path to the YAML file to validate
    #[clap(value_name = "FILE")]
    file: PathBuf,
    
    /// Output format
    #[clap(short, long, default_value = "text")]
    format: String,
}

fn main() {
    let args = Args::parse();
    let validator = TemplateValidator::new().expect("Failed to initialize validator");
    
    let content = std::fs::read_to_string(&args.file)
        .expect("Failed to read file");
    
    match validator.validate_yaml(&content) {
        Ok(()) => {
            println!("✅ Template is valid");
            std::process::exit(0);
        }
        Err(errors) => {
            println!("❌ Template validation failed:");
            for error in errors {
                println!("  - {}", error);
            }
            std::process::exit(1);
        }
    }
}
```

#### Ruby CLI Tool
```ruby
#!/usr/bin/env ruby
# bin/validate-template

require 'optparse'
require 'tasker_core/template/validator'

options = {}
OptionParser.new do |opts|
  opts.banner = "Usage: validate-template [options] FILE"
  
  opts.on("-f", "--format FORMAT", "Output format (text, json)") do |f|
    options[:format] = f
  end
end.parse!

file_path = ARGV[0]
unless file_path
  puts "Error: Please provide a file path"
  exit 1
end

validator = TaskerCore::Template::Validator.new
result = validator.validate_file(file_path)

if options[:format] == 'json'
  require 'json'
  puts JSON.pretty_generate(result)
else
  if result[:valid]
    puts "✅ Template is valid"
    
    unless result[:warnings].empty?
      puts "\n⚠️  Warnings:"
      result[:warnings].each { |w| puts "  - #{w}" }
    end
  else
    puts "❌ Template validation failed:"
    result[:errors].each { |e| puts "  - #{e}" }
    exit 1
  end
end
```

### VS Code Extension Configuration

#### .vscode/settings.json
```json
{
  "yaml.schemas": {
    "https://tasker.systems/schemas/v1/task-template.json": [
      "templates/**/*.yaml",
      "templates/**/*.yml",
      "config/tasker/templates/**/*.yaml",
      "config/tasker/templates/**/*.yml"
    ]
  },
  "yaml.customTags": [
    "!ruby/object:TaskerCore::Types::TaskTemplate"
  ],
  "files.associations": {
    "**/templates/*.yaml": "tasker-template",
    "**/templates/*.yml": "tasker-template"
  }
}
```

### GitHub Actions Validation

#### .github/workflows/validate-templates.yml
```yaml
name: Validate TaskTemplates

on:
  pull_request:
    paths:
      - 'templates/**/*.yaml'
      - 'templates/**/*.yml'
      - 'schemas/task-template.json'

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Set up Ruby
        uses: ruby/setup-ruby@v1
        with:
          ruby-version: '3.2'
          bundler-cache: true
      
      - name: Validate templates
        run: |
          for file in templates/**/*.{yaml,yml}; do
            echo "Validating $file..."
            bundle exec bin/validate-template "$file"
          done
      
      - name: Check schema compliance
        uses: ajv-validator/github-action@v1
        with:
          files: templates/**/*.yaml
          schema: schemas/task-template.json
```

## Benefits of Schema-Driven Approach

### Development Benefits
1. **IDE Support**: Auto-completion, validation, and inline documentation in VS Code, JetBrains, and other IDEs
2. **Early Error Detection**: Catch configuration errors before runtime
3. **Self-Documenting**: Schema serves as living documentation for template structure
4. **Type Safety**: Generated TypeScript/Rust types from schema ensure compile-time safety

### Operational Benefits
1. **API Consistency**: OpenAPI spec ensures consistent REST API implementation
2. **Client Generation**: Auto-generate API clients in multiple languages
3. **Validation Pipeline**: CI/CD validation prevents invalid templates from deployment
4. **Version Management**: Schema versioning enables backward compatibility

### Integration Benefits
1. **Tool Ecosystem**: Integrate with existing JSON Schema and OpenAPI tooling
2. **Documentation Generation**: Auto-generate API docs from OpenAPI spec
3. **Testing**: Schema-based property testing and fuzzing
4. **Migration**: Schema evolution tools for template migrations