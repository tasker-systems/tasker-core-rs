# frozen_string_literal: true

module TaskerCore
  module Events
    module Concerns
      # EventBasedTransitions provides simple helper methods for event-driven state transitions
      #
      # This concern trusts Rust orchestration as the single source of truth for state logic.
      # Ruby's role is purely reactive - handling events and surfacing errors based on what
      # Rust tells us, rather than duplicating validation logic.
      #
      # The Rust state machine handles all validation, and publishes appropriate events:
      # - Valid transitions publish success events (task.completed, step.failed, etc.)
      # - Invalid transitions publish error events (workflow.invalid_transition)
      # - Ruby reacts to these events to surface appropriate responses
      module EventBasedTransitions
        # Simple idempotency check - skip if transitioning to same state
        #
        # This is the only validation Ruby should do, since it's purely about
        # avoiding duplicate events rather than business logic validation.
        #
        # @param current_state [String, nil] The current state
        # @param target_state [String] The desired target state
        # @param context [Hash] Optional context for logging
        # @return [Boolean] True if transition event should be published
        def should_publish_transition?(current_state, target_state, context = {})
          # Always publish if we don't know the current state
          return true if current_state.nil?

          # Skip if states are the same (idempotency)
          if current_state == target_state
            log_transition_skip(current_state, target_state, context)
            return false
          end

          log_transition_intent(current_state, target_state, context)
          true
        end

        # Publish a transition request to Rust and let it handle validation
        #
        # This is the new approach - Ruby sends transition requests and Rust
        # publishes back the appropriate events (success or error).
        #
        # @param entity_type [String] 'task' or 'step'
        # @param entity_id [String] The ID of the task/step
        # @param from_state [String, nil] Current state
        # @param to_state [String] Target state
        # @param context [Hash] Additional context for the transition
        # @return [Boolean] True if request was sent (doesn't guarantee success)
        def request_transition(entity_type, entity_id, from_state, to_state, context = {})
          # Skip if no actual transition needed
          return false unless should_publish_transition?(from_state, to_state, {
                                                           "#{entity_type}_id": entity_id
                                                         })

          # Send transition request to Rust orchestration
          # Rust will validate and publish appropriate success/error events
          publish_transition_request(entity_type, entity_id, from_state, to_state, context)
          true
        end

        private

        # Publish transition request event that Rust orchestration will handle
        def publish_transition_request(entity_type, entity_id, from_state, to_state, context)
          event_name = 'workflow.transition_requested'
          payload = context.merge(
            entity_type: entity_type,
            entity_id: entity_id,
            from_state: from_state,
            to_state: to_state,
            requested_at: Time.now.iso8601
          )

          # This would ideally call back to Rust via FFI, but for now we can
          # use the Publisher to demonstrate the pattern
          TaskerCore::Events::Publisher.instance.publish(event_name, payload)
        end

        # Log when a transition is skipped for idempotency
        def log_transition_skip(_current_state, target_state, context)
          logger.debug do
            "Skipping transition event: already in #{target_state} state" +
              (context.any? ? " (#{context})" : '')
          end
        end

        # Log transition intent
        def log_transition_intent(current_state, target_state, context)
          logger.debug do
            "Requesting transition: #{current_state} -> #{target_state}" +
              (context.any? ? " (#{context})" : '')
          end
        end

        # Get logger instance
        def logger
          @logger ||= TaskerCore::Logging::Logger.new
        end
      end
    end
  end
end
