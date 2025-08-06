# frozen_string_literal: true

require_relative 'internal/distributed_handler_registry'
require_relative 'internal/orchestration_manager'

module TaskerCore
  # Domain module for orchestration operations with pgmq-based architecture
  #
  # This module provides a clean, Ruby-idiomatic API for workflow orchestration
  # using PostgreSQL message queues (pgmq) for reliable step enqueueing and processing.
  #
  # Examples:
  #   health = TaskerCore::Orchestration.health_check
  #   TaskerCore::Orchestration.bootstrap_queues
  module Orchestration
    class << self
      # Perform orchestration health check
      # @return [Hash] Health status and metrics
      def health_check
        manager = Internal::OrchestrationManager.instance
        info = manager.info

        {
          'status' => info[:status] == 'initialized' ? 'healthy' : 'unhealthy',
          'architecture' => 'pgmq',
          'mode' => info[:mode],
          'initialized' => info[:initialized],
          'pgmq_available' => info[:pgmq_available],
          'embedded_orchestrator_available' => info[:embedded_orchestrator_available],
          'queues_initialized' => info[:queues_initialized],
          'checked_at' => Time.now.utc.iso8601
        }
      rescue StandardError => e
        {
          'status' => 'unhealthy',
          'architecture' => 'pgmq',
          'error' => e.message,
          'checked_at' => Time.now.utc.iso8601
        }
      end

      # Get health status of the Orchestration domain
      # @return [Hash] Health status and metadata
      def health
        health_result = health_check

        {
          'health_status' => health_result['status'] || 'unknown',
          'domain' => 'Orchestration',
          'architecture' => 'pgmq',
          'mode' => health_result['mode'],
          'metrics' => health_result,
          'checked_at' => Time.now.utc.iso8601
        }
      end

      # Get information about the Orchestration domain for debugging
      # @return [Hash] Domain status and metadata
      def info
        manager = Internal::OrchestrationManager.instance
        manager_info = manager.info

        base_info = {
          'domain' => 'Orchestration',
          'architecture' => 'pgmq',
          'mode' => manager_info[:mode],
          'status' => manager_info[:status],
          'initialized' => manager_info[:initialized],
          'initialized_at' => manager_info[:initialized_at]&.iso8601,
          'available_methods' => %w[health_check bootstrap_queues info]
        }

        base_info.merge(manager_info.transform_keys(&:to_s))
      rescue StandardError => e
        { 'error' => e.message, 'status' => 'unavailable', 'domain' => 'Orchestration', 'architecture' => 'pgmq' }
      end

      # Bootstrap orchestration queues based on configuration
      # @return [Hash] Bootstrap operation result
      def bootstrap_queues
        logger.info '🗂️ Bootstrapping orchestration queues'

        manager = Internal::OrchestrationManager.instance
        manager.bootstrap_orchestration_system unless manager.initialized?

        {
          'status' => 'success',
          'message' => 'Orchestration system bootstrapped',
          'queues_initialized' => manager.info[:queues_initialized],
          'bootstrapped_at' => Time.now.utc.iso8601
        }
      rescue StandardError => e
        logger.error "❌ Failed to bootstrap queues: #{e.message}"
        {
          'status' => 'error',
          'error' => e.message,
          'bootstrapped_at' => Time.now.utc.iso8601
        }
      end

      # Get current orchestration mode
      # @return [String] Current orchestration mode ('embedded' or 'distributed')
      def orchestration_mode
        Internal::OrchestrationManager.instance.orchestration_mode
      end

      private

      # Get logger instance
      def logger
        TaskerCore::Logging::Logger.instance
      end
    end

    # Provide access to internal orchestration components for backward compatibility
    Manager = Internal::OrchestrationManager
    HandlerRegistry = DistributedHandlerRegistry
  end
end
