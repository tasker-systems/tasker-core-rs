# frozen_string_literal: true

require 'json'
require 'time'
require 'logger'
require_relative '../types/execution_types'
require_relative '../task_handler/base'
require_relative '../step_handler/base'

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
    #     handler.handle(command)
    #   end
    #
    class BatchExecutionHandler
      attr_reader :logger, :task_handlers, :step_results

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @task_handlers = {}
        @step_results = {}
        @handler_mutex = Mutex.new
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
          # Process each step in the batch
          step_summaries = []
          total_start_time = Time.now

          steps.each do |step_request|
            logger.debug("Processing step: #{step_request.step_name} (ID: #{step_request.step_id})")

            step_summary = process_step(step_request, task_template)
            step_summaries << step_summary

            # Store step result for potential dependencies
            @step_results[step_request.step_id] = step_summary
          end

          total_execution_time = ((Time.now - total_start_time) * 1000).to_i

          # Create batch completion response
          create_batch_completion_response(
            command.command_id,
            batch_id,
            step_summaries,
            total_execution_time
          )

        rescue StandardError => e
          logger.error("Batch execution failed: #{e.message}")
          logger.error(e.backtrace.join("\n")) if logger.debug?

          create_batch_error_response(
            command.command_id,
            batch_id,
            "Batch execution failed: #{e.message}"
          )
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
          # Get or create task handler for this template
          task_handler = get_task_handler(task_template)

          # Find step template for this step
          step_template = find_step_template(task_template, step_request.step_name)
          unless step_template
            raise HandlerError, "Step template not found: #{step_request.step_name}"
          end

          # Create step handler
          step_handler = create_step_handler(step_template)

          # Prepare step execution context
          task_context = prepare_task_context(step_request)
          sequence_context = prepare_sequence_context(step_request, task_template)
          step_context = prepare_step_context(step_request, step_template)

          # Execute the step
          logger.debug("Executing step handler: #{step_template.handler_class}")
          result = step_handler.handle(task_context, sequence_context, step_context)

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

      # Prepare task context from step request
      #
      # @param step_request [TaskerCore::Types::ExecutionTypes::StepExecutionRequest] Step request
      # @return [OpenStruct] Task context object
      def prepare_task_context(step_request)
        require 'ostruct'

        task_data = step_request.task_context.merge(
          'id' => step_request.task_id,
          'task_id' => step_request.task_id
        )

        OpenStruct.new(task_data)
      end

      # Prepare sequence context from step request and template
      #
      # @param step_request [TaskerCore::Types::ExecutionTypes::StepExecutionRequest] Step request
      # @param task_template [TaskerCore::Types::ExecutionTypes::TaskTemplate] Task template
      # @return [OpenStruct] Sequence context object
      def prepare_sequence_context(step_request, task_template)
        require 'ostruct'

        # Create sequence with all steps and dependencies
        sequence_data = {
          'task_id' => step_request.task_id,
          'all_steps' => task_template.step_templates.map do |template|
            {
              'name' => template.name,
              'handler_class' => template.handler_class,
              'depends_on' => template.depends_on || []
            }
          end,
          'dependencies' => collect_step_dependencies(step_request)
        }

        sequence = OpenStruct.new(sequence_data)

        # Add steps method for compatibility
        sequence.steps = sequence_data['all_steps'].map { |step_data| OpenStruct.new(step_data) }
        sequence.dependencies = sequence_data['dependencies'].map { |dep_data| OpenStruct.new(dep_data) }

        sequence
      end

      # Prepare step context from step request and template
      #
      # @param step_request [TaskerCore::Types::ExecutionTypes::StepExecutionRequest] Step request
      # @param step_template [TaskerCore::Types::ExecutionTypes::StepTemplate] Step template
      # @return [OpenStruct] Step context object
      def prepare_step_context(step_request, step_template)
        require 'ostruct'

        step_data = {
          'id' => step_request.step_id,
          'step_id' => step_request.step_id,
          'task_id' => step_request.task_id,
          'name' => step_request.step_name,
          'handler_class' => step_request.handler_class,
          'handler_config' => step_request.handler_config,
          'attempt' => step_request.metadata.attempt,
          'retry_limit' => step_request.metadata.retry_limit,
          'timeout_ms' => step_request.metadata.timeout_ms,
          'depends_on' => step_template.depends_on || [],
          'previous_results' => step_request.previous_results
        }

        OpenStruct.new(step_data)
      end

      # Collect step dependencies from previous results and step results
      #
      # @param step_request [TaskerCore::Types::ExecutionTypes::StepExecutionRequest] Step request
      # @return [Array<Hash>] Dependency data
      def collect_step_dependencies(step_request)
        dependencies = []

        # Add previous results as dependencies
        step_request.previous_results.each do |step_name, result_data|
          dependencies << {
            'step_name' => step_name,
            'result' => result_data,
            'status' => 'completed'
          }
        end

        # Add any step results from current batch
        @step_results.each do |step_id, step_summary|
          next if step_id == step_request.step_id # Don't include self

          dependencies << {
            'step_name' => step_summary[:step_name],
            'result' => step_summary[:result],
            'status' => step_summary[:status]
          }
        end

        dependencies
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

      # Create batch completion response
      #
      # @param command_id [String] Original command ID
      # @param batch_id [String] Batch ID
      # @param step_summaries [Array<Hash>] Step execution summaries
      # @param total_execution_time [Integer] Total execution time in ms
      # @return [Hash] Batch completion response
      def create_batch_completion_response(command_id, batch_id, step_summaries, total_execution_time)
        {
          command_type: 'BatchExecuted',
          command_id: generate_response_id,
          correlation_id: command_id,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: { id: 'batch_execution_handler' }
            }
          },
          payload: {
            type: 'BatchExecuted',
            data: {
              batch_id: batch_id,
              step_summaries: step_summaries,
              total_execution_time_ms: total_execution_time,
              steps_processed: step_summaries.size,
              steps_succeeded: step_summaries.count { |s| s[:status] == 'completed' },
              steps_failed: step_summaries.count { |s| s[:status] == 'failed' }
            }
          }
        }
      end

      # Create batch error response
      #
      # @param command_id [String] Original command ID
      # @param batch_id [String] Batch ID
      # @param error_message [String] Error message
      # @return [Hash] Batch error response
      def create_batch_error_response(command_id, batch_id, error_message)
        {
          command_type: 'Error',
          command_id: generate_response_id,
          correlation_id: command_id,
          metadata: {
            timestamp: Time.now.utc.iso8601,
            source: {
              type: 'RubyWorker',
              data: { id: 'batch_execution_handler' }
            }
          },
          payload: {
            type: 'BatchExecutionError',
            data: {
              batch_id: batch_id,
              error_type: 'BatchExecutionError',
              message: error_message,
              retryable: true
            }
          }
        }
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
