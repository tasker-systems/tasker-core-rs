# frozen_string_literal: true

require 'dry-types'
require 'dry-struct'
require 'time'

module TaskerCore
  module Types
    # TaskTemplate Types - Self-Describing Workflow Configuration
    #
    # This module implements the self-describing TaskTemplate structure, featuring:
    # - Callable-based handlers for maximum flexibility
    # - Structured handler initialization configuration
    # - Clear system dependency declarations
    # - First-class domain event support
    # - Enhanced environment-specific overrides
    # - JSON Schema-based input validation
    
    module Types
      include Dry.Types()
    end

    # Template metadata for documentation and discovery
    class TemplateMetadata < Dry::Struct
      attribute :author, Types::String.optional.default(nil)
      attribute :tags, Types::Array.of(Types::String).default([].freeze)
      attribute :documentation_url, Types::String.optional.default(nil)
      attribute :created_at, Types::String.optional.default(nil)  # ISO8601 format
      attribute :updated_at, Types::String.optional.default(nil)  # ISO8601 format
    end

    # Handler definition with callable and initialization
    class HandlerDefinition < Dry::Struct
      attribute :callable, Types::Strict::String
      attribute :initialization, Types::Hash.default({}.freeze)
    end

    # External system dependencies
    class SystemDependencies < Dry::Struct
      attribute :primary, Types::String.default('default')
      attribute :secondary, Types::Array.of(Types::String).default([].freeze)
    end

    # Domain event definition with schema
    class DomainEventDefinition < Dry::Struct
      attribute :name, Types::Strict::String
      attribute :description, Types::String.optional.default(nil)
      attribute :schema, Types::Hash.optional.default(nil)  # JSON Schema
    end

    # Retry configuration with backoff strategies
    class RetryConfiguration < Dry::Struct
      attribute :retryable, Types::Bool.default(true)
      attribute :limit, Types::Integer.default(3)
      attribute :backoff, Types::String.default('exponential').enum('none', 'linear', 'exponential', 'fibonacci')
      attribute :backoff_base_ms, Types::Integer.optional.default(1000)
      attribute :max_backoff_ms, Types::Integer.optional.default(30000)
    end

    # Individual workflow step definition
    class StepDefinition < Dry::Struct
      attribute :name, Types::Strict::String
      attribute :description, Types::String.optional.default(nil)
      attribute :handler, HandlerDefinition
      attribute :system_dependency, Types::String.optional.default(nil)
      attribute :dependencies, Types::Array.of(Types::String).default([].freeze)
      attribute :retry, RetryConfiguration.default { RetryConfiguration.new }
      attribute :timeout_seconds, Types::Integer.optional.default(nil)
      attribute :publishes_events, Types::Array.of(Types::String).default([].freeze)

      # Check if this step depends on another step
      def depends_on?(other_step_name)
        dependencies.include?(other_step_name.to_s)
      end
    end

    # Handler override for environments
    class HandlerOverride < Dry::Struct
      attribute :initialization, Types::Hash.optional.default(nil)
    end

    # Step override for environments
    class StepOverride < Dry::Struct
      attribute :name, Types::Strict::String  # Step name or "ALL" for all steps
      attribute :handler, HandlerOverride.optional.default(nil)
      attribute :timeout_seconds, Types::Integer.optional.default(nil)
      attribute :retry, RetryConfiguration.optional.default(nil)
    end

    # Environment-specific overrides
    class EnvironmentOverride < Dry::Struct
      attribute :task_handler, HandlerOverride.optional.default(nil)
      attribute :steps, Types::Array.of(StepOverride).default([].freeze)
    end

    # Main TaskTemplate structure with self-describing configuration
    class TaskTemplate < Dry::Struct
      # Semantic version pattern validation
      VERSION_PATTERN = /\A\d+\.\d+\.\d+\z/

      # Core required attributes
      attribute :name, Types::Strict::String
      attribute :namespace_name, Types::Strict::String
      attribute :version, Types::String.constrained(format: VERSION_PATTERN).default('1.0.0')

      # Self-describing structure
      attribute :description, Types::String.optional.default(nil)
      attribute :metadata, TemplateMetadata.optional.default(nil)
      attribute :task_handler, HandlerDefinition.optional.default(nil)
      attribute :system_dependencies, SystemDependencies.default { SystemDependencies.new }
      attribute :domain_events, Types::Array.of(DomainEventDefinition).default([].freeze)
      attribute :input_schema, Types::Hash.optional.default(nil)  # JSON Schema
      attribute :steps, Types::Array.of(StepDefinition).default([].freeze)
      attribute :environments, Types::Hash.map(Types::String, EnvironmentOverride).default({}.freeze)

      # Metadata (not persisted to database)
      attribute :loaded_from, Types::String.optional.default(nil)

      # Generate a unique key for this template
      def template_key
        "#{namespace_name}/#{name}:#{version}"
      end

      # Extract all callable references
      def all_callables
        callables = []
        callables << task_handler.callable if task_handler
        steps.each { |step| callables << step.handler.callable }
        callables
      end

      # Check if template is valid for registration
      def valid_for_registration?
        return false if name.empty? || namespace_name.empty?
        return false unless version.match?(VERSION_PATTERN)

        # Ensure all steps have callables
        return false if steps.any? { |step| step.handler.callable.empty? }

        true
      end

      # Resolve template for specific environment
      def resolve_for_environment(environment_name)
        resolved_template = deep_dup
        
        if environments[environment_name]
          env_override = environments[environment_name]
          
          # Apply task handler overrides
          if env_override.task_handler && resolved_template.task_handler
            if env_override.task_handler.initialization
              resolved_template.task_handler.initialization.merge!(env_override.task_handler.initialization)
            end
          end
          
          # Apply step overrides
          env_override.steps.each do |step_override|
            if step_override.name == 'ALL'
              # Apply to all steps
              resolved_template.steps.each { |step| apply_step_override(step, step_override) }
            else
              # Apply to specific step
              step = resolved_template.steps.find { |s| s.name == step_override.name }
              apply_step_override(step, step_override) if step
            end
          end
        end
        
        resolved_template
      end

      private

      # Deep duplicate for environment resolution
      def deep_dup
        # Simple deep dup implementation for dry-struct
        TaskTemplate.new(to_h)
      end

      # Apply step override to a step
      def apply_step_override(step, step_override)
        if step_override.handler&.initialization
          step.handler.initialization.merge!(step_override.handler.initialization)
        end
        
        step.timeout_seconds = step_override.timeout_seconds if step_override.timeout_seconds
        step.retry = step_override.retry if step_override.retry
      end
    end
  end
end