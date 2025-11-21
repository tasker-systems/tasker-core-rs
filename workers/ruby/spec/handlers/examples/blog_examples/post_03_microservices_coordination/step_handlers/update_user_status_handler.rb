# frozen_string_literal: true

module Microservices
  module StepHandlers
    # Update user status handler demonstrating workflow completion
    #
    # This handler demonstrates:
    # - Final step in multi-service coordination
    # - Accessing results from all prior steps
    # - Workflow completion validation
    # - Summary generation
    class UpdateUserStatusHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        logger.info "✔️  UpdateUserStatusHandler: Updating user status to active - task_uuid=#{task.task_uuid}"

        # Collect results from all prior steps
        user_data = sequence.get_results('create_user_account')
        billing_data = sequence.get_results('setup_billing_profile')
        preferences_data = sequence.get_results('initialize_preferences')
        welcome_data = sequence.get_results('send_welcome_sequence')

        # Validate all prior steps completed
        validate_workflow_completion!(user_data, billing_data, preferences_data, welcome_data)

        user_id = user_data['user_id'] || user_data[:user_id]
        plan = user_data['plan'] || user_data[:plan] || 'free'

        logger.info "   User ID: #{user_id}"
        logger.info "   Plan: #{plan}"

        # Simulate updating user status in user service
        result = simulate_user_status_update(user_id, plan, user_data, billing_data, preferences_data, welcome_data)

        logger.info "✅ UpdateUserStatusHandler: User status updated to #{result[:status]}"
        logger.info "   Registration complete!"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'update_user_status',
            service: 'user_service',
            plan: plan,
            workflow_complete: true,
            completed_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ UpdateUserStatusHandler: Status update failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def validate_workflow_completion!(user_data, billing_data, preferences_data, welcome_data)
        missing_steps = []
        missing_steps << 'create_user_account' unless user_data
        missing_steps << 'setup_billing_profile' unless billing_data
        missing_steps << 'initialize_preferences' unless preferences_data
        missing_steps << 'send_welcome_sequence' unless welcome_data

        return if missing_steps.empty?

        raise TaskerCore::Errors::PermanentError.new(
          "Cannot complete registration: missing results from steps: #{missing_steps.join(', ')}",
          error_code: 'INCOMPLETE_WORKFLOW'
        )
      end

      def simulate_user_status_update(user_id, plan, user_data, billing_data, preferences_data, welcome_data)
        # Build registration summary
        registration_summary = build_registration_summary(
          user_id, plan, user_data, billing_data, preferences_data, welcome_data
        )

        # Simulate updating user status in user service
        {
          user_id: user_id,
          status: 'active',
          plan: plan,
          registration_summary: registration_summary,
          activation_timestamp: Time.now.utc.iso8601,
          all_services_coordinated: true,
          services_completed: [
            'user_service',
            'billing_service',
            'preferences_service',
            'notification_service'
          ]
        }
      end

      def build_registration_summary(user_id, plan, user_data, billing_data, preferences_data, welcome_data)
        summary = {
          user_id: user_id,
          email: user_data['email'] || user_data[:email],
          plan: plan,
          registration_status: 'complete'
        }

        # Add billing summary if applicable
        if plan != 'free'
          summary[:billing_id] = billing_data['billing_id'] || billing_data[:billing_id]
          summary[:next_billing_date] = billing_data['next_billing_date'] || billing_data[:next_billing_date]
        end

        # Add preferences summary
        prefs = preferences_data['preferences'] || preferences_data[:preferences]
        summary[:preferences_count] = prefs.is_a?(Hash) ? prefs.keys.count : 0

        # Add welcome sequence summary
        summary[:welcome_sent] = true
        summary[:notification_channels] = welcome_data['channels_used'] || welcome_data[:channels_used] || []

        # Add timestamps
        summary[:user_created_at] = user_data['created_at'] || user_data[:created_at]
        summary[:registration_completed_at] = Time.now.utc.iso8601

        summary
      end
    end
  end
end
