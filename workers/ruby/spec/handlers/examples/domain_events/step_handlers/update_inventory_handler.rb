# frozen_string_literal: true

# TAS-65: Domain Event Publishing - Update Inventory Step Handler
#
# This handler demonstrates step-level domain event declarations with:
# - Always condition event (inventory.updated)
# - Fast delivery mode (in-memory)
#
# The "always" condition means the event is published regardless of
# whether the step succeeds or fails.
#
# Events published:
#   - inventory.updated (fast, always condition)
#
module DomainEvents
  module StepHandlers
    class UpdateInventoryHandler < TaskerCore::StepHandler::Base
      # Process the inventory update step
      #
      # @param task [Object] The task being executed
      # @param _sequence [Object] The workflow sequence (unused)
      # @param _step [Object] The current step (unused)
      # @return [TaskerCore::Types::StepHandlerCallResult] The step result
      def call(task, _sequence, _step)
        start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

        # Extract context data
        context = task.context || {}
        order_id = context['order_id'] || 'unknown'

        # Get configuration from handler config
        inventory_source = config['inventory_source'] || 'mock'

        log_info("Updating inventory for order: #{order_id}, source: #{inventory_source}")

        # Mock inventory items
        items = [
          { sku: 'ITEM-001', quantity: 1 },
          { sku: 'ITEM-002', quantity: 2 }
        ]

        execution_time_ms = ((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i

        # Return success - event publishing is handled by worker callback
        # Note: inventory.updated event is published with condition: always
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            order_id: order_id,
            items: items,
            success: true,
            updated_at: Time.now.iso8601
          },
          metadata: {
            execution_time_ms: execution_time_ms,
            inventory_source: inventory_source
          }
        )
      end

      private

      def log_info(message)
        puts "[DomainEvents::UpdateInventoryHandler] #{message}" if ENV['TASKER_ENV'] == 'test'
      end
    end
  end
end
