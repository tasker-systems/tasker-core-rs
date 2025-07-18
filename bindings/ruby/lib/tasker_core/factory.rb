# frozen_string_literal: true

require 'logger'

module TaskerCore
  # Domain module for test data creation with singleton handle management
  #
  # This module provides a clean, Ruby-idiomatic API for creating test data
  # while internally managing a persistent OrchestrationHandle for optimal performance.
  #
  # Examples:
  #   task = TaskerCore::Factory.task(name: "payment_processing")
  #   step = TaskerCore::Factory.workflow_step(task_id: task['id'], name: "validate_card")
  #   foundation = TaskerCore::Factory.foundation(namespace: "payments")
  module Factory
    class << self
      # Create a test task with the specified options
      # @param options [Hash] Task creation parameters
      # @return [Hash] Created task data
      # @raise [TaskerCore::Error] If task creation fails
      def task(options = {})
        # Extract primitive parameters from options
        namespace = options[:namespace] || options['namespace'] || 'default'
        task_name = options[:name] || options['name'] || 'test_task'
        version = options[:version] || options['version'] || '0.1.0'
        initiator = options[:initiator] || options['initiator']

        # Check if context is provided
        if options.key?(:context) || options.key?('context')
          context = options[:context] || options['context'] || {}
          # Use context-aware handle method with version and initiator
          handle.create_test_task_with_context_and_initiator(namespace, task_name, context, version, initiator)
        else
          # Use simple handle method with version and initiator
          handle.create_test_task_simple_with_initiator(namespace, task_name, version, initiator)
        end
      rescue => e
        raise TaskerCore::Error, "Failed to create task: #{e.message}"
      end

      # Create a test workflow step with the specified options
      # @param options [Hash] Workflow step creation parameters
      # @return [Hash] Created workflow step data
      # @raise [TaskerCore::Error] If workflow step creation fails
      def workflow_step(options = {})
        # Extract primitive parameters from options
        task_id = options[:task_id] || options['task_id'] || raise(ArgumentError, "task_id is required")
        step_name = options[:name] || options['name'] || 'test_step'

        # Use primitive FFI method
        handle.create_test_workflow_step_simple(task_id, step_name)
      rescue => e
        raise TaskerCore::Error, "Failed to create workflow step: #{e.message}"
      end

      # Create test foundation data with the specified options
      # @param options [Hash] Foundation data creation parameters
      # @return [Hash] Created foundation data
      # @raise [TaskerCore::Error] If foundation creation fails
      def foundation(options = {})
        puts "🔍 FACTORY.foundation: Called with options: #{options.inspect}"

        # Extract primitive parameters from options
        namespace = options[:namespace] || options['namespace'] || 'default'
        task_name = options[:task_name] || options['task_name'] || 'test_task'

        puts "🔍 FACTORY.foundation: Using namespace=#{namespace}, task_name=#{task_name}"
        puts "🔍 FACTORY.foundation: About to call handle.create_test_foundation with primitives"

        # Use primitive FFI method (no JSON conversion!)
        result = handle.create_test_foundation(namespace, task_name)
        puts "🔍 FACTORY.foundation: Result: #{result.inspect}"
        result
      rescue => e
        puts "🔍 FACTORY.foundation: Error: #{e.message}"
        puts "🔍 FACTORY.foundation: Backtrace: #{e.backtrace.first(3).join(', ')}"
        raise TaskerCore::Error, "Failed to create foundation: #{e.message}"
      end

      # Get information about the internal handle for debugging
      # @return [Hash] Handle status and metadata
      def handle_info
        handle.info
      rescue => e
        { error: e.message, status: "unavailable" }
      end

      private

      # Get or create the singleton OrchestrationHandle for this domain
      # This ensures we reuse the same handle across all Factory operations
      # for optimal performance and resource utilization.
      def handle
        @handle ||= begin
          TaskerCore::Logging::Logger.instance.info("🏭 FACTORY DOMAIN: Creating singleton OrchestrationHandle")
          handle = TaskerCore.create_orchestration_handle
          puts "🔍 FACTORY.handle: Created handle class: #{handle.class}"
          puts "🔍 FACTORY.handle: Handle methods: #{handle.methods.grep(/foundation/).inspect}"
          handle
        end
      end
    end
  end
end
