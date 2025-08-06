# frozen_string_literal: true

require_relative 'registry/step_handler_registry'

module TaskerCore
  # Database-backed registry system for step handler resolution
  #
  # The Registry module provides proper database-backed configuration lookup
  # using TaskTemplate and StepTemplate configurations instead of guessing
  # class names from step names.
  #
  # Key features:
  # - Database-first approach using named_tasks.configuration
  # - In-memory caching with TTL for performance
  # - Proper TaskTemplate/StepTemplate deserialization
  # - Handler class availability checking
  # - Thread-safe concurrent access
  module Registry
    module_function

    # Get the global step handler registry instance
    #
    # @return [StepHandlerRegistry] Global registry instance
    def step_handler_registry
      @step_handler_registry ||= StepHandlerRegistry.new
    end

    # Resolve step handler from step message (convenience method)
    #
    # @param step_message [TaskerCore::Types::StepMessage] Step message
    # @return [StepHandlerRegistry::ResolvedStepHandler, nil] Resolved handler or nil
    def resolve_step_handler(step_message)
      step_handler_registry.resolve_step_handler(step_message)
    end

    # Check if we can handle a step (convenience method)
    #
    # @param step_message [TaskerCore::Types::StepMessage] Step message
    # @return [Boolean] true if handler is available
    def can_handle_step?(step_message)
      step_handler_registry.can_handle_step?(step_message)
    end

    # Clear all registry caches (useful for testing)
    def clear_caches!
      step_handler_registry.clear_cache!
    end

    # Get registry system status
    #
    # @return [Hash] Registry status information
    def handle_info
      {
        step_handler_cache_stats: step_handler_registry.cache_stats,
        ready: true,
        checked_at: Time.now.utc.iso8601
      }
    end
  end
end
