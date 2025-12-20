# frozen_string_literal: true

module Microservices
  module Handlers
    # User registration workflow handler demonstrating microservices coordination
    #
    # This handler demonstrates:
    # - Parallel execution of independent services (billing + preferences)
    # - Sequential dependencies (welcome after billing + preferences)
    # - Error classification (RetryableError vs PermanentError)
    # - Idempotent operations (conflict handling)
    # - Graceful degradation (optional steps)
    #
    # Workflow Structure:
    # 1. create_user_account (sequential)
    # 2. setup_billing_profile + initialize_preferences (parallel)
    # 3. send_welcome_sequence (depends on billing + preferences)
    # 4. update_user_status (final step)
    class UserRegistrationHandler < TaskerCore::TaskHandler::Base
      def initialize(context = {})
        super
        @logger = context[:logger] || Logger.new($stdout)
        @timeout_seconds = context[:timeout_seconds] || 120
      end

      def call(context)
        @logger.info "🚀 UserRegistrationHandler: Starting user registration workflow for task #{task.task_uuid}"

        user_info = context.task.context.deep_symbolize_keys[:user_info] || {}

        @logger.info "   User: #{user_info[:email]}"
        @logger.info "   Plan: #{user_info[:plan] || 'free'}"
        @logger.info "   Workflow: create_user → (billing ∥ preferences) → welcome → status"
        @logger.info "   Total steps: 5 (demonstrating parallel execution)"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            registration_started: true,
            task_uuid: task.task_uuid,
            total_steps: 5,
            parallel_steps: 2,
            user_email: user_info[:email]
          },
          metadata: {
            handler_class: self.class.name,
            workflow_type: 'microservices_coordination',
            started_at: Time.now.utc.iso8601
          }
        )
      end
    end
  end
end
