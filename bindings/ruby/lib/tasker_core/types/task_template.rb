# frozen_string_literal: true

require 'dry-types'
require 'dry-struct'

module TaskerCore
  module Types
    # Dry-struct types for TaskTemplate registration and validation
    #
    # These types ensure proper validation, normalization, and defaults
    # for TaskTemplate data structures used in the database-first registry.

    # Step template configuration
    class StepTemplate < Dry::Struct
      attribute :name, Types::Strict::String
      attribute :description, Types::String.optional.default(nil)
      attribute :handler_class, Types::Strict::String
      attribute :handler_config, Types::Hash.default({}.freeze)
      attribute :depends_on_step, Types::String.optional.default(nil)
      attribute :depends_on_steps, Types::Array.of(Types::String).default([].freeze)
      attribute :default_retryable, Types::Bool.default(true)
      attribute :default_retry_limit, Types::Integer.default(3)
      attribute :timeout_seconds, Types::Integer.optional.default(nil)
    end

    class StepOverride < Dry::Struct
      attribute :name, Types::Strict::String
      attribute? :description, Types::String.optional.default(nil)
      attribute? :handler_class, Types::Strict::String.optional.default(nil)
      attribute? :handler_config, Types::Hash.default({}.freeze)
      attribute? :depends_on_step, Types::String.optional.default(nil)
      attribute? :depends_on_steps, Types::Array.of(Types::Coercible::String).optional.default(nil)
      attribute? :default_retryable, Types::Bool.optional.default(nil)
      attribute? :default_retry_limit, Types::Integer.optional.default(nil)
      attribute? :timeout_seconds, Types::Integer.optional.default(nil)
    end

    # Environment configuration
    class EnvironmentConfig < Dry::Struct
      attribute :step_templates, Types::Array.of(StepOverride).default([].freeze)
    end

    # Main TaskTemplate structure
    class TaskTemplate < Dry::Struct
      # Semantic version pattern validation
      VERSION_PATTERN = /\A\d+\.\d+\.\d+\z/

      # Required attributes
      attribute :name, Types::Strict::String
      attribute :namespace_name, Types::Strict::String
      attribute :version, Types::String.constrained(format: VERSION_PATTERN).default('1.0.0')

      # Optional attributes with defaults
      attribute :task_handler_class, Types::Coercible::String.optional.default(nil)
      attribute :module_namespace, Types::Coercible::String.optional.default(nil)
      attribute :description, Types::Coercible::String.optional.default(nil)
      attribute :default_dependent_system, Types::Coercible::String.optional.default(nil)
      attribute :schema, Types::Hash.optional.default(nil)
      attribute :named_steps, Types::Array.of(Types::Coercible::String).default([].freeze)
      attribute :step_templates, Types::Array.of(StepTemplate).default([].freeze)
      attribute :environments, Types::Hash.map(Types::Coercible::String, EnvironmentConfig).default({}.freeze)
      attribute :handler_config, Types::Hash.default({}.freeze)
      attribute :custom_events, Types::Array.of(Types::Coercible::String).default([].freeze)

      # Metadata (not persisted to database)
      attribute :loaded_from, Types::String.optional.default(nil)

      # Generate a unique key for this template
      def template_key
        "#{namespace_name}/#{name}:#{version}"
      end

      # Extract all handler class names from this template
      def handler_class_names
        handlers = []
        handlers << task_handler_class if task_handler_class
        step_templates.each do |step|
          handlers << step.handler_class
        end
        handlers.compact.uniq
      end

      # Check if template is valid for registration
      def valid_for_registration?
        return false if name.empty? || namespace_name.empty?
        return false unless version.match?(VERSION_PATTERN)

        # All step templates must have handler classes
        step_templates.all? { |step| !step.handler_class.empty? }
      end
    end

    # Database models for persistence (mirror Rust structure)

    # TaskNamespace database record
    class TaskNamespaceRecord < Dry::Struct
      attribute :id, Types::Integer
      attribute :name, Types::Strict::String
      attribute :created_at, Types::Time
      attribute :updated_at, Types::Time
    end

    # NamedTask database record
    class NamedTaskRecord < Dry::Struct
      attribute :id, Types::Integer
      attribute :namespace_id, Types::Integer
      attribute :name, Types::Strict::String
      attribute :version, Types::String
      attribute :task_handler_class, Types::String.optional
      attribute :module_namespace, Types::String.optional
      attribute :description, Types::String.optional
      attribute :configuration, Types::Hash # JSON column with full config
      attribute :created_at, Types::Time
      attribute :updated_at, Types::Time
    end

    # StepTemplate database record
    class StepTemplateRecord < Dry::Struct
      attribute :id, Types::Integer
      attribute :named_task_id, Types::Integer
      attribute :name, Types::Strict::String
      attribute :handler_class, Types::Strict::String
      attribute :position, Types::Integer
      attribute :configuration, Types::Hash # JSON column with step config
      attribute :created_at, Types::Time
      attribute :updated_at, Types::Time
    end
  end
end
