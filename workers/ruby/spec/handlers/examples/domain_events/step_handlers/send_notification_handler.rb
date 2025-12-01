# frozen_string_literal: true

# TAS-65: Domain Event Publishing - Send Notification Step Handler
#
# This handler demonstrates step-level domain event declarations with:
# - Success condition event (notification.sent)
# - Fast delivery mode (in-memory)
#
# Events published on success:
#   - notification.sent (fast)
#
module DomainEvents
  module StepHandlers
    class SendNotificationHandler < TaskerCore::StepHandler::Base
      # Process the notification step
      #
      # @param task [Object] The task being executed
      # @param _sequence [Object] The workflow sequence (unused)
      # @param _step [Object] The current step (unused)
      # @return [TaskerCore::Types::StepHandlerCallResult] The step result
      def call(task, _sequence, _step)
        start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

        # Extract context data
        context = task.context || {}
        customer_id = context['customer_id'] || 'unknown'

        # Get configuration from handler config
        notification_type = config['notification_type'] || 'email'

        log_info("Sending notification to: #{customer_id}, type: #{notification_type}")

        # Generate notification ID
        notification_id = "NOTIF-#{SecureRandom.uuid}"
        execution_time_ms = ((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i

        # Return success - event publishing is handled by worker callback
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            notification_id: notification_id,
            channel: notification_type,
            recipient: customer_id,
            sent_at: Time.now.iso8601,
            status: 'delivered'
          },
          metadata: {
            execution_time_ms: execution_time_ms,
            notification_type: notification_type
          }
        )
      end

      private

      def log_info(message)
        puts "[DomainEvents::SendNotificationHandler] #{message}" if ENV['TASKER_ENV'] == 'test'
      end
    end
  end
end
