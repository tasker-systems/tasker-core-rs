# frozen_string_literal: true

require 'statesman'
require_relative '../constants'
# require 'tasker/events/event_payload_builder'
# require_relative '../concerns/event_publisher'

module TaskerCore
  module StateMachine
    # StepStateMachine defines state transitions for workflow steps using Statesman
    #
    # This state machine manages workflow step lifecycle states and integrates with
    # the existing event system to provide declarative state management.
    class StepStateMachine
      class << self
        def logger
          TaskerCore::Logging::Logger.instance
        end
      end

      attr_reader :logger

      include Statesman::Machine
      # extend Tasker::Concerns::EventPublisher

      # Define all step states using existing constants
      state Constants::WorkflowStepStatuses::PENDING, initial: true
      state Constants::WorkflowStepStatuses::IN_PROGRESS
      state Constants::WorkflowStepStatuses::COMPLETE
      state Constants::WorkflowStepStatuses::ERROR
      state Constants::WorkflowStepStatuses::CANCELLED
      state Constants::WorkflowStepStatuses::RESOLVED_MANUALLY

      # Define state transitions based on existing StateTransition definitions
      transition from: Constants::WorkflowStepStatuses::PENDING,
                 to: [Constants::WorkflowStepStatuses::IN_PROGRESS,
                      Constants::WorkflowStepStatuses::ERROR,
                      Constants::WorkflowStepStatuses::CANCELLED,
                      Constants::WorkflowStepStatuses::RESOLVED_MANUALLY] # Allow manual resolution

      transition from: Constants::WorkflowStepStatuses::IN_PROGRESS,
                 to: [Constants::WorkflowStepStatuses::COMPLETE,
                      Constants::WorkflowStepStatuses::ERROR,
                      Constants::WorkflowStepStatuses::CANCELLED]

      transition from: Constants::WorkflowStepStatuses::ERROR,
                 to: [Constants::WorkflowStepStatuses::PENDING,
                      Constants::WorkflowStepStatuses::RESOLVED_MANUALLY]

      # Callbacks for state transitions
      before_transition do |step, transition|
        # Handle idempotent transitions using existing helper method
        if StepStateMachine.idempotent_transition?(step, transition.to_state)
          # Abort the transition by raising GuardFailedError
          raise Statesman::GuardFailedError, "Already in target state #{transition.to_state}"
        end

        # Log the transition for debugging
        effective_current_state = StepStateMachine.effective_current_state(step)
        .logger.debug do
          "Step #{step.workflow_step_id} transitioning from #{effective_current_state} to #{transition.to_state}"
        end
      end

      after_transition do |step, transition|
        # Determine the appropriate event name based on the transition
        event_name = determine_transition_event_name(transition.from_state, transition.to_state)

        # Only fire the event if we have a valid event name
        if event_name
          # Fire the lifecycle event with step context
          StepStateMachine.safe_fire_event(
            event_name,
            {
              task_id: step.task_id,
              step_id: step.workflow_step_id,
              step_name: step.name,
              from_state: transition.from_state,
              to_state: transition.to_state,
              transitioned_at: Time.zone.now
            }
          )
        end
      end

      # Guard clauses for business logic only
      # Let Statesman handle state transition validation and idempotent calls

      guard_transition(to: Constants::WorkflowStepStatuses::IN_PROGRESS) do |step, _transition|
        # Only business rule: check dependencies are met
        StepStateMachine.step_dependencies_met?(step)
      end

      # No other guard clauses needed!
      # - State transition validation is handled by the transition definitions above
      # - Idempotent transitions are handled by Statesman automatically
      # - Simple state changes (PENDING->ERROR, IN_PROGRESS->COMPLETE, etc.) don't need guards

      # Frozen constant mapping state transitions to event names
      # This provides O(1) lookup performance and ensures consistency
      TRANSITION_EVENT_MAP = {
        # Initial state transitions (from nil/initial)
        [nil, Constants::WorkflowStepStatuses::PENDING] => Constants::StepEvents::INITIALIZE_REQUESTED,
        [nil, Constants::WorkflowStepStatuses::IN_PROGRESS] => Constants::StepEvents::EXECUTION_REQUESTED,
        [nil, Constants::WorkflowStepStatuses::COMPLETE] => Constants::StepEvents::COMPLETED,
        [nil, Constants::WorkflowStepStatuses::ERROR] => Constants::StepEvents::FAILED,
        [nil, Constants::WorkflowStepStatuses::CANCELLED] => Constants::StepEvents::CANCELLED,
        [nil, Constants::WorkflowStepStatuses::RESOLVED_MANUALLY] => Constants::StepEvents::RESOLVED_MANUALLY,

        # Normal state transitions
        [Constants::WorkflowStepStatuses::PENDING,
         Constants::WorkflowStepStatuses::IN_PROGRESS] => Constants::StepEvents::EXECUTION_REQUESTED,
        [Constants::WorkflowStepStatuses::PENDING,
         Constants::WorkflowStepStatuses::ERROR] => Constants::StepEvents::FAILED,
        [Constants::WorkflowStepStatuses::PENDING,
         Constants::WorkflowStepStatuses::CANCELLED] => Constants::StepEvents::CANCELLED,
        [Constants::WorkflowStepStatuses::PENDING,
         Constants::WorkflowStepStatuses::RESOLVED_MANUALLY] => Constants::StepEvents::RESOLVED_MANUALLY,

        [Constants::WorkflowStepStatuses::IN_PROGRESS,
         Constants::WorkflowStepStatuses::COMPLETE] => Constants::StepEvents::COMPLETED,
        [Constants::WorkflowStepStatuses::IN_PROGRESS,
         Constants::WorkflowStepStatuses::ERROR] => Constants::StepEvents::FAILED,
        [Constants::WorkflowStepStatuses::IN_PROGRESS,
         Constants::WorkflowStepStatuses::CANCELLED] => Constants::StepEvents::CANCELLED,

        [Constants::WorkflowStepStatuses::ERROR,
         Constants::WorkflowStepStatuses::PENDING] => Constants::StepEvents::RETRY_REQUESTED,
        [Constants::WorkflowStepStatuses::ERROR,
         Constants::WorkflowStepStatuses::RESOLVED_MANUALLY] => Constants::StepEvents::RESOLVED_MANUALLY
      }.freeze

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
      end

      # Override current_state to work with custom transition model
      # Since WorkflowStepTransition doesn't include Statesman::Adapters::ActiveRecordTransition,
      # we need to implement our own current_state logic using the most_recent column
      def current_state
        most_recent_transition = object.workflow_step_transitions.where(most_recent: true).first

        if most_recent_transition
          # Ensure we never return empty strings or nil - always return a valid state
          state = most_recent_transition.to_state
          state.presence || Constants::WorkflowStepStatuses::PENDING
        else
          # Return initial state if no transitions exist
          Constants::WorkflowStepStatuses::PENDING
        end
      end

      # Override Statesman's transition building to ensure proper from_state handling
      # This is called by Statesman when creating new transitions
      def create_transition(from_state, to_state, metadata = {})
        # Ensure from_state is properly set - never allow empty strings
        effective_from_state = case from_state
                               when nil, ''
                                 # For initial transitions or empty strings, use nil
                                 nil
                               else
                                 # For existing states, ensure it's a valid state
                                 from_state.presence
                               end

        # Log transition creation for debugging
        logger.debug do
          "StepStateMachine: Creating transition for step #{object.workflow_step_id}: " \
            "'#{effective_from_state}' → '#{to_state}'"
        end

        # Get the next sort key
        next_sort_key = next_sort_key_value

        # Create the transition with proper from_state handling
        transition = TaskerCore::Database::Models::WorkflowStepTransition.create!(
          workflow_step_id: object.workflow_step_id,
          to_state: to_state,
          from_state: effective_from_state, # Use nil instead of empty string
          most_recent: true,
          sort_key: next_sort_key,
          metadata: metadata || {},
          created_at: Time.current,
          updated_at: Time.current
        )

        # Update previous transitions to not be most recent
        object.workflow_step_transitions
              .where(most_recent: true)
              .where.not(id: transition.id)
              .update_all(most_recent: false)

        transition
      end

      # Get the next sort key for transitions
      def next_sort_key_value
        max_sort_key = object.workflow_step_transitions.maximum(:sort_key) || -1
        max_sort_key + 10 # Use increments of 10 for flexibility
      end

      # Initialize the state machine with the initial state
      # This ensures the state machine is properly initialized when called explicitly
      # DEFENSIVE: Only creates transitions when explicitly needed
      def initialize_state_machine!
        # Check if state machine is already initialized
        return current_state if TaskerCore::Database::Models::WorkflowStepTransition.exists?(workflow_step_id: object.workflow_step_id)

        # DEFENSIVE: Use a rescue block instead of transaction to handle race conditions gracefully
        begin
          # Create the initial transition only if none exists
          initial_transition = TaskerCore::Database::Models::WorkflowStepTransition.create!(
            workflow_step_id: object.workflow_step_id,
            to_state: Constants::WorkflowStepStatuses::PENDING,
            from_state: nil, # Explicitly set to nil for initial transition
            most_recent: true,
            sort_key: 0,
            metadata: { initialized_by: 'state_machine' },
            created_at: Time.current,
            updated_at: Time.current
          )

          logger.debug do
            "StepStateMachine: Initialized state machine for step #{object.workflow_step_id} with initial transition to PENDING"
          end

          initial_transition.to_state
        rescue ActiveRecord::RecordNotUnique => e
          # Handle duplicate key violations gracefully - another thread may have initialized the state machine
          logger.debug do
            "StepStateMachine: State machine for step #{object.workflow_step_id} already initialized by another process: #{e.message}"
          end

          # Return the current state since we know it's initialized
          current_state
        rescue ActiveRecord::StatementInvalid => e
          # Handle transaction issues gracefully
          logger.warn do
            "StepStateMachine: Transaction issue initializing state machine for step #{object.workflow_step_id}: #{e.message}"
          end

          # Check if the step actually has transitions now (another process may have created them)
          if TaskerCore::Database::Models::WorkflowStepTransition.exists?(workflow_step_id: object.workflow_step_id)
            current_state
          else
            # If still no transitions, return the default state without creating a transition
            Constants::WorkflowStepStatuses::PENDING
          end
        end
      end

      # Class methods for state machine management
      class << self
        # Class-level wrapper methods for guard clause context
        # These delegate to instance methods to provide clean access from guard clauses

        # Check if a transition is idempotent (current state == target state)
        #
        # @param step [WorkflowStep] The step to check
        # @param target_state [String] The target state
        # @return [Boolean] True if this is an idempotent transition
        def idempotent_transition?(step, target_state)
          current_state = step.state_machine.current_state
          effective_current_state = current_state.presence || Constants::WorkflowStepStatuses::PENDING
          is_idempotent = effective_current_state == target_state

          if is_idempotent
            logger.debug do
              "StepStateMachine: Allowing idempotent transition to #{target_state} for step #{step.workflow_step_id}"
            end
          end

          is_idempotent
        end

        # Get the effective current state, handling blank/empty states
        #
        # @param step [WorkflowStep] The step to check
        # @return [String] The effective current state (blank states become PENDING)
        def effective_current_state(step)
          current_state = step.state_machine.current_state
          current_state.presence || Constants::WorkflowStepStatuses::PENDING
        end

        # Log an invalid from-state transition
        #
        # @param step [WorkflowStep] The step
        # @param current_state [String] The current state
        # @param target_state [String] The target state
        # @param reason [String] The reason for the restriction
        def log_invalid_from_state(step, current_state, target_state, reason)
          logger.debug do
            "StepStateMachine: Cannot transition to #{target_state} from '#{current_state}' " \
              "(step #{step.workflow_step_id}). #{reason}."
          end
        end

        # Log when dependencies are not met
        #
        # @param step [WorkflowStep] The step
        # @param target_state [String] The target state
        def log_dependencies_not_met(step, target_state)
          logger.debug do
            "StepStateMachine: Cannot transition step #{step.workflow_step_id} to #{target_state} - " \
              'dependencies not satisfied. Check parent step completion status.'
          end
        end

        # Log the result of a transition check
        #
        # @param step [WorkflowStep] The step
        # @param target_state [String] The target state
        # @param result [Boolean] Whether the transition is allowed
        # @param reason [String] The reason for the result
        def log_transition_result(step, target_state, result, reason)
          if result
            logger.debug do
              "StepStateMachine: Allowing transition to #{target_state} for step #{step.workflow_step_id} (#{reason})"
            end
          else
            logger.debug do
              "StepStateMachine: Blocking transition to #{target_state} for step #{step.workflow_step_id} (#{reason} failed)"
            end
          end
        end

        # Check if step dependencies are met
        #
        # @param step [WorkflowStep] The step to check
        # @return [Boolean] True if all dependencies are satisfied
        def step_dependencies_met?(step)
          # Handle cases where step doesn't have parents association or it's not loaded

          # If step doesn't respond to parents, assume no dependencies
          return true unless step.respond_to?(:parents)

          # If parents association exists but is empty, no dependencies to check
          parents = step.parents
          return true if parents.blank?

          # Check if all parent steps are complete
          parents.all? do |parent|
            completion_states = [
              Constants::WorkflowStepStatuses::COMPLETE,
              Constants::WorkflowStepStatuses::RESOLVED_MANUALLY
            ]
            # Use state_machine.current_state to avoid circular reference with parent.status
            current_state = parent.state_machine.current_state
            parent_status = current_state.presence || Constants::WorkflowStepStatuses::PENDING
            is_complete = completion_states.include?(parent_status)

            unless is_complete
              logger.debug do
                "StepStateMachine: Step #{step.workflow_step_id} dependency not met - " \
                  "parent step #{parent.workflow_step_id} is '#{parent_status}', needs to be complete"
              end
            end

            is_complete
          end
        rescue StandardError => e
          # If there's an error checking dependencies, log it and assume dependencies are met
          # This prevents dependency checking from blocking execution in edge cases
          logger.warn do
            "StepStateMachine: Error checking dependencies for step #{step.workflow_step_id}: #{e.message}. " \
              'Assuming dependencies are met.'
          end
          true
        end

        # Safely fire a lifecycle event using dry-events bus
        #
        # @param event_name [String] The event name
        # @param context [Hash] The event context
        # @return [void]
        def safe_fire_event(_event_name, context = {})
          # Use EventPayloadBuilder for consistent payload structure
          # step = extract_step_from_context(context)
          # task = step&.task

          # if step && task
          #   # Determine event type from event name
          #   event_type = determine_event_type_from_name(event_name)

          #   # TODO: Implement EventPayloadBuilder for standardized payload
          #   enhanced_context = Tasker::Events::EventPayloadBuilder.build_step_payload(
          #     step,
          #     task,
          #     event_type: event_type,
          #     additional_context: context
          #   )
          # else
          #   # TODO: Implement fallback logic for enhanced context
          #   enhanced_context = build_standardized_payload(event_name, context)
          # end

          # publish_event(event_name, enhanced_context)
        end

        # Extract step object from context for EventPayloadBuilder
        #
        # @param context [Hash] The event context
        # @return [WorkflowStep, nil] The step object if available
        def extract_step_from_context(context)
          step_id = context[:step_id]
          return nil unless step_id

          # Try to find the step - handle both string and numeric IDs
          TaskerCore::Database::Models::WorkflowStep.find_by(workflow_step_id: step_id) ||
            TaskerCore::Database::Models::WorkflowStep.find_by(id: step_id)
        rescue StandardError => e
          logger.warn { "Could not find step with ID #{step_id}: #{e.message}" }
          nil
        end

        # Determine event type from event name for EventPayloadBuilder
        #
        # @param event_name [String] The event name
        # @return [Symbol] The event type
        def determine_event_type_from_name(event_name)
          case event_name
          when /completed/i
            :completed
          when /failed/i, /error/i
            :failed
          when /execution_requested/i, /started/i
            :started
          when /retry/i
            :retry
          when /backoff/i
            :backoff
          else
            :unknown
          end
        end

        # Build standardized event payload with all expected keys (legacy fallback)
        #
        # @param event_name [String] The event name
        # @param context [Hash] The base context
        # @return [Hash] Enhanced context with standardized payload structure
        def build_standardized_payload(_event_name, context)
          # Base payload with core identifiers
          enhanced_context = {
            # Core identifiers (always present)
            task_id: context[:task_id],
            step_id: context[:step_id],
            step_name: context[:step_name],

            # State transition information
            from_state: context[:from_state],
            to_state: context[:to_state],

            # Timing information (provide defaults for missing keys)
            started_at: context[:started_at] || context[:transitioned_at],
            completed_at: context[:completed_at] || context[:transitioned_at],
            execution_duration: context[:execution_duration] || 0.0,

            # Error information (for error events)
            error_message: context[:error_message] || context[:error] || 'Unknown error',
            exception_class: context[:exception_class] || 'StandardError',
            attempt_number: context[:attempt_number] || 1,

            # Additional context
            transitioned_at: context[:transitioned_at] || Time.zone.now
          }

          # Merge in any additional context provided
          enhanced_context.merge!(context.except(
                                    :task_id, :step_id, :step_name, :from_state, :to_state,
                                    :started_at, :completed_at, :execution_duration,
                                    :error_message, :exception_class, :attempt_number, :transitioned_at
                                  ))

          enhanced_context
        end

        # Determine the appropriate event name for a state transition using constant lookup
        #
        # @param from_state [String, nil] The source state
        # @param to_state [String] The target state
        # @return [String, nil] The event name or nil if no mapping exists
        def determine_transition_event_name(from_state, to_state)
          transition_key = [from_state, to_state]
          event_name = TRANSITION_EVENT_MAP[transition_key]

          if event_name.nil?
            # For unexpected transitions, log a warning and return nil to skip event firing
            logger.warn do
              "Unexpected step state transition: #{from_state || 'initial'} → #{to_state}. " \
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
