# frozen_string_literal: true

module TaskerCore
  module TestHelpers
    # Helper module for step-by-step workflow testing with dependency management
    #
    # This module provides utilities to test individual workflow steps in isolation
    # while respecting their dependency chains. It enables two types of testing:
    #
    # 1. Production-like `handle(task_id)` tests - full integration testing
    # 2. Sequential step tests - execute steps one by one with dependency validation
    #
    # @example Sequential step testing
    #   include TaskerCore::TestHelpers::StepTestHelpers
    #   
    #   steps = get_steps_in_dependency_order(task_id)
    #   steps.each do |step_info|
    #     execute_steps_up_to(task_id, step_info[:name])
    #     result = handler.handle_one_step(step_info[:id])
    #     expect(result.success?).to be(true)
    #   end
    #
    # @example Dependency validation testing
    #   step_id = find_step_by_name(task_id, "process_payment")
    #   
    #   # This should fail - dependencies not met
    #   result = handler.handle_one_step(step_id)
    #   expect(result.dependencies_not_met?).to be(true)
    #   expect(result.missing_dependencies).to include("validate_order")
    #   
    #   # Execute prerequisites and try again
    #   execute_steps_up_to(task_id, "validate_order") 
    #   result = handler.handle_one_step(step_id)
    #   expect(result.success?).to be(true)
    module StepTestHelpers
      # Get all workflow steps for a task in dependency execution order
      #
      # This performs a topological sort of the workflow steps based on their
      # dependency relationships, ensuring that prerequisite steps are listed
      # before steps that depend on them.
      #
      # @param task_id [Integer] The task ID to get steps for
      # @return [Array<Hash>] Array of step info hashes with keys:
      #   - :id [Integer] - workflow_step_id
      #   - :name [String] - step name  
      #   - :dependencies [Array<String>] - names of prerequisite steps
      #   - :state [String] - current step state
      #   - :retryable [Boolean] - whether step can be retried
      #
      # @example
      #   steps = get_steps_in_dependency_order(task_id)
      #   # => [
      #   #   {id: 101, name: "validate_order", dependencies: [], state: "pending"},
      #   #   {id: 102, name: "reserve_inventory", dependencies: ["validate_order"], state: "pending"},
      #   #   {id: 103, name: "process_payment", dependencies: ["validate_order", "reserve_inventory"], state: "pending"}
      #   # ]
      def get_steps_in_dependency_order(task_id)
        # Query database for workflow steps and their dependencies
        orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
        steps_data = orchestration_manager.get_workflow_steps_for_task(task_id)
        
        return [] unless steps_data && steps_data['steps']
        
        steps_with_deps = steps_data['steps'].map do |step|
          {
            id: step['workflow_step_id'],
            name: step['name'],
            dependencies: step['dependencies'] || [],
            state: step['current_state'] || 'pending',
            retryable: step['retryable'] || false,
            retry_limit: step['retry_limit'] || 0
          }
        end
        
        # Perform topological sort to get dependency order
        topological_sort(steps_with_deps)
      rescue StandardError => e
        TaskerCore::Logging::Logger.instance.error("Failed to get steps in dependency order: #{e.message}")
        []
      end

      # Execute all prerequisite steps up to (but not including) the target step
      #
      # This method ensures that all dependency steps for the target step are
      # completed before the target step is executed. It's useful for setting up
      # the proper state for isolated step testing.
      #
      # @param task_id [Integer] The task ID
      # @param target_step_name [String] Name of the step to prepare dependencies for
      # @param handler [TaskerCore::TaskHandler::Base] Optional handler instance (will find if not provided)
      # @return [Hash] Execution summary with keys:
      #   - :executed_steps [Array<String>] - names of steps that were executed
      #   - :already_completed [Array<String>] - steps that were already completed
      #   - :failed_steps [Array<String>] - steps that failed execution
      #   - :target_ready [Boolean] - whether target step is ready for execution
      #
      # @example
      #   result = execute_steps_up_to(task_id, "process_payment")
      #   puts "Executed: #{result[:executed_steps].join(', ')}"
      #   puts "Target ready: #{result[:target_ready]}"
      def execute_steps_up_to(task_id, target_step_name, handler = nil)
        steps = get_steps_in_dependency_order(task_id)
        target_step = steps.find { |s| s[:name] == target_step_name }
        
        unless target_step
          return {
            executed_steps: [],
            already_completed: [],
            failed_steps: [],
            target_ready: false,
            error: "Target step '#{target_step_name}' not found"
          }
        end
        
        # Find all steps that need to be completed before target step
        prerequisite_steps = find_prerequisites_for_step(steps, target_step_name)
        
        # Get handler if not provided
        handler ||= find_handler_for_task(task_id)
        unless handler
          return {
            executed_steps: [],
            already_completed: [],
            failed_steps: [],
            target_ready: false,
            error: "No handler found for task #{task_id}"
          }
        end
        
        executed_steps = []
        already_completed = []
        failed_steps = []
        
        # Execute each prerequisite step
        prerequisite_steps.each do |step|
          if step[:state] == 'completed'
            already_completed << step[:name]
            next
          end
          
          begin
            result = handler.handle_one_step(step[:id])
            if result.success?
              executed_steps << step[:name]
              # Update step state in our local tracking
              step[:state] = 'completed'
            else
              failed_steps << step[:name]
              TaskerCore::Logging::Logger.instance.warn(
                "Step execution failed: #{step[:name]} - #{result.error_message}"
              )
            end
          rescue StandardError => e
            failed_steps << step[:name]
            TaskerCore::Logging::Logger.instance.error(
              "Error executing step #{step[:name]}: #{e.message}"
            )
          end
        end
        
        {
          executed_steps: executed_steps,
          already_completed: already_completed,
          failed_steps: failed_steps,
          target_ready: failed_steps.empty?
        }
      end

      # Check if a specific step is ready for execution (all dependencies completed)
      #
      # @param step_id [Integer] The workflow_step_id to check
      # @return [Hash] Status hash with keys:
      #   - :ready [Boolean] - whether step can be executed now
      #   - :missing_dependencies [Array<String>] - names of incomplete prerequisite steps
      #   - :completed_dependencies [Array<String>] - names of completed prerequisite steps
      #   - :step_state [String] - current state of the step itself
      #
      # @example
      #   status = step_ready_for_execution?(step_id)
      #   if status[:ready]
      #     result = handler.handle_one_step(step_id)
      #   else
      #     puts "Missing: #{status[:missing_dependencies].join(', ')}"
      #   end
      def step_ready_for_execution?(step_id)
        begin
          orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
          dependencies_info = orchestration_manager.get_step_dependencies(step_id)
          
          unless dependencies_info
            return {
              ready: false,
              missing_dependencies: [],
              completed_dependencies: [],
              step_state: 'unknown',
              error: "Could not load step dependencies"
            }
          end
          
          missing_deps = []
          completed_deps = []
          
          dependencies_info['dependencies']&.each do |dep|
            if dep['current_state'] == 'completed'
              completed_deps << dep['name']
            else
              missing_deps << dep['name']
            end
          end
          
          {
            ready: missing_deps.empty?,
            missing_dependencies: missing_deps,
            completed_dependencies: completed_deps,
            step_state: dependencies_info['step_state'] || 'unknown'
          }
        rescue StandardError => e
          TaskerCore::Logging::Logger.instance.error("Error checking step readiness: #{e.message}")
          {
            ready: false,
            missing_dependencies: [],
            completed_dependencies: [],
            step_state: 'unknown',
            error: e.message
          }
        end
      end

      # Find a workflow step by name within a task
      #
      # @param task_id [Integer] The task ID
      # @param step_name [String] The name of the step to find
      # @return [Hash, nil] Step info hash or nil if not found
      def find_step_by_name(task_id, step_name)
        steps = get_steps_in_dependency_order(task_id)
        steps.find { |step| step[:name] == step_name }
      end

      # Get a summary of all steps and their current states
      #
      # @param task_id [Integer] The task ID
      # @return [Hash] Summary with keys:
      #   - :total_steps [Integer] - total number of steps
      #   - :completed_steps [Integer] - number of completed steps
      #   - :pending_steps [Integer] - number of pending steps
      #   - :failed_steps [Integer] - number of failed steps
      #   - :ready_steps [Array<String>] - names of steps ready for execution
      #   - :blocked_steps [Array<String>] - names of steps blocked by dependencies
      def get_task_step_summary(task_id)
        steps = get_steps_in_dependency_order(task_id)
        
        completed = steps.select { |s| s[:state] == 'completed' }
        pending = steps.select { |s| s[:state] == 'pending' }
        failed = steps.select { |s| s[:state] == 'failed' }
        
        ready_steps = []
        blocked_steps = []
        
        pending.each do |step|
          status = step_ready_for_execution?(step[:id])
          if status[:ready]
            ready_steps << step[:name]
          else
            blocked_steps << step[:name]
          end
        end
        
        {
          total_steps: steps.length,
          completed_steps: completed.length,
          pending_steps: pending.length,
          failed_steps: failed.length,
          ready_steps: ready_steps,
          blocked_steps: blocked_steps
        }
      end

      private

      # Perform topological sort on steps based on dependencies
      def topological_sort(steps)
        # Build adjacency list and in-degree count
        adj_list = {}
        in_degree = {}
        step_map = {}
        
        steps.each do |step|
          name = step[:name]
          adj_list[name] = []
          in_degree[name] = 0
          step_map[name] = step
        end
        
        # Build edges and count incoming dependencies
        steps.each do |step|
          step[:dependencies].each do |dep_name|
            if adj_list[dep_name] # Only if dependency exists in this task
              adj_list[dep_name] << step[:name]
              in_degree[step[:name]] += 1
            end
          end
        end
        
        # Kahn's algorithm for topological sort
        queue = []
        result = []
        
        # Start with steps that have no dependencies
        in_degree.each do |name, degree|
          queue << name if degree == 0
        end
        
        while !queue.empty?
          current = queue.shift
          result << step_map[current]
          
          # Process all steps that depend on current step
          adj_list[current].each do |dependent|
            in_degree[dependent] -= 1
            queue << dependent if in_degree[dependent] == 0
          end
        end
        
        # Check for circular dependencies
        if result.length != steps.length
          TaskerCore::Logging::Logger.instance.warn("Circular dependency detected in workflow steps")
          # Fall back to original order
          return steps
        end
        
        result
      end

      # Find all prerequisite steps that must be completed before target step
      def find_prerequisites_for_step(steps, target_step_name)
        target_step = steps.find { |s| s[:name] == target_step_name }
        return [] unless target_step
        
        prerequisites = []
        visited = Set.new
        
        # Recursive function to collect all transitive dependencies
        collect_dependencies = lambda do |step_name|
          return if visited.include?(step_name)
          visited.add(step_name)
          
          step = steps.find { |s| s[:name] == step_name }
          return unless step
          
          step[:dependencies].each do |dep_name|
            collect_dependencies.call(dep_name)
            dep_step = steps.find { |s| s[:name] == dep_name }
            prerequisites << dep_step if dep_step && !prerequisites.include?(dep_step)
          end
        end
        
        # Collect all dependencies for target step
        target_step[:dependencies].each do |dep_name|
          collect_dependencies.call(dep_name)
        end
        
        # Sort prerequisites by dependency order
        topological_sort(prerequisites)
      end

      # Find the task handler instance for a given task
      def find_handler_for_task(task_id)
        orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
        task_info = orchestration_manager.get_task_metadata(task_id)
        
        return nil unless task_info
        
        # Find and initialize handler
        handler_result = TaskerCore::Registry.find_handler_and_initialize(
          name: "#{task_info['namespace']}/#{task_info['name']}",
          version: task_info['version']
        )
        
        handler_result['handler_instance']
      rescue StandardError => e
        TaskerCore::Logging::Logger.instance.error("Failed to find handler for task #{task_id}: #{e.message}")
        nil
      end
    end
  end
end