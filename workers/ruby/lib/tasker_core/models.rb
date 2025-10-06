# frozen_string_literal: true

module TaskerCore
  module Models
    # Wrapper for TaskSequenceStep data received from Rust FFI.
    #
    # This class provides a Ruby-friendly interface to the complete step execution context,
    # including task metadata, workflow step state, dependency results, and step definition.
    # The data structure is provided by the Rust orchestration system via FFI when a step
    # is ready for execution.
    #
    # @example Accessing task context in a handler
    #   def call(task, sequence, step)
    #     even_number = task.context['even_number']
    #     # or use the convenience method
    #     even_number = sequence.get_task_field('even_number')
    #   end
    #
    # @example Accessing dependency results
    #   def call(task, sequence, step)
    #     previous_result = sequence.get_dependency_result('previous_step_name')
    #   end
    class TaskSequenceStepWrapper
      # @return [TaskWrapper] Wrapped task with context and metadata
      # @return [WorkflowStepWrapper] Wrapped workflow step with execution state
      # @return [DependencyResultsWrapper] Results from parent steps
      # @return [StepDefinitionWrapper] Step definition from task template
      attr_reader :task, :workflow_step, :dependency_results, :step_definition

      # Creates a new TaskSequenceStepWrapper from FFI data
      #
      # @param step_data [Hash] The complete step execution data from Rust
      # @option step_data [Hash] :task Task and namespace metadata
      # @option step_data [Hash] :workflow_step Step execution state and results
      # @option step_data [Hash] :dependency_results Results from parent steps
      # @option step_data [Hash] :step_definition Step configuration from template
      def initialize(step_data)
        @task = TaskWrapper.new(step_data[:task])
        @workflow_step = WorkflowStepWrapper.new(step_data[:workflow_step])
        @dependency_results = DependencyResultsWrapper.new(step_data[:dependency_results])
        @step_definition = StepDefinitionWrapper.new(step_data[:step_definition])
      end

      # Convenience method to access task context fields
      #
      # @param field_name [String, Symbol] The field name in task context
      # @return [Object, nil] The field value or nil if not found
      #
      # @example
      #   sequence.get_task_field('even_number') # => 2
      def get_task_field(field_name)
        @task.context[field_name.to_s]
      end

      # Convenience method to access dependency results
      #
      # @param step_name [String, Symbol] The name of the parent step
      # @return [Object, nil] The result from the parent step or nil
      #
      # @example
      #   result = sequence.get_dependency_result('linear_step_1') # => 36
      def get_dependency_result(step_name)
        @dependency_results.get_result(step_name)
      end

      # Convenience method to access dependency results (alias for compatibility)
      #
      # @param step_name [String, Symbol] The name of the parent step
      # @return [Object, nil] The result from the parent step or nil
      #
      # @example
      #   result = sequence.get_results('linear_step_1') # => 36
      def get_results(step_name)
        @dependency_results.get_results(step_name)
      end
    end

    # Wrapper for task metadata and context.
    #
    # Handles the nested task structure from Rust FFI where the actual task data
    # is nested under a :task key within the parent task hash.
    #
    # @example Accessing task properties
    #   task.task_uuid        # => "0199a46a-11a8-7d53-83da-0b13513dab49"
    #   task.context          # => { "even_number" => 2 }
    #   task.namespace_name   # => "linear_workflow"
    class TaskWrapper
      # @return [String] UUID v7 of the task instance
      # @return [ActiveSupport::HashWithIndifferentAccess] Task context with input data and accumulated state
      # @return [String] Namespace name from task template
      # @return [String] Task template name
      # @return [String] Task template version
      attr_reader :task_uuid, :context, :namespace_name, :task_name, :task_version

      # Creates a new TaskWrapper from FFI data
      #
      # @param task_data [Hash] Task data with nested structure from Rust
      # @option task_data [Hash] :task Nested task object with context
      # @option task_data [String] :task_name Template name
      # @option task_data [String] :namespace_name Namespace from template
      # @option task_data [String] :task_version Template version
      def initialize(task_data)
        # Handle nested task structure from Rust FFI
        # Data comes as: { task: { task_uuid: ..., context: ... }, task_name: "...", namespace_name: "..." }
        # The actual task fields are nested under :task key
        inner_task = task_data[:task] || task_data

        @task_uuid = inner_task[:task_uuid]
        # Use HashWithIndifferentAccess for developer-friendly context access
        @context = (inner_task[:context] || {}).with_indifferent_access
        @namespace_name = task_data[:namespace_name] || inner_task[:namespace_name]
        @task_name = task_data[:task_name] || inner_task[:task_name]
        @task_version = task_data[:task_version] || inner_task[:task_version]
      end
    end

    # Wrapper for workflow step execution state and metadata.
    #
    # Provides access to step execution tracking, retry configuration, and results.
    #
    # @example Checking step state
    #   step.name              # => "linear_step_1"
    #   step.attempts          # => 1
    #   step.max_attempts       # => 3
    #   step.in_process        # => false
    class WorkflowStepWrapper
      # @return [String] UUID v7 of the workflow step instance
      # @return [String] UUID v7 of the task this step belongs to
      # @return [String] UUID v7 of the named step definition
      # @return [String] Step name from template
      # @return [Boolean] Whether step can be retried
      # @return [Integer] Maximum retry attempts
      # @return [Boolean] Whether step is currently being processed
      # @return [Boolean] Whether step has been processed
      # @return [Time, nil] When step was last processed
      # @return [Integer] Number of execution attempts
      # @return [Time, nil] When step was last attempted
      # @return [Integer] Backoff delay in seconds for retry
      # @return [ActiveSupport::HashWithIndifferentAccess] Step inputs from template
      # @return [ActiveSupport::HashWithIndifferentAccess] Step execution results
      # @return [Boolean] Whether step can be skipped
      # @return [Time] When step was created
      # @return [Time] When step was last updated
      attr_reader :workflow_step_uuid, :task_uuid, :named_step_uuid, :name,
                  :retryable, :max_attempts, :in_process, :processed, :processed_at,
                  :attempts, :last_attempted_at, :backoff_request_seconds,
                  :inputs, :results, :skippable, :created_at, :updated_at

      # Creates a new WorkflowStepWrapper from FFI data
      #
      # @param step_data [Hash] Workflow step data from Rust
      def initialize(step_data)
        @workflow_step_uuid = step_data[:workflow_step_uuid]
        @task_uuid = step_data[:task_uuid]
        @named_step_uuid = step_data[:named_step_uuid]
        @name = step_data[:name]
        @retryable = step_data[:retryable]
        @max_attempts = step_data[:max_attempts]
        @in_process = step_data[:in_process]
        @processed = step_data[:processed]
        @processed_at = step_data[:processed_at]
        @attempts = step_data[:attempts]
        @last_attempted_at = step_data[:last_attempted_at]
        @backoff_request_seconds = step_data[:backoff_request_seconds]
        # Use HashWithIndifferentAccess for nested hashes
        @inputs = (step_data[:inputs] || {}).with_indifferent_access
        @results = (step_data[:results] || {}).with_indifferent_access
        @skippable = step_data[:skippable]
        @created_at = step_data[:created_at]
        @updated_at = step_data[:updated_at]
      end
    end

    # Wrapper for dependency results from parent steps.
    #
    # Provides access to results from steps that this step depends on.
    # Results are keyed by step name.
    #
    # @note Two methods for accessing results:
    #   - `get_result(name)` returns the full result hash with metadata
    #   - `get_results(name)` returns just the computed value (recommended for handlers)
    #
    # @example Accessing dependency results
    #   deps.get_results('previous_step')  # => 36 (just the value)
    #   deps.get_result('previous_step')   # => { result: 36, metadata: {...} }
    #   deps['previous_step']              # => { result: 36, metadata: {...} }
    class DependencyResultsWrapper
      # Creates a new DependencyResultsWrapper
      #
      # @param results_data [Hash] Hash of step names to their results
      def initialize(results_data)
        # Use HashWithIndifferentAccess for developer-friendly key access
        @results = (results_data || {}).with_indifferent_access
      end

      # Get result from a parent step (returns full result hash)
      #
      # @param step_name [String, Symbol] Name of the parent step
      # @return [Hash, nil] The full result hash or nil if not found
      def get_result(step_name)
        @results[step_name] # HashWithIndifferentAccess handles string/symbol automatically
      end

      # Get the actual result value from a parent step (extracts 'result' field)
      #
      # This is the typical method handlers use to get the actual computed value
      # from a parent step, rather than the full result metadata hash.
      #
      # @param step_name [String, Symbol] Name of the parent step
      # @return [Object, nil] The result value or nil if not found
      #
      # @example
      #   deps.get_results('linear_step_1')  # => 36 (the actual value)
      #   deps.get_result('linear_step_1')   # => { result: 36, metadata: {...} }
      def get_results(step_name)
        result_hash = @results[step_name]
        return nil unless result_hash

        # If it's a hash with a 'result' key, extract that value
        # Otherwise return the whole thing (might be a primitive value)
        if result_hash.is_a?(Hash) && result_hash.key?('result')
          result_hash['result']
        else
          result_hash
        end
      end

      # Array-style access to dependency results
      #
      # @param step_name [String, Symbol] Name of the parent step
      # @return [Hash, nil] The full result hash or nil if not found
      def [](step_name)
        get_result(step_name)
      end

      # Get all dependency result step names
      #
      # @return [Array<String>] List of step names that have results
      def keys
        @results.keys
      end

      # Check if a dependency result exists
      #
      # @param step_name [String, Symbol] Name of the parent step
      # @return [Boolean] True if result exists
      def key?(step_name)
        @results.key?(step_name) # HashWithIndifferentAccess handles string/symbol automatically
      end
    end

    # Wrapper for step definition from task template.
    #
    # Provides access to step configuration including handler specification,
    # dependencies, retry policy, and timeout settings.
    #
    # @example Accessing step definition
    #   step_def.name                    # => "linear_step_1"
    #   step_def.description             # => "Square the initial even number..."
    #   step_def.handler.callable        # => "LinearWorkflow::StepHandlers::LinearStep1Handler"
    #   step_def.dependencies            # => ["previous_step"]
    #   step_def.timeout_seconds         # => 30
    class StepDefinitionWrapper
      # @return [String] Step name
      # @return [String] Human-readable description
      # @return [HandlerWrapper] Handler configuration
      # @return [String, nil] System dependency name
      # @return [Array<String>] Names of parent steps this depends on
      # @return [ActiveSupport::HashWithIndifferentAccess] Retry configuration
      # @return [Integer] Timeout in seconds
      # @return [Array<String>] Events this step publishes
      attr_reader :name, :description, :handler, :system_dependency,
                  :dependencies, :retry, :timeout_seconds, :publishes_events

      # Creates a new StepDefinitionWrapper from template data
      #
      # @param definition_data [Hash] Step definition from task template
      def initialize(definition_data)
        @name = definition_data[:name]
        @description = definition_data[:description]
        @handler = HandlerWrapper.new(definition_data[:handler])
        @system_dependency = definition_data[:system_dependency]
        @dependencies = definition_data[:dependencies] || []
        # Use HashWithIndifferentAccess for retry configuration
        @retry = (definition_data[:retry] || {}).with_indifferent_access
        @timeout_seconds = definition_data[:timeout_seconds]
        @publishes_events = definition_data[:publishes_events] || []
      end
    end

    # Wrapper for handler configuration.
    #
    # Provides access to handler class name and initialization parameters
    # from the task template.
    #
    # @example Accessing handler configuration
    #   handler.callable       # => "LinearWorkflow::StepHandlers::LinearStep1Handler"
    #   handler.initialization # => { operation: "square", step_number: 1 }
    class HandlerWrapper
      # @return [String] Fully-qualified handler class name
      # @return [ActiveSupport::HashWithIndifferentAccess] Initialization parameters for the handler
      attr_reader :callable, :initialization

      # Creates a new HandlerWrapper from template data
      #
      # @param handler_data [Hash] Handler configuration from template
      # @option handler_data [String] :callable Handler class name
      # @option handler_data [Hash] :initialization Handler init params
      def initialize(handler_data)
        @callable = handler_data[:callable]
        # Use HashWithIndifferentAccess for initialization parameters
        @initialization = (handler_data[:initialization] || {}).with_indifferent_access
      end
    end
  end
end
