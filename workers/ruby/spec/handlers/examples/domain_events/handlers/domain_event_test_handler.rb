# frozen_string_literal: true

# TAS-65: Domain Event Publishing - Task Handler
#
# This task handler manages the domain event publishing workflow.
# It coordinates the 4-step workflow that demonstrates domain event
# publishing with various conditions and delivery modes.
#
# Steps:
#   1. domain_events_validate_order - Publishes order.validated (fast, success)
#   2. domain_events_process_payment - Publishes payment.processed/failed (durable)
#   3. domain_events_update_inventory - Publishes inventory.updated (fast, always)
#   4. domain_events_send_notification - Publishes notification.sent (fast, success)
#
module DomainEvents
  class DomainEventTestHandler < TaskerCore::TaskHandler::Base
    # Initialize the workflow
    #
    # @param context [Hash] The initial task context
    # @param initialization_data [Hash] Configuration from YAML
    # @return [Hash] Updated context for the workflow
    def initialize_task(context, initialization_data)
      test_scenario = initialization_data['test_scenario'] || 'event_publishing'
      validate_events = initialization_data['validate_events'] || true

      log_info('Initializing domain event publishing workflow')
      log_info("  Test scenario: #{test_scenario}")
      log_info("  Validate events: #{validate_events}")

      # Merge initialization settings into context
      context.merge(
        'workflow_type' => 'domain_event_publishing',
        'test_scenario' => test_scenario,
        'validate_events' => validate_events,
        'initialized_at' => Time.now.iso8601
      )
    end

    # Handle workflow completion
    #
    # @param task [Object] The completed task
    # @param results [Hash] Results from all steps
    # @return [void]
    def finalize_task(task, results)
      log_info('Domain event publishing workflow completed')
      log_info("  Task UUID: #{task.task_uuid}")
      log_info("  Step results: #{results.keys.join(', ')}")
    end

    private

    def log_info(message)
      puts "[DomainEvents::DomainEventTestHandler] #{message}" if ENV['TASKER_ENV'] == 'test'
    end
  end
end
