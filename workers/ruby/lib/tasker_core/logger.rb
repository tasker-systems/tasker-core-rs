# frozen_string_literal: true

require 'singleton'
require 'logger'
require 'json'

module TaskerCore
  # Structured logging system with Rust-compatible output format
  #
  # Provides both traditional string-based logging and enhanced structured logging
  # methods that match Rust patterns. The logger uses emojis and component prefixes
  # for easy visual scanning while maintaining structured data for JSON processing.
  #
  # The logger supports two logging approaches:
  # 1. **Traditional Logging**: Simple string messages for backward compatibility
  # 2. **Structured Logging**: Component-based logging with JSON metadata
  #
  # Structured logs include:
  # - Emoji prefix for visual identification
  # - Component label for categorization
  # - Operation description
  # - Structured metadata as JSON
  # - ISO 8601 timestamps
  #
  # @example Traditional logging (backward compatibility)
  #   logger = TaskerCore::Logger.instance
  #   logger.info "Worker started successfully"
  #   logger.error "Failed to process step: #{error.message}"
  #   logger.debug "Processing step #{step_uuid}"
  #
  # @example Structured logging with component context
  #   logger.log_task(:info, "initialization",
  #     task_uuid: "550e8400-e29b-41d4-a716-446655440000",
  #     namespace: "payments",
  #     step_count: 4,
  #     priority: "high"
  #   )
  #   # Output: [2025-10-02 12:00:00] INFO TaskerCore: 📋 TASK_OPERATION: initialization | {"task_uuid":"...","namespace":"payments",...}
  #
  # @example Queue worker logging
  #   logger.log_queue_worker(:debug, "claiming_step",
  #     namespace: "payments",
  #     step_uuid: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  #     retry_count: 2,
  #     queue_depth: 15
  #   )
  #   # Output: [2025-10-02 12:00:01] DEBUG TaskerCore: 🔄 QUEUE_WORKER: claiming_step (namespace: payments) | {...}
  #
  # @example Step execution logging
  #   logger.log_step(:info, "handler_execution",
  #     step_uuid: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  #     handler_class: "ProcessPaymentHandler",
  #     execution_time_ms: 125,
  #     result_size_bytes: 1024
  #   )
  #
  # @example FFI operation logging
  #   logger.log_ffi(:info, "bootstrap_worker",
  #     component: "RubyWorker",
  #     worker_id: "ruby-worker-123",
  #     deployment_mode: "Hybrid"
  #   )
  #
  # @example Database operation logging
  #   logger.log_database(:debug, "step_transition",
  #     step_uuid: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  #     from_state: "in_progress",
  #     to_state: "complete",
  #     duration_ms: 15
  #   )
  #
  # @example Configuration logging
  #   logger.log_config(:info, "environment_loaded",
  #     environment: "production",
  #     config_file: "/etc/tasker/config.yml",
  #     overrides_applied: 3
  #   )
  #
  # @example Handler registry logging
  #   logger.log_registry(:debug, "handler_registered",
  #     namespace: "payments",
  #     name: "ProcessPaymentHandler",
  #     handler_count: 12
  #   )
  #
  # Component Types and Their Emojis:
  # - **📋 TASK_OPERATION**: Task-level operations (creation, completion, state changes)
  # - **🔄 QUEUE_WORKER**: Queue processing and step claiming
  # - **🚀 ORCHESTRATOR**: Orchestration coordination and workflow management
  # - **🔧 STEP_OPERATION**: Step execution and handler invocation
  # - **💾 DATABASE**: Database operations and state persistence
  # - **🌉 FFI**: Rust FFI bridge operations and cross-language communication
  # - **⚙️ CONFIG**: Configuration loading and validation
  # - **📚 REGISTRY**: Handler registry operations and discovery
  #
  # Log Levels:
  # - **:debug**: Detailed diagnostic information
  # - **:info**: General informational messages
  # - **:warn**: Warning messages for potential issues
  # - **:error**: Error messages for failures
  # - **:fatal**: Critical failures requiring immediate attention
  #
  # @see #log_task For task-level operations
  # @see #log_queue_worker For queue processing
  # @see #log_orchestrator For orchestration operations
  # @see #log_step For step execution
  # @see #log_database For database operations
  # @see #log_ffi For FFI bridge operations
  # @see #log_config For configuration operations
  # @see #log_registry For handler registry operations
  class Logger
    include Singleton

    attr_reader :logger

    # Traditional string-based logging methods (backward compatibility)
    def info(message, &)
      @logger.info(message, &)
    end

    def warn(message, &)
      @logger.warn(message, &)
    end

    def error(message, &)
      @logger.error(message, &)
    end

    def fatal(message, &)
      @logger.fatal(message, &)
    end

    def debug(message, &)
      @logger.debug(message, &)
    end

    # Enhanced structured logging methods that match Rust patterns
    # These provide structured data while maintaining emoji + component format consistency

    def log_task(level, operation, **data, &)
      message = build_unified_message('📋 TASK_OPERATION', operation, **data)
      @logger.send(level, message, &)
    end

    def log_queue_worker(level, operation, namespace: nil, **data, &)
      message = if namespace
                  build_unified_message('🔄 QUEUE_WORKER', "#{operation} (namespace: #{namespace})", **data)
                else
                  build_unified_message('🔄 QUEUE_WORKER', operation, **data)
                end
      @logger.send(level, message, &)
    end

    def log_orchestrator(level, operation, **data, &)
      message = build_unified_message('🚀 ORCHESTRATOR', operation, **data)
      @logger.send(level, message, &)
    end

    def log_step(level, operation, **data, &)
      message = build_unified_message('🔧 STEP_OPERATION', operation, **data)
      @logger.send(level, message, &)
    end

    def log_database(level, operation, **data, &)
      message = build_unified_message('💾 DATABASE', operation, **data)
      @logger.send(level, message, &)
    end

    def log_ffi(level, operation, component: nil, **data, &)
      message = if component
                  build_unified_message('🌉 FFI', "#{operation} (#{component})", **data)
                else
                  build_unified_message('🌉 FFI', operation, **data)
                end
      @logger.send(level, message, &)
    end

    def log_config(level, operation, **data, &)
      message = build_unified_message('⚙️ CONFIG', operation, **data)
      @logger.send(level, message, &)
    end

    def log_registry(level, operation, namespace: nil, name: nil, **data, &)
      message = if namespace && name
                  build_unified_message('📚 REGISTRY', "#{operation} (#{namespace}/#{name})", **data)
                else
                  build_unified_message('📚 REGISTRY', operation, **data)
                end
      @logger.send(level, message, &)
    end

    def initialize
      @logger = ::Logger.new($stdout).tap do |log|
        log.level = ::Logger::INFO
        log.formatter = method(:unified_formatter)
      end
    end

    private

    # Build unified message format that matches Rust structured logging output
    def build_unified_message(component, operation, **data)
      base_message = "#{component}: #{operation}"

      # Add structured data if provided (for JSON logs or debugging)
      if data.any?
        structured_data = data.merge(
          timestamp: Time.now.utc.iso8601,
          component: component.gsub(/[🎯📋🔄🚀🔧💾🌉⚙️📚❌⚠✅]/, '').strip, # Remove emoji for structured field
          operation: operation
        )

        "#{base_message} | #{JSON.generate(structured_data)}"
      else
        base_message
      end
    end

    # Custom formatter that maintains readability while supporting structured data
    def unified_formatter(severity, datetime, progname, msg)
      # Check if message contains structured data (has JSON part)
      if msg.include?(' | {')
        base_msg, json_data = msg.split(' | ', 2)
        # In production, include structured data
        "[#{datetime}] #{severity} TaskerCore: #{base_msg} #{json_data}\n"
      else
        # Traditional format for simple messages
        "[#{datetime}] #{severity} TaskerCore #{progname}: #{msg}\n"
      end
    end
  end
end
