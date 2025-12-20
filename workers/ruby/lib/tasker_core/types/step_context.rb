# frozen_string_literal: true

module TaskerCore
  module Types
    # StepContext provides a unified context for step handler execution.
    #
    # This is the cross-language standard context object passed to handler.call(context).
    # It wraps the FFI-provided TaskSequenceStepWrapper and adds convenience accessors
    # that match Python and Rust naming conventions.
    #
    # Cross-language standard fields:
    # - task_uuid: UUID of the task
    # - step_uuid: UUID of the workflow step
    # - input_data: Step input data from workflow_step.inputs
    # - step_inputs: Alias for input_data
    # - step_config: Handler configuration from step_definition.handler.initialization
    # - retry_count: Current retry attempt count
    # - max_retries: Maximum retry attempts allowed
    # - dependency_results: Results from parent steps
    #
    # Ruby-specific accessors (for backward compatibility):
    # - task: TaskWrapper instance
    # - workflow_step: WorkflowStepWrapper instance
    # - step_definition: StepDefinitionWrapper instance
    #
    # @example Accessing context in a handler
    #   def call(context)
    #     # Cross-language standard fields
    #     task_uuid = context.task_uuid
    #     step_uuid = context.step_uuid
    #     input_data = context.input_data
    #     deps = context.dependency_results
    #
    #     # Convenience methods
    #     even_number = context.get_task_field('even_number')
    #     prev_result = context.get_dependency_result('step_1')
    #
    #     # Ruby-specific accessors (backward compat)
    #     task = context.task
    #     step = context.workflow_step
    #   end
    #
    # @see TaskerCore::Models::TaskSequenceStepWrapper The underlying wrapper
    class StepContext
      # @return [TaskerCore::Models::TaskWrapper] Task metadata and context
      attr_reader :task

      # @return [TaskerCore::Models::WorkflowStepWrapper] Step execution state
      attr_reader :workflow_step

      # @return [TaskerCore::Models::DependencyResultsWrapper] Results from parent steps
      attr_reader :dependency_results

      # @return [TaskerCore::Models::StepDefinitionWrapper] Step definition from template
      attr_reader :step_definition

      # @return [String] The handler name for this step
      attr_reader :handler_name

      # Creates a StepContext from FFI step data
      #
      # @param step_data [Hash, TaskerCore::Models::TaskSequenceStepWrapper] The step data from Rust FFI
      # @param handler_name [String, nil] Optional handler name override
      def initialize(step_data, handler_name: nil)
        if step_data.is_a?(TaskerCore::Models::TaskSequenceStepWrapper)
          @task = step_data.task
          @workflow_step = step_data.workflow_step
          @dependency_results = step_data.dependency_results
          @step_definition = step_data.step_definition
        else
          wrapper = TaskerCore::Models::TaskSequenceStepWrapper.new(step_data)
          @task = wrapper.task
          @workflow_step = wrapper.workflow_step
          @dependency_results = wrapper.dependency_results
          @step_definition = wrapper.step_definition
        end

        @handler_name = handler_name || @step_definition.handler&.callable
      end

      # ========================================================================
      # CROSS-LANGUAGE STANDARD FIELDS
      # ========================================================================

      # Cross-language standard: task_uuid
      # @return [String] UUID of the task
      def task_uuid
        @task.task_uuid
      end

      # Cross-language standard: step_uuid
      # @return [String] UUID of the workflow step
      def step_uuid
        @workflow_step.workflow_step_uuid
      end

      # Cross-language standard: input_data
      # Returns the step inputs from the workflow step.
      # @return [ActiveSupport::HashWithIndifferentAccess] Step input data
      def input_data
        @workflow_step.inputs
      end

      # Cross-language standard: step_inputs (alias for input_data)
      # @return [ActiveSupport::HashWithIndifferentAccess] Step input data
      alias step_inputs input_data

      # Cross-language standard: step_config
      # Returns the handler configuration from the step definition.
      # @return [ActiveSupport::HashWithIndifferentAccess] Handler configuration from template
      def step_config
        @step_definition.handler&.initialization || {}.with_indifferent_access
      end

      # Cross-language standard: retry_count
      # @return [Integer] Current retry attempt count
      def retry_count
        @workflow_step.attempts || 0
      end

      # Cross-language standard: max_retries
      # @return [Integer] Maximum retry attempts allowed
      def max_retries
        @workflow_step.max_attempts || 3
      end

      # ========================================================================
      # CONVENIENCE METHODS
      # ========================================================================

      # Get a field from the task context.
      #
      # @param field_name [String, Symbol] Field name in task context
      # @return [Object, nil] The field value or nil if not found
      #
      # @example
      #   even_number = context.get_task_field('even_number')
      def get_task_field(field_name)
        @task.context[field_name.to_s]
      end

      # Get a dependency result from a parent step.
      #
      # This returns the actual result value, not the full metadata hash.
      # Use dependency_results.get_result(step_name) for full metadata.
      #
      # @param step_name [String, Symbol] Name of the parent step
      # @return [Object, nil] The result value or nil if not found
      #
      # @example
      #   prev_result = context.get_dependency_result('step_1')
      def get_dependency_result(step_name)
        @dependency_results.get_results(step_name)
      end

      # ========================================================================
      # ADDITIONAL ACCESSORS
      # ========================================================================

      # @return [String, nil] Namespace name from task template
      def namespace_name
        @task.respond_to?(:namespace_name) ? @task.namespace_name : nil
      end

      # @return [String] Step name from workflow step
      def step_name
        @workflow_step.name
      end

      # @return [ActiveSupport::HashWithIndifferentAccess] Full task context
      def context
        @task.context
      end

      # @return [Boolean] Whether the step can be retried
      def retryable?
        @workflow_step.retryable
      end

      # String representation for debugging
      def to_s
        "#<StepContext task_uuid=#{task_uuid} step_uuid=#{step_uuid} step_name=#{step_name}>"
      end

      # Detailed inspection for debugging
      def inspect
        "#<StepContext:#{object_id} " \
          "task_uuid=#{task_uuid.inspect} " \
          "step_uuid=#{step_uuid.inspect} " \
          "step_name=#{step_name.inspect} " \
          "handler_name=#{handler_name.inspect}>"
      end
    end
  end
end
