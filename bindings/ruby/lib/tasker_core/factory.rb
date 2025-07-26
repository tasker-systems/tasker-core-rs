# frozen_string_literal: true

require 'logger'
require 'securerandom'
require_relative 'orchestration/orchestration_manager'

module TaskerCore
  module TestHelpers
    # Test data creation factories with robust uniqueness guarantees
    #
    # This module provides test-specific factories for creating test data
    # with automatic unique naming to prevent database constraint violations.
    # This is NOT intended for production use.
    #
    # Examples:
    #   task = TaskerCore::TestHelpers::Factories.task(name: "payment_processing")
    #   step = TaskerCore::TestHelpers::Factories.workflow_step(task_id: task['id'], name: "validate_card")
    #   foundation = TaskerCore::TestHelpers::Factories.foundation(namespace: "payments")
    module Factories
    class << self
      # Create a test task with the specified options
      # @param options [Hash] Task creation parameters
      # @return [Hash] Created task data
      # @raise [TaskerCore::Error] If task creation fails
      def task(options = {})
        # Ensure unique naming to prevent constraint violations
        enhanced_options = ensure_unique_naming(options)

        # Use the TestHelpers factory function with enhanced options
        TaskerCore::TestHelpers.create_test_task(enhanced_options)
      rescue => e
        # Check if it's a constraint violation and retry with more unique name
        if e.message.include?("duplicate key value violates unique constraint")
          retry_options = enhance_uniqueness(enhanced_options)
          TaskerCore::TestHelpers.create_test_task(retry_options)
        else
          raise TaskerCore::Error, "Failed to create task: #{e.message}"
        end
      end

      # Create a test workflow step with the specified options
      # @param options [Hash] Workflow step creation parameters
      # @return [Hash] Created workflow step data
      # @raise [TaskerCore::Error] If workflow step creation fails
      def workflow_step(options = {})
        # Validate required parameters
        unless options[:task_id] || options['task_id']
          raise ArgumentError, "task_id is required"
        end

        # Use the TestHelpers factory function with full options
        TaskerCore::TestHelpers.create_test_step(options)
      rescue => e
        # For graceful error handling tests, return error response instead of raising
        if e.message.include?("not found") || e.message.include?("Test step creation failed")
          {
            'error' => e.message,
            'task_id' => options[:task_id] || options['task_id'],
            'name' => options[:name] || options['name'] || 'unknown_step',
            'status' => 'error'
          }
        else
          raise TaskerCore::Error, "Failed to create workflow step: #{e.message}"
        end
      end

      # Create test foundation data with the specified options
      # @param options [Hash] Foundation data creation parameters
      # @return [Hash] Created foundation data
      # @raise [TaskerCore::Error] If foundation creation fails
      def foundation(options = {})
        puts "🔍 FACTORY.foundation: Called with options: #{options.inspect}"

        # Ensure unique naming to prevent constraint violations
        enhanced_options = ensure_unique_naming(options)

        # Use the TestHelpers factory function with enhanced options
        result = TaskerCore::TestHelpers.create_test_foundation(enhanced_options)
        puts "🔍 FACTORY.foundation: Result: #{result.inspect}"
        result
      rescue => e
        # Check if it's a constraint violation and retry with more unique name
        if e.message.include?("duplicate key value violates unique constraint")
          retry_options = enhance_uniqueness(enhanced_options)
          result = TaskerCore::TestHelpers.create_test_foundation(retry_options)
          puts "🔍 FACTORY.foundation: Retry successful with unique naming"
          result
        else
          puts "🔍 FACTORY.foundation: Error: #{e.message}"
          puts "🔍 FACTORY.foundation: Backtrace: #{e.backtrace.first(3).join(', ')}"
          raise TaskerCore::Error, "Failed to create foundation: #{e.message}"
        end
      end

      # Get information about the Factory domain for debugging
      # @return [Hash] Domain status and metadata
      def handle_info
        {
          domain: "Factory",
          backend: "TaskerCore::TestHelpers",
          status: "operational",
          methods: ["task", "workflow_step", "foundation"],
          checked_at: Time.now.utc.iso8601,
          handle_architecture: "active"  # Indicates handle-based architecture is working
        }
      end

      private

      # Get orchestration handle with automatic refresh
      # @return [OrchestrationHandle] Active handle instance
      def handle
        TaskerCore::Internal::OrchestrationManager.instance.orchestration_handle
      end

      # Ensure unique naming for factory operations to prevent constraint violations
      # @param options [Hash] Original options hash
      # @return [Hash] Enhanced options with unique naming
      def ensure_unique_naming(options)
        enhanced = options.dup

        # Generate unique namespace if not provided or if using common names
        namespace = enhanced[:namespace] || enhanced['namespace']
        if namespace.nil? || %w[test default].include?(namespace)
          enhanced[:namespace] = generate_unique_namespace(namespace || 'test')
        end

        # Generate unique task name if using common names
        task_name = enhanced[:name] || enhanced['name'] || enhanced[:task_name] || enhanced['task_name']
        if task_name.nil? || %w[test_task default_task].include?(task_name)
          base_name = task_name || 'test_task'
          enhanced[:name] = generate_unique_name(base_name)
          enhanced[:task_name] = enhanced[:name] # Ensure both keys are set
        end

        enhanced
      end

      # Enhance uniqueness further when constraint violations occur
      # @param options [Hash] Previously enhanced options that still caused conflicts
      # @return [Hash] Options with maximum uniqueness
      def enhance_uniqueness(options)
        enhanced = options.dup

        # Add extra uniqueness to namespace
        current_namespace = enhanced[:namespace] || enhanced['namespace']
        enhanced[:namespace] = "#{current_namespace}_retry_#{SecureRandom.hex(3)}"

        # Add extra uniqueness to task name
        current_name = enhanced[:name] || enhanced['name']
        enhanced[:name] = "#{current_name}_retry_#{SecureRandom.hex(3)}"
        enhanced[:task_name] = enhanced[:name]

        enhanced
      end

      # Generate a unique namespace name
      # @param base_name [String] Base name for the namespace
      # @return [String] Unique namespace name
      def generate_unique_namespace(base_name = 'test')
        timestamp = Time.now.to_i
        random_suffix = SecureRandom.hex(4)
        "#{base_name}_#{timestamp}_#{random_suffix}"
      end

      # Generate a unique name
      # @param base_name [String] Base name
      # @return [String] Unique name
      def generate_unique_name(base_name = 'test')
        timestamp = Time.now.to_i
        random_suffix = SecureRandom.hex(3)
        "#{base_name}_#{timestamp}_#{random_suffix}"
      end
    end
    end
  end
end
