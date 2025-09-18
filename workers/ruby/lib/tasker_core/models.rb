# frozen_string_literal: true

module TaskerCore
  module Models
    # Wrapper for TaskSequenceStep data from FFI
    class TaskSequenceStepWrapper
      attr_reader :task, :workflow_step, :dependency_results, :step_definition

      def initialize(step_data)
        @task = TaskWrapper.new(step_data[:task])
        @workflow_step = WorkflowStepWrapper.new(step_data[:workflow_step])
        @dependency_results = DependencyResultsWrapper.new(step_data[:dependency_results])
        @step_definition = StepDefinitionWrapper.new(step_data[:step_definition])
      end

      # Utility methods for handler access
      def get_task_field(field_name)
        @task.context[field_name.to_s]
      end

      def get_dependency_result(step_name)
        @dependency_results.get_result(step_name)
      end
    end

    # Task data wrapper
    class TaskWrapper
      attr_reader :task_uuid, :context, :namespace_name

      def initialize(task_data)
        @task_uuid = task_data[:task_uuid]
        @context = task_data[:context]
        @namespace_name = task_data[:namespace_name]
      end
    end

    # Workflow step wrapper
    class WorkflowStepWrapper
      attr_reader :workflow_step_uuid, :name

      def initialize(step_data)
        @workflow_step_uuid = step_data[:workflow_step_uuid]
        @name = step_data[:name]
      end
    end

    # Dependency results wrapper
    class DependencyResultsWrapper
      def initialize(results_data)
        @results = results_data.deep_symbolize_keys
      end

      def get_result(step_name)
        @results[step_name.to_sym]
      end

      def [](step_name)
        get_result(step_name)
      end
    end

    # Step definition wrapper
    class StepDefinitionWrapper
      attr_reader :name, :handler

      def initialize(definition_data)
        @name = definition_data[:name]
        @handler = HandlerWrapper.new(definition_data[:handler])
      end
    end

    # Handler configuration wrapper
    class HandlerWrapper
      attr_reader :callable, :initialization

      def initialize(handler_data)
        @callable = handler_data[:callable]
        @initialization = handler_data[:initialization] || {}
      end
    end
  end
end
