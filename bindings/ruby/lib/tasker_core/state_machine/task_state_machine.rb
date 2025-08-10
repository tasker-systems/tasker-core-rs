# frozen_string_literal: true

require 'statesman'
require_relative '../constants'
# require 'tasker/events/event_payload_builder'
# require_relative '../concerns/event_publisher'

module TaskerCore
  module StateMachine
    # TaskStateMachine defines state transitions for tasks using Statesman
    #
    # This state machine manages task lifecycle states and integrates with
    # the existing event system to provide declarative state management.
    class TaskStateMachine
      class << self
        def logger
          TaskerCore::Logging::Logger.instance
        end
      end

      include Statesman::Machine
      # extend Tasker::Concerns::EventPublisher

      # Define all task states using existing constants
      state Constants::TaskStatuses::PENDING, initial: true
      state Constants::TaskStatuses::IN_PROGRESS
      state Constants::TaskStatuses::COMPLETE
      state Constants::TaskStatuses::ERROR
      state Constants::TaskStatuses::CANCELLED
      state Constants::TaskStatuses::RESOLVED_MANUALLY

      # Define state transitions based on existing StateTransition definitions
      # Fixed: Added missing transitions that guard clauses were handling
      transition from: Constants::TaskStatuses::PENDING,
                 to: [Constants::TaskStatuses::IN_PROGRESS,
                      Constants::TaskStatuses::CANCELLED,
                      Constants::TaskStatuses::ERROR] # Allow direct error from pending

      transition from: Constants::TaskStatuses::IN_PROGRESS,
                 to: [Constants::TaskStatuses::COMPLETE,
                      Constants::TaskStatuses::ERROR,
                      Constants::TaskStatuses::CANCELLED,
                      Constants::TaskStatuses::PENDING] # Allow reset to pending

      transition from: Constants::TaskStatuses::ERROR,
                 to: [Constants::TaskStatuses::PENDING,
                      Constants::TaskStatuses::RESOLVED_MANUALLY]

      # Allow cancellation from complete/error states (admin override scenarios)
      transition from: Constants::TaskStatuses::COMPLETE,
                 to: Constants::TaskStatuses::CANCELLED

      transition from: Constants::TaskStatuses::RESOLVED_MANUALLY,
                 to: Constants::TaskStatuses::CANCELLED

      # Callbacks for lifecycle event integration
      before_transition do |task, transition|
        # Handle idempotent transitions using existing helper method
        if TaskStateMachine.idempotent_transition?(task, transition.to_state)
          # Abort the transition by raising GuardFailedError
          raise Statesman::GuardFailedError, "Already in target state #{transition.to_state}"
        end

        # Log the transition for debugging
        effective_current_state = TaskStateMachine.effective_current_state(task)
        logger.debug do
          "Task #{task.task_id} transitioning from #{effective_current_state} to #{transition.to_state}"
        end

        # Fire before transition event
        TaskStateMachine.safe_fire_event(
          Constants::TaskEvents::BEFORE_TRANSITION,
          {
            task_id: task.task_id,
            from_state: transition.from_state,
            to_state: transition.to_state,
            transition_event: TaskStateMachine.determine_transition_event_name(transition.from_state,
                                                                               transition.to_state)
          }
        )
      end

      after_transition do |task, transition|
        # Determine the appropriate event name based on the transition
        event_name = determine_transition_event_name(transition.from_state, transition.to_state)

        # Only fire the event if we have a valid event name
        if event_name
          # Fire the lifecycle event with task context
          TaskStateMachine.safe_fire_event(
            event_name,
            {
              task_id: task.task_id,
              task_name: task.name,
              task_context: task.context,
              from_state: transition.from_state,
              to_state: transition.to_state,
              transitioned_at: Time.zone.now
            }
          )
        end
      end

      # We do not transition to complete unless the steps are also complete
      guard_transition(to: Constants::TaskStatuses::COMPLETE) do |task, _transition|
        task.all_steps_complete?
      end

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
      end

      # No other guard clauses needed!
      # - State transition validation is handled by the transition definitions above
      # - Idempotent transitions are handled by Statesman automatically
      # - Simple state changes don't need business logic validation

      # Override current_state to work with custom transition model
      # Since TaskTransition doesn't include Statesman::Adapters::ActiveRecordTransition,
      # we need to implement our own current_state logic using the most_recent column
      def current_state
        most_recent_transition = object.task_transitions.where(most_recent: true).first

        if most_recent_transition
          most_recent_transition.to_state
        else
          # Return initial state if no transitions exist
          Constants::TaskStatuses::PENDING
        end
      end

      # Class methods for state machine management
      class << self
        # Check if a transition is idempotent (current state == target state)
        #
        # @param task [Task] The task to check
        # @param target_state [String] The target state
        # @return [Boolean] True if this is an idempotent transition
        def idempotent_transition?(task, target_state)
          task.state_machine.current_state == target_state
        end

        # Get the effective current state, handling blank/empty states
        #
        # @param task [Task] The task to check
        # @return [String] The effective current state (blank states become PENDING)
        def effective_current_state(task)
          current_state = task.state_machine.current_state
          current_state.presence || Constants::TaskStatuses::PENDING
        end

        # Safely fire a lifecycle event using dry-events bus
        #
        # @param event_name [String] The event name
        # @param context [Hash] The event context
        # @return [void]
        def safe_fire_event(event_name, context = {})
          # publish_event(event_name, context)
        end

        # Determine the transition event name based on states using hashmap lookup
        #
        # @param from_state [String] The from state
        # @param to_state [String] The to state
        # @return [String, nil] The event name or nil if no mapping exists
        def determine_transition_event_name(from_state, to_state)
          transition_key = [from_state, to_state]
          event_name = Constants::TASK_TRANSITION_EVENT_MAP[transition_key]

          if event_name.nil?
            logger.warn do
              "Unexpected task state transition: #{from_state || 'initial'} → #{to_state}. " \
                'No event will be fired for this transition.'
            end
          end

          event_name
        end
      end

      private

      # Safely fire a lifecycle event
      #
      # @param event_name [String] The event name
      # @param context [Hash] The event context
      # @return [void]
      def safe_fire_event(event_name, context = {})
        self.class.safe_fire_event(event_name, context)
      end

      # Determine the appropriate event name for a state transition
      #
      # @param from_state [String, nil] The source state
      # @param to_state [String] The target state
      # @return [String] The event name
      def determine_transition_event_name(from_state, to_state)
        self.class.determine_transition_event_name(from_state, to_state)
      end
    end
  end
end
