# frozen_string_literal: true

require 'ostruct'
require 'json'
require 'time'

# require_relative '../types'  # Temporarily disabled due to dry-types version issues

module TaskerCore
  module StepHandler

    class Base
      attr_reader :config, :logger, :rust_integration, :orchestration_system

      def initialize(config: {}, logger: nil)
        @config = config || {}
        @logger = logger || TaskerCore::Logging::Logger.instance
        orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
        @orchestration_system = orchestration_manager.orchestration_system
      end

      # ========================================================================
      # MAIN STEP EXECUTION INTERFACE (implemented by subclasses)
      # ========================================================================

      # Main business logic method - Rails engine signature: process(task, sequence, step)
      # Subclasses implement the actual business logic in this method
      # @param task [Tasker::Task] Task model instance with context data
      # @param sequence [Tasker::Types::StepSequence] Step sequence for navigation
      # @param step [Tasker::WorkflowStep] Current step being processed
      # @return [Object] Step results (Hash, Array, String, etc.)
      def call(task, sequence, step)
        raise NotImplementedError, 'Subclasses must implement #call(task, sequence, step)'
      end


      # ========================================================================
      # CONFIGURATION AND METADATA
      # ========================================================================

      # Validate step handler configuration
      # @param config_hash [Hash] Configuration to validate
      # @return [Boolean] Whether configuration is valid
      def validate_config(config_hash)
        # Basic validation - subclasses can override for specific requirements
        config_hash.is_a?(Hash)
      end

      # Get handler name for registration and logging
      # @return [String] Handler name
      def handler_name
        class_name = self.class.name.split('::').last
        # Convert CamelCase to snake_case (like Rails underscore method)
        class_name.gsub(/([A-Z])/, '_\1').downcase.sub(/^_/, '')
      end

      # Get handler metadata for monitoring and introspection
      # @return [Hash] Handler metadata
      def metadata
        {
          handler_name: handler_name,
          handler_class: self.class.name,
          version: begin
            self.class.const_get(:VERSION)
          rescue StandardError
            '1.0.0'
          end,
          capabilities: capabilities,
          config_schema: config_schema,
          ruby_version: RUBY_VERSION,
          created_at: Time.now.iso8601
        }
      end

      # Get handler capabilities
      # @return [Array<String>] List of capabilities
      def capabilities
        caps = ['process']
        caps << 'process_results' if respond_to?(:process_results, true)
        caps << 'async' if respond_to?(:process_async, true)
        caps << 'streaming' if respond_to?(:process_stream, true)
        caps
      end

      # Get configuration schema for validation
      # @return [Hash] JSON schema describing expected configuration
      def config_schema
        {
          type: 'object',
          properties: {
            timeout: { type: 'integer', minimum: 1, default: 300 },
            retries: { type: 'integer', minimum: 0, default: 3 },
            log_level: { type: 'string', enum: %w[debug info warn error], default: 'info' }
          },
          additionalProperties: true
        }
      end

      # ========================================================================
      # RAILS ENGINE INTEGRATION NOTES
      # ========================================================================

      # NOTE: In the Rails engine execution flow, step handlers are called via:
      # 1. TaskHandler.handle(task) → WorkflowCoordinator
      # 2. WorkflowCoordinator → TaskHandler.handle_one_step(task, sequence, step)
      # 3. handle_one_step → get_step_handler(step) → step_handler.call(task, sequence, step)
      #
      # The Rust foundation provides the same flow, so there's no need for a separate
      # execute_step method. The Rails engine signature process(task, sequence, step)
      # is the main entry point that frameworks call.
      #
      # The lifecycle management (events, error handling, retries) is handled by:
      # - Rust orchestration layer with TaskHandler/WorkflowCoordinator foundation
      # - Ruby subclasses provide business logic hooks via call()

      # ========================================================================
      # INTERNAL PROCESSING METHODS
      # ========================================================================

      private

      # Validate step configuration before execution
      def validate_step_configuration(config_hash)
        # Basic validation - subclasses can override for specific requirements
        config_hash.is_a?(Hash)
      end

      # Extract step information for logging
      def extract_step_info(step)
        {
          step_id: extract_attribute(step, :id) || extract_attribute(step, :workflow_step_id),
          step_name: extract_attribute(step, :name),
          step_class: step.class.name
        }
      end

      # Extract task information for logging
      def extract_task_info(task)
        {
          task_id: extract_attribute(task, :task_id) || extract_attribute(task, :id),
          task_class: task.class.name
        }
      end

      # Safe attribute extraction from Ruby objects
      def extract_attribute(object, attribute)
        object.respond_to?(attribute) ? object.send(attribute) : nil
      rescue StandardError
        nil
      end

      # Validate process method result
      def validate_process_result(result)
        unless result.is_a?(Hash)
          raise TaskerCore::PermanentError.new(
            "Step process method must return Hash, got #{result.class}",
            error_code: 'INVALID_PROCESS_RESULT',
            error_category: 'validation'
          )
        end

        return if result.key?(:status) || result.key?('status')

        logger.warn('Step process result missing status field - assuming success')
        result[:status] = 'completed'
      end

      # Classify unexpected errors into permanent vs retryable
      def classify_unexpected_error(error, _context)
        case error
        when ArgumentError, TypeError, NoMethodError
          # Programming errors are permanent
          TaskerCore::PermanentError.new(
            "Programming error in step handler: #{error.message}",
            error_code: 'PROGRAMMING_ERROR',
            error_category: 'validation',
            context: { original_error: error.class.name, backtrace: error.backtrace&.first(5) }
          )
        when Timeout::Error
          # Timeouts are retryable
          TaskerCore::RetryableError.new(
            "Step handler timeout: #{error.message}",
            error_category: 'timeout',
            context: { timeout_class: error.class.name }
          )
        when IOError, SystemCallError
          # IO errors are usually retryable
          TaskerCore::RetryableError.new(
            "IO error in step handler: #{error.message}",
            error_category: 'network',
            context: { io_error_class: error.class.name }
          )
        else
          # Unknown errors - assume retryable for safety
          logger.error("Unexpected error in step handler: #{error.class} - #{error.message}")
          TaskerCore::RetryableError.new(
            "Unexpected error in step handler: #{error.message}",
            error_category: 'unknown',
            context: {
              original_error: error.class.name,
              backtrace: error.backtrace&.first(5)
            }
          )
        end
      end

      # Format successful result for Rust layer
      def format_success_result(result, duration)
        {
          status: 'completed',
          success: true,
          data: result,
          duration_seconds: duration,
          handler: handler_name,
          timestamp: Time.now.iso8601
        }
      end

      # Format error result for Rust layer
      def format_error_result(error, duration, permanent:)
        {
          status: 'error',
          success: false,
          error: {
            message: error.message,
            type: error.class.name,
            permanent: permanent,
            retry_after: error.respond_to?(:retry_after) ? error.retry_after : nil,
            error_code: error.respond_to?(:error_code) ? error.error_code : nil,
            error_category: error.respond_to?(:error_category) ? error.error_category : 'unknown',
            context: error.respond_to?(:context) ? error.context : {}
          },
          duration_seconds: duration,
          handler: handler_name,
          timestamp: Time.now.iso8601
        }
      end

      # Default Rust integration
      def default_rust_integration
        # Return nil - integration is optional
        nil
      end
    end
  end
end
