# frozen_string_literal: true

require 'json'
require 'time'
require 'logger'
require 'concurrent'
require 'concurrent-ruby'
require_relative '../types/execution_types'
require_relative '../task_handler/base'
require_relative '../step_handler/base'
require_relative 'command_backplane'

module TaskerCore
  module Execution
    # Batch execution handler for processing ExecuteBatch commands from Rust orchestrator
    #
    # This handler receives ExecuteBatch commands containing a TaskTemplate and
    # multiple StepExecutionRequests, then processes them using the existing
    # Ruby TaskHandler and StepHandler infrastructure.
    #
    # @example Basic usage
    #   handler = BatchExecutionHandler.new
    #
    #   # Register with CommandListener
    #   listener.register_handler(:execute_batch) do |command|
    #     handler.call(command)
    #   end
    #
    class BatchExecutionHandler
      attr_reader :logger, :task_handlers, :step_results

      def initialize(worker_id: nil)
        @logger = TaskerCore::Logging::Logger.instance
        @task_handlers = {}
        @step_results = Concurrent::Map.new  # Thread-safe map for concurrent step result storage
        @handler_mutex = Mutex.new
        @worker_id = worker_id
      end

      # Handle ExecuteBatch command from Rust orchestrator
      #
      # @param command [TaskerCore::Types::ExecutionTypes::ExecuteBatchResponse] ExecuteBatch command
      # @return [Hash] Batch execution response
      def handle(command)
        unless command.execute_batch?
          raise ArgumentError, "Expected ExecuteBatch command, got #{command.command_type}"
        end

        batch_id = command.batch_id
        steps = command.steps
        task_template = command.task_template

        logger.info("Processing ExecuteBatch: batch_id=#{batch_id}, steps=#{steps.size}")

        begin
          # Process all steps concurrently - Rust orchestration has already determined viable steps
          total_start_time = Time.now

          logger.debug("Processing #{steps.length} steps concurrently using Promises (orchestration pre-selected viable steps)")
          
          # Create promises for all steps to run in parallel
          step_promises = steps.map do |step_request|
            Concurrent::Promises.future do
              process_step_and_send_results(step_request, task_template, batch_id)
            end
          end
          
          logger.debug("All #{step_promises.length} step promises created, waiting for completion...")
          
          # Wait for all promises to complete and collect results (this is the only blocking operation)
          step_summaries = Concurrent::Promises.zip(*step_promises).value!
          
          logger.debug("All step promises completed, collected #{step_summaries.size} step summaries")

          total_execution_time = ((Time.now - total_start_time) * 1000).to_i

          logger.debug("Sending batch completion response for #{step_summaries.size} steps")
          
          # Send final batch completion response
          send_batch_completion_response(
            command.command_id,
            batch_id,
            step_summaries,
            total_execution_time
          )

        rescue StandardError => e
          logger.error("Batch execution failed: #{e.message}")
          logger.error("Error class: #{e.class}")
          logger.error(e.backtrace.join("\n"))

          begin
            send_batch_error_response(
              command.command_id,
              batch_id,
              "Batch execution failed: #{e.message}"
            )
          rescue StandardError => send_error
            logger.error("Failed to send batch error response: #{send_error.message}")
            logger.error("Send error class: #{send_error.class}")
            logger.error(send_error.backtrace.join("\n"))
          end
        end
      end

      # Process a step and immediately send results - wrapper for true concurrent execution
      #
      # @param step_request [TaskerCore::Types::ExecutionTypes::StepExecutionRequest] Step request
      # @param task_template [TaskerCore::Types::ExecutionTypes::TaskTemplate] Task template
      # @param batch_id [String] Batch ID for result reporting
      # @return [Hash] Step execution summary
      def process_step_and_send_results(step_request, task_template, batch_id)
        logger.debug("Processing step: #{step_request.step_name} (ID: #{step_request.step_id})")
        
        begin
          # Process the step
          step_summary = process_step(step_request, task_template)
          
          logger.debug("Step #{step_request.step_id} completed, sending partial result")
          
          # Immediately send partial result to Rust orchestrator (don't block other steps)
          send_partial_result_command(
            batch_id,
            step_request.step_id,
            step_summary,
            step_summary[:execution_time_ms]
          )
          
          # Store result in thread-safe map for final collection
          @step_results[step_request.step_id] = step_summary
          
          logger.debug("Step #{step_request.step_id} result stored and sent successfully")
          step_summary
          
        rescue StandardError => e
          logger.error("Failed to process or send step result for #{step_request.step_name}: #{e.message}")
          logger.error("Step processing error: #{e.class}")
          logger.error(e.backtrace.first(3).join(", "))
          
          # Create error step summary
          error_summary = {
            step_id: step_request.step_id,
            step_name: step_request.step_name,
            status: 'failed',
            execution_time_ms: 0,
            result: nil,
            error: e.message,
            retryable: true,
            handler_class: 'unknown'
          }
          
          # Store error result in thread-safe map
          @step_results[step_request.step_id] = error_summary
          
          # Attempt to send error result as partial result
          begin
            send_partial_result_command(
              batch_id,
              step_request.step_id,
              error_summary,
              0
            )
          rescue StandardError => send_error
            logger.error("Failed to send partial error result for step #{step_request.step_id}: #{send_error.message}")
          end
          
          error_summary
        end
      end

      # Process a single step execution request
      #
      # @param step_request [TaskerCore::Types::ExecutionTypes::StepExecutionRequest] Step request
      # @param task_template [TaskerCore::Types::ExecutionTypes::TaskTemplate] Task template
      # @return [Hash] Step execution summary
      def process_step(step_request, task_template)
        step_start_time = Time.now

        begin
          # Find step template for this step
          step_template = find_step_template(task_template, step_request.step_name)
          unless step_template
            raise HandlerError, "Step template not found: #{step_request.step_name}"
          end

          # Create step handler
          step_handler = create_step_handler(step_template)

          # Create properly typed context objects using dry-struct types
          task_context = TaskerCore::Types::ExecutionTypes::TaskContext.from_step_request(step_request)
          sequence_context = TaskerCore::Types::ExecutionTypes::SequenceContext.from_step_request(step_request, task_template, @step_results)
          step_context = TaskerCore::Types::ExecutionTypes::StepContext.from_step_request(step_request, step_template)

          # Create task wrapper with proper interface for step handlers
          task_wrapper = TaskerCore::Types::ExecutionTypes::TaskWrapper.from_task_context(task_context)

          result = step_handler.call(task_wrapper, sequence_context, step_context)

          execution_time = ((Time.now - step_start_time) * 1000).to_i

          # Create successful step summary
          {
            step_id: step_request.step_id,
            step_name: step_request.step_name,
            status: 'completed',
            execution_time_ms: execution_time,
            result: result,
            error: nil,
            retryable: false,
            handler_class: step_template.handler_class
          }

        rescue StandardError => e
          execution_time = ((Time.now - step_start_time) * 1000).to_i

          logger.error("Step execution failed: #{step_request.step_name} - #{e.message}")

          # Create failed step summary
          {
            step_id: step_request.step_id,
            step_name: step_request.step_name,
            status: 'failed',
            execution_time_ms: execution_time,
            result: nil,
            error: e.message,
            retryable: step_request.metadata.attempt < step_request.metadata.retry_limit,
            handler_class: step_template&.handler_class || 'unknown'
          }
        end
      end

      private

      # Get or create task handler for template
      #
      # @param task_template [TaskerCore::Types::ExecutionTypes::TaskTemplate] Task template
      # @return [TaskerCore::TaskHandler::Base] Task handler instance
      def get_task_handler(task_template)
        cache_key = "#{task_template.namespace_name}/#{task_template.name}/#{task_template.version}"

        @handler_mutex.synchronize do
          return @task_handlers[cache_key] if @task_handlers[cache_key]

          # Convert TaskTemplate to config hash for Ruby TaskHandler
          task_config = {
            'name' => task_template.name,
            'namespace_name' => task_template.namespace_name,
            'version' => task_template.version,
            'task_handler_class' => task_template.task_handler_class,
            'module_namespace' => task_template.module_namespace,
            'description' => task_template.description,
            'default_dependent_system' => task_template.default_dependent_system,
            'named_steps' => task_template.named_steps || [],
            'schema' => task_template.schema,
            'step_templates' => task_template.step_templates.map(&:to_h),
            'environments' => task_template.environments,
            'custom_events' => task_template.custom_events
          }

          # Create task handler instance
          handler_class = constantize_or_raise(task_template.task_handler_class)
          if handler_class && handler_class < TaskerCore::TaskHandler::Base
            @task_handlers[cache_key] = handler_class.new(task_config: task_config)
          else
            # Fall back to base task handler
            @task_handlers[cache_key] = TaskerCore::TaskHandler::Base.new(task_config: task_config)
          end

          logger.debug("Created task handler: #{cache_key}")
          @task_handlers[cache_key]
        end
      end

      # Find step template in task template
      #
      # @param task_template [TaskerCore::Types::ExecutionTypes::TaskTemplate] Task template
      # @param step_name [String] Step name to find
      # @return [TaskerCore::Types::ExecutionTypes::StepTemplate, nil] Step template or nil
      def find_step_template(task_template, step_name)
        task_template.step_templates.find { |template| template.name == step_name }
      end

      # Create step handler for step template
      #
      # @param step_template [TaskerCore::Types::ExecutionTypes::StepTemplate] Step template
      # @return [TaskerCore::StepHandler::Base] Step handler instance
      def create_step_handler(step_template)
        handler_class = constantize_or_raise(step_template.handler_class)

        if handler_class && handler_class < TaskerCore::StepHandler::Base
          handler_class.new(
            config: step_template.handler_config || {},
            logger: @logger
          )
        else
          # Fall back to base step handler if class not found
          TaskerCore::StepHandler::Base.new(
            config: step_template.handler_config || {},
            logger: @logger
          )
        end
      end


      # Safe constantize implementation
      #
      # @param class_name [String] Class name to constantize
      # @return [Class, nil] Class or nil if not found
      def constantize_or_raise(class_name)
        raise TaskerCore::ValidationError, "Class name is required" if class_name.nil? || class_name.empty?

        constants = class_name.split('::')
        constants.inject(Object) do |context, constant|
          context.const_get(constant)
        end
      end

      # Map Ruby status strings to Rust StepStatus enum variants
      def map_status_to_rust(ruby_status)
        case ruby_status&.to_s&.downcase
        when 'completed', 'success'
          'Completed'
        when 'failed', 'error'
          'Failed'
        when 'timeout'
          'Timeout'
        when 'cancelled', 'canceled'
          'Cancelled'
        else
          'Failed' # Default to Failed for unknown statuses
        end
      end

      def command_client
        TaskerCore::Execution::CommandBackplane.instance.command_client
      end

      def send_partial_result_command(batch_id, step_id, step_summary, execution_time_ms)
        # Convert Ruby step summary to Rust StepResult structure for FFI
        step_result = {
          status: map_status_to_rust(step_summary[:status]),
          output: step_summary[:result], # Handler output goes to StepResult.output
          error: step_summary[:error] ? {
            error_type: step_summary[:retryable] ? 'RetryableError' : 'PermanentError',
            message: step_summary[:error],
            metadata: {}
          } : nil,
          metadata: {
            step_name: step_summary[:step_name],
            handler_class: step_summary[:handler_class],
            retryable: step_summary[:retryable]
          }
        }

        # Use command client FFI method instead of manually creating command structure
        command_client.report_partial_result({
          batch_id: batch_id,
          step_id: step_id,
          result: step_result,
          execution_time_ms: execution_time_ms,
          worker_id: @worker_id || 'ruby_batch_handler'
        })
      end

      def send_batch_completion_response(command_id, batch_id, step_summaries, total_execution_time)
        # Convert Ruby step summaries to Rust StepSummary structures for FFI
        # Note: Rust StepSummary only has 4 fields: step_id, final_status, execution_time_ms, worker_id
        rust_step_summaries = step_summaries.map do |summary|
          {
            step_id: summary[:step_id],
            final_status: map_status_to_rust(summary[:status]),
            execution_time_ms: summary[:execution_time_ms],
            worker_id: @worker_id || 'ruby_batch_handler'
          }
        end

        # Use command client FFI method instead of manually creating command structure
        command_client.report_batch_completion({
          batch_id: batch_id,
          step_summaries: rust_step_summaries,
          total_execution_time_ms: total_execution_time
        })
      end

      def send_batch_error_response(command_id, batch_id, error_message)
        # For errors, we can report via batch completion with all failed steps
        # Note: Rust StepSummary only has 4 fields: step_id, final_status, execution_time_ms, worker_id
        error_summaries = [{
          step_id: -1, # Use -1 to indicate batch-level error
          final_status: 'Failed',
          execution_time_ms: 0,
          worker_id: @worker_id || 'ruby_batch_handler'
        }]

        # Use command client FFI method for error reporting
        command_client.report_batch_completion({
          batch_id: batch_id,
          step_summaries: error_summaries,
          total_execution_time_ms: 0
        })
      end

      # Generate unique response ID
      #
      # @return [String] Unique response identifier
      def generate_response_id
        "ruby_batch_#{Process.pid}_#{Time.now.to_f}_#{rand(1000)}"
      end

      # Create default logger
      #
      # @return [Logger] Default logger instance
      def default_logger
        logger = Logger.new(STDOUT)
        logger.level = Logger::INFO
        logger.formatter = proc do |severity, datetime, progname, msg|
          "[#{datetime}] BatchExecutionHandler #{severity}: #{msg}\n"
        end
        logger
      end
    end
  end
end
