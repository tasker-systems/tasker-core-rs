# frozen_string_literal: true

require 'singleton'
require 'json'

module TaskerCore
  # DEPRECATED: Legacy logger wrapper that delegates to TaskerCore::Tracing
  #
  # This class is maintained for backward compatibility but all new code should
  # use TaskerCore::Tracing directly for unified structured logging via FFI.
  #
  # TAS-29 Phase 6: This logger now delegates to the Rust tracing infrastructure
  # through the FFI bridge, providing unified structured logging across Ruby and Rust.
  #
  # Migration Guide:
  #   # Old:
  #   logger = TaskerCore::Logger.instance
  #   logger.info("Message")
  #
  #   # New:
  #   TaskerCore::Tracing.info("Message", { operation: "example" })
  #
  # The logger supports two logging approaches:
  # 1. **Traditional Logging**: Simple string messages (delegates to Tracing)
  # 2. **Structured Logging**: Component-based logging (converts to Tracing fields)
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

    # Traditional string-based logging methods (delegates to Tracing)
    def info(message, &)
      Tracing.info(message.to_s, extract_context_fields)
    end

    def warn(message, &)
      Tracing.warn(message.to_s, extract_context_fields)
    end

    def error(message, &)
      Tracing.error(message.to_s, extract_context_fields)
    end

    def fatal(message, &)
      Tracing.error(message.to_s, extract_context_fields.merge(severity: 'fatal'))
    end

    def debug(message, &)
      Tracing.debug(message.to_s, extract_context_fields)
    end

    # Enhanced structured logging methods (delegates to Tracing with appropriate fields)

    def log_task(level, operation, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'task_operation'
      ).merge(data)
      Tracing.send(level, operation, fields)
    end

    def log_queue_worker(level, operation, namespace: nil, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'queue_worker'
      ).merge(data)
      fields[:namespace] = namespace if namespace
      Tracing.send(level, operation, fields)
    end

    def log_orchestrator(level, operation, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'orchestrator'
      ).merge(data)
      Tracing.send(level, operation, fields)
    end

    def log_step(level, operation, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'step_operation'
      ).merge(data)
      Tracing.send(level, operation, fields)
    end

    def log_database(level, operation, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'database'
      ).merge(data)
      Tracing.send(level, operation, fields)
    end

    def log_ffi(level, operation, component: nil, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: component || 'ffi'
      ).merge(data)
      Tracing.send(level, operation, fields)
    end

    def log_config(level, operation, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'config'
      ).merge(data)
      Tracing.send(level, operation, fields)
    end

    def log_registry(level, operation, namespace: nil, name: nil, **data)
      fields = extract_context_fields.merge(
        operation: operation,
        component: 'registry'
      ).merge(data)
      fields[:namespace] = namespace if namespace
      fields[:handler_name] = name if name
      Tracing.send(level, operation, fields)
    end

    def initialize
      # No-op - everything delegates to Tracing now
    end

    private

    # Extract context fields from thread-local or instance variables if available
    def extract_context_fields
      fields = {}

      # Try to extract correlation_id from thread-local storage if available
      fields[:correlation_id] = Thread.current[:correlation_id] if Thread.current[:correlation_id]

      # Try to extract task/step UUIDs if available
      fields[:task_uuid] = Thread.current[:task_uuid] if Thread.current[:task_uuid]

      fields[:step_uuid] = Thread.current[:step_uuid] if Thread.current[:step_uuid]

      fields
    end
  end
end
