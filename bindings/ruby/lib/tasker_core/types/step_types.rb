# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Step-related type definitions for workflow execution
    module StepTypes
      module Types
        include Dry.Types()
      end

      # Step completion struct for workflow step results
      class StepCompletion < Dry::Struct
        attribute :step_name, Types::Coercible::String
        attribute :status, Types::Coercible::String.enum('complete', 'failed', 'pending')
        attribute :results, Types::Hash.default({}.freeze)
        attribute :duration_ms, Types::Integer.optional
        attribute :completed_at, Types::Constructor(Time).optional
        attribute :error_message, Types::Coercible::String.optional

        # Validation for step completion data
        def valid?
          !step_name.empty? && 
          %w[complete failed pending].include?(status) &&
          results.is_a?(Hash)
        end

        # Check if step completed successfully
        def completed?
          status == 'complete'
        end

        # Check if step failed
        def failed?
          status == 'failed'
        end

        # Check if step is still pending
        def pending?
          status == 'pending'
        end

        # Get execution duration in seconds
        def duration_seconds
          return nil unless duration_ms
          duration_ms / 1000.0
        end

        def to_s
          "#<StepCompletion #{step_name} status=#{status} duration=#{duration_seconds}s>"
        end
      end
    end
  end
end