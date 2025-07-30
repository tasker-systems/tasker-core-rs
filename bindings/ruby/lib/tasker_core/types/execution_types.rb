# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'
require_relative '../monkeypatch'

module TaskerCore
  module Types
    # Type definitions for TaskerCore execution responses from TCP executor
    #
    # This module defines structured response types for all TCP executor commands,
    # providing type safety, validation, and a Ruby-native API instead of raw hashes.
    #
    # @example Health check response
    #   response = ExecutionTypes::HealthCheckResponse.new(
    #     command_type: 'HealthCheckResult',
    #     command_id: 'abc-123',
    #     correlation_id: 'ruby_cmd_123',
    #     payload: {
    #       type: 'HealthCheckResult',
    #       data: {
    #         status: 'healthy',
    #         uptime_seconds: 120,
    #         total_workers: 5,
    #         active_commands: 2,
    #         diagnostics: { current_load: 0.1 }
    #       }
    #     }
    #   )
    #
    #   response.healthy?                    # => true
    #   response.uptime_seconds             # => 120
    #   response.diagnostics.current_load   # => 0.1
    #
    # @example Worker registration response
    #   response = ExecutionTypes::WorkerRegistrationResponse.new(
    #     command_type: 'Success',
    #     payload: {
    #       type: 'WorkerRegistered',
    #       data: { worker_id: 'worker-123', registered_at: '2025-01-01T00:00:00Z' }
    #     }
    #   )
    #
    #   response.worker_registered?  # => true
    #   response.worker_id          # => 'worker-123'
    #
    module ExecutionTypes
      module Types
        include Dry.Types()
      end

      # Valid command types enum (matching Rust CommandType enum)
      CommandTypeEnum = Types::Coercible::String.enum(
        # Task management operations
        'InitializeTask',

        # Step execution operations
        'ExecuteBatch',

        # Task readiness operations
        'TryTaskIfReady',

        # Result reporting
        'ReportPartialResult',
        'ReportBatchCompletion',

        # Worker lifecycle management
        'RegisterWorker',
        'UnregisterWorker',
        'WorkerHeartbeat',

        # Task handler registration
        'RegisterTaskHandler',
        'UnregisterTaskHandler',

        # System operations
        'HealthCheck',

        # Generic FFI operation (future migration path)
        'FfiOperation',

        # Response types
        'Success',
        'Error',
        'WorkerRegistered',
        'WorkerUnregistered',
        'HeartbeatAcknowledged',
        'BatchExecuted',
        'TaskInitialized',
        'TaskReadinessResult',
        'HealthCheckResult',
        'TaskHandlerRegistered',
        'TaskHandlerUnregistered'
      )

      # Valid health statuses enum
      HealthStatusEnum = Types::Coercible::String.enum(
        'healthy',
        'unhealthy',
        'degraded',
        'unknown'
      )

      # Command source type
      class CommandSource < Dry::Struct
        attribute :type, Types::Coercible::String
        attribute :data, Types::Hash
      end

      # Command metadata with proper structure
      class CommandMetadata < Dry::Struct
        attribute :timestamp, Types::Coercible::String
        attribute :source, CommandSource
        attribute? :target, Types::Hash.optional
        attribute? :timeout_ms, Types::Integer.optional
        attribute? :retry_policy, Types::Hash.optional
        attribute? :namespace, Types::Coercible::String.optional
        attribute? :priority, Types::Coercible::String.optional
      end

      # Base response structure for all TCP executor commands
      class BaseResponse < Dry::Struct
        # Command response metadata
        attribute :command_type, CommandTypeEnum
        attribute? :command_id, Types::Coercible::String
        attribute? :correlation_id, Types::Coercible::String

        # Command metadata (timestamps, source info, etc.)
        attribute? :metadata, CommandMetadata.optional

        # Actual response payload with type and data
        attribute :payload, Types::Coercible::Hash

        # Extract the response type from payload
        def response_type
          payload.dig(:type) || payload.dig('type')
        end

        # Extract the response data from payload
        def response_data
          payload.dig(:data) || payload.dig('data') || {}
        end

        # Check if this is a success response
        def success?
          command_type == 'Success' || command_type.end_with?('Result')
        end

        # Check if this is an error response
        def error?
          command_type == 'Error' || command_type.end_with?('Error')
        end
      end

      # Health check diagnostic data
      class HealthCheckDiagnostics < Dry::Struct
        attribute? :current_load, Types::Coercible::Float.optional
        attribute? :status, HealthStatusEnum.optional
        attribute? :total_capacity, Types::Integer.optional
        attribute? :available_capacity, Types::Integer.optional
        attribute? :total_steps_processed, Types::Integer.optional
        attribute? :successful_steps, Types::Integer.optional
        attribute? :failed_steps, Types::Integer.optional
        attribute? :namespace_distribution, Types::Hash.optional
      end

      # Health check response data
      class HealthCheckData < Dry::Struct
        attribute :status, HealthStatusEnum
        attribute :uptime_seconds, Types::Integer
        attribute :total_workers, Types::Integer
        attribute :active_commands, Types::Integer
        attribute? :diagnostics, HealthCheckDiagnostics.optional
      end

      # Health check response with diagnostic information
      class HealthCheckResponse < BaseResponse
        # Override payload to use structured data
        def health_data
          @health_data ||= HealthCheckData.new(response_data)
        end

        # Health check specific methods using structured data
        def healthy?
          health_data.status == 'healthy'
        end

        def uptime_seconds
          health_data.uptime_seconds
        end

        def total_workers
          health_data.total_workers
        end

        def active_commands
          health_data.active_commands
        end

        def diagnostics
          health_data.diagnostics
        end

        def status
          health_data.status
        end

        # Validate this is actually a health check response
        def validate_response_type!
          unless response_type == 'HealthCheckResult'
            raise ArgumentError, "Expected HealthCheckResult, got #{response_type}"
          end
        end
      end

      # Worker registration response data
      class WorkerRegistrationData < Dry::Struct
        attribute :worker_id, Types::Coercible::String
        attribute :assigned_pool, Types::Coercible::String
        attribute :queue_position, Types::Integer
      end

      # Worker registration response
      class WorkerRegistrationResponse < BaseResponse
        # Override payload to use structured data
        def registration_data
          @registration_data ||= WorkerRegistrationData.new(response_data)
        end

        def worker_registered?
          response_type == 'WorkerRegistered'
        end

        def worker_id
          registration_data.worker_id
        end

        def assigned_pool
          registration_data.assigned_pool
        end

        def queue_position
          registration_data.queue_position
        end

        # Legacy compatibility
        def registered_at
          nil # This field wasn't in the original Rust response, keeping for compatibility
        end

        # Validate this is actually a worker registration response
        def validate_response_type!
          unless worker_registered? || command_type == 'Success'
            raise ArgumentError, "Expected WorkerRegistered or Success, got #{response_type}"
          end
        end
      end

      # Worker heartbeat response data
      class HeartbeatData < Dry::Struct
        attribute :worker_id, Types::Coercible::String
        attribute :acknowledged_at, Types::Coercible::String
        attribute :status, HealthStatusEnum
        attribute? :next_heartbeat_in, Types::Integer.optional
      end

      # Worker heartbeat response
      class HeartbeatResponse < BaseResponse
        # Override payload to use structured data
        def heartbeat_data
          @heartbeat_data ||= HeartbeatData.new(response_data)
        end

        def heartbeat_acknowledged?
          response_type == 'HeartbeatAcknowledged' || command_type == 'Success'
        end

        def worker_id
          heartbeat_data.worker_id
        end

        def acknowledged_at
          heartbeat_data.acknowledged_at
        end

        def status
          heartbeat_data.status
        end

        def next_heartbeat_in
          heartbeat_data.next_heartbeat_in
        end

        # Validate this is actually a heartbeat response
        def validate_response_type!
          unless heartbeat_acknowledged?
            raise ArgumentError, "Expected HeartbeatAcknowledged or Success, got #{response_type}"
          end
        end
      end

      # Task readiness response data
      class TaskReadinessData < Dry::Struct
        attribute :task_id, Types::Coercible::Integer
        attribute :ready, Types::Bool
        attribute :batch_info, Types::Hash.optional
        attribute :ready_steps_count, Types::Coercible::Integer
        attribute :error_message, Types::String.optional
      end

      # Task readiness response
      class TaskReadinessResponse < BaseResponse
        # Override payload to use structured data
        def readiness_data
          @readiness_data ||= TaskReadinessData.new(response_data)
        end

        def ready?
          readiness_data.ready
        end

        def task_id
          readiness_data.task_id
        end

        def batch_info
          readiness_data.batch_info
        end

        def ready_steps_count
          readiness_data.ready_steps_count
        end

        def error_message
          readiness_data.error_message
        end

        # Validate this is actually a task readiness response
        def validate_response_type!
          unless response_type == 'TaskReadinessResult'
            raise ArgumentError, "Expected TaskReadinessResult, got #{response_type}"
          end
        end
      end

      # Worker unregistration response data
      class WorkerUnregistrationData < Dry::Struct
        attribute :worker_id, Types::Coercible::String
        attribute :unregistered_at, Types::Coercible::String
        attribute :reason, Types::Coercible::String
      end

      # Worker unregistration response
      class WorkerUnregistrationResponse < BaseResponse
        # Override payload to use structured data
        def unregistration_data
          @unregistration_data ||= WorkerUnregistrationData.new(response_data)
        end

        def worker_unregistered?
          response_type == 'WorkerUnregistered' || command_type == 'Success'
        end

        def worker_id
          unregistration_data.worker_id
        end

        def unregistered_at
          unregistration_data.unregistered_at
        end

        def reason
          unregistration_data.reason
        end

        # Validate this is actually a worker unregistration response
        def validate_response_type!
          unless worker_unregistered?
            raise ArgumentError, "Expected WorkerUnregistered or Success, got #{response_type}"
          end
        end
      end

      # Error response data
      class ErrorData < Dry::Struct
        attribute :error_type, Types::Coercible::String
        attribute :message, Types::Coercible::String
        attribute? :details, Types::Hash.optional
        attribute :retryable, Types::Bool
      end

      # Step request metadata for ExecuteBatch commands
      class StepRequestMetadata < Dry::Struct
        attribute :attempt, Types::Integer
        attribute :retry_limit, Types::Integer
        attribute :timeout_ms, Types::Integer
        attribute :priority, Types::Coercible::String.optional
      end

      # Step execution request for ExecuteBatch commands
      class StepExecutionRequest < Dry::Struct
        attribute :step_id, Types::Integer
        attribute :task_id, Types::Integer
        attribute :step_name, Types::Coercible::String
        attribute :handler_class, Types::Coercible::String
        attribute :handler_config, Types::Hash
        attribute :task_context, Types::Coercible::Hash
        attribute :previous_results, Types::Hash
        attribute :metadata, StepRequestMetadata
      end

      # Step template for TaskTemplate
      class StepTemplate < Dry::Struct
        attribute :name, Types::Coercible::String
        attribute? :description, Types::Coercible::String.optional
        attribute :handler_class, Types::Coercible::String
        attribute? :handler_config, Types::Hash.optional
        attribute? :depends_on, Types::Array.of(Types::Coercible::String).optional
        attribute? :retryable, Types::Bool.optional
        attribute? :retry_limit, Types::Integer.optional
        attribute? :timeout_seconds, Types::Integer.optional
      end

      # Task template for ExecuteBatch commands
      class TaskTemplate < Dry::Struct
        attribute :name, Types::Coercible::String
        attribute? :module_namespace, Types::Coercible::String.optional
        attribute :task_handler_class, Types::Coercible::String
        attribute :namespace_name, Types::Coercible::String
        attribute :version, Types::Coercible::String
        attribute? :description, Types::Coercible::String.optional
        attribute? :default_dependent_system, Types::Coercible::String.optional
        attribute? :named_steps, Types::Array.of(Types::Coercible::String).optional
        attribute? :schema, Types::Hash.optional
        attribute :step_templates, Types::Array.of(StepTemplate)
        attribute? :environments, Types::Hash.optional
        attribute? :custom_events, Types::Array.optional
      end

      # ExecuteBatch payload data
      class ExecuteBatchData < Dry::Struct
        attribute :batch_id, Types::Coercible::String
        attribute :steps, Types::Array.of(StepExecutionRequest)
        attribute :task_template, TaskTemplate
      end

      # ExecuteBatch command response
      class ExecuteBatchResponse < BaseResponse
        # Override payload to use structured data
        def batch_data
          @batch_data ||= ExecuteBatchData.new(response_data)
        end

        def batch_id
          batch_data.batch_id
        end

        def steps
          batch_data.steps
        end

        def task_template
          batch_data.task_template
        end

        def step_count
          steps.size
        end

        # Check if this is an ExecuteBatch command
        def execute_batch?
          command_type == 'ExecuteBatch'
        end

        # Validate this is actually an ExecuteBatch command
        def validate_response_type!
          unless execute_batch?
            raise ArgumentError, "Expected ExecuteBatch, got #{command_type}"
          end
        end
      end

      # Task initialization payload data
      class InitializeTaskData < Dry::Struct
        attribute :task_context, Types::Hash
        attribute :workflow_steps, Types::Array.of(StepTemplate)
      end

      class WorkflowStepInitializationData < Dry::Struct
        attribute :workflow_step_id, Types::Integer
        attribute :backoff_request_seconds, Types::Coercible::Integer.optional
        attribute :created_at, Types::Params::DateTime
        attribute :in_process, Types::Bool
        attribute :inputs, Types::Hash
        attribute :last_attempted_at, Types::Params::DateTime.optional
        attribute :named_step_id, Types::Integer
        attribute :processed, Types::Bool
        attribute :processed_at, Types::Params::DateTime.optional
        attribute :results, Types::Hash.optional
        attribute :retry_limit, Types::Integer
        attribute :retryable, Types::Bool
        attribute :skippable, Types::Bool
        attribute :task_id, Types::Integer
        attribute :updated_at, Types::Params::DateTime.optional
      end

      # Task initialization response data
      class TaskInitializedData < Dry::Struct
        attribute :task_id, Types::Integer
        attribute :success, Types::Bool
        attribute :step_count, Types::Integer
        attribute :workflow_steps, Types::Array.of(WorkflowStepInitializationData)
        attribute? :error_message, Types::String.optional
      end

      # Task initialization response
      class TaskInitializedResponse < BaseResponse
        def initialization_data
          @initialization_data ||= TaskInitializedData.new(response_data)
        end

        def task_initialized?
          response_type == 'TaskInitialized'
        end

        def task_id
          initialization_data.task_id
        end

        def step_count
          initialization_data.step_count
        end

        def workflow_steps
          initialization_data.workflow_steps
        end

        def error_message
          initialization_data.error_message
        end

        def validate_response_type!
          unless task_initialized?
            raise ArgumentError, "Expected TaskInitialized, got #{response_type}"
          end
        end
      end

      # Report partial result payload data
      class ReportPartialResultData < Dry::Struct
        attribute :batch_id, Types::Coercible::String
        attribute :step_id, Types::Integer
        attribute :result, Types::Hash
        attribute :execution_time_ms, Types::Integer
        attribute :worker_id, Types::Coercible::String
      end

      # Report batch completion payload data
      class ReportBatchCompletionData < Dry::Struct
        attribute :batch_id, Types::Coercible::String
        attribute :completion_status, Types::Coercible::String
        attribute :total_steps, Types::Integer
        attribute :successful_steps, Types::Integer
        attribute :failed_steps, Types::Integer
        attribute :total_execution_time_ms, Types::Integer
        attribute :worker_id, Types::Coercible::String
      end

      # Task handler registration payload data
      class RegisterTaskHandlerData < Dry::Struct
        attribute :handler_class, Types::Coercible::String
        attribute :namespace, Types::Coercible::String
        attribute :capabilities, Types::Array.of(Types::Coercible::String)
        attribute :worker_id, Types::Coercible::String
      end

      # Task handler registration response data
      class TaskHandlerRegistrationData < Dry::Struct
        attribute :handler_class, Types::Coercible::String
        attribute :namespace, Types::Coercible::String
        attribute :registered_at, Types::Coercible::String
        attribute :registration_id, Types::Coercible::String
      end

      # Task handler registration response
      class TaskHandlerRegistrationResponse < BaseResponse
        def registration_data
          @registration_data ||= TaskHandlerRegistrationData.new(response_data)
        end

        def handler_registered?
          response_type == 'TaskHandlerRegistered'
        end

        def handler_class
          registration_data.handler_class
        end

        def namespace
          registration_data.namespace
        end

        def registered_at
          registration_data.registered_at
        end

        def registration_id
          registration_data.registration_id
        end

        def validate_response_type!
          unless handler_registered?
            raise ArgumentError, "Expected TaskHandlerRegistered, got #{response_type}"
          end
        end
      end

      # Task handler unregistration response data
      class TaskHandlerUnregistrationData < Dry::Struct
        attribute :handler_class, Types::Coercible::String
        attribute :namespace, Types::Coercible::String
        attribute :unregistered_at, Types::Coercible::String
        attribute :reason, Types::Coercible::String
      end

      # Task handler unregistration response
      class TaskHandlerUnregistrationResponse < BaseResponse
        def unregistration_data
          @unregistration_data ||= TaskHandlerUnregistrationData.new(response_data)
        end

        def handler_unregistered?
          response_type == 'TaskHandlerUnregistered'
        end

        def handler_class
          unregistration_data.handler_class
        end

        def namespace
          unregistration_data.namespace
        end

        def unregistered_at
          unregistration_data.unregistered_at
        end

        def reason
          unregistration_data.reason
        end

        def validate_response_type!
          unless handler_unregistered?
            raise ArgumentError, "Expected TaskHandlerUnregistered, got #{response_type}"
          end
        end
      end

      # Generic error response
      class ErrorResponse < BaseResponse
        # Override payload to use structured data
        def error_data
          @error_data ||= ErrorData.new(response_data)
        end

        def error_message
          error_data.message
        end

        def error_type
          error_data.error_type
        end

        def error_details
          error_data.details || {}
        end

        def retryable?
          error_data.retryable
        end

        # Legacy compatibility
        def error_code
          error_type # Map error_type to error_code for backward compatibility
        end

        # Always an error
        def error?
          true
        end

        def success?
          false
        end
      end

      # Factory for creating typed responses from raw hash data
      class ResponseFactory
        class << self
          # Create a typed response from raw TCP executor response hash
          def create_response(raw_response)
            raw_response = raw_response.deep_symbolize_keys
            command_type = raw_response[:command_type]
            payload = raw_response[:payload] || {}
            response_type = payload[:type]

            # Determine the appropriate response class based on command and response type
            response_class = case response_type
            when 'HealthCheckResult'
              HealthCheckResponse
            when 'WorkerRegistered'
              WorkerRegistrationResponse
            when 'HeartbeatAcknowledged'
              HeartbeatResponse
            when 'WorkerUnregistered'
              WorkerUnregistrationResponse
            when 'TaskReadinessResult'
              TaskReadinessResponse
            when 'ExecuteBatch'
              ExecuteBatchResponse
            when 'TaskInitialized'
              TaskInitializedResponse
            when 'TaskHandlerRegistered'
              TaskHandlerRegistrationResponse
            when 'TaskHandlerUnregistered'
              TaskHandlerUnregistrationResponse
            else
              # Handle generic Success/Error responses based on command type
              if command_type == 'Error' || command_type&.end_with?('Error')
                ErrorResponse
              else
                # Try to infer from command type for Success responses
                case command_type
                when /Health/i
                  HealthCheckResponse
                when /RegisterWorker/i, /Worker.*Success/
                  WorkerRegistrationResponse
                when /RegisterTaskHandler/i
                  TaskHandlerRegistrationResponse
                when /UnregisterTaskHandler/i
                  TaskHandlerUnregistrationResponse
                when /InitializeTask/i
                  TaskInitializedResponse
                when /Heartbeat/i
                  HeartbeatResponse
                when /Unregister/i
                  WorkerUnregistrationResponse
                when 'ExecuteBatch'
                  ExecuteBatchResponse
                else
                  BaseResponse
                end
              end
            end

            # Create the typed response
            response = response_class.new(raw_response)

            # Validate the response type if the class supports it
            response.validate_response_type! if response.respond_to?(:validate_response_type!)

            response
          rescue Dry::Struct::Error => e
            # If type construction fails, wrap in an error response
            ErrorResponse.new(
              command_type: 'Error',
              command_id: raw_response[:command_id],
              correlation_id: raw_response[:correlation_id],
              metadata: raw_response[:metadata],
              payload: {
                type: 'ValidationError',
                data: {
                  message: "Failed to parse response: #{e.message}",
                  original_response: raw_response
                }
              }
            )
          end

          # Convenience methods for creating specific response types
          def health_check_response(raw_response)
            HealthCheckResponse.new(raw_response)
          end

          def worker_registration_response(raw_response)
            WorkerRegistrationResponse.new(raw_response)
          end

          def heartbeat_response(raw_response)
            HeartbeatResponse.new(raw_response)
          end

          def worker_unregistration_response(raw_response)
            WorkerUnregistrationResponse.new(raw_response)
          end

          def error_response(raw_response)
            ErrorResponse.new(raw_response)
          end

          def execute_batch_response(raw_response)
            ExecuteBatchResponse.new(raw_response)
          end

          def task_initialized_response(raw_response)
            TaskInitializedResponse.new(raw_response)
          end

          def task_handler_registration_response(raw_response)
            TaskHandlerRegistrationResponse.new(raw_response)
          end

          def task_handler_unregistration_response(raw_response)
            TaskHandlerUnregistrationResponse.new(raw_response)
          end

          def task_readiness_response(raw_response)
            TaskReadinessResponse.new(raw_response)
          end
        end
      end
    end
  end
end
