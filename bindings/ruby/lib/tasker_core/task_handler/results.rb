# frozen_string_literal: true

module TaskerCore
  module TaskHandler
    # Result objects for TaskHandler operations (legacy Ruby classes)

    # Result from initialize_task operation
    class InitializeResult
      attr_reader :task_id, :step_count, :step_mapping, :handler_config_name, :workflow_steps

      def initialize(task_id:, step_count:, step_mapping:, handler_config_name: nil, workflow_steps: nil)
        @task_id = task_id
        @step_count = step_count
        @step_mapping = step_mapping
        @handler_config_name = handler_config_name
        @workflow_steps = workflow_steps || []
      end

      def success?
        task_id && task_id > 0
      end

      def to_h
        {
          'task_id' => task_id,
          'step_count' => step_count,
          'step_mapping' => step_mapping,
          'handler_config_name' => handler_config_name
        }
      end

      def workflow_steps
        # Use workflow_steps from Rust if available, otherwise convert step_mapping
        return @workflow_steps if @workflow_steps && !@workflow_steps.empty?

        # Fallback: Convert step_mapping to workflow_steps format for compatibility
        step_mapping&.map do |step_name, step_id|
          {
            'name' => step_name,
            'id' => step_id,
            'depends_on' => [], # This would be filled in by actual step dependencies
            'retryable' => true,
            'retry_limit' => 3
          }
        end || []
      end
    end

    # Result from handle operation
    class HandleResult
      attr_reader :task_id, :status, :completed_steps, :error_message, :steps_published

      def initialize(task_id:, status:, completed_steps:, error_message: nil, steps_published: nil)
        @task_id = task_id
        @status = status
        @completed_steps = completed_steps
        @error_message = error_message
        @steps_published = steps_published
      end

      def success?
        status == 'complete' || status == 'completed'
      end

      def to_h
        {
          'task_id' => task_id,
          'status' => status,
          'completed_steps' => completed_steps,
          'error_message' => error_message,
          'steps_published' => steps_published
        }.compact
      end
    end

    # Result from handle_one_step operation with comprehensive debugging information
    class StepHandleResult
      attr_reader :step_id, :task_id, :step_name, :status, :execution_time_ms,
                  :result_data, :error_message, :retry_count, :handler_class,
                  :dependencies_met, :missing_dependencies, :dependency_results,
                  :step_state_before, :step_state_after, :task_context

      def initialize(step_id:, task_id:, step_name:, status:, execution_time_ms:,
                     result_data: nil, error_message: nil, retry_count: 0, 
                     handler_class:, dependencies_met: true, missing_dependencies: [],
                     dependency_results: {}, step_state_before:, step_state_after:,
                     task_context: {})
        @step_id = step_id
        @task_id = task_id
        @step_name = step_name
        @status = status
        @execution_time_ms = execution_time_ms
        @result_data = result_data
        @error_message = error_message
        @retry_count = retry_count
        @handler_class = handler_class
        @dependencies_met = dependencies_met
        @missing_dependencies = missing_dependencies
        @dependency_results = dependency_results
        @step_state_before = step_state_before
        @step_state_after = step_state_after
        @task_context = task_context
      end

      def success?
        status == 'completed'
      end

      def failed?
        status == 'failed'
      end

      def dependencies_not_met?
        status == 'dependencies_not_met'
      end

      def retry_eligible?
        status == 'retrying'
      end

      def to_h
        {
          'step_id' => step_id,
          'task_id' => task_id,
          'step_name' => step_name,
          'status' => status,
          'execution_time_ms' => execution_time_ms,
          'result_data' => result_data,
          'error_message' => error_message,
          'retry_count' => retry_count,
          'handler_class' => handler_class,
          'dependencies_met' => dependencies_met,
          'missing_dependencies' => missing_dependencies,
          'dependency_results' => dependency_results,
          'step_state_before' => step_state_before,
          'step_state_after' => step_state_after,
          'task_context' => task_context,
          'success' => success?
        }.compact
      end
    end
  end

  # Ruby wrapper classes to convert FFI hashes back to objects with expected methods
  # These provide backward compatibility for step handlers that expect object access

  # Task wrapper that provides method access to hash data
  class TaskWrapper
    attr_reader :task_id, :named_task_id, :complete, :created_at, :updated_at,
                :context, :metadata

    def initialize(task_hash)
      @task_id = task_hash['task_id']
      @named_task_id = task_hash['named_task_id']
      @complete = task_hash['complete']
      @created_at = task_hash['created_at']
      @updated_at = task_hash['updated_at']
      
      # Convert JSON fields back to Ruby objects
      @context = parse_json_field(task_hash['context'])
      @metadata = parse_json_field(task_hash['metadata'])
    end

    private

    def parse_json_field(field)
      return {} if field.nil?
      field.is_a?(Hash) ? field : JSON.parse(field.to_s)
    rescue JSON::ParserError
      {}
    end
  end

  # Step wrapper that provides method access to hash data
  class StepWrapper
    attr_reader :workflow_step_id, :task_id, :named_step_id, :name, :retryable,
                :retry_limit, :in_process, :processed, :processed_at, :attempts,
                :last_attempted_at, :backoff_request_seconds, :skippable,
                :created_at, :updated_at, :inputs

    def initialize(step_hash)
      @workflow_step_id = step_hash['workflow_step_id']
      @task_id = step_hash['task_id']
      @named_step_id = step_hash['named_step_id']
      @name = step_hash['name']
      @retryable = step_hash['retryable']
      @retry_limit = step_hash['retry_limit']
      @in_process = step_hash['in_process']
      @processed = step_hash['processed']
      @processed_at = step_hash['processed_at']
      @attempts = step_hash['attempts']
      @last_attempted_at = step_hash['last_attempted_at']
      @backoff_request_seconds = step_hash['backoff_request_seconds']
      @skippable = step_hash['skippable']
      @created_at = step_hash['created_at']
      @updated_at = step_hash['updated_at']
      
      # Convert JSON fields back to Ruby objects
      @inputs = parse_json_field(step_hash['inputs'])
      @results_data = parse_json_field(step_hash['results'])
    end

    def results
      @results_data || {}
    end

    def results=(new_results)
      @results_data = new_results
    end

    private

    def parse_json_field(field)
      return nil if field.nil?
      field.is_a?(Hash) ? field : JSON.parse(field.to_s)
    rescue JSON::ParserError
      nil
    end
  end

  # Step sequence wrapper that provides method access to hash data
  class StepSequenceWrapper
    attr_reader :total_steps, :current_position, :current_step_id

    def initialize(sequence_hash)
      @total_steps = sequence_hash['total_steps']
      @current_position = sequence_hash['current_position']
      @current_step_id = sequence_hash['current_step_id']
      
      # Convert dependency and step arrays to wrapper objects
      @dependencies = convert_steps_array(sequence_hash['dependencies'] || [])
      @all_steps = convert_steps_array(sequence_hash['all_steps'] || [])
    end

    def dependencies
      @dependencies
    end

    def steps
      @all_steps
    end

    def first_step?
      @current_position == 0
    end

    def last_step?
      @current_position == @total_steps - 1
    end

    def progress
      return 0.0 if @total_steps == 0
      @current_position.to_f / @total_steps.to_f
    end

    def remaining_steps
      [@total_steps - @current_position - 1, 0].max
    end

    def dependencies_count
      @dependencies.length
    end

    private

    def convert_steps_array(steps_array)
      steps_array.map { |step_hash| StepWrapper.new(step_hash) }
    end
  end
end
