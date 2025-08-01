# frozen_string_literal: true

require_relative 'orchestration/enhanced_handler_registry'
require_relative 'orchestration/orchestration_manager'

module TaskerCore
  # Domain module for orchestration operations with singleton handle management
  #
  # This module provides a clean, Ruby-idiomatic API for workflow orchestration
  # while internally managing a persistent OrchestrationHandle for optimal performance.
  #
  # Examples:
  #   result = TaskerCore::Orchestration.execute_workflow(task_id: 123)
  #   health = TaskerCore::Orchestration.health_check
  #   batch = TaskerCore::Orchestration.execute_batch(steps: [...])
  module Orchestration
    class << self
      # Execute a batch of workflow steps
      # @param steps [Array<Hash>] Array of step execution requests
      # @param namespace [String] Namespace for step execution (default: "default")
      # @return [Hash] Batch execution result
      # @raise [TaskerCore::Error] If batch execution fails
      def execute_batch(steps:, namespace: "default")
        # Convert steps to proper format if needed
        formatted_steps = steps.map do |step|
          step.is_a?(Hash) ? step : step.to_h
        end

        Internal::OrchestrationManager.instance.execute_batch_with_handle(
          steps: formatted_steps,
          namespace: namespace
        )
      rescue => e
        raise TaskerCore::Error, "Failed to execute batch: #{e.message}"
      end

      # Perform orchestration health check
      # @return [Hash] Health status and metrics
      def health_check
        Internal::OrchestrationManager.instance.orchestration_health_check
      rescue => e
        {
          'status' => 'unhealthy',
          'error' => e.message,
          'checked_at' => Time.now.utc.iso8601
        }
      end

      # Get health status of the Orchestration domain
      # @return [Hash] Health status and metadata
      def health
        health_result = health_check
        handle_status = handle_info

        {
          'health_status' => health_result['status'] || 'unknown',
          'domain' => 'Orchestration',
          'handle_status' => handle_status['status'],
          'metrics' => health_result,
          'checked_at' => Time.now.utc.iso8601
        }
      end

      # Get information about the Orchestration domain for debugging
      # @return [Hash] Domain status and metadata
      def handle_info
        info = handle.info
        # Handle OrchestrationHandleInfo object
        base_info = if info.is_a?(Hash)
          info
        else
          {
            'handle_id' => "shared_orchestration_handle",
            'status' => 'operational',
            'handle_type' => 'orchestration_handle',
            'created_at' => Time.now.utc.iso8601
          }
        end

        base_info.merge(
          'domain' => 'Orchestration',
          'available_methods' => %w[execute_workflow execute_batch health_check]
        )
      rescue => e
        { 'error' => e.message, 'status' => "unavailable", 'domain' => "Orchestration" }
      end

      private

      # Get orchestration handle with automatic refresh
      # @return [OrchestrationHandle] Active handle instance
      def handle
        TaskerCore::Internal::OrchestrationManager.instance.orchestration_handle
      end
    end

    # Provide access to internal orchestration components for backward compatibility
    Manager = Internal::OrchestrationManager
    HandlerRegistry = EnhancedHandlerRegistry
  end
end
