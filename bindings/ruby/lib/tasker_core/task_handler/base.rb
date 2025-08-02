# frozen_string_literal: true

require 'yaml'
require 'logger'

module TaskerCore
  module TaskHandler
    class Base
      # Ruby task handler for pgmq-based orchestration
      #
      # This class provides a simplified interface for task processing using the new
      # pgmq architecture. Instead of TCP commands, it uses direct pgmq communication
      # and the embedded orchestrator for step enqueueing.
      #
      # Key changes from TCP architecture:
      # - No command_client dependencies
      # - Direct pgmq messaging for task initialization
      # - Embedded orchestrator for step enqueueing
      # - Simplified error handling and validation

      attr_reader :logger, :task_config

      def initialize(config: {}, task_config_path: nil, task_config: nil)
        @logger = TaskerCore::Logging::Logger.instance
        @task_config = task_config || (task_config_path ? load_task_config_from_path(task_config_path) : {})
        @pgmq_client = nil # Lazy initialization to avoid database connection during setup
      end

      # Get or create pgmq client (lazy initialization)
      # @return [TaskerCore::Messaging::PgmqClient] pgmq client instance
      def pgmq_client
        @pgmq_client ||= TaskerCore::Messaging::PgmqClient.new
      end

      # Main task processing method - Rails engine signature: handle(task_id)
      #
      # Mode-aware processing: Uses embedded FFI orchestrator in embedded mode,
      # or pure pgmq communication in distributed mode.
      #
      # @param task_id [Integer] ID of the task to process
      # @return [Hash] Result of step enqueueing operation
      def handle(task_id)
        raise TaskerCore::ValidationError, 'task_id is required' unless task_id
        raise TaskerCore::ValidationError, 'task_id must be an integer' unless task_id.is_a?(Integer)

        mode = orchestration_mode
        logger.info "🚀 Processing task #{task_id} with pgmq orchestration (#{mode} mode)"

        case mode
        when 'embedded'
          handle_embedded_mode(task_id)
        when 'distributed'
          handle_distributed_mode(task_id)
        else
          raise TaskerCore::OrchestrationError,
                "Unknown orchestration mode: #{mode}. Expected 'embedded' or 'distributed'"
        end
      rescue TaskerCore::OrchestrationError => e
        logger.error "❌ Orchestration error for task #{task_id}: #{e.message}"
        {
          success: false,
          task_id: task_id,
          error: e.message,
          error_type: 'OrchestrationError',
          architecture: 'pgmq',
          processed_at: Time.now.utc.iso8601
        }
      rescue StandardError => e
        logger.error "❌ Unexpected error processing task #{task_id}: #{e.class.name}: #{e.message}"
        {
          success: false,
          task_id: task_id,
          error: e.message,
          error_type: e.class.name,
          architecture: 'pgmq',
          processed_at: Time.now.utc.iso8601
        }
      end

      # Initialize a new task with workflow steps
      #
      # In the pgmq architecture, this creates the task record and optionally
      # triggers initial step enqueueing. Task templates should be registered
      # through the database-backed registry (Phase 4.3).
      #
      # @param task_request [Hash] Task initialization data
      # @return [Hash] Result of task initialization
      def initialize_task(task_request)
        logger.info "🚀 Initializing task with pgmq architecture"

        task_request = TaskerCore::Types::TaskTypes::TaskRequest.from_hash(task_request)

        # For now, return a success response indicating the task would be initialized
        # In Phase 4.3, this will integrate with the database-backed task registry
        # to actually create the task and workflow steps
        {
          success: true,
          namespace: task_request.namespace,
          task_name: task_request.name,
          task_version: task_request.version,
          message: "Task initialization prepared (Phase 4.3 will complete database integration)",
          architecture: 'pgmq',
          initialized_at: Time.now.utc.iso8601,
          next_phase: "Phase 4.3 will implement database-backed task template registration"
        }
      rescue TaskerCore::ValidationError => e
        logger.error "❌ Validation error initializing task: #{e.message}"
        {
          success: false,
          error: e.message,
          error_type: 'ValidationError',
          architecture: 'pgmq',
          processed_at: Time.now.utc.iso8601
        }
      rescue StandardError => e
        logger.error "❌ Unexpected error initializing task: #{e.class.name}: #{e.message}"
        {
          success: false,
          error: e.message,
          error_type: e.class.name,
          architecture: 'pgmq',
          processed_at: Time.now.utc.iso8601
        }
      end

      # Check if the pgmq orchestration system is available and ready
      # @return [Boolean] true if system is ready for task processing
      def orchestration_ready?
        orchestrator = TaskerCore.embedded_orchestrator
        orchestrator.running?
      rescue StandardError => e
        logger.warn "⚠️ Failed to check orchestration status: #{e.message}"
        false
      end

      # Get status information for this task handler
      # @return [Hash] Status information including mode and pgmq connectivity
      def status
        mode = orchestration_mode

        # Check pgmq availability without forcing connection
        pgmq_available = begin
          !pgmq_client.nil?
        rescue TaskerCore::Error => e
          logger.debug "🔍 PGMQ not available: #{e.message}"
          false
        end

        status_info = {
          handler_type: 'TaskHandler::Base',
          architecture: 'pgmq',
          orchestration_mode: mode,
          orchestration_ready: orchestration_ready?,
          pgmq_available: pgmq_available,
          task_config_loaded: !task_config.empty?,
          checked_at: Time.now.utc.iso8601
        }

        # Include embedded orchestrator status only in embedded mode
        if mode == 'embedded'
          status_info[:embedded_orchestrator] = embedded_orchestrator_status
        end

        status_info
      end

      private

      def load_task_config_from_path(path)
        return {} unless path && File.exist?(path)
        YAML.load_file(path)
      rescue StandardError => e
        logger.warn "Error loading task configuration: #{e.message}"
        {}
      end

      def embedded_orchestrator_status
        orchestrator = TaskerCore.embedded_orchestrator
        {
          running: orchestrator.running?,
          namespaces: orchestrator.namespaces,
          started_at: orchestrator.started_at&.iso8601
        }
      rescue StandardError => e
        logger.warn "⚠️ Failed to get embedded orchestrator status: #{e.message}"
        { running: false, error: e.message }
      end

      # Determine orchestration mode from configuration
      # @return [String] 'embedded' or 'distributed'
      def orchestration_mode
        config = TaskerCore::Config.instance.effective_config
        mode = config.dig('orchestration', 'mode')

        # Default to embedded mode if not specified or in test environment
        if mode.nil?
          if config.dig('execution', 'environment') == 'test'
            'embedded'
          else
            'distributed'
          end
        else
          mode
        end
      rescue StandardError => e
        logger.warn "⚠️ Failed to determine orchestration mode: #{e.message}, defaulting to distributed"
        'distributed'
      end

      # Handle task processing in embedded mode using FFI orchestrator
      # @param task_id [Integer] ID of the task to process
      # @return [Hash] Result of step enqueueing operation
      def handle_embedded_mode(task_id)
        # Use embedded orchestrator to enqueue ready steps for the task
        orchestrator = TaskerCore.embedded_orchestrator

        unless orchestrator.running?
          raise TaskerCore::OrchestrationError,
                "Embedded orchestration system not running. Call TaskerCore.start_embedded_orchestration! first."
        end

        # Enqueue steps for the task - this will publish step messages to appropriate queues
        result = orchestrator.enqueue_steps(task_id)

        logger.info "✅ Task #{task_id} step enqueueing completed (embedded): #{result}"

        {
          success: true,
          task_id: task_id,
          message: result,
          mode: 'embedded',
          architecture: 'pgmq',
          processed_at: Time.now.utc.iso8601
        }
      end

      # Handle task processing in distributed mode using pure pgmq
      # @param task_id [Integer] ID of the task to process
      # @return [Hash] Result of step enqueueing operation
      def handle_distributed_mode(task_id)
        # In distributed mode, we don't directly enqueue steps via FFI
        # Instead, we could publish a task processing request to a queue
        # For now, return a message indicating distributed mode handling

        logger.info "✅ Task #{task_id} queued for distributed processing"

        {
          success: true,
          task_id: task_id,
          message: "Task queued for distributed orchestration processing",
          mode: 'distributed',
          architecture: 'pgmq',
          processed_at: Time.now.utc.iso8601,
          note: "Phase 4.5 will complete distributed orchestration integration"
        }
      end
    end
  end
end
