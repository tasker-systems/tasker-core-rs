# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  # Type definitions for TaskerCore domain objects
  module Types
    include Dry.Types()

    # Make types available in class scope
    Types = Dry.Types()

    # Task request struct with full validation
    class TaskRequest < Dry::Struct
      # Core identification
      attribute :namespace, Types::Coercible::String
      attribute :name, Types::Coercible::String
      attribute :version, Types::Coercible::String.default('1.0.0'.freeze)

      # Task state and metadata
      attribute :status, Types::Coercible::String.default('pending'.freeze).enum(
        'pending',
        'in_progress',
        'completed',
        'failed',
        'cancelled',
        'paused'
      )
      attribute :initiator, Types::Coercible::String
      attribute :source_system, Types::Coercible::String
      attribute :reason, Types::Coercible::String
      attribute :complete, Types::Strict::Bool.default(false)
      attribute :tags, Types::Array.of(Types::Coercible::String).default([].freeze)

      # Task context (business data)
      attribute :context, Types::Coercible::Hash

      # Optional fields for advanced use cases
      attribute? :priority, Types::Integer.default(5) # 1-10 scale
      attribute? :max_retries, Types::Integer.default(3)
      attribute? :timeout_seconds, Types::Integer.default(300)
      attribute? :parent_task_id, Types::Integer
      attribute? :correlation_id, Types::Coercible::String
      attribute? :bypass_steps, Types::Array.of(Types::Coercible::String).default([].freeze)
      attribute? :requested_at, Types::Constructor(Time).default { Time.now }

      # Validation methods
      def valid_for_creation?
        !namespace.nil? && !namespace.empty? &&
        !name.nil? && !name.empty? &&
        !initiator.nil? && !initiator.empty? &&
        !source_system.nil? && !source_system.empty? &&
        !reason.nil? && !reason.empty? &&
        context.is_a?(Hash)
      end

      # Convert to hash suitable for FFI serialization
      def to_ffi_hash
        # Build options hash for fields that go in the options field
        options_hash = {}
        options_hash['priority'] = priority if priority
        options_hash['max_retries'] = max_retries if max_retries
        options_hash['timeout_seconds'] = timeout_seconds if timeout_seconds
        options_hash['parent_task_id'] = parent_task_id if parent_task_id
        options_hash['correlation_id'] = correlation_id if correlation_id

        # Build the exact fields that Rust TaskRequest expects
        rust_hash = {
          namespace: namespace,
          name: name,
          version: version,
          status: status,
          initiator: initiator,
          source_system: source_system,
          reason: reason,
          complete: complete,
          tags: tags,
          context: context,
          bypass_steps: bypass_steps,
          requested_at: requested_at&.utc&.strftime('%Y-%m-%dT%H:%M:%S')
        }

        # Add options field only if there are options to add
        rust_hash[:options] = options_hash unless options_hash.empty?

        rust_hash
      end

      # Create from hash with proper type coercion
      def self.from_hash(hash)
        new(hash.transform_keys(&:to_sym))
      rescue Dry::Struct::Error => e
        raise TaskerCore::ValidationError, "Invalid TaskRequest: #{e.message}"
      end

      # Quick factory method for tests
      def self.build_test(namespace:, name:, context:, **options)
        from_hash({
          namespace: namespace,
          name: name,
          context: context,
          initiator: options[:initiator] || 'test',
          source_system: options[:source_system] || 'rspec',
          reason: options[:reason] || 'testing',
          tags: options[:tags] || ['test'],
          **options
        })
      end

      # Pretty string representation
      def to_s
        "#<TaskRequest #{namespace}/#{name}:#{version} status=#{status} initiator=#{initiator}>"
      end

      def inspect
        to_s
      end
    end

    # Task response struct (for initialize_task results)
    class TaskResponse < Dry::Struct
      attribute :task_id, Types::Integer
      attribute :status, Types::Coercible::String.enum(
        'pending',
        'in_progress',
        'completed',
        'failed',
        'cancelled',
        'paused'
      )
      attribute :workflow_steps, Types::Array.of(Types::Hash)
      attribute? :created_at, Types::Constructor(Time)
      attribute? :estimated_completion, Types::Constructor(Time)
      attribute? :metadata, Types::Hash
    end

    # Step completion struct
    class StepCompletion < Dry::Struct
      attribute :step_name, Types::Coercible::String
      attribute :status, Types::Coercible::String.enum('complete', 'failed', 'pending')
      attribute :results, Types::Hash.default({}.freeze)
      attribute :duration_ms, Types::Integer.optional
      attribute :completed_at, Types::Constructor(Time).optional
      attribute :error_message, Types::Coercible::String.optional
    end
  end
end
