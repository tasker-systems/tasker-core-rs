# frozen_string_literal: true

module TaskerCore
  # Domain module for performance monitoring and analytics with singleton handle management
  # 
  # This module provides a clean, Ruby-idiomatic API for performance operations
  # while internally managing a persistent OrchestrationHandle for optimal performance.
  #
  # Examples:
  #   health = TaskerCore::Performance.system_health
  #   metrics = TaskerCore::Performance.analytics
  #   deps = TaskerCore::Performance.dependencies(task_id)
  module Performance
    class << self
      # Get current system health metrics
      # @return [Hash] System health data including database, pool, and service status
      # @raise [TaskerCore::Error] If health check fails
      def system_health
        OrchestrationManager.instance.get_system_health_with_handle
      rescue => e
        raise TaskerCore::Error, "Failed to get system health: #{e.message}"
      end

      # Get analytics metrics for system performance
      # @return [Hash] Analytics data including performance metrics and statistics
      # @raise [TaskerCore::Error] If analytics retrieval fails
      def analytics
        OrchestrationManager.instance.get_analytics_metrics_with_handle
      rescue => e
        raise TaskerCore::Error, "Failed to get analytics: #{e.message}"
      end

      # Analyze dependencies for a specific task
      # @param task_id [Integer] Task ID to analyze
      # @return [Hash] Dependency analysis including resolution paths and bottlenecks
      # @raise [TaskerCore::Error] If dependency analysis fails
      def dependencies(task_id)
        OrchestrationManager.instance.analyze_dependencies_with_handle(task_id)
      rescue => e
        raise TaskerCore::Error, "Failed to analyze dependencies: #{e.message}"
      end

      # Get task execution context for performance analysis
      # @param task_id [Integer] Task ID to get context for
      # @return [Hash] Task execution context with performance data
      # @raise [TaskerCore::Error] If context retrieval fails
      def task_execution_context(task_id)
        OrchestrationManager.instance.get_task_execution_context_with_handle(task_id)
      rescue => e
        raise TaskerCore::Error, "Failed to get task execution context: #{e.message}"
      end

      # Discover viable steps for a task
      # @param task_id [Integer] Task ID to discover steps for
      # @return [Array<Hash>] List of viable steps with readiness information
      # @raise [TaskerCore::Error] If step discovery fails
      def viable_steps(task_id)
        OrchestrationManager.instance.discover_viable_steps_with_handle(task_id)
      rescue => e
        raise TaskerCore::Error, "Failed to discover viable steps: #{e.message}"
      end

      # Get handle information for debugging
      # @return [Hash] Handle status and metadata
      def handle_info
        # Performance operations use OrchestrationManager singleton handle
        OrchestrationManager.instance.handle_info
      rescue => e
        { error: e.message, status: "unavailable" }
      end
    end
  end
end