# frozen_string_literal: true

require_relative 'correlation_id_generator'

module TaskerCore
  module Logging
    # StructuredLogging provides correlation ID tracking and consistent JSON formatting
    #
    # This concern builds on Tasker's existing event system to add production-grade
    # structured logging with correlation IDs for distributed tracing across workflows.
    #
    # Key Features:
    # - Automatic correlation ID generation and propagation
    # - JSON structured logging with consistent format
    # - Integration with existing telemetry configuration
    # - Parameter filtering for sensitive data
    # - Performance-optimized with minimal overhead
    #
    # Usage:
    #   include Tasker::Concerns::StructuredLogging
    #
    #   # Basic structured logging
    #   log_structured(:info, "Task started", task_id: task.task_id)
    #
    #   # Domain-specific helpers
    #   log_task_event(task, :started, execution_mode: 'async')
    #   log_step_event(step, :completed, duration: 1.5)
    #
    #   # Correlation ID propagation
    #   with_correlation_id(request_id) do
    #     # All logging within this block includes the correlation ID
    #     log_structured(:info, "Processing workflow")
    #   end
    module StructuredLogging
      extend ActiveSupport::Concern

      # Thread-local storage for correlation ID to ensure thread safety
      CORRELATION_ID_KEY = :tasker_correlation_id

      # ========================================================================
      # CORE STRUCTURED LOGGING API
      # ========================================================================

      # Log with structured JSON format including correlation ID and context
      #
      # @param level [Symbol] Log level (:debug, :info, :warn, :error, :fatal)
      # @param message [String] Human-readable log message
      # @param context [Hash] Additional structured context data
      # @return [void]
      #
      # @example Basic usage
      #   log_structured(:info, "Task execution started",
      #     task_id: "task_123",
      #     task_name: "order_processing"
      #   )
      #
      # @example With performance data
      #   log_structured(:debug, "Step completed successfully",
      #     step_id: "step_456",
      #     duration_ms: 250.5,
      #     memory_delta_mb: 12.3
      #   )
      def log_structured(level, message, **context)
        return unless should_log?(level)

        structured_data = build_structured_log_entry(message, context)
        filtered_data = apply_parameter_filtering(structured_data)

        TaskerCore::Logging::Logger.instance.public_send(level, format_log_output(filtered_data))
      rescue StandardError => e
        # Failsafe logging - never let logging errors break application flow
        TaskerCore::Logging::Logger.instance.error("Structured logging failed: #{e.message}")
        TaskerCore::Logging::Logger.instance.public_send(level, message) # Fallback to simple logging
      end

      # ========================================================================
      # CORRELATION ID MANAGEMENT
      # ========================================================================

      # Get the current correlation ID, generating one if needed
      #
      # @return [String] The correlation ID for the current execution context
      def correlation_id
        Thread.current[CORRELATION_ID_KEY] ||= generate_correlation_id
      end

      # Set a specific correlation ID for the current execution context
      #
      # @param id [String] The correlation ID to use
      # @return [String] The correlation ID that was set
      def correlation_id=(id)
        Thread.current[CORRELATION_ID_KEY] = id
      end

      # Execute a block with a specific correlation ID
      #
      # @param id [String] The correlation ID to use during block execution
      # @yield Block to execute with the correlation ID
      # @return [Object] The result of the yielded block
      #
      # @example
      #   with_correlation_id("req_abc123") do
      #     process_workflow
      #     # All logging within this block will include req_abc123
      #   end
      def with_correlation_id(id)
        previous_id = Thread.current[CORRELATION_ID_KEY]
        Thread.current[CORRELATION_ID_KEY] = id
        yield
      ensure
        Thread.current[CORRELATION_ID_KEY] = previous_id
      end

      # ========================================================================
      # DOMAIN-SPECIFIC LOGGING HELPERS
      # ========================================================================

      # Log task-related events with standardized format
      #
      # @param task [Tasker::Task] The task object
      # @param event_type [Symbol] The type of event (:started, :completed, :failed, etc.)
      # @param context [Hash] Additional context to include in the log
      # @return [void]
      #
      # @example
      #   log_task_event(task, :started, execution_mode: 'async', priority: 'high')
      #   log_task_event(task, :completed, duration: 120.5, step_count: 5)
      #   log_task_event(task, :failed, error: exception.message)
      def log_task_event(task, event_type, **context)
        log_structured(:info, "Task #{event_type}",
                       entity_type: 'task',
                       entity_id: task.task_id,
                       entity_name: task.name,
                       event_type: event_type,
                       task_status: task.status,
                       **context)
      end

      # Log step-related events with standardized format
      #
      # @param step [Tasker::WorkflowStep] The step object
      # @param event_type [Symbol] The type of event (:started, :completed, :failed, etc.)
      # @param duration [Float, nil] Execution duration in seconds
      # @param context [Hash] Additional context to include in the log
      # @return [void]
      #
      # @example
      #   log_step_event(step, :started)
      #   log_step_event(step, :completed, duration: 2.5, records_processed: 150)
      #   log_step_event(step, :failed, duration: 1.2, error: "Connection timeout")
      def log_step_event(step, event_type, duration: nil, **context)
        step_context = {
          entity_type: 'step',
          entity_id: step.workflow_step_id,
          entity_name: step.name,
          event_type: event_type,
          step_status: step.status,
          task_id: step.task.task_id,
          task_name: step.task.name
        }

        # Add performance data if provided
        if duration
          step_context[:duration_ms] = (duration * 1000).round(2)
          step_context[:performance_category] = categorize_duration(duration)
        end

        log_structured(:info, "Step #{event_type}", **step_context, **context)
      end

      # Log orchestration events (workflow coordination, reenqueuing, etc.)
      #
      # @param operation [String] The orchestration operation name
      # @param event_type [Symbol] The type of event (:started, :completed, :failed, etc.)
      # @param context [Hash] Additional context to include in the log
      # @return [void]
      #
      # @example
      #   log_orchestration_event("workflow_coordination", :started, task_id: "task_123")
      #   log_orchestration_event("step_execution_batch", :completed,
      #     step_count: 3, total_duration: 5.2)
      def log_orchestration_event(operation, event_type, **context)
        log_structured(:debug, "Orchestration #{event_type}",
                       entity_type: 'orchestration',
                       operation: operation,
                       event_type: event_type,
                       **context)
      end

      # Log performance-related events with timing and resource usage
      #
      # @param operation [String] The operation being measured
      # @param duration [Float] Duration in seconds
      # @param context [Hash] Additional performance context
      # @return [void]
      #
      # @example
      #   log_performance_event("dependency_graph_analysis", 0.85,
      #     node_count: 25, complexity: "medium")
      #   log_performance_event("sql_query", 2.1,
      #     query_type: "select", table: "workflow_steps")
      def log_performance_event(operation, duration, **context)
        performance_context = {
          entity_type: 'performance',
          operation: operation,
          duration_ms: (duration * 1000).round(2),
          performance_category: categorize_duration(duration),
          is_slow: duration > telemetry_config.slow_query_threshold_seconds
        }

        level = performance_context[:is_slow] ? :warn : :debug
        log_structured(level, 'Performance measurement', **performance_context, **context)
      end

      # ========================================================================
      # ERROR AND EXCEPTION LOGGING
      # ========================================================================

      # Log exceptions with full context and structured format
      #
      # @param exception [Exception] The exception to log
      # @param context [Hash] Additional context about when/where the exception occurred
      # @param level [Symbol] Log level (defaults to :error)
      # @return [void]
      #
      # @example
      #   log_exception(e, operation: "step_execution", step_id: "step_123")
      #   log_exception(e, operation: "task_finalization", task_id: "task_456", level: :fatal)
      def log_exception(exception, context: {}, level: :error)
        exception_context = {
          entity_type: 'exception',
          exception_class: exception.class.name,
          exception_message: exception.message,
          backtrace: extract_relevant_backtrace(exception),
          **context
        }

        log_structured(level, "Exception occurred: #{exception.class.name}", **exception_context)
      end

      private

      # ========================================================================
      # INTERNAL IMPLEMENTATION
      # ========================================================================

      # Build the core structured log entry with standard fields
      #
      # @param message [String] The log message
      # @param context [Hash] Additional context
      # @return [Hash] Structured log entry
      def build_structured_log_entry(message, context)
        {
          timestamp: Time.current.iso8601(3), # Millisecond precision
          correlation_id: correlation_id,
          component: determine_component_name,
          message: message,
          **extract_standard_context,
          **context
        }
      end

      # Apply parameter filtering to remove sensitive data
      #
      # @param data [Hash] The log data to filter
      # @return [Hash] Filtered log data
      def apply_parameter_filtering(data)
        return data unless telemetry_config.parameter_filter

        # ActiveSupport::ParameterFilter works directly with hashes
        telemetry_config.parameter_filter.filter(data)
      rescue StandardError
        # If filtering fails, return original data rather than breaking
        data
      end

      # Format log output based on configuration
      #
      # @param data [Hash] The structured log data
      # @return [String] Formatted log output
      def format_log_output(data)
        case telemetry_config.log_format
        when 'json'
          data.to_json
        when 'pretty_json'
          JSON.pretty_generate(data)
        when 'logfmt'
          format_as_logfmt(data)
        else
          data.to_json # Default to JSON
        end
      end

      # Format data as logfmt (key=value pairs)
      #
      # @param data [Hash] The data to format
      # @return [String] Logfmt formatted string
      def format_as_logfmt(data)
        data.map do |key, value|
          formatted_value = value.is_a?(String) ? "\"#{value}\"" : value
          "#{key}=#{formatted_value}"
        end.join(' ')
      end

      # Generate a unique correlation ID
      #
      # @return [String] A unique correlation ID
      def generate_correlation_id
        TaskerCore::Logging::CorrelationIdGenerator.generate
      end

      # Determine the component name for logging context
      #
      # @return [String] Component name
      def determine_component_name
        self.class.name.demodulize.underscore
      end

      # Extract standard context available in all log entries
      #
      # @return [Hash] Standard context fields
      def extract_standard_context
        context = {
          environment: ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || ENV['RACK_ENV'] || 'development',
          tasker_version: TaskerCore::Version
        }

        # Add process/thread info if available
        context[:process_id] = Process.pid if defined?(Process.pid)
        context[:thread_id] = Thread.current.object_id.to_s(16)

        # Add request info if in a web request context
        context[:request_id] = Current.request_id if defined?(Current) && Current.respond_to?(:request_id)

        context
      end

      # Extract relevant backtrace lines (filter out gem noise)
      #
      # @param exception [Exception] The exception
      # @return [Array<String>] Relevant backtrace lines
      def extract_relevant_backtrace(exception)
        return [] unless exception.backtrace

        exception.backtrace.first(20)
      end

      # Categorize duration for performance analysis
      #
      # @param duration [Float] Duration in seconds
      # @return [String] Performance category
      def categorize_duration(duration)
        case duration
        when 0..0.1 then 'fast'
        when 0.1..1.0 then 'moderate'
        when 1.0..5.0 then 'slow'
        else 'very_slow'
        end
      end

      # Check if we should log at the given level
      #
      # @param level [Symbol] The log level
      # @return [Boolean] Whether to log
      def should_log?(level)
        return true unless telemetry_config.respond_to?(:log_level)

        level_hierarchy = { debug: 0, info: 1, warn: 2, error: 3, fatal: 4 }
        current_level = level_hierarchy[telemetry_config.log_level.to_sym] || 1
        requested_level = level_hierarchy[level] || 1

        requested_level >= current_level
      end


      # TODO: pull telemetry config from the orchestration manager
      # Get telemetry configuration
      #
      # @return [TaskerCore::Types::TelemetryConfig] Telemetry configuration
      def telemetry_config
        @telemetry_config ||= TaskerCore::OrchestrationManager.instance.telemetry_config
      end
    end
  end
end
