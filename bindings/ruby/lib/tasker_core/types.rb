# frozen_string_literal: true

require 'dry-types'
require 'dry-struct'

module TaskerCore
  module Types
    include Dry.Types()
    
    # ============================================================================
    # CORE PRIMITIVE TYPES
    # ============================================================================
    
    # Basic types with validation
    StepId = Coercible::Integer.constrained(gt: 0)
    TaskId = Coercible::Integer.constrained(gt: 0)
    NamedStepId = Coercible::Integer.constrained(gt: 0)
    NamedTaskId = Coercible::Integer.constrained(gt: 0)
    
    # State types (matches Rust enums)
    StepState = String.enum('pending', 'in_progress', 'complete', 'error', 'cancelled')
    TaskState = String.enum('pending', 'in_progress', 'complete', 'error', 'cancelled')
    
    # Error classification types
    ErrorCategory = String.enum(
      'validation', 'authorization', 'business_logic', 'network', 
      'rate_limit', 'service_unavailable', 'server_error', 'unknown'
    )
    
    # Health status types  
    HealthStatus = String.enum('healthy', 'degraded', 'unhealthy', 'unknown')
    ExecutionStatus = String.enum('pending', 'ready', 'processing', 'blocked', 'complete', 'error')
    
    # Percentage type
    Percentage = Coercible::Float.constrained(gteq: 0.0, lteq: 100.0)
    
    # Duration types
    DurationSeconds = Coercible::Integer.constrained(gteq: 0)
    RetryAfterSeconds = Coercible::Integer.constrained(gteq: 0).optional
    
    # Context data (flexible JSON-like structure)
    ContextData = Hash.map(String, Any)
    
    # ============================================================================
    # STRUCTURED RESULT TYPES (mirror Rust Magnus structs)
    # ============================================================================
    
    # Base struct for all TaskerCore structured types
    class BaseStruct < Dry::Struct
      # Enable type transformation on initialization
      transform_keys(&:to_sym)
      
      # Add common methods for all structs
      def to_hash
        to_h
      end
      
      def to_json(*args)
        to_h.to_json(*args)
      end
    end
    
    # Task execution context (mirrors RubyTaskExecutionContext)
    class TaskExecutionContext < BaseStruct
      attribute :task_id, TaskId
      attribute :total_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :completed_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :pending_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :error_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :ready_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :blocked_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :completion_percentage, Percentage
      attribute :estimated_duration_seconds, DurationSeconds.optional
      attribute :recommended_action, String
      attribute :next_steps_to_execute, Array.of(StepId)
      attribute :critical_path_steps, Array.of(StepId)
      attribute :bottleneck_steps, Array.of(StepId)
      
      # Derived methods (matches Rust implementation)
      def execution_status
        return 'complete' if is_complete?
        return 'blocked' if is_blocked?
        return 'ready' if has_ready_steps?
        return 'pending' if pending_steps > 0
        'unknown'
      end
      
      def health_status
        return 'healthy' if error_steps == 0
        error_rate = error_steps.to_f / total_steps.to_f
        return 'unhealthy' if error_rate > 0.5
        return 'degraded' if error_rate > 0.1
        'healthy'
      end
      
      def can_proceed?
        ready_steps > 0
      end
      
      def is_complete?
        completion_percentage >= 100.0
      end
      
      def is_blocked?
        ready_steps == 0 && pending_steps > 0
      end
      
      def has_ready_steps?
        ready_steps > 0
      end
      
      def has_failures?
        error_steps > 0
      end
      
      def completion_ratio
        completion_percentage / 100.0
      end
      
      def get_priority_steps
        critical_path_steps.any? ? critical_path_steps : next_steps_to_execute
      end
    end
    
    # Viable step (mirrors RubyViableStep)
    class ViableStep < BaseStruct
      attribute :workflow_step_id, StepId
      attribute :step_name, String
      attribute :dependencies_satisfied, Bool
      attribute :is_ready, Bool
      attribute :estimated_duration_seconds, DurationSeconds.optional
      attribute :priority_score, Coercible::Float.constrained(gteq: 0.0)
      
      def can_execute_now?
        is_ready && dependencies_satisfied
      end
    end
    
    # System health (mirrors RubySystemHealth)
    class SystemHealth < BaseStruct
      attribute :overall_status, HealthStatus
      attribute :active_tasks, Coercible::Integer.constrained(gteq: 0)
      attribute :pending_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :error_rate, Coercible::Float.constrained(gteq: 0.0, lteq: 1.0)
      attribute :average_step_duration, Coercible::Float.constrained(gteq: 0.0)
      attribute :system_load, Coercible::Float.constrained(gteq: 0.0, lteq: 1.0)
      attribute :last_updated_at, String # ISO8601 timestamp
      
      def is_healthy?
        overall_status == 'healthy'
      end
      
      def is_under_load?
        system_load > 0.8
      end
      
      def needs_attention?
        overall_status == 'unhealthy' || error_rate > 0.1
      end
    end
    
    # Analytics metrics (mirrors RubyAnalyticsMetrics)
    class AnalyticsMetrics < BaseStruct
      attribute :active_tasks_count, Coercible::Integer.constrained(gteq: 0)
      attribute :total_namespaces_count, Coercible::Integer.constrained(gteq: 0)
      attribute :unique_task_types_count, Coercible::Integer.constrained(gteq: 0)
      attribute :system_health_score, Coercible::Float.constrained(gteq: 0.0, lteq: 1.0)
      attribute :task_throughput, Coercible::Integer.constrained(gteq: 0)
      attribute :completion_count, Coercible::Integer.constrained(gteq: 0)
      attribute :error_count, Coercible::Integer.constrained(gteq: 0)
      attribute :completion_rate, Coercible::Float.constrained(gteq: 0.0, lteq: 1.0)
      attribute :error_rate, Coercible::Float.constrained(gteq: 0.0, lteq: 1.0)
      attribute :avg_task_duration, Coercible::Float.constrained(gteq: 0.0)
      attribute :avg_step_duration, Coercible::Float.constrained(gteq: 0.0)
      attribute :step_throughput, Coercible::Integer.constrained(gteq: 0)
      attribute :analysis_period_start, String # ISO8601 timestamp
      attribute :calculated_at, String # ISO8601 timestamp
      
      def is_healthy?
        system_health_score > 0.8 && error_rate < 0.1
      end
      
      def has_performance_issues?
        system_health_score < 0.5 || error_rate > 0.2
      end
    end
    
    # Dependency level (mirrors RubyDependencyLevel)
    class DependencyLevel < BaseStruct
      attribute :workflow_step_id, StepId
      attribute :dependency_level, Coercible::Integer.constrained(gteq: 0)
      
      def is_root_level?
        dependency_level == 0
      end
      
      def can_run_parallel_with?(other)
        dependency_level == other.dependency_level
      end
    end
    
    # Dependency analysis (mirrors RubyDependencyAnalysis)
    class DependencyAnalysis < BaseStruct
      attribute :task_id, TaskId
      attribute :has_cycles, Bool
      attribute :max_depth, Coercible::Integer.constrained(gteq: 0)
      attribute :parallel_branches, Coercible::Integer.constrained(gteq: 0)
      attribute :critical_path_length, Coercible::Integer.constrained(gteq: 0)
      attribute :total_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :ready_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :blocked_steps, Coercible::Integer.constrained(gteq: 0)
      attribute :completion_percentage, Percentage
      attribute :health_status, HealthStatus
      attribute :analysis_complexity, String
      attribute :parallelization_factor, Coercible::Float.constrained(gteq: 0.0)
      
      def is_complex?
        max_depth > 5 || total_steps > 20
      end
      
      def has_good_parallelization?
        parallelization_factor > 0.5
      end
      
      def needs_optimization?
        parallelization_factor < 0.3 && total_steps > 10
      end
    end
    
    # Step execution context (for step handler process method)
    class StepContext < BaseStruct
      attribute :step_id, StepId
      attribute :task_id, TaskId
      attribute :named_step_id, NamedStepId
      attribute :step_name, String
      attribute :input_data, ContextData
      attribute :context, ContextData.optional
      attribute :previous_results, ContextData.optional
      attribute :attempt_number, Coercible::Integer.constrained(gteq: 1).default(1)
      attribute :max_attempts, Coercible::Integer.constrained(gteq: 1).default(3)
      attribute :config, ContextData.optional
      attribute :created_at, String # ISO8601 timestamp
      attribute :started_at, String.optional # ISO8601 timestamp
      
      def is_retry?
        attempt_number > 1
      end
      
      def can_retry?
        attempt_number < max_attempts
      end
      
      def get_previous_result(step_name)
        previous_results&.dig(step_name) || previous_results&.dig(step_name.to_s)
      end
      
      def get_config_value(key)
        config&.dig(key) || config&.dig(key.to_s)
      end
      
      def get_input_value(key)
        input_data&.dig(key) || input_data&.dig(key.to_s)
      end
      
      def get_context_value(key)
        context&.dig(key) || context&.dig(key.to_s)
      end
    end
    
    # Task execution context (for task handler methods)
    class TaskContext < BaseStruct
      attribute :task_id, TaskId
      attribute :named_task_id, NamedTaskId
      attribute :task_name, String
      attribute :namespace, String
      attribute :input_data, ContextData
      attribute :context, ContextData.optional
      attribute :status, TaskState
      attribute :priority, Coercible::Integer.constrained(gteq: 0).default(0)
      attribute :config, ContextData.optional
      attribute :created_at, String # ISO8601 timestamp
      attribute :started_at, String.optional # ISO8601 timestamp
      attribute :completed_at, String.optional # ISO8601 timestamp
      
      def is_retry?
        status == 'retrying'
      end
      
      def can_retry?
        !['complete', 'cancelled'].include?(status)
      end
      
      def get_config_value(key)
        config&.dig(key) || config&.dig(key.to_s)
      end
      
      def get_input_value(key)
        input_data&.dig(key) || input_data&.dig(key.to_s)
      end
      
      def get_context_value(key)
        context&.dig(key) || context&.dig(key.to_s)
      end
      
      def duration_seconds
        return nil unless started_at
        
        end_time = completed_at ? Time.parse(completed_at) : Time.now
        start_time = Time.parse(started_at)
        
        (end_time - start_time).to_i
      end
    end
    
    # ============================================================================
    # VALIDATION SCHEMAS
    # ============================================================================
    
    # Step input validation
    StepInputSchema = Dry::Schema.Params do
      required(:step_id).filled(StepId)
      required(:input_data).hash
      optional(:context).hash
      optional(:retry_attempt).filled(:integer, gteq?: 0)
    end
    
    # Task input validation  
    TaskInputSchema = Dry::Schema.Params do
      required(:task_id).filled(TaskId)
      required(:input_data).hash
      optional(:context).hash
      optional(:priority).filled(:integer, gteq?: 0)
    end
    
    # Batch update validation
    BatchUpdateSchema = Dry::Schema.Params do
      required(:updates).array(:hash) do
        required(:step_id).filled(StepId)
        required(:new_state).filled(StepState)
        optional(:context_data).hash
      end
    end
  end
end