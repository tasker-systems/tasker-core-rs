# frozen_string_literal: true

require 'json'

module TaskerCore
  module Messaging
    class MessageManager
      def self.logger
        TaskerCore::Logging::Logger.instance
      end

      # Result object for step claiming operations
      class StepClaimResult
        attr_reader :records, :message, :error_type

        def initialize(success:, records: nil, message: nil, error_type: nil, retryable: false)
          @success = success
          @records = records
          @message = message
          @error_type = error_type
          @retryable = retryable
        end

        def success?
          @success
        end

        def retryable?
          @retryable
        end

        def self.success(records, message = 'Step successfully claimed')
          new(success: true, records: records, message: message)
        end

        def self.failure(message, error_type: 'StepClaimFailed', retryable: false)
          new(success: false, message: message, error_type: error_type, retryable: retryable)
        end
      end

      # TAS-32: Claim a step for processing and get ActiveRecord models
      # This combines step claiming with model loading for atomic operation
      #
      # @param msg_data [TaskerCore::Types::SimpleQueueMessageData] Simple queue message data
      # @param worker_namespace [String] The namespace of the worker attempting to claim
      # @return [StepClaimResult] Result containing models or failure information
      def self.claim_step_and_get_records(msg_data, worker_namespace)
        logger.debug("🔍 MESSAGE_MANAGER: Attempting to claim step #{msg_data.step_uuid} for worker namespace '#{worker_namespace}'")

        begin
          # 1. Load ActiveRecord models
          task, sequence, step = get_records_from_message(msg_data)

          # 2. Attempt to claim the step atomically
          claim_success = attempt_step_claim(step, worker_namespace)

          if claim_success
            StepClaimResult.success([task, sequence, step],
                                    "Step #{msg_data.step_uuid} successfully claimed by worker '#{worker_namespace}'")
          else
            StepClaimResult.failure(
              "Step #{msg_data.step_uuid} already claimed by another worker",
              error_type: 'StepAlreadyClaimed',
              retryable: false
            )
          end
        rescue ActiveRecord::RecordNotFound => e
          logger.error("❌ MESSAGE_MANAGER: Record not found for step #{msg_data.step_uuid}: #{e.message}")
          StepClaimResult.failure(
            "Database record not found: #{e.message}",
            error_type: 'RecordNotFound',
            retryable: false
          )
        rescue StandardError => e
          logger.error("❌ MESSAGE_MANAGER: Unexpected error claiming step #{msg_data.step_uuid}: #{e.message}")
          logger.error("❌ MESSAGE_MANAGER: #{e.backtrace.first(3).join("\n")}")
          StepClaimResult.failure(
            "Unexpected error during step claiming: #{e.message}",
            error_type: 'UnexpectedError',
            retryable: true
          )
        end
      end

      # @param msg_data [TaskerCore::Types::SimpleQueueMessageData] Simple queue message data
      # @return [Array<TaskerCore::Database::Models::Task, TaskerCore::Execution::StepSequence, TaskerCore::Database::Models::WorkflowStep>] Task, sequence, and step
      def self.get_records_from_message(msg_data)
        task = TaskerCore::Database::Models::Task.with_steps_and_transitions.find_by!(task_uuid: msg_data.task_uuid)
        # we load all of the steps beforehand so this select should be only in memory
        step = task.workflow_steps.select { |step| step.workflow_step_uuid == msg_data.step_uuid }.first
        dependency_steps = []
        unless msg_data.ready_dependency_step_uuids.empty?
          # same as above, this should be in memory because we loaded them above
          dependency_steps = task.workflow_steps.select do |step|
            msg_data.ready_dependency_step_uuids.include?(step.workflow_step_uuid)
          end
        end
        sequence = TaskerCore::Execution::StepSequence.new(dependency_steps)
        [task, sequence, step]
      end

      # TAS-32: Attempt to claim a step by transitioning enqueued → in_progress using Statesman
      # This atomic operation prevents race conditions between workers
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step to claim
      # @param worker_namespace [String] The namespace of the worker attempting to claim
      # @return [Boolean] true if successfully claimed, false if already claimed
      def self.attempt_step_claim(step, worker_namespace)
        logger.debug("🔄 MESSAGE_MANAGER: Checking step #{step.workflow_step_uuid} eligibility for claiming")

        # 1. Check if step is eligible for claiming
        return false unless step_eligible_for_claiming?(step)

        # 2. Attempt atomic state transition using Statesman
        perform_statesman_claim_transition(step, worker_namespace)
      end

      # Check if a step is eligible for claiming based on its current state using Statesman
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step to check
      # @return [Boolean] true if step can be claimed
      def self.step_eligible_for_claiming?(step)
        current_state = step.state_machine.current_state

        eligible_states = [
          TaskerCore::Constants::WorkflowStepStatuses::ENQUEUED,
          TaskerCore::Constants::WorkflowStepStatuses::PENDING # Backward compatibility
        ]

        unless eligible_states.include?(current_state)
          logger.debug("🚫 MESSAGE_MANAGER: Step #{step.workflow_step_uuid} is in state '#{current_state}', cannot claim (not enqueued)")
          return false
        end

        logger.debug("✅ MESSAGE_MANAGER: Step #{step.workflow_step_uuid} is eligible for claiming (state: '#{current_state}')")
        true
      end

      # Perform atomic state transition using Statesman to claim the step
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step to claim
      # @param worker_namespace [String] The namespace of the worker attempting to claim
      # @return [Boolean] true if transition succeeded, false if failed
      def self.perform_statesman_claim_transition(step, worker_namespace)
        # Use Statesman to transition to IN_PROGRESS with claiming metadata
        # Statesman handles all the complexity: atomic transitions, state validation,
        # transition creation, and preventing duplicate/invalid transitions
        claim_metadata = build_claim_metadata(worker_namespace)

        begin
          # Statesman will handle the atomic transaction and state validation
          # Note: Use transition_to! with metadata as named parameter for error throwing
          step.state_machine.transition_to!(
            TaskerCore::Constants::WorkflowStepStatuses::IN_PROGRESS,
            metadata: claim_metadata
          )

          logger.debug("✅ MESSAGE_MANAGER: Successfully claimed step #{step.workflow_step_uuid} using Statesman")
          true
        rescue Statesman::GuardFailedError => e
          # EXPECTED: This happens when the transition is not allowed (e.g., already claimed)
          # This is normal distributed behavior - another worker claimed the step first
          logger.debug("🤝 MESSAGE_MANAGER: Step #{step.workflow_step_uuid} claim blocked by guard (expected distributed behavior): #{e.message}")
          false
        rescue Statesman::TransitionFailedError => e
          # EXPECTED: This happens when the transition validation fails
          # This is normal distributed behavior - state validation prevented the transition
          logger.debug("🤝 MESSAGE_MANAGER: Step #{step.workflow_step_uuid} transition failed (expected distributed behavior): #{e.message}")
          false
        rescue StandardError => e
          # SERVER ERROR: Unexpected errors that indicate system issues
          logger.error("🚨 MESSAGE_MANAGER: SERVER ERROR during Statesman transition for step #{step.workflow_step_uuid}: #{e.message}")
          logger.error('🚨 MESSAGE_MANAGER: This indicates a system configuration or code issue, not normal distributed claiming')
          logger.error("🚨 MESSAGE_MANAGER: #{e.backtrace.first(5).join("\n🚨 MESSAGE_MANAGER: ")}")
          false
        end
      end

      private_class_method :perform_statesman_claim_transition

      # Build metadata for the claiming transition
      #
      # @param worker_namespace [String] The namespace of the claiming worker
      # @return [Hash] Metadata hash for the transition
      def self.build_claim_metadata(worker_namespace)
        {
          claimed_by: "queue_worker_#{worker_namespace}",
          claimed_at: Time.current.iso8601,
          claim_source: 'message_manager_step_claiming'
        }
      end

      private_class_method :build_claim_metadata

      # TAS-32: Complete step execution by saving results and transitioning state
      # This handles both success and failure cases with proper state transitions
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step that was executed
      # @param handler_output [Object] Raw output from the step handler
      # @param execution_time_ms [Integer] Execution time in milliseconds
      # @return [TaskerCore::Types::StepHandlerCallResult::Success, TaskerCore::Types::StepHandlerCallResult::Error] The structured result
      def self.complete_step_execution(step, handler_output, execution_time_ms)
        logger.debug("🏁 MESSAGE_MANAGER: Completing execution for step #{step.workflow_step_uuid}")

        TaskerCore::Database::Models::WorkflowStepTransition.transaction do
          # 1. Convert handler output to structured result
          structured_result = TaskerCore::Types::StepHandlerCallResult.from_handler_output(handler_output)

          # 2. Add execution metadata to the existing result
          enhanced_metadata = structured_result.metadata.merge(
            execution_time_ms: execution_time_ms,
            completed_at: Time.current.iso8601,
            completed_by: 'queue_worker'
          )

          # 3. Update the structured result with enhanced metadata
          if structured_result.success?
            result_with_metadata = TaskerCore::Types::StepHandlerCallResult.success(
              result: structured_result.result,
              metadata: enhanced_metadata
            )
            complete_step_success(step, result_with_metadata)
          else
            result_with_metadata = TaskerCore::Types::StepHandlerCallResult.error(
              error_type: structured_result.error_type,
              message: structured_result.message,
              error_code: structured_result.error_code,
              retryable: structured_result.retryable,
              metadata: enhanced_metadata
            )
            complete_step_failure(step, result_with_metadata)
          end
        end
      rescue StandardError => e
        logger.error("❌ MESSAGE_MANAGER: Failed to complete step execution for #{step.workflow_step_uuid}: #{e.message}")
        logger.error("❌ MESSAGE_MANAGER: #{e.backtrace.first(3).join("\n")}")

        # Create error result for unexpected completion failures
        TaskerCore::Types::StepHandlerCallResult.error(
          error_type: 'StepCompletionError',
          message: "Failed to complete step execution: #{e.message}",
          retryable: true,
          metadata: {
            execution_time_ms: execution_time_ms,
            original_error: e.class.name,
            completion_failure: true
          }
        )
      end

      # Complete a successful step execution
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step
      # @param result [TaskerCore::Types::StepHandlerCallResult::Success] The enhanced success result
      def self.complete_step_success(step, result)
        # We already have a StepHandlerCallResult::Success instance with enhanced metadata
        # Just save it directly
        step.update!(
          results: result.to_h, # Save the entire StepHandlerCallResult as JSON
          processed: true,
          processed_at: Time.current
        )

        # Transition to complete state using Statesman
        step.state_machine.transition_to!(
          TaskerCore::Constants::WorkflowStepStatuses::COMPLETE,
          metadata: result.metadata # Use metadata from the enhanced result
        )

        logger.debug("✅ MESSAGE_MANAGER: Step #{step.workflow_step_uuid} completed successfully")

        # Return the enhanced result
        # NOTE: Task state transitions are handled by the orchestration core, not workers
        result
      end

      private_class_method :complete_step_success

      # Complete a failed step execution
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step
      # @param result [TaskerCore::Types::StepHandlerCallResult::Error] The enhanced error result
      def self.complete_step_failure(step, result)
        # Create error metadata for state transition (includes execution metadata and error details)
        error_metadata = result.metadata.merge(
          error_type: result.error_type,
          error_message: result.message,
          error_code: result.error_code,
          retryable: result.retryable
        )

        # Update step with error information - save the complete enhanced result
        step.update!(
          processed: true,
          processed_at: Time.current,
          results: result.to_h # Save the entire enhanced StepHandlerCallResult::Error as JSON
        )

        # Transition to error state using Statesman
        step.state_machine.transition_to!(
          TaskerCore::Constants::WorkflowStepStatuses::ERROR,
          metadata: error_metadata
        )

        logger.debug("❌ MESSAGE_MANAGER: Step #{step.workflow_step_uuid} completed with error: #{result.message}")

        # Return the enhanced error result
        result
      end

      private_class_method :complete_step_failure

      # TAS-32: Complete step execution when handler throws an exception
      # This handles exception cases with proper error categorization
      #
      # @param step [TaskerCore::Database::Models::WorkflowStep] The step that was executed
      # @param exception [Exception] The exception that was raised
      # @param execution_time_ms [Integer] Execution time in milliseconds
      # @return [TaskerCore::Types::StepHandlerCallResult::Error] The error result
      def self.complete_step_execution_with_exception(step, exception, execution_time_ms)
        logger.debug("💥 MESSAGE_MANAGER: Completing execution for step #{step.workflow_step_uuid} with exception: #{exception.class}")

        begin
          TaskerCore::Database::Models::WorkflowStepTransition.transaction do
            # 1. Convert exception to structured error result
            structured_result = TaskerCore::Types::StepHandlerCallResult.from_exception(exception)

            # 2. Add execution metadata
            enhanced_metadata = structured_result.metadata.merge(
              execution_time_ms: execution_time_ms,
              completed_at: Time.current.iso8601,
              completed_by: 'queue_worker',
              exception_handled: true
            )

            # 3. Create result with enhanced metadata and handle failure
            result_with_metadata = TaskerCore::Types::StepHandlerCallResult.error(
              error_type: structured_result.error_type,
              message: structured_result.message,
              error_code: structured_result.error_code,
              retryable: structured_result.retryable,
              metadata: enhanced_metadata
            )

            complete_step_failure(step, result_with_metadata)
          end
        rescue StandardError => e
          logger.error("❌ MESSAGE_MANAGER: Failed to complete step execution with exception for #{step.workflow_step_uuid}: #{e.message}")
          logger.error("❌ MESSAGE_MANAGER: Original exception: #{exception.message}")

          # Create fallback error result
          TaskerCore::Types::StepHandlerCallResult.error(
            error_type: 'StepCompletionError',
            message: "Failed to complete step after exception: #{e.message}",
            retryable: true,
            metadata: {
              original_exception: exception.class.name,
              original_message: exception.message,
              completion_failure: true
            }
          )
        end
      end
    end
  end
end
