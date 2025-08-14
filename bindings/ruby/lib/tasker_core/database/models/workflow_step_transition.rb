# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      # WorkflowStepTransition represents state transitions for WorkflowStep entities using Statesman
      #
      # This model stores the audit trail of all workflow step state changes, providing
      # a complete history of step lifecycle events with metadata and timestamps.
      class WorkflowStepTransition < ApplicationRecord
        self.primary_key = :workflow_step_transition_uuid
        # NOTE: We don't include Statesman::Adapters::ActiveRecordTransition because:
        # 1. It tries to serialize metadata, but our JSONB column handles this natively
        # 2. We implement the minimal interface Statesman needs without the serialization conflict

        # Statesman compatibility: Provide the interface Statesman expects without the serialization
        # conflicts from ActiveRecordTransition module
        
        # Statesman expects these attributes to exist on transition records:
        # - to_state (string) - the target state
        # - metadata (hash) - additional data for the transition  
        # - sort_key (integer) - ordering for transitions
        # - most_recent (boolean) - flag for the current transition
        # Our JSONB metadata column handles serialization natively, no need for ActiveRecord serialize

        # Provide Statesman compatibility without serialization conflicts
        def self.updated_timestamp_column
          :updated_at
        end

        # Associations
        belongs_to :workflow_step, foreign_key: :workflow_step_uuid, primary_key: :workflow_step_uuid,
                                   inverse_of: :workflow_step_transitions

        # Validations
        validates :to_state, inclusion: {
          in: %w[pending enqueued in_progress complete error cancelled resolved_manually],
          message: 'is not a valid workflow step state'
        }

        # Validate from_state - allow nil (for initial transitions) but not empty strings
        validates :from_state, inclusion: {
          in: %w[pending enqueued in_progress complete error cancelled resolved_manually],
          message: 'is not a valid workflow step state'
        }, allow_nil: true

        # Prevent empty string states that can cause state machine issues
        validate :from_state_cannot_be_empty_string
        validate :to_state_cannot_be_empty_string

        # Ensure metadata is always a hash
        after_initialize :ensure_metadata_hash
        # Clean up any empty string states before validation
        before_validation :normalize_empty_string_states

        # Validate that the workflow step exists before creating transition
        validate :workflow_step_must_exist

        validates :sort_key, presence: true, uniqueness: { scope: :workflow_step_uuid }
        # Custom validation for metadata that allows empty hash but not nil
        validate :metadata_must_be_hash

        # Ensure metadata defaults to empty hash if not provided
        before_validation :ensure_metadata_presence

        # Scopes
        scope :recent, -> { order(sort_key: :desc) }
        scope :to_state, ->(state) { where(to_state: state) }
        scope :with_metadata_key, ->(key) { where('metadata ? :key', key: key.to_s) }
        scope :for_task, ->(task_uuid) { joins(:workflow_step).where(workflow_steps: { task_uuid: task_uuid }) }

        # Class methods for querying transitions
        class << self
          # Find the most recent transition to a specific state
          #
          # @param state [String, Symbol] The state to find the most recent transition to
          # @return [WorkflowStepTransition, nil] The most recent transition to the given state
          def most_recent_to_state(state)
            to_state(state.to_s).order(sort_key: :desc).first
          end

          # Find all transitions that occurred within a time range
          #
          # @param start_time [Time] The start of the time range
          # @param end_time [Time] The end of the time range
          # @return [ActiveRecord::Relation] Transitions within the time range
          def in_time_range(start_time, end_time)
            where(created_at: start_time..end_time)
          end

          # Find transitions with specific metadata values
          #
          # @param key [String, Symbol] The metadata key to search for
          # @param value [Object] The value to match
          # @return [ActiveRecord::Relation] Transitions with matching metadata
          def with_metadata_value(key, value)
            where('metadata->:key = :value', key: key.to_s, value: value.to_json.delete('"'))
          end

          # Find retry transitions
          #
          # @return [ActiveRecord::Relation] Transitions from error back to pending (retries)
          def retry_transitions
            joins("INNER JOIN #{table_name} prev ON prev.workflow_step_uuid = #{table_name}.workflow_step_uuid")
              .where(to_state: 'pending')
              .where('prev.to_state = ? AND prev.sort_key < ?', 'error', arel_table[:sort_key])
          end

          # Find transitions by attempt number
          #
          # @param attempt [Integer] The attempt number to filter by
          # @return [ActiveRecord::Relation] Transitions for the given attempt
          def for_attempt(attempt)
            with_metadata_value('attempt_number', attempt)
          end

          # Get transition statistics for analytics
          #
          # @return [Hash] Statistics about transitions
          def statistics
            {
              total_transitions: count,
              states: group(:to_state).count,
              recent_activity: where(created_at: 24.hours.ago..Time.current).count,
              retry_attempts: retry_transitions.count,
              average_execution_time: average_execution_time,
              average_time_between_transitions: average_time_between_transitions
            }
          end

          private

          # Calculate average execution time for completed steps
          #
          # @return [Float, nil] Average execution time in seconds
          def average_execution_time
            completed_transitions = with_metadata_key('execution_duration')
            return nil if completed_transitions.empty?

            total_time = completed_transitions.sum { |t| t.get_metadata('execution_duration', 0) }
            total_time / completed_transitions.count
          end

          # Calculate average time between transitions
          #
          # @return [Float, nil] Average seconds between transitions
          def average_time_between_transitions
            transitions = order(:created_at).pluck(:created_at)
            return nil if transitions.size < 2

            total_time = 0
            (1...transitions.size).each do |i|
              total_time += (transitions[i] - transitions[i - 1])
            end

            total_time / (transitions.size - 1)
          end
        end

        # Instance methods

        # Get the associated task through the workflow step
        #
        # @return [Task] The task that owns this workflow step
        delegate :task, to: :workflow_step

        # Get the step name through the workflow step
        #
        # @return [String] The name of the workflow step
        def step_name
          workflow_step.name
        end

        # Get the duration since the previous transition
        #
        # @return [Float, nil] Duration in seconds since previous transition
        def duration_since_previous
          previous_transition = self.class.where(workflow_step_uuid: workflow_step_uuid)
                                    .where(sort_key: ...sort_key)
                                    .order(sort_key: :desc)
                                    .first

          return nil unless previous_transition

          created_at - previous_transition.created_at
        end

        # Check if this transition represents an error state
        #
        # @return [Boolean] True if transitioning to an error state
        def error_transition?
          to_state == 'error'
        end

        # Check if this transition represents completion
        #
        # @return [Boolean] True if transitioning to a completion state
        def completion_transition?
          %w[complete resolved_manually].include?(to_state)
        end

        # Check if this transition represents cancellation
        #
        # @return [Boolean] True if transitioning to cancelled state
        def cancellation_transition?
          to_state == 'cancelled'
        end

        # Check if this transition represents a retry attempt
        #
        # @return [Boolean] True if this is a retry transition
        def retry_transition?
          to_state == 'pending' && has_metadata?('retry_attempt')
        end

        # Get the attempt number for this transition
        #
        # @return [Integer] The attempt number
        def attempt_number
          get_metadata('attempt_number', 1)
        end

        # Get the execution duration if available
        #
        # @return [Float, nil] Execution duration in seconds
        def execution_duration
          get_metadata('execution_duration')
        end

        # Get human-readable description of the transition
        #
        # @return [String] Description of what this transition represents
        def description
          TransitionDescriptionFormatter.format(self)
        end

        # Get formatted metadata for display
        #
        # @return [Hash] Formatted metadata with additional computed fields
        def formatted_metadata
          base_metadata = metadata.dup

          # Add computed fields
          base_metadata['duration_since_previous'] = duration_since_previous
          base_metadata['transition_description'] = description
          base_metadata['transition_timestamp'] = created_at.iso8601
          base_metadata['step_name'] = step_name
          base_metadata['task_uuid'] = workflow_step.task_uuid

          base_metadata
        end

        # Check if transition has specific metadata
        #
        # @param key [String, Symbol] The metadata key to check for
        # @return [Boolean] True if the metadata contains the key
        def has_metadata?(key)
          metadata.key?(key.to_s)
        end

        # Get metadata value with default
        #
        # @param key [String, Symbol] The metadata key
        # @param default [Object] Default value if key not found
        # @return [Object] The metadata value or default
        def get_metadata(key, default = nil)
          metadata.fetch(key.to_s, default)
        end

        # Set metadata value
        #
        # @param key [String, Symbol] The metadata key
        # @param value [Object] The value to set
        # @return [Object] The set value
        def set_metadata(key, value)
          self.metadata = metadata.merge(key.to_s => value)
          value
        end

        # Get backoff information if this is an error transition
        #
        # @return [Hash, nil] Backoff information or nil if not applicable
        def backoff_info
          return nil unless error_transition? && has_metadata?('backoff_until')

          {
            backoff_until: Time.zone.parse(get_metadata('backoff_until')),
            backoff_seconds: get_metadata('backoff_seconds'),
            retry_available: get_metadata('retry_available', false)
          }
        end

        private

        # Ensure metadata is always initialized as a hash
        #
        # @return [void]
        def ensure_metadata_hash
          self.metadata ||= {}
        end

        # Ensure metadata is present for validation
        #
        # @return [void]
        def ensure_metadata_presence
          self.metadata = {} if metadata.blank?
        end

        # Custom validation for metadata
        #
        # @return [void]
        def metadata_must_be_hash
          if metadata.nil?
            self.metadata = {}
          elsif !metadata.is_a?(Hash)
            errors.add(:metadata, 'must be a hash')
          end
        end

        # Validate that the workflow step exists
        #
        # @return [void]
        def workflow_step_must_exist
          return if workflow_step_uuid.blank?

          return if TaskerCore::Database::Models::WorkflowStep.exists?(workflow_step_uuid: workflow_step_uuid)

          errors.add(:workflow_step_uuid, 'must reference an existing workflow step')
        end

        # Prevent empty string from_state values that cause state machine failures
        def from_state_cannot_be_empty_string
          return unless from_state == ''

          errors.add(:from_state, 'cannot be an empty string (use nil for initial transitions)')
        end

        # Prevent empty string to_state values
        def to_state_cannot_be_empty_string
          return unless to_state == ''

          errors.add(:to_state, 'cannot be an empty string')
        end

        # Clean up any empty string states before validation
        # This prevents empty strings from being saved and causing state machine issues
        def normalize_empty_string_states
          # Convert empty strings to nil for from_state (initial transitions)
          self.from_state = nil if from_state == ''

          # For to_state, empty strings are always invalid - convert to nil and let validation catch it
          return unless to_state == ''

          self.to_state = nil
          errors.add(:to_state, 'cannot be empty - must specify a valid state')
        end

        # Service class to format transition descriptions
        # Reduces complexity by organizing description logic by state
        class TransitionDescriptionFormatter
          class << self
            # Format transition description based on state
            #
            # @param transition [WorkflowStepTransition] The transition to describe
            # @return [String] Formatted description
            def format(transition)
              case transition.to_state
              when 'pending'
                format_pending_description(transition)
              when 'in_progress'
                format_in_progress_description(transition)
              when 'complete'
                format_complete_description(transition)
              when 'error'
                format_error_description(transition)
              when 'cancelled'
                format_cancelled_description(transition)
              when 'resolved_manually'
                format_resolved_description(transition)
              else
                format_unknown_description(transition)
              end
            end

            private

            # Format description for pending transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_pending_description(transition)
              if transition.retry_transition?
                "Step retry attempt ##{transition.attempt_number}"
              else
                'Step initialized and ready for execution'
              end
            end

            # Format description for in_progress transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_in_progress_description(transition)
              "Step execution started (attempt ##{transition.attempt_number})"
            end

            # Format description for complete transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_complete_description(transition)
              duration_text = transition.execution_duration ? " in #{transition.execution_duration.round(2)}s" : ''
              "Step completed successfully#{duration_text}"
            end

            # Format description for error transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_error_description(transition)
              error_msg = transition.get_metadata('error_message', 'Unknown error')
              backoff_text = transition.has_metadata?('backoff_until') ? ' (retry scheduled)' : ''
              "Step failed: #{error_msg}#{backoff_text}"
            end

            # Format description for cancelled transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_cancelled_description(transition)
              reason = transition.get_metadata('triggered_by', 'manual cancellation')
              "Step cancelled due to #{reason}"
            end

            # Format description for resolved_manually transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_resolved_description(transition)
              resolver = transition.get_metadata('resolved_by', 'unknown')
              "Step manually resolved by #{resolver}"
            end

            # Format description for unknown transitions
            #
            # @param transition [WorkflowStepTransition] The transition
            # @return [String] Formatted description
            def format_unknown_description(transition)
              "Step transitioned to #{transition.to_state}"
            end
          end
        end
      end
    end
  end
end
