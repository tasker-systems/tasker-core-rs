# frozen_string_literal: true

module TaskerCore
  # Embedded Orchestration System for Testing
  #
  # Provides lightweight lifecycle management for the Rust orchestration system
  # within Ruby processes, enabling local integration testing without docker-compose.
  #
  # Design Principles:
  # - Lightweight: Only lifecycle management (start/stop/status)
  # - No complex state sharing between Ruby and Rust
  # - Same pgmq-based architecture, just running in-process
  # - Perfect for local development and testing scenarios
  #
  # Usage:
  #   orchestrator = TaskerCore::EmbeddedOrchestrator.new(['fulfillment', 'inventory'])
  #   orchestrator.start
  #   # ... run tests ...
  #   orchestrator.stop
  class EmbeddedOrchestrator
    attr_reader :namespaces, :started_at

    def initialize(namespaces = %w[fulfillment inventory notifications])
      @namespaces = Array(namespaces)
      @started_at = nil
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Start the embedded orchestration system
    #
    # Initializes the Rust orchestration system in a background thread using
    # the same pgmq architecture that runs in production.
    #
    # @return [Boolean] true if started successfully, false if already running
    # @raise [TaskerCore::OrchestrationError] if startup fails
    def start
      return false if running?

      @logger.info "🚀 EMBEDDED_ORCHESTRATOR: Starting with namespaces: #{@namespaces.join(', ')}"

      begin
        result = TaskerCore.start_embedded_orchestration(@namespaces)
        @started_at = Time.now
        @logger.info "✅ EMBEDDED_ORCHESTRATOR: #{result}"
        true
      rescue StandardError => e
        @logger.error "❌ EMBEDDED_ORCHESTRATOR: Failed to start - #{e.message}"
        raise TaskerCore::OrchestrationError, "Failed to start embedded orchestration: #{e.message}"
      end
    end

    # Stop the embedded orchestration system
    #
    # Gracefully shuts down the background orchestration system.
    #
    # @return [Boolean] true if stopped successfully, false if not running
    def stop
      return false unless running?

      @logger.info '🛑 EMBEDDED_ORCHESTRATOR: Stopping orchestration system'

      begin
        result = TaskerCore.stop_embedded_orchestration
        @started_at = nil
        @logger.info "✅ EMBEDDED_ORCHESTRATOR: #{result}"
        true
      rescue StandardError => e
        @logger.error "❌ EMBEDDED_ORCHESTRATOR: Failed to stop - #{e.message}"
        # Don't raise error on stop - just log and continue
        false
      end
    end

    # Check if orchestration system is running
    #
    # @return [Boolean] true if system is running
    def running?
      status_info[:running] == true
    rescue StandardError
      false
    end

    # Get detailed status information
    #
    # @return [Hash] status information including:
    #   - running: boolean indicating if system is active
    #   - database_pool_size: size of database connection pool
    #   - database_pool_idle: number of idle database connections
    #   - uptime_seconds: seconds since startup (if running)
    def status
      base_status = status_info

      base_status[:uptime_seconds] = (Time.now - @started_at).to_i if base_status[:running] && @started_at

      base_status.merge(
        namespaces: @namespaces,
        started_at: @started_at&.iso8601
      )
    rescue StandardError => e
      @logger.error "❌ EMBEDDED_ORCHESTRATOR: Failed to get status - #{e.message}"
      {
        running: false,
        error: e.message,
        namespaces: @namespaces,
        started_at: @started_at&.iso8601
      }
    end

    # Enqueue ready steps for a task (testing helper)
    #
    # This provides a simple way for tests to trigger step enqueueing
    # without complex orchestration setup.
    #
    # @param task_uuid [Integer] the task ID to process
    # @return [String] result message
    # @raise [TaskerCore::OrchestrationError] if system not running or enqueueing fails
    def enqueue_steps(task_uuid)
      raise TaskerCore::OrchestrationError, 'Orchestration system not running' unless running?

      @logger.info "🚀 EMBEDDED_ORCHESTRATOR: Enqueueing steps for task #{task_uuid}"

      begin
        result = TaskerCore.enqueue_task_steps(task_uuid)
        @logger.info "✅ EMBEDDED_ORCHESTRATOR: #{result}"
        result
      rescue StandardError => e
        @logger.error "❌ EMBEDDED_ORCHESTRATOR: Failed to enqueue steps for task #{task_uuid} - #{e.message}"
        raise TaskerCore::OrchestrationError, "Failed to enqueue steps: #{e.message}"
      end
    end

    # Automatic cleanup when object is garbage collected
    def finalize
      stop if running?
    end

    # Ensure cleanup happens
    def self.finalize(orchestrator_proc)
      proc { orchestrator_proc.call }
    end

    private

    # Get raw status information from Rust
    def status_info
      TaskerCore.embedded_orchestration_status
    rescue StandardError => e
      @logger.debug "Status check failed: #{e.message}"
      { running: false }
    end
  end

  class << self
    # Global embedded orchestrator instance for convenience
    #
    # This provides a singleton-like interface for tests that don't need
    # multiple orchestrator instances.
    def embedded_orchestrator
      @embedded_orchestrator ||= EmbeddedOrchestrator.new
    end

    # Start global embedded orchestrator
    def start_embedded_orchestration!(namespaces = nil)
      @embedded_orchestrator = EmbeddedOrchestrator.new(namespaces) if namespaces
      embedded_orchestrator.start
    end

    # Stop global embedded orchestrator
    def stop_embedded_orchestration!
      embedded_orchestrator.stop if @embedded_orchestrator
    end

    # Check if global embedded orchestrator is running
    def embedded_orchestration_running?
      @embedded_orchestrator&.running? || false
    end
  end
end

# Ensure cleanup on process exit
at_exit do
  TaskerCore.stop_embedded_orchestration! if TaskerCore.embedded_orchestration_running?
end
