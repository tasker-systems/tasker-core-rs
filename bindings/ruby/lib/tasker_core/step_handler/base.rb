# frozen_string_literal: true

require 'ostruct'
require 'json'
require 'time'

# require_relative '../types'  # Temporarily disabled due to dry-types version issues

module TaskerCore
  module StepHandler
    # Ruby wrapper that delegates to Rust RubyStepHandler for orchestration integration.
    # This uses composition instead of inheritance to work around Magnus FFI limitations.
    #
    # Architecture:
    # - Rust RubyStepHandler: Core orchestration, state management, transactions
    # - Ruby wrapper: Business logic hooks, Ruby-specific tooling, validation
    #
    # Developer Interface (same as Rails engine):
    # - process(task, sequence, step) - Main business logic (Ruby implementation)
    # - process_results(step, output, initial_results = nil) - Optional transformation
    #
    # FFI Bridge Interface (called by Rust orchestration):
    # - process_with_context(context_json) - Bridge to Rails signature
    # - process_results_with_context(context_json, result_json) - Bridge to Rails signature
    class Base
      # NOTE: We no longer include Dry::Events::Publisher directly because
      # events come from the Rust orchestration layer (source of truth).
      # This class provides Ruby-specific functionality that wraps around
      # the Rust foundation where event publishing actually happens.

      attr_reader :config, :logger, :rust_integration, :rust_handler

      def initialize(config: {}, logger: nil, rust_integration: nil)
        @config = config || {}
        @logger = logger || TaskerCore::Logging::Logger.new
        @rust_integration = rust_integration || default_rust_integration

        # Initialize the Rust RubyStepHandler with composition (delegation pattern)
        begin
          handler_class = self.class.name
          step_name = extract_step_name_from_class
          @rust_handler = TaskerCore::RubyStepHandler.new(handler_class, step_name, @config)
          @logger.info 'Successfully created Rust RubyStepHandler instance'
        rescue StandardError => e
          @logger.error "Failed to create Rust RubyStepHandler: #{e.message}"
          @rust_handler = nil
        end

        # NOTE: Step handlers do not register themselves with the orchestration system
        # They are discovered through task configuration by task handlers
      end

      # ========================================================================
      # STEP HANDLER IDENTIFICATION
      # ========================================================================

      # Extract step name from class name
      # e.g., "PaymentProcessing::StepHandler::ValidatePaymentHandler" -> "validate_payment"
      # e.g., "TestStepHandler" -> "test_step"
      def extract_step_name_from_class
        class_name = self.class.name.split('::').last
        # Remove "StepHandler" or "Handler" suffix if present
        step_name = class_name.sub(/(Step)?Handler$/, '')
        # Convert to snake_case
        snake_case = step_name.gsub(/([A-Z])/, '_\1').downcase.sub(/^_/, '')
        # Handle special case where result is just "test" -> "test_step"
        snake_case == 'test' ? 'test_step' : snake_case
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
      def process(task, sequence, step)
        raise NotImplementedError, 'Subclasses must implement #process(task, sequence, step)'
      end

      # Bridge method for Rust StepHandler trait compatibility
      # This method is called by the Rust orchestration system and converts
      # the StepExecutionContext to the Rails-compatible signature
      # @param context_json [String] JSON representation of StepExecutionContext
      # @return [String] JSON representation of the result
      def process_with_context(context_json)
        # 🎯 HANDLE-BASED: Direct Ruby context conversion instead of FFI bridge
        # The RubyStepHandler in Rust handles the orchestration integration,
        # this method provides the Ruby-side processing logic

        # Parse the StepExecutionContext from JSON
        context = JSON.parse(context_json)

        # Convert context to Rails-compatible objects
        # Note: This is a simplified conversion - in a full implementation,
        # we would create proper Task, StepSequence, and WorkflowStep objects
        mock_task = create_mock_task_from_context(context)
        mock_sequence = create_mock_sequence_from_context(context)
        mock_step = create_mock_step_from_context(context)

        # Call the Rails-compatible process method
        result = process(mock_task, mock_sequence, mock_step)

        # Ensure result is JSON serializable
        JSON.generate(result)
      rescue JSON::ParserError => e
        # Return error in JSON format for parsing errors
        JSON.generate({
                        error: {
                          message: "Invalid JSON context: #{e.message}",
                          type: e.class.name,
                          permanent: true,
                          error_code: 'INVALID_JSON_CONTEXT',
                          error_category: 'validation'
                        }
                      })
      rescue StandardError => e
        # Return error in JSON format
        JSON.generate({
                        error: {
                          message: e.message,
                          type: e.class.name,
                          permanent: e.is_a?(TaskerCore::PermanentError),
                          error_code: e.respond_to?(:error_code) ? e.error_code : nil,
                          error_category: e.respond_to?(:error_category) ? e.error_category : 'unknown'
                        }
                      })
      end

      # Optional result transformation method - Rails engine signature
      # @param step [Tasker::WorkflowStep] Current step being processed
      # @param process_output [Object] Result from process() method
      # @param initial_results [Object] Previous results if this is a retry
      # @return [Object] Transformed result (defaults to process_output)
      def process_results(_step, process_output, _initial_results = nil)
        # Default implementation returns process output unchanged
        # Rails handlers can override this to transform results before storage
        process_output
      end

      # Bridge method for Rust StepHandler trait compatibility
      # This method is called by the Rust orchestration system for result processing
      # @param context_json [String] JSON representation of StepExecutionContext
      # @param result_json [String] JSON representation of StepResult
      # @return [String] JSON representation of success/failure
      def process_results_with_context(context_json, result_json)
        # The RubyStepHandler is for orchestration-level integration, not direct FFI calls
        # For now, use manual JSON processing to call the existing process_results method

        # Parse the inputs from JSON - check context first for early error detection
        JSON.parse(context_json)
        result = JSON.parse(result_json)

        # TODO: Create Ruby objects from the JSON data
        # For now, just call the existing process_results method with minimal conversion

        # Extract key data from result
        process_output = result['output_data'] || {}

        # Call the Rails-compatible process_results method
        # Note: We're passing simplified parameters since we don't have full models
        transformed_result = process_results(nil, process_output, nil)

        # Return success response
        JSON.generate({ status: 'success', transformed_result: transformed_result })
      rescue JSON::ParserError => e
        # Return error response for JSON parsing errors
        JSON.generate({
                        status: 'error',
                        error: {
                          message: e.message,
                          type: e.class.name,
                          permanent: true
                        }
                      })
      rescue StandardError => e
        # Return error response
        JSON.generate({
                        status: 'error',
                        error: {
                          message: e.message,
                          type: e.class.name,
                          permanent: e.is_a?(TaskerCore::PermanentError)
                        }
                      })
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
      # 3. handle_one_step → get_step_handler(step) → step_handler.process(task, sequence, step)
      #
      # The Rust foundation provides the same flow, so there's no need for a separate
      # execute_step method. The Rails engine signature process(task, sequence, step)
      # is the main entry point that frameworks call.
      #
      # The lifecycle management (events, error handling, retries) is handled by:
      # - Rust orchestration layer with TaskHandler/WorkflowCoordinator foundation
      # - Ruby subclasses provide business logic hooks via process() and process_results()

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
        TaskerCore::RailsIntegration.new
      end

      # 🎯 HANDLE-BASED: Context conversion helpers for process_with_context
      # These methods convert StepExecutionContext JSON to Rails-compatible objects

      # Create mock Task object from StepExecutionContext
      def create_mock_task_from_context(context)
        # Create a simple object that responds to the methods needed by process()
        OpenStruct.new(
          task_id: context['task_id'],
          id: context['task_id'],
          name: context['task_name'],
          status: context['task_status'] || 'processing',
          context_data: context['context_data'] || {},
          created_at: (Time.parse(context['created_at']) rescue Time.now),
          updated_at: (Time.parse(context['updated_at']) rescue Time.now)
        )
      end

      # Create mock StepSequence object from StepExecutionContext
      def create_mock_sequence_from_context(context)
        # Create a simple object that responds to the methods needed by process()
        OpenStruct.new(
          total_steps: context['total_steps'] || 1,
          current_position: context['current_position'] || 0,
          completed_steps: context['completed_steps'] || 0,
          failed_steps: context['failed_steps'] || 0,
          pending_steps: context['pending_steps'] || 1
        )
      end

      # Create mock WorkflowStep object from StepExecutionContext
      def create_mock_step_from_context(context)
        # Create a simple object that responds to the methods needed by process()
        OpenStruct.new(
          id: context['step_id'],
          workflow_step_id: context['step_id'],
          name: context['step_name'],
          status: context['step_status'] || 'pending',
          step_type: context['step_type'] || 'processing',
          config: context['step_config'] || {},
          dependencies: context['dependencies'] || [],
          created_at: (Time.parse(context['step_created_at']) rescue Time.now),
          updated_at: (Time.parse(context['step_updated_at']) rescue Time.now)
        )
      end
    end
  end
end
