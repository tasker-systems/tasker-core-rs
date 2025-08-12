# frozen_string_literal: true

require 'json'

module TaskerCore
  module Execution
    # Wrapper for dependency steps that provides handler-expected interface
    #
    # This class wraps the dependency steps loaded from the database via UUIDs
    # and provides a convenient interface that step handlers expect. Instead of
    # complex DependencyChain objects built from serialized data, this works
    # directly with ActiveRecord models.
    #
    # @example Usage in queue worker
    #   # Fetch dependencies from database using UUIDs
    #   dependency_steps = TaskerCore::Database::Models::WorkflowStep.where(
    #     step_uuid: message.ready_dependency_step_uuids
    #   ).includes(:results, :named_step)
    #
    #   # Create sequence wrapper for handler
    #   sequence = StepSequence.new(dependency_steps)
    #
    #   # Call handler with real ActiveRecord models
    #   handler.call(task, sequence, step)
    #
    # @example Handler usage
    #   def call(task, sequence, step)
    #     # Access dependencies by name
    #     order_validation = sequence.get('validate_order')
    #     inventory_check = sequence['check_inventory']
    #
    #     # Iterate over all dependencies
    #     sequence.each do |dep_step|
    #       puts "Dependency: #{dep_step.name}, Results: #{dep_step.results}"
    #     end
    #
    #     # Check if specific dependency exists
    #     return { status: 'skip' } unless sequence.has?('payment_authorized')
    #   end
    class StepSequence
      include Enumerable

      # Initialize with dependency steps from database
      # @param dependency_steps [ActiveRecord::Relation, Array] dependency step records
      def initialize(dependency_steps)
        @dependency_steps = Array(dependency_steps)
        @index = build_name_index
      end

      # Get dependency step by name
      # @param step_name [String, Symbol] name of the dependency step
      # @return [TaskerCore::Database::Models::WorkflowStep, nil] dependency step or nil if not found
      def get(step_name)
        @index[step_name.to_s]
      end
      alias [] get

      # Get all dependency steps
      # @return [Array<TaskerCore::Database::Models::WorkflowStep>] all dependency steps
      def all
        @dependency_steps
      end

      # Get dependency step names
      # @return [Array<String>] names of all dependency steps
      def names
        @dependency_steps.map(&:name)
      end

      # Check if dependency exists
      # @param step_name [String, Symbol] name of the dependency step
      # @return [Boolean] true if dependency exists
      def has?(step_name)
        @index.key?(step_name.to_s)
      end
      alias include? has?

      # Get number of dependencies
      # @return [Integer] number of dependency steps
      def count
        @dependency_steps.count
      end
      alias size count
      alias length count

      # Check if no dependencies
      # @return [Boolean] true if no dependency steps
      def empty?
        @dependency_steps.empty?
      end

      # Check if any dependencies
      # @return [Boolean] true if has dependency steps
      def any?
        !empty?
      end

      # Iterate over dependency steps
      # @yield [TaskerCore::Database::Models::WorkflowStep] each dependency step
      def each(&)
        @dependency_steps.each(&)
      end

      # Map over dependency steps
      # @yield [TaskerCore::Database::Models::WorkflowStep] each dependency step
      # @return [Array] mapped results
      def map(&)
        @dependency_steps.map(&)
      end

      # Filter dependency steps
      # @yield [TaskerCore::Database::Models::WorkflowStep] each dependency step
      # @return [Array<TaskerCore::Database::Models::WorkflowStep>] filtered steps
      def select(&)
        @dependency_steps.select(&)
      end

      # Find dependency step by condition
      # @yield [TaskerCore::Database::Models::WorkflowStep] each dependency step
      # @return [TaskerCore::Database::Models::WorkflowStep, nil] first matching step
      def find(&)
        @dependency_steps.find(&)
      end

      # Convert to array for compatibility
      # @return [Array<TaskerCore::Database::Models::WorkflowStep>] array of dependency steps
      def to_a
        @dependency_steps.dup
      end

      # Convert to hash representation (for debugging/logging)
      # @return [Hash] hash representation
      def to_h
        {
          dependencies: @dependency_steps.map do |step|
            {
              step_uuid: step.workflow_step_uuid,
              name: step.name,
              results: step.results,
              complete: step.complete?
            }
          end,
          count: count
        }
      end

      # Get results from all dependency steps
      # @return [Hash<String, Hash>] step name -> results mapping
      def results_by_name
        @dependency_steps.each_with_object({}) do |step, hash|
          hash[step.name] = step.results || {}
        end
      end

      # Get specific result from a dependency step
      # @param step_name [String, Symbol] name of the dependency step
      # @param key [String, Symbol] key within the results
      # @return [Object, nil] result value or nil if not found
      def get_result(step_name, key = nil)
        step = get(step_name)
        return nil unless step&.results

        if key
          step.results[key.to_s] || step.results[key.to_sym]
        else
          step.results
        end
      end

      def get_results(step_name)
        step = get(step_name)
        return nil unless step&.results

        result = (step.results.is_a?(String) ? JSON.parse(step.results) : step.results).deep_symbolize_keys
        result[:result]
      end

      # Check if all dependencies are complete
      # @return [Boolean] true if all dependency steps are complete
      def all_complete?
        @dependency_steps.all?(&:complete?)
      end

      # Get completed dependency steps
      # @return [Array<TaskerCore::Database::Models::WorkflowStep>] completed steps
      def completed
        @dependency_steps.select(&:complete?)
      end

      # Get pending dependency steps
      # @return [Array<TaskerCore::Database::Models::WorkflowStep>] pending steps
      def pending
        @dependency_steps.select(&:pending?)
      end

      # Get failed dependency steps
      # @return [Array<TaskerCore::Database::Models::WorkflowStep>] failed steps
      def failed
        @dependency_steps.select(&:in_error?)
      end

      private

      # Build name-based index for fast lookups
      # @return [Hash<String, TaskerCore::Database::Models::WorkflowStep>] name -> step mapping
      def build_name_index
        @dependency_steps.each_with_object({}) do |step, hash|
          hash[step.name] = step
        end
      end
    end
  end
end
