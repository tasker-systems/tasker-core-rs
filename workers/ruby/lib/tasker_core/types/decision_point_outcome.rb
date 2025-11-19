# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # DecisionPointOutcome - TAS-53 Dynamic Workflow Decision Points
    #
    # Represents the outcome of a decision point handler execution.
    # Decision point handlers make runtime decisions about which workflow steps
    # to create dynamically based on business logic.
    #
    # ## Usage Patterns
    #
    # ### No Branches (No additional steps needed)
    # ```ruby
    # DecisionPointOutcome.no_branches
    # ```
    #
    # ### Create Specific Steps
    # ```ruby
    # # Simple: create one or more steps by name
    # DecisionPointOutcome.create_steps(["approval_required"])
    #
    # # Multiple branches based on condition
    # if high_value?
    #   DecisionPointOutcome.create_steps(["manager_approval", "finance_review"])
    # else
    #   DecisionPointOutcome.create_steps(["auto_approve"])
    # end
    # ```
    #
    # ## Integration with Step Handlers
    #
    # Decision point handlers should return this outcome in their result:
    #
    # ```ruby
    # def call(task, sequence, step)
    #   # Business logic to determine which steps to create
    #   steps_to_create = if task.context['amount'] > 1000
    #     ['manager_approval', 'finance_review']
    #   else
    #     ['auto_approve']
    #   end
    #
    #   outcome = DecisionPointOutcome.create_steps(steps_to_create)
    #
    #   StepHandlerCallResult.success(
    #     result: {
    #       decision_point_outcome: outcome.to_h
    #     },
    #     metadata: {
    #       operation: 'routing_decision',
    #       branches_created: steps_to_create.size
    #     }
    #   )
    # end
    # ```
    module DecisionPointOutcome
      module Types
        include Dry.Types()
      end

      # NoBranches outcome - no additional steps needed
      class NoBranches < Dry::Struct
        # Type discriminator (matches Rust serde tag field)
        attribute :type, Types::String.default('no_branches')

        # Convert to hash for serialization to Rust
        # Note: Rust expects lowercase snake_case due to #[serde(rename_all = "snake_case")]
        def to_h
          { type: 'no_branches' }
        end

        # Check if this outcome requires step creation
        def requires_step_creation?
          false
        end

        # Get step names (empty for NoBranches)
        def step_names
          []
        end
      end

      # CreateSteps outcome - dynamically create specified workflow steps
      class CreateSteps < Dry::Struct
        # Type discriminator (matches Rust serde tag field)
        attribute :type, Types::String.default('create_steps')

        # Array of step names to create
        attribute :step_names, Types::Array.of(Types::Strict::String).constrained(min_size: 1)

        # Convert to hash for serialization to Rust
        # Note: Rust expects lowercase snake_case due to #[serde(rename_all = "snake_case")]
        def to_h
          {
            type: 'create_steps',
            step_names: step_names
          }
        end

        # Check if this outcome requires step creation
        def requires_step_creation?
          true
        end
      end

      # Factory methods for creating outcomes
      class << self
        # Create a NoBranches outcome
        #
        # Use when the decision point determines that no additional workflow
        # steps are needed.
        #
        # @return [NoBranches] A no-branches outcome instance
        #
        # @example
        #   DecisionPointOutcome.no_branches
        def no_branches
          NoBranches.new
        end

        # Create a CreateSteps outcome with specified step names
        #
        # Use when the decision point determines that one or more workflow
        # steps should be dynamically created.
        #
        # @param step_names [Array<String>] Names of steps to create (must be valid step names from template)
        # @return [CreateSteps] A create-steps outcome instance
        # @raise [ArgumentError] if step_names is empty
        #
        # @example Create single step
        #   DecisionPointOutcome.create_steps(['approval_required'])
        #
        # @example Create multiple steps
        #   DecisionPointOutcome.create_steps(['manager_approval', 'finance_review'])
        def create_steps(step_names)
          raise ArgumentError, 'step_names cannot be empty' if step_names.nil? || step_names.empty?

          CreateSteps.new(step_names: step_names)
        end

        # Parse a DecisionPointOutcome from a hash
        #
        # Used to deserialize outcomes from step handler results.
        #
        # @param hash [Hash] The hash representation of an outcome
        # @return [NoBranches, CreateSteps, nil] The parsed outcome or nil if invalid
        #
        # @example
        #   outcome = DecisionPointOutcome.from_hash({
        #     type: 'create_steps',
        #     step_names: ['approval_required']
        #   })
        def from_hash(hash)
          return nil unless hash.is_a?(Hash)

          # Support both symbol and string keys
          # Note: Rust sends lowercase snake_case due to #[serde(rename_all = "snake_case")]
          outcome_type = hash[:type] || hash['type']

          case outcome_type
          when 'no_branches'
            NoBranches.new
          when 'create_steps'
            step_names = hash[:step_names] || hash['step_names']
            return nil unless step_names.is_a?(Array) && !step_names.empty?

            CreateSteps.new(step_names: step_names)
          end
        end
      end
    end
  end
end
