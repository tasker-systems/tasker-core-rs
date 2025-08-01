# frozen_string_literal: true

module TaskerCore
  # Clean Events Domain API
  #
  # This provides a clean, Ruby-idiomatic API for all event operations
  # following the proven Factory/Registry pattern with handle-based optimization.
  #
  # Examples:
  #   TaskerCore::Events.publish(name: "task.completed", payload: {task_id: 123})
  #   TaskerCore::Events.publish_orchestration(type: "step.started", namespace: "workflow")
  #   TaskerCore::Events.statistics
  #   TaskerCore::Events.subscribe(pattern: "task.*", callback: my_callback)
  module Events
  end
end

require_relative 'events/base'
require_relative 'events/domain'
require_relative 'events/publisher'
require_relative 'events/subscribers/base_subscriber'
require_relative 'events/subscribers/error_surfacing_subscriber'
require_relative 'events/concerns/event_based_transitions'
