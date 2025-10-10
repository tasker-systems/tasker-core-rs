# frozen_string_literal: true

require 'singleton'

module TaskerCore
  # Unified structured logging via Rust tracing FFI
  #
  # TAS-29 Phase 6: Replace Ruby's Logger with FFI calls to Rust's tracing
  # infrastructure, enabling unified structured logging across Ruby and Rust.
  #
  # ## Architecture
  #
  #   Ruby Handler → Tracing.info() → FFI Bridge → Rust tracing → OpenTelemetry
  #
  # ## Usage
  #
  #   # Simple message
  #   Tracing.info("Task initialized")
  #
  #   # With structured fields
  #   Tracing.info("Step completed", {
  #     correlation_id: correlation_id,
  #     task_uuid: task.uuid,
  #     step_uuid: step.uuid,
  #     namespace: "order_fulfillment",
  #     operation: "validate_inventory",
  #     duration_ms: elapsed_ms
  #   })
  #
  #   # Error logging
  #   Tracing.error("Payment processing failed", {
  #     correlation_id: correlation_id,
  #     error_class: error.class.name,
  #     error_message: error.message
  #   })
  #
  # ## Structured Field Conventions (TAS-29 Phase 6.2)
  #
  # **Required fields** (when applicable):
  # - correlation_id: Always include for distributed tracing
  # - task_uuid: Include for task-level operations
  # - step_uuid: Include for step-level operations
  # - namespace: Include for namespace-specific operations
  # - operation: Operation identifier (e.g., "validate_inventory")
  #
  # **Optional fields**:
  # - duration_ms: For timed operations
  # - error_class, error_message: For error context
  # - entity_id: For domain entity operations
  # - retry_count: For retryable operations
  #
  # ## Log Level Guidelines (TAS-29 Phase 6.1)
  #
  # - ERROR: Unrecoverable failures requiring intervention
  # - WARN: Degraded operation, retryable failures
  # - INFO: Lifecycle events, state transitions
  # - DEBUG: Detailed diagnostic information
  # - TRACE: Very verbose, hot-path entry/exit
  #
  class Tracing
    include Singleton

    # Log ERROR level message with structured fields
    #
    # @param message [String] Log message
    # @param fields [Hash] Structured fields (correlation_id, task_uuid, etc.)
    # @return [void]
    def self.error(message, fields = {})
      fields_hash = normalize_fields(fields)
      TaskerCore::FFI.log_error(message.to_s, fields_hash)
    rescue StandardError => e
      # Fallback to stderr if FFI logging fails
      warn "FFI logging failed: #{e.message}"
      warn "#{message} | #{fields.inspect}"
    end

    # Log WARN level message with structured fields
    #
    # @param message [String] Log message
    # @param fields [Hash] Structured fields
    # @return [void]
    def self.warn(message, fields = {})
      fields_hash = normalize_fields(fields)
      TaskerCore::FFI.log_warn(message.to_s, fields_hash)
    rescue StandardError => e
      warn "FFI logging failed: #{e.message}"
      warn "#{message} | #{fields.inspect}"
    end

    # Log INFO level message with structured fields
    #
    # @param message [String] Log message
    # @param fields [Hash] Structured fields
    # @return [void]
    def self.info(message, fields = {})
      fields_hash = normalize_fields(fields)
      TaskerCore::FFI.log_info(message.to_s, fields_hash)
    rescue StandardError => e
      warn "FFI logging failed: #{e.message}"
      warn "#{message} | #{fields.inspect}"
    end

    # Log DEBUG level message with structured fields
    #
    # @param message [String] Log message
    # @param fields [Hash] Structured fields
    # @return [void]
    def self.debug(message, fields = {})
      fields_hash = normalize_fields(fields)
      TaskerCore::FFI.log_debug(message.to_s, fields_hash)
    rescue StandardError => e
      warn "FFI logging failed: #{e.message}"
      warn "#{message} | #{fields.inspect}"
    end

    # Log TRACE level message with structured fields
    #
    # @param message [String] Log message
    # @param fields [Hash] Structured fields
    # @return [void]
    def self.trace(message, fields = {})
      fields_hash = normalize_fields(fields)
      TaskerCore::FFI.log_trace(message.to_s, fields_hash)
    rescue StandardError => e
      warn "FFI logging failed: #{e.message}"
      warn "#{message} | #{fields.inspect}"
    end

    # Normalize fields to string keys and values for FFI
    #
    # @param fields [Hash] Input fields
    # @return [Hash<String, String>] Normalized fields
    # @api private
    def self.normalize_fields(fields)
      return {} if fields.nil? || fields.empty?

      fields.transform_keys(&:to_s).transform_values { |v| normalize_value(v) }
    end

    # Normalize a value to string for FFI
    #
    # @param value [Object] Value to normalize
    # @return [String] String representation
    # @api private
    def self.normalize_value(value)
      case value
      when nil
        'nil'
      when String
        value
      when Symbol
        value.to_s
      when Numeric, TrueClass, FalseClass
        value.to_s
      when Exception
        "#{value.class}: #{value.message}"
      else
        value.to_s
      end
    rescue StandardError
      '<serialization_error>'
    end

    private_class_method :normalize_fields, :normalize_value
  end
end
