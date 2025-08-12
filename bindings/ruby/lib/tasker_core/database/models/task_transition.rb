# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      # TaskTransition represents state transitions for Task entities using Statesman
      #
      # This model stores the audit trail of all task state changes, providing
      # a complete history of task lifecycle events with metadata and timestamps.
      class TaskTransition < ApplicationRecord
        self.primary_key = :task_transition_uuid
        # NOTE: We don't include Statesman::Adapters::ActiveRecordTransition
        # because we're using PostgreSQL JSONB column for metadata

        # Associations
        belongs_to :task, foreign_key: :task_uuid, primary_key: :task_uuid, inverse_of: :task_transitions

        # Validations
        validates :to_state, inclusion: {
          in: %w[pending in_progress complete error cancelled resolved_manually],
          message: 'is not a valid task state'
        }

        # Validate that the task exists before creating transition
        validate :task_must_exist

        validates :sort_key, presence: true, uniqueness: { scope: :task_uuid }
        # Custom validation for metadata that allows empty hash but not nil
        validate :metadata_must_be_hash

        # Ensure metadata is always a hash
        after_initialize :ensure_metadata_hash
        # Ensure metadata defaults to empty hash if not provided
        before_validation :ensure_metadata_presence

        # Scopes
        scope :recent, -> { order(sort_key: :desc) }
        scope :to_state, ->(state) { where(to_state: state) }
        scope :with_metadata_key, ->(key) { where('metadata ? :key', key: key.to_s) }

        # Class methods for querying transitions
        class << self
          # Find the most recent transition to a specific state
          #
          # @param state [String, Symbol] The state to find the most recent transition to
          # @return [TaskTransition, nil] The most recent transition to the given state
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

          # Get transition statistics for analytics
          #
          # @return [Hash] Statistics about transitions
          def statistics
            {
              total_transitions: count,
              states: group(:to_state).count,
              recent_activity: where(created_at: 24.hours.ago..Time.current).count,
              average_time_between_transitions: average_time_between_transitions
            }
          end

          private

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

        # Get the duration since the previous transition
        #
        # @return [Float, nil] Duration in seconds since previous transition
        def duration_since_previous
          previous_transition = self.class.where(task_uuid: task_uuid)
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

        # Get human-readable description of the transition
        #
        # @return [String] Description of what this transition represents
        def description
          case to_state
          when 'pending'
            'Task initialized and ready for processing'
          when 'in_progress'
            'Task execution started'
          when 'complete'
            'Task completed successfully'
          when 'error'
            'Task encountered an error'
          when 'cancelled'
            'Task was cancelled'
          when 'resolved_manually'
            'Task was manually resolved'
          else
            "Task transitioned to #{to_state}"
          end
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

        # Validate that the task exists
        #
        # @return [void]
        def task_must_exist
          return if task_uuid.blank?

          unless TaskerCore::Database::Models::Task.exists?(task_uuid: task_uuid)
            errors.add(:task,
                       'must exist before creating transition')
          end
        rescue ActiveRecord::StatementInvalid => e
          # Handle cases where the table might not exist (e.g., during migrations)
          logger.warn { "Could not validate task existence: #{e.message}" }
        end
      end
    end
  end
end
