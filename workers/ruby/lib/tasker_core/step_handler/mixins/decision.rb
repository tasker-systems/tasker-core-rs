# frozen_string_literal: true

module TaskerCore
  module StepHandler
    module Mixins
      # Decision mixin for TAS-53 Dynamic Workflow Decision Points
      #
      # ## TAS-112: Composition Pattern
      #
      # This module follows the composition-over-inheritance pattern. Instead of
      # inheriting from a specialized Decision handler class, include this mixin
      # in your Base handler.
      #
      # ## Usage
      #
      # ```ruby
      # class MyDecisionHandler < TaskerCore::StepHandler::Base
      #   include TaskerCore::StepHandler::Mixins::Decision
      #
      #   def call(context)
      #     amount = context.get_task_field('amount')
      #
      #     if amount < 1000
      #       decision_success(
      #         steps: ['auto_approve'],
      #         result_data: { route_type: 'auto', amount: amount }
      #       )
      #     else
      #       decision_success(
      #         steps: ['manager_approval', 'finance_review'],
      #         result_data: { route_type: 'dual', amount: amount }
      #       )
      #     end
      #   end
      # end
      # ```
      #
      # ## No-Branch Pattern
      #
      # ```ruby
      # def call(context)
      #   if context.get_task_field('skip_approval')
      #     decision_no_branches(result_data: { reason: 'skipped' })
      #   else
      #     decision_success(steps: ['standard_approval'])
      #   end
      # end
      # ```
      module Decision
        # Hook called when module is included
        def self.included(base)
          base.extend(ClassMethods)
        end

        # Class methods added to including class
        module ClassMethods
          # No class methods needed for now
        end

        # Override capabilities to include decision-specific features
        def capabilities
          super + %w[decision_point dynamic_workflow step_creation]
        end

        # Enhanced configuration schema for decision handlers
        def config_schema
          super.merge({
                        properties: super[:properties].merge(
                          decision_thresholds: {
                            type: 'object',
                            description: 'Thresholds for decision routing logic'
                          },
                          decision_metadata: {
                            type: 'object',
                            description: 'Additional metadata for decision logging'
                          }
                        )
                      })
        end

        # ========================================================================
        # DECISION OUTCOME HELPER METHODS
        # ========================================================================

        # Return a successful decision outcome that creates specified steps
        #
        # @param steps [Array<String>, String] Step name(s) to create dynamically
        # @param result_data [Hash] Additional result data (route_type, amounts, etc.)
        # @param metadata [Hash] Optional metadata for observability
        # @return [TaskerCore::Types::StepHandlerCallResult] Success result with decision outcome
        #
        # @example Single step
        #   decision_success(
        #     steps: 'approval_required',
        #     result_data: { route_type: 'standard' }
        #   )
        #
        # @example Multiple steps
        #   decision_success(
        #     steps: ['manager_approval', 'finance_review'],
        #     result_data: { route_type: 'dual_approval', amount: 10000 }
        #   )
        def decision_success(steps:, result_data: {}, metadata: {})
          # Normalize steps to array
          step_names = Array(steps)

          # Validate step names
          validate_step_names!(step_names)

          # Create decision outcome using type-safe factory
          outcome = TaskerCore::Types::DecisionPointOutcome.create_steps(step_names)

          # Build result with decision outcome embedded
          result = result_data.merge(
            decision_point_outcome: outcome.to_h
          )

          # Create success result
          TaskerCore::Types::StepHandlerCallResult.success(
            result: result,
            metadata: build_decision_metadata(metadata, outcome)
          )
        end

        # Return a decision outcome indicating no additional steps needed
        #
        # Use this when the decision point determines that workflow can proceed
        # without creating any dynamic steps.
        #
        # @param result_data [Hash] Result data explaining why no branches needed
        # @param metadata [Hash] Optional metadata for observability
        # @return [TaskerCore::Types::StepHandlerCallResult] Success result with no-branches outcome
        #
        # @example
        #   decision_no_branches(
        #     result_data: { reason: 'amount_below_threshold', amount: 50 }
        #   )
        def decision_no_branches(result_data: {}, metadata: {})
          # Create no-branches outcome
          outcome = TaskerCore::Types::DecisionPointOutcome.no_branches

          # Build result with outcome embedded
          result = result_data.merge(
            decision_point_outcome: outcome.to_h
          )

          # Create success result
          TaskerCore::Types::StepHandlerCallResult.success(
            result: result,
            metadata: build_decision_metadata(metadata, outcome)
          )
        end

        # Alias for decision_no_branches for cross-language consistency
        alias skip_branches decision_no_branches

        # Validate that a decision point outcome is properly structured
        #
        # @param outcome [Hash, DecisionPointOutcome] Outcome to validate
        # @raise [TaskerCore::PermanentError] if outcome is invalid
        def validate_decision_outcome!(outcome)
          # Convert to hash if it's a DecisionPointOutcome type
          outcome_hash = if outcome.respond_to?(:to_h)
                           outcome.to_h
                         elsif outcome.is_a?(Hash)
                           outcome
                         else
                           raise_invalid_outcome!('Outcome must be Hash or DecisionPointOutcome')
                         end

          # Validate type field exists
          outcome_type = outcome_hash[:type] || outcome_hash['type'] ||
                         outcome_hash[:outcome_type] || outcome_hash['outcome_type']
          unless %w[NoBranches CreateSteps no_branches create_steps].include?(outcome_type)
            raise_invalid_outcome!("Invalid outcome_type: #{outcome_type}")
          end

          # Validate CreateSteps has step_names
          normalized_type = outcome_type.downcase.gsub('_', '')
          if normalized_type == 'createsteps'
            step_names = outcome_hash[:step_names] || outcome_hash['step_names']
            validate_step_names!(step_names)
          end

          outcome_hash
        end

        # Build a decision result with a custom outcome
        #
        # Use this for advanced scenarios where you need full control over the outcome
        # structure. Most handlers should use decision_success or decision_no_branches.
        #
        # @param outcome [DecisionPointOutcome, Hash] The decision outcome
        # @param result_data [Hash] Additional result data
        # @param metadata [Hash] Optional metadata
        # @return [TaskerCore::Types::StepHandlerCallResult] Success result
        def decision_with_custom_outcome(outcome:, result_data: {}, metadata: {})
          # Validate outcome structure
          validated_outcome = validate_decision_outcome!(outcome)

          # Build result
          result = result_data.merge(
            decision_point_outcome: validated_outcome
          )

          # Create success result
          TaskerCore::Types::StepHandlerCallResult.success(
            result: result,
            metadata: build_decision_metadata(metadata, outcome)
          )
        end

        private

        # Validate step names for decision outcomes
        def validate_step_names!(step_names)
          unless step_names.is_a?(Array) && !step_names.empty?
            raise_invalid_outcome!('step_names must be non-empty array')
          end

          unless step_names.all? { |name| name.is_a?(String) && !name.empty? }
            raise_invalid_outcome!('All step names must be non-empty strings')
          end

          true
        end

        # Build metadata for decision outcomes
        def build_decision_metadata(custom_metadata, outcome)
          base_metadata = {
            decision_point: true,
            outcome_type: outcome.type,
            branches_created: outcome.step_names.size,
            processed_at: Time.now.utc.iso8601,
            processed_by: handler_name
          }

          base_metadata.merge(custom_metadata)
        end

        # Raise a permanent error for invalid decision outcomes
        def raise_invalid_outcome!(message)
          raise TaskerCore::Errors::PermanentError.new(
            "Invalid decision point outcome: #{message}",
            error_code: 'INVALID_DECISION_OUTCOME',
            context: { error_category: 'validation' }
          )
        end
      end
    end
  end
end
