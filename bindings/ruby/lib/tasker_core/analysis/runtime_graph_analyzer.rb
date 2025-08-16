# frozen_string_literal: true

module TaskerCore
  module Analysis
    # Runtime Graph Analyzer for Workflow Dependencies
    #
    # Provides comprehensive analysis of workflow step dependencies, execution flow,
    # and performance bottlenecks using database-backed graph analysis. Leverages
    # existing SQL functions for optimal performance and consistency.
    #
    # @example Basic usage
    #   analyzer = RuntimeGraphAnalyzer.new(task: my_task)
    #   analysis = analyzer.analyze
    #   puts analysis[:critical_paths][:longest_path_length]
    #
    # @example Analyzing bottlenecks
    #   bottlenecks = analyzer.analyze[:bottlenecks]
    #   critical_bottlenecks = bottlenecks[:bottlenecks].select { |b| b[:severity] == 'Critical' }
    #
    # @since 2.2.1
    class RuntimeGraphAnalyzer
      # @return [Tasker::Task] The task being analyzed
      attr_reader :task

      # @return [Integer] The task ID for database queries
      attr_reader :task_uuid

      # Initialize the analyzer for a specific task
      #
      # @param task [Tasker::Task] The task to analyze
      # @raise [ArgumentError] if task is nil or invalid
      def initialize(task:)
        @task = task
        @task_uuid = task.task_uuid
        @cache = {}
      end

      # Perform comprehensive workflow analysis
      #
      # Returns a complete analysis of the workflow including dependency graphs,
      # critical paths, parallelism opportunities, error chains, and bottlenecks.
      # Results are cached using IntelligentCacheManager with adaptive TTL.
      #
      # @return [Hash] Complete analysis with the following keys:
      #   - :dependency_graph [Hash] Graph structure with nodes, edges, and adjacency lists
      #   - :critical_paths [Hash] Critical path analysis with longest paths and bottlenecks
      #   - :parallelism_opportunities [Hash] Analysis of parallel execution opportunities
      #   - :error_chains [Hash] Error propagation analysis and recovery strategies
      #   - :bottlenecks [Hash] Bottleneck identification with impact scoring
      #
      # @example
      #   analysis = analyzer.analyze
      #   puts "Longest path: #{analysis[:critical_paths][:longest_path_length]} steps"
      #   puts "Critical bottlenecks: #{analysis[:bottlenecks][:critical_bottlenecks]}"
      def analyze
        {
          dependency_graph: build_dependency_graph,
          critical_paths: analyze_critical_paths,
          parallelism_opportunities: analyze_parallelism,
          error_chains: analyze_error_chains,
          bottlenecks: identify_bottlenecks,
          generated_at: Time.now,
          task_uuid: task_uuid
        }
      end

      # Build complete dependency graph structure
      #
      # Constructs a comprehensive graph representation of workflow step dependencies
      # using database edges and SQL-based topological sorting for optimal performance.
      #
      # @return [Hash] Graph structure containing:
      #   - :nodes [Array<Hash>] Graph nodes with id, name, and dependency level
      #   - :edges [Array<Hash>] Graph edges with from/to IDs and step names
      #   - :adjacency_list [Hash] Forward adjacency list for graph traversal
      #   - :reverse_adjacency_list [Hash] Reverse adjacency list for dependency analysis
      #   - :dependency_levels [Hash] Step ID to dependency level mapping
      #
      # @example
      #   graph = analyzer.build_dependency_graph
      #   root_steps = graph[:nodes].select { |n| n[:level] == 0 }
      #   puts "Root steps: #{root_steps.map { |s| s[:name] }}"
      def build_dependency_graph
        steps = load_workflow_steps
        step_map = steps.index_by(&:workflow_step_uuid)
        edges = load_workflow_edges
        adjacency_lists = build_adjacency_lists(steps, edges)
        dependency_levels = calculate_dependency_levels_sql

        {
          nodes: build_graph_nodes(steps, dependency_levels),
          edges: build_graph_edges(edges, step_map),
          adjacency_list: adjacency_lists[:forward],
          reverse_adjacency_list: adjacency_lists[:reverse],
          dependency_levels: dependency_levels
        }
      end

      private

      # Get configuration for dependency graph calculations
      #
      # @return [Tasker::Types::DependencyGraphConfig] Configuration for calculations
      # @api private
      def dependency_graph_config
        @dependency_graph_config ||= TaskerCore::Config.instance.dependency_graph_config
      end

      # Generate cache version based on task state for intelligent cache invalidation
      #
      # @return [String] Cache version string that changes when task state changes
      # @api private
      def task_analysis_cache_version
        # Include task updated_at and step count to invalidate when task changes
        step_count = task.workflow_steps.count
        task_updated = task.updated_at.to_i
        "v1:#{task_updated}:#{step_count}"
      end

      # Load workflow steps with named step associations
      #
      # @return [ActiveRecord::Relation] Workflow steps with eager-loaded named steps
      # @api private
      def load_workflow_steps
        task.workflow_steps.includes(:named_step)
      end

      # Load workflow edges for dependency analysis
      #
      # @return [ActiveRecord::Relation] Workflow step edges for this task
      # @api private
      def load_workflow_edges
        TaskerCore::Database::Models::WorkflowStepEdge.joins(:from_step, :to_step)
                                                      .where(from_step: { task_uuid: task_uuid })
                                                      .select(:from_step_uuid, :to_step_uuid)
      end

      # Build forward and reverse adjacency lists for graph traversal
      #
      # Creates both forward (parent → children) and reverse (child → parents)
      # adjacency lists for efficient graph analysis in both directions.
      #
      # @param steps [Array] Workflow steps to initialize
      # @param edges [Array] Workflow step edges to populate
      # @return [Hash] Hash with :forward and :reverse adjacency lists
      # @api private
      def build_adjacency_lists(steps, edges)
        adjacency_list = {}
        reverse_adjacency_list = {}

        # Initialize empty lists for all steps
        steps.each do |step|
          adjacency_list[step.workflow_step_uuid] = []
          reverse_adjacency_list[step.workflow_step_uuid] = []
        end

        # Populate adjacency lists from edges
        edges.each do |edge|
          adjacency_list[edge.from_step_uuid] << edge.to_step_uuid
          reverse_adjacency_list[edge.to_step_uuid] << edge.from_step_uuid
        end

        { forward: adjacency_list, reverse: reverse_adjacency_list }
      end

      # Build graph nodes with dependency level information
      #
      # @param steps [Array] Workflow steps to convert to nodes
      # @param dependency_levels [Hash] Step ID to dependency level mapping
      # @return [Array<Hash>] Graph nodes with id, name, and level
      # @api private
      def build_graph_nodes(steps, dependency_levels)
        steps.map do |step|
          {
            id: step.workflow_step_uuid,
            name: step.named_step.name,
            level: dependency_levels[step.workflow_step_uuid] || 0
          }
        end
      end

      # Build graph edges with human-readable step names
      #
      # @param edges [Array] Raw workflow step edges
      # @param step_map [Hash] Step ID to step object mapping
      # @return [Array<Hash>] Graph edges with from/to IDs and names
      # @api private
      def build_graph_edges(edges, step_map)
        edges.map do |edge|
          {
            from: edge.from_step_uuid,
            to: edge.to_step_uuid,
            from_name: step_map[edge.from_step_uuid]&.named_step&.name,
            to_name: step_map[edge.to_step_uuid]&.named_step&.name
          }
        end
      end

      # Calculate dependency levels using optimized SQL function
      #
      # @return [Hash] Step ID to dependency level mapping
      # @api private
      def calculate_dependency_levels_sql
        TaskerCore::Database::Functions::FunctionBasedDependencyLevels.levels_hash_for_task(task_uuid)
      end

      # Analyze critical paths through the dependency graph
      def analyze_critical_paths
        graph = build_dependency_graph
        root_nodes = find_root_nodes(graph)
        leaf_nodes = find_leaf_nodes(graph)
        all_paths = find_all_root_to_leaf_paths(root_nodes, leaf_nodes, graph[:adjacency_list])
        critical_paths = analyze_all_paths_for_criticality(all_paths, graph)

        {
          total_paths: all_paths.length,
          longest_path_length: critical_paths.first&.dig(:length) || 0,
          paths: critical_paths.first(5), # Top 5 critical paths
          root_nodes: root_nodes,
          leaf_nodes: leaf_nodes
        }
      end

      # Find root nodes (nodes with no incoming edges)
      def find_root_nodes(graph)
        graph[:adjacency_list].select do |node, _children|
          graph[:reverse_adjacency_list][node].empty?
        end.keys
      end

      # Find leaf nodes (nodes with no outgoing edges)
      def find_leaf_nodes(graph)
        graph[:adjacency_list].select do |_node, children|
          children.empty?
        end.keys
      end

      # Find all paths from root nodes to leaf nodes
      def find_all_root_to_leaf_paths(root_nodes, leaf_nodes, adjacency_list)
        all_paths = []
        root_nodes.each do |root|
          leaf_nodes.each do |leaf|
            paths = find_all_paths(root, leaf, adjacency_list)
            all_paths.concat(paths)
          end
        end
        all_paths
      end

      # Analyze all paths for criticality and sort by importance
      def analyze_all_paths_for_criticality(all_paths, graph)
        all_paths.map do |path|
          analyze_path_criticality(path, graph)
        end.sort_by { |analysis| -analysis[:criticality_score] }
      end

      # Find all paths between two nodes using DFS
      def find_all_paths(start_node, end_node, adjacency_list, current_path = [], visited = Set.new)
        return [] if visited.include?(start_node)

        current_path += [start_node]
        visited += [start_node]

        return [current_path] if start_node == end_node

        paths = []
        adjacency_list[start_node].each do |neighbor|
          neighbor_paths = find_all_paths(neighbor, end_node, adjacency_list, current_path, visited)
          paths.concat(neighbor_paths)
        end

        paths
      end

      # Analyze a specific path for criticality factors
      def analyze_path_criticality(path, _graph)
        # Get step readiness data for analysis
        step_readiness_data = get_step_readiness_data
        task_context = get_task_execution_context
        path_steps = path.filter_map { |step_uuid| step_readiness_data[step_uuid] }

        # Use efficient counting methods for path-specific metrics
        path_metrics = calculate_path_metrics(path_steps)

        # Calculate estimated duration and bottlenecks
        estimated_duration = calculate_path_duration(path_steps)
        bottlenecks = identify_path_bottlenecks(path_steps)

        # Calculate overall criticality score using task context for efficiency
        criticality_score = calculate_path_criticality_score(
          path.length, path_metrics, estimated_duration, task_context
        )

        {
          path: path,
          length: path.length,
          step_names: path_steps.map(&:name),
          **path_metrics,
          estimated_duration: estimated_duration,
          bottlenecks: bottlenecks,
          criticality_score: criticality_score,
          completion_percentage: (path_metrics[:completed_steps].to_f / path.length * 100).round(1)
        }
      end

      # Calculate metrics for a specific path efficiently
      def calculate_path_metrics(path_steps)
        # Use single pass through path_steps for all counts
        metrics = {
          completed_steps: 0,
          blocked_steps: 0,
          error_steps: 0,
          retry_steps: 0,
          ready_steps: 0
        }

        path_steps.each do |step|
          metrics[:completed_steps] += 1 if step.current_state.in?(%w[complete resolved_manually])
          metrics[:blocked_steps] += 1 unless step.dependencies_satisfied
          metrics[:error_steps] += 1 if step.current_state == 'error'
          metrics[:retry_steps] += 1 if step.attempts.positive? && step.current_state == 'error'
          metrics[:ready_steps] += 1 if step.ready_for_execution
        end

        metrics
      end

      # Calculate criticality score using task context for efficiency
      # Calculate path criticality score using configurable multipliers
      #
      # Uses configurable impact multipliers for flexible path scoring.
      #
      # @param path_length [Integer] Length of the path
      # @param path_metrics [Hash] Metrics about the path steps
      # @param duration [Integer] Estimated path duration
      # @param task_context [Object] Task execution context
      # @return [Float] Path criticality score using configurable weights
      def calculate_path_criticality_score(path_length, path_metrics, duration, task_context)
        config = dependency_graph_config

        # Base score from path length relative to total task size
        score = path_length * config.impact_multipliers[:path_length_weight]

        # Factor in task-wide context for better scoring
        task_completion_factor = task_context.completion_percentage / 100.0
        task_health_factor = task_context.health_status == 'healthy' ? 1.0 : 1.5

        # Reduce score for completed work
        score -= path_metrics[:completed_steps] * config.impact_multipliers[:completed_penalty]

        # Increase score for problems (weighted by task health)
        score += path_metrics[:blocked_steps] * config.impact_multipliers[:blocked_penalty] * task_health_factor
        score += path_metrics[:error_steps] * config.impact_multipliers[:error_penalty] * task_health_factor
        score += path_metrics[:retry_steps] * config.impact_multipliers[:retry_penalty]

        # Factor in estimated duration and task completion
        score += (duration / 60.0) * 2 # 2 points per minute
        score *= (1.0 - (task_completion_factor * 0.3)) # Reduce score for mostly complete tasks

        [score, 0].max # Ensure non-negative score
      end

      # Get step readiness data with memoization
      #
      # Retrieves and caches step readiness status for all steps in the task.
      # Uses SQL-based functions for optimal performance and consistency.
      #
      # @return [Hash] Step ID to step readiness status object mapping
      # @api private
      def get_step_readiness_data
        @get_step_readiness_data ||= begin
          data = TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.for_task(task_uuid)
          data.index_by(&:workflow_step_uuid)
        end
      end

      # Get task execution context with memoization
      #
      # Retrieves and caches task-wide execution metrics including completion
      # percentages, step counts, and health status.
      #
      # @return [Object] Task execution context with metrics and status
      # @api private
      def get_task_execution_context
        @get_task_execution_context ||= TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task_uuid)
      end

      # Calculate estimated duration for a path using configurable estimates
      #
      # Uses configurable duration constants for flexible time estimation.
      #
      # @param path_steps [Array] Steps in the path
      # @return [Integer] Estimated duration in seconds using configurable estimates
      def calculate_path_duration(path_steps)
        config = dependency_graph_config

        # Base estimation: configurable seconds per step
        base_duration = path_steps.length * config.duration_estimates[:base_step_seconds]

        # Calculate penalties in a single pass
        error_penalty = 0
        retry_penalty = 0

        path_steps.each do |step|
          error_penalty += config.duration_estimates[:error_penalty_seconds] if step.current_state == 'error'
          retry_penalty += step.attempts * config.duration_estimates[:retry_penalty_seconds]
        end

        base_duration + error_penalty + retry_penalty
      end

      # Identify bottlenecks within a specific path
      def identify_path_bottlenecks(path_steps)
        bottlenecks = []

        path_steps.each do |step|
          # A step is a bottleneck if it's blocking and has high retry count or long backoff
          next unless !step.dependencies_satisfied || step.current_state == 'error'

          severity = calculate_bottleneck_severity(step)
          bottlenecks << {
            step_uuid: step.workflow_step_uuid,
            step_name: step.name,
            reason: determine_bottleneck_reason(step),
            severity: severity,
            impact: "Blocks #{path_steps.length - path_steps.index(step) - 1} downstream steps"
          }
        end

        bottlenecks.sort_by { |b| -b[:severity] }
      end

      # Calculate bottleneck severity score
      def calculate_bottleneck_severity(step)
        severity = 0
        severity += 10 if step.current_state == 'error'
        severity += step.attempts * 5 # More attempts = higher severity
        severity += 20 if step.attempts >= (step.retry_limit || 3) # Exhausted retries
        severity += 5 unless step.dependencies_satisfied # Dependency issues
        severity
      end

      # Determine the reason for a bottleneck using step readiness status
      #
      # Leverages the existing step readiness infrastructure to provide consistent
      # and accurate bottleneck reason identification. Maps technical blocking
      # reasons to user-friendly descriptions.
      #
      # @param step [Object] Step readiness status object
      # @return [String] User-friendly description of why the step is a bottleneck
      #
      # @example
      #   reason = determine_bottleneck_reason(step)
      #   puts "Step blocked: #{reason}"  # "Dependencies not satisfied"
      #
      # @api private
      def determine_bottleneck_reason(step)
        # Use the existing blocking_reason from step readiness status
        blocking_reason = step.blocking_reason

        # Map the technical blocking reasons to user-friendly descriptions
        case blocking_reason
        when 'dependencies_not_satisfied'
          'Dependencies not satisfied'
        when 'retry_not_eligible'
          if step.attempts >= (step.retry_limit || 3)
            'Exhausted retries'
          else
            'In backoff period'
          end
        when 'invalid_state'
          case step.current_state
          when 'error'
            'In error state'
          else
            "Invalid state: #{step.current_state}"
          end
        when 'unknown'
          'Unknown blocking condition'
        when nil
          # Step is ready for execution, shouldn't be a bottleneck
          'Not blocked (ready for execution)'
        else
          "Unrecognized blocking reason: #{blocking_reason}"
        end
      end

      # Analyze parallelism opportunities in the workflow
      def analyze_parallelism
        graph = build_dependency_graph
        dependency_levels = graph[:dependency_levels]
        step_readiness_data = get_step_readiness_data
        task_context = get_task_execution_context

        # Group steps by dependency level
        levels_groups = dependency_levels.group_by { |_step_uuid, level| level }

        parallelism_analysis = levels_groups.map do |level, step_level_pairs|
          step_uuids = step_level_pairs.map(&:first)
          step_data = step_uuids.filter_map { |id| step_readiness_data[id] }

          analyze_level_parallelism(level, step_data)
        end.sort_by { |analysis| analysis[:level] }

        {
          # Use task context for overall counts (more efficient)
          total_steps: task_context.total_steps,
          total_levels: levels_groups.keys.max + 1,
          max_parallel_steps: parallelism_analysis.pluck(:total_steps).max || 0,
          current_parallel_opportunities: task_context.ready_steps,
          blocked_parallel_opportunities: parallelism_analysis.sum { |a| a[:blocked_steps] },
          overall_completion: task_context.completion_percentage,
          levels: parallelism_analysis,
          parallelism_efficiency: calculate_parallelism_efficiency(parallelism_analysis)
        }
      end

      # Analyze parallelism opportunities at a specific dependency level
      def analyze_level_parallelism(level, step_data)
        # For level-specific analysis, we still need to count within this level
        # But we can optimize by using the step data directly
        ready_steps = step_data.count(&:ready_for_execution)
        blocked_steps = step_data.count { |s| !s.dependencies_satisfied }
        error_steps = step_data.count { |s| s.current_state == 'error' }
        completed_steps = step_data.count { |s| s.current_state.in?(%w[complete resolved_manually]) }

        {
          level: level,
          total_steps: step_data.length,
          ready_steps: ready_steps,
          blocked_steps: blocked_steps,
          error_steps: error_steps,
          completed_steps: completed_steps,
          step_names: step_data.map(&:name),
          parallelism_potential: if ready_steps > 1
                                   'High'
                                 else
                                   ready_steps == 1 ? 'Medium' : 'Low'
                                 end,
          bottleneck_risk: blocked_steps > (step_data.length / 2) ? 'High' : 'Low'
        }
      end

      # Calculate overall parallelism efficiency
      def calculate_parallelism_efficiency(parallelism_analysis)
        total_steps = parallelism_analysis.sum { |a| a[:total_steps] }
        return 0 if total_steps.zero?

        # Efficiency is based on how well steps are distributed across levels
        # and how many can run in parallel
        parallel_opportunities = parallelism_analysis.count { |a| a[:ready_steps] > 1 }
        total_levels = parallelism_analysis.length

        return 0 if total_levels.zero?

        (parallel_opportunities.to_f / total_levels * 100).round(1)
      end

      # Analyze error propagation chains in the workflow
      def analyze_error_chains
        graph = build_dependency_graph
        step_readiness_data = get_step_readiness_data
        task_context = get_task_execution_context

        # Use task context for efficient error step count
        return empty_error_analysis if task_context.failed_steps.zero?

        # Find all steps currently in error state
        error_steps = step_readiness_data.values.select { |s| s.current_state == 'error' }

        error_chains = error_steps.map do |error_step|
          analyze_error_impact_chain(error_step, graph, step_readiness_data)
        end.sort_by { |chain| -chain[:total_impact] }

        {
          # Use task context for overall metrics (more efficient)
          total_error_steps: task_context.failed_steps,
          total_steps: task_context.total_steps,
          error_percentage: (task_context.failed_steps.to_f / task_context.total_steps * 100).round(1),
          total_blocked_by_errors: error_chains.sum { |c| c[:blocked_downstream_steps] },
          most_critical_error: error_chains.first,
          error_chains: error_chains,
          recovery_priority: determine_recovery_priority(error_chains),
          task_health: task_context.health_status
        }
      end

      # Return empty error analysis when no errors exist
      def empty_error_analysis
        {
          total_error_steps: 0,
          total_steps: get_task_execution_context.total_steps,
          error_percentage: 0.0,
          total_blocked_by_errors: 0,
          most_critical_error: nil,
          error_chains: [],
          recovery_priority: [],
          task_health: get_task_execution_context.health_status
        }
      end

      # Analyze the impact chain of a specific error step
      def analyze_error_impact_chain(error_step, graph, step_readiness_data)
        # Find all downstream steps affected by this error
        downstream_steps = find_downstream_steps(error_step.workflow_step_uuid, graph[:adjacency_list])
        blocked_steps = downstream_steps.select do |step_uuid|
          step_data = step_readiness_data[step_uuid]
          step_data && !step_data.dependencies_satisfied
        end

        # Calculate impact metrics
        total_downstream = downstream_steps.length
        blocked_downstream = blocked_steps.length

        # Analyze retry situation
        retry_analysis = analyze_error_retry_situation(error_step)

        {
          error_step_uuid: error_step.workflow_step_uuid,
          error_step_name: error_step.name,
          error_attempts: error_step.attempts,
          retry_limit: error_step.retry_limit || 3,
          retry_analysis: retry_analysis,
          downstream_steps: downstream_steps,
          blocked_downstream_steps: blocked_downstream,
          total_downstream_steps: total_downstream,
          total_impact: calculate_error_impact_score(error_step, blocked_downstream, total_downstream),
          recovery_strategies: suggest_recovery_strategies(error_step, retry_analysis)
        }
      end

      # Find all downstream steps from a given step
      def find_downstream_steps(step_uuid, adjacency_list, visited = Set.new)
        return [] if visited.include?(step_uuid)

        visited.add(step_uuid)
        downstream = []

        adjacency_list[step_uuid].each do |child_id|
          downstream << child_id
          downstream.concat(find_downstream_steps(child_id, adjacency_list, visited))
        end

        downstream.uniq
      end

      # Analyze the retry situation for an error step
      def analyze_error_retry_situation(error_step)
        retry_limit = error_step.retry_limit || 3
        attempts = error_step.attempts

        {
          exhausted: attempts >= retry_limit,
          remaining_attempts: [retry_limit - attempts, 0].max,
          in_backoff: error_step.next_retry_at && error_step.next_retry_at > Time.current,
          next_retry_at: error_step.next_retry_at,
          retryable: error_step.retry_eligible
        }
      end

      # Calculate impact score for an error using configurable weights
      #
      # Uses configurable impact multipliers for flexible error impact assessment.
      #
      # @param error_step [Object] Error step object
      # @param blocked_downstream [Integer] Number of blocked downstream steps
      # @param total_downstream [Integer] Total downstream steps
      # @return [Integer] Error impact score using configurable weights
      def calculate_error_impact_score(error_step, blocked_downstream, total_downstream)
        config = dependency_graph_config
        score = 0

        # Base impact from blocked downstream steps
        score += blocked_downstream * config.impact_multipliers[:blocked_weight]

        # Additional impact for total downstream reach
        score += total_downstream * config.impact_multipliers[:downstream_weight]

        # Penalty for exhausted retries (permanent blockage)
        score += config.penalty_constants[:exhausted_retry] if error_step.attempts >= (error_step.retry_limit || 3)

        # Penalty for high retry count (instability)
        score += error_step.attempts * config.penalty_constants[:retry_instability]

        score
      end

      # Suggest recovery strategies for an error step
      def suggest_recovery_strategies(_error_step, retry_analysis)
        if retry_analysis[:exhausted]
          exhausted_retry_strategies
        elsif retry_analysis[:in_backoff]
          backoff_strategies(retry_analysis)
        elsif retry_analysis[:retryable]
          retryable_strategies(retry_analysis)
        else
          non_retryable_strategies
        end
      end

      # Recovery strategies for steps that have exhausted retries
      #
      # @return [Array<Hash>] Strategy recommendations with priority and description
      # @api private
      def exhausted_retry_strategies
        [
          {
            strategy: 'Manual Resolution',
            priority: 'High',
            description: 'Step has exhausted retries and requires manual intervention'
          },
          {
            strategy: 'Increase Retry Limit',
            priority: 'Medium',
            description: 'Consider increasing retry limit if error is transient'
          }
        ]
      end

      # Recovery strategies for steps in backoff period
      #
      # @param retry_analysis [Hash] Retry analysis data including next_retry_at
      # @return [Array<Hash>] Strategy recommendations with timing information
      # @api private
      def backoff_strategies(retry_analysis)
        [
          {
            strategy: 'Wait for Backoff',
            priority: 'Low',
            description: "Step will retry automatically at #{retry_analysis[:next_retry_at]}"
          }
        ]
      end

      # Recovery strategies for retryable steps
      #
      # @param retry_analysis [Hash] Retry analysis data including remaining_attempts
      # @return [Array<Hash>] Strategy recommendations with attempt information
      # @api private
      def retryable_strategies(retry_analysis)
        [
          {
            strategy: 'Monitor Retries',
            priority: 'Medium',
            description: "Step has #{retry_analysis[:remaining_attempts]} retry attempts remaining"
          }
        ]
      end

      # Recovery strategies for non-retryable steps
      #
      # @return [Array<Hash>] Strategy recommendations for investigation
      # @api private
      def non_retryable_strategies
        [
          {
            strategy: 'Investigate Root Cause',
            priority: 'High',
            description: 'Step is not retryable, requires investigation'
          }
        ]
      end

      # Determine recovery priority across all error chains
      def determine_recovery_priority(error_chains)
        return [] if error_chains.empty?

        # Sort by impact score and return top priorities
        error_chains.first(3).map do |chain|
          {
            step_name: chain[:error_step_name],
            priority_level: determine_priority_level(chain[:total_impact]),
            reason: "Blocking #{chain[:blocked_downstream_steps]} downstream steps",
            recommended_action: chain[:recovery_strategies].first&.dig(:strategy) || 'Investigate'
          }
        end
      end

      # Determine priority level based on impact score
      #
      # Uses configurable thresholds for consistent severity classification.
      #
      # @param impact_score [Integer] The calculated impact score
      # @return [String] Priority level: 'Critical', 'High', 'Medium', or 'Low'
      def determine_priority_level(impact_score)
        config = dependency_graph_config

        return 'Critical' if impact_score >= config.severity_thresholds[:critical]
        return 'High' if impact_score >= config.severity_thresholds[:high]
        return 'Medium' if impact_score >= config.severity_thresholds[:medium]

        'Low'
      end

      # Identify bottlenecks with impact scoring
      def identify_bottlenecks
        graph = build_dependency_graph
        step_readiness_data = get_step_readiness_data
        task_context = get_task_execution_context

        # Early return if no blocking issues (use task context for efficiency)
        return empty_bottleneck_analysis if task_context.failed_steps.zero? &&
                                            task_context.pending_steps.zero? &&
                                            task_context.ready_steps == task_context.total_steps

        # Find potential bottleneck steps more efficiently
        bottleneck_candidates = find_bottleneck_candidates(step_readiness_data, task_context)

        # Analyze each bottleneck candidate
        bottleneck_analysis = bottleneck_candidates.map do |step|
          analyze_bottleneck_impact(step, graph, step_readiness_data)
        end.sort_by { |analysis| -analysis[:impact_score] }

        {
          total_bottlenecks: bottleneck_analysis.length,
          critical_bottlenecks: bottleneck_analysis.count { |b| b[:severity] == 'Critical' },
          total_blocked_steps: bottleneck_analysis.sum { |b| b[:blocked_downstream_count] },
          bottlenecks: bottleneck_analysis.first(10), # Top 10 bottlenecks
          task_health: task_context.health_status,
          task_completion: task_context.completion_percentage,
          failed_steps_ratio: (task_context.failed_steps.to_f / task_context.total_steps * 100).round(1),
          overall_impact: calculate_overall_bottleneck_impact(bottleneck_analysis, task_context)
        }
      end

      # Find bottleneck candidates efficiently using task context
      def find_bottleneck_candidates(step_readiness_data, task_context)
        # If we have many failed steps, focus on those first
        if task_context.failed_steps.positive?
          candidates = step_readiness_data.values.select { |step| step.current_state == 'error' }
          return candidates unless candidates.empty?
        end

        # Otherwise, look for other blocking conditions
        step_readiness_data.values.select do |step|
          !step.dependencies_satisfied ||
            step.attempts >= 2 ||
            !step.retry_eligible
        end
      end

      # Return empty bottleneck analysis when no bottlenecks exist
      def empty_bottleneck_analysis
        task_context = get_task_execution_context
        {
          total_bottlenecks: 0,
          critical_bottlenecks: 0,
          total_blocked_steps: 0,
          bottlenecks: [],
          task_health: task_context.health_status,
          overall_impact: 'None'
        }
      end

      # Analyze the comprehensive impact of a specific bottleneck step
      #
      # Performs detailed analysis of how a bottleneck step affects the overall
      # workflow, including downstream impact, severity assessment, and resolution
      # recommendations.
      #
      # @param step [Object] Step readiness status object
      # @param graph [Hash] Dependency graph structure
      # @param step_readiness_data [Hash] Step ID to readiness status mapping
      # @return [Hash] Complete bottleneck analysis including:
      #   - Step metadata (ID, name, state, attempts, etc.)
      #   - Downstream impact counts
      #   - Impact score and severity level
      #   - Bottleneck type classification
      #   - Resolution strategy and time estimate
      #
      # @api private
      def analyze_bottleneck_impact(step, graph, step_readiness_data)
        downstream_impact = calculate_downstream_impact(step, graph, step_readiness_data)
        impact_score = calculate_bottleneck_impact_score(step, downstream_impact[:downstream_steps_count],
                                                         downstream_impact[:blocked_downstream_count])

        {
          **extract_step_metadata(step),
          **downstream_impact,
          impact_score: impact_score,
          severity: determine_bottleneck_severity_level(impact_score),
          bottleneck_type: determine_bottleneck_type(step),
          resolution_strategy: suggest_bottleneck_resolution(step),
          estimated_resolution_time: estimate_resolution_time(step)
        }
      end

      # Calculate the downstream impact of a bottleneck step
      #
      # Determines how many steps are affected by this bottleneck, both in terms
      # of total downstream steps and those that are actually blocked.
      #
      # @param step [Object] Step readiness status object
      # @param graph [Hash] Dependency graph structure
      # @param step_readiness_data [Hash] Step ID to readiness status mapping
      # @return [Hash] Impact metrics with :downstream_steps_count and :blocked_downstream_count
      #
      # @api private
      def calculate_downstream_impact(step, graph, step_readiness_data)
        downstream_steps = find_downstream_steps(step.workflow_step_uuid, graph[:adjacency_list])
        blocked_downstream = count_blocked_downstream_steps(downstream_steps, step_readiness_data)

        {
          downstream_steps_count: downstream_steps.length,
          blocked_downstream_count: blocked_downstream
        }
      end

      # Extract essential metadata from a step for analysis
      #
      # @param step [Object] Step readiness status object
      # @return [Hash] Step metadata including ID, name, state, attempts, and dependencies
      #
      # @api private
      def extract_step_metadata(step)
        {
          step_uuid: step.workflow_step_uuid,
          step_name: step.name,
          current_state: step.current_state,
          attempts: step.attempts,
          retry_limit: step.retry_limit || 3,
          dependencies_satisfied: step.dependencies_satisfied
        }
      end

      # Count downstream steps that are actually blocked by dependencies
      #
      # @param downstream_steps [Array<Integer>] List of downstream step IDs
      # @param step_readiness_data [Hash] Step ID to readiness status mapping
      # @return [Integer] Number of downstream steps that are blocked
      #
      # @api private
      def count_blocked_downstream_steps(downstream_steps, step_readiness_data)
        downstream_steps.count do |step_uuid|
          downstream_step = step_readiness_data[step_uuid]
          downstream_step && !downstream_step.dependencies_satisfied
        end
      end

      # Calculate comprehensive impact score for a bottleneck
      #
      # Combines base impact, state severity multipliers, and penalty scores to
      # provide a comprehensive bottleneck impact assessment.
      #
      # @param step [Object] Step readiness status object
      # @param downstream_count [Integer] Total number of downstream steps
      # @param blocked_count [Integer] Number of blocked downstream steps
      # @return [Integer] Rounded impact score for severity classification
      #
      # @api private
      def calculate_bottleneck_impact_score(step, downstream_count, blocked_count)
        base_score = calculate_base_impact_score(downstream_count, blocked_count)
        state_multiplier = calculate_state_severity_multiplier(step)
        penalty_score = calculate_bottleneck_penalties(step)

        ((base_score * state_multiplier) + penalty_score).round
      end

      # Calculate base impact score from downstream effects
      #
      # Provides the foundation score based on how many steps are affected.
      # Blocked steps are weighted more heavily than total downstream steps.
      # Uses configurable multipliers for flexible impact weighting.
      #
      # @param downstream_count [Integer] Total number of downstream steps
      # @param blocked_count [Integer] Number of blocked downstream steps
      # @return [Integer] Base impact score using configurable weights
      #
      # @api private
      def calculate_base_impact_score(downstream_count, blocked_count)
        config = dependency_graph_config
        (downstream_count * config.impact_multipliers[:downstream_weight]) +
          (blocked_count * config.impact_multipliers[:blocked_weight])
      end

      # Calculate severity multiplier based on step state
      #
      # Applies multipliers based on the current state of the step, with error
      # states receiving higher multipliers, especially for exhausted retries.
      # Uses configurable multipliers for flexible severity weighting.
      #
      # @param step [Object] Step readiness status object
      # @return [Float] Severity multiplier using configurable weights
      #
      # @api private
      def calculate_state_severity_multiplier(step)
        config = dependency_graph_config

        case step.current_state
        when 'error'
          multiplier = config.severity_multipliers[:error_state] # Errors are serious
          # Exhausted retries are critical
          multiplier += config.severity_multipliers[:exhausted_retry_bonus] if step.attempts >= (step.retry_limit || 3)
          multiplier
        when 'pending'
          step.dependencies_satisfied ? 1.0 : config.severity_multipliers[:dependency_issue] # Dependency issues
        else
          1.0
        end
      end

      # Calculate additional penalty scores for problematic conditions
      #
      # Adds penalty points for retry instability, non-retryable issues,
      # and exhausted retry attempts. Uses configurable constants for
      # flexible penalty weighting.
      #
      # @param step [Object] Step readiness status object
      # @return [Integer] Total penalty score using configurable penalties
      #
      # @api private
      def calculate_bottleneck_penalties(step)
        config = dependency_graph_config
        penalty = 0
        penalty += step.attempts * config.penalty_constants[:retry_instability] # Retry instability
        penalty += config.penalty_constants[:non_retryable] unless step.retry_eligible # Non-retryable issues
        # Exhausted retries
        penalty += config.penalty_constants[:exhausted_retry] if step.attempts >= (step.retry_limit || 3)
        penalty
      end

      # Determine severity level based on impact score
      #
      # Uses configurable thresholds for flexible severity classification.
      #
      # @param impact_score [Integer] The calculated impact score
      # @return [String] Severity level: 'Critical', 'High', 'Medium', or 'Low'
      def determine_bottleneck_severity_level(impact_score)
        config = dependency_graph_config

        return 'Critical' if impact_score >= config.severity_thresholds[:critical]
        return 'High' if impact_score >= config.severity_thresholds[:high]
        return 'Medium' if impact_score >= config.severity_thresholds[:medium]

        'Low'
      end

      # Determine the type of bottleneck using step readiness status
      #
      # Classifies the bottleneck type by leveraging step readiness status methods
      # for consistent and accurate categorization.
      #
      # @param step [Object] Step readiness status object
      # @return [String] Bottleneck type classification:
      #   - 'Permanent Failure' for exhausted retries
      #   - 'Active Error' for steps in error state
      #   - 'Dependency Block' for dependency issues
      #   - 'Retry Exhaustion' for high retry counts
      #   - 'Non-Retryable' for non-retryable steps
      #   - 'Unknown' for unclassified cases
      #
      # @api private
      def determine_bottleneck_type(step)
        # Use the existing retry_status and dependency_status from step readiness
        retry_status = step.retry_status
        dependency_status = step.dependency_status

        # Check for permanent failures first
        return 'Permanent Failure' if retry_status == 'max_retries_reached'

        # Check current state issues
        return 'Active Error' if step.current_state == 'error'

        # Check dependency issues
        return 'Dependency Block' if dependency_status != 'all_satisfied' && dependency_status != 'no_dependencies'

        # Check retry issues
        return 'Retry Exhaustion' if step.attempts >= 2
        return 'Non-Retryable' if retry_status != 'retry_eligible'

        'Unknown'
      end

      # Suggest resolution strategy using step readiness status
      #
      # Provides actionable resolution strategies based on step readiness status
      # methods for more accurate and helpful suggestions.
      #
      # @param step [Object] Step readiness status object
      # @return [String] Recommended resolution strategy
      #
      # @api private
      def suggest_bottleneck_resolution(step)
        # Use step readiness status methods for more accurate suggestions
        retry_status = step.retry_status
        step.dependency_status
        blocking_reason = step.blocking_reason

        return 'Manual intervention required' if retry_status == 'max_retries_reached'
        return 'Investigate error cause' if step.current_state == 'error'
        return 'Check upstream dependencies' if blocking_reason == 'dependencies_not_satisfied'
        return 'Wait for backoff period' if blocking_reason == 'retry_not_eligible' && step.next_retry_at
        return 'Monitor retry attempts' if step.attempts >= 1

        'Review step configuration'
      end

      # Estimate time to resolve bottleneck using precise step readiness data
      #
      # Provides accurate time estimates by leveraging step readiness status
      # time calculations, including exact minute-based estimates when available.
      #
      # @param step [Object] Step readiness status object
      # @return [String] Time estimate for resolution:
      #   - 'Immediate action needed' for exhausted retries
      #   - Specific minute estimates when time_until_ready is available
      #   - General estimates for other conditions
      #
      # @api private
      def estimate_resolution_time(step)
        retry_status = step.retry_status
        blocking_reason = step.blocking_reason
        time_until_ready = step.time_until_ready

        return 'Immediate action needed' if retry_status == 'max_retries_reached'

        # If we have a specific time until ready, use it
        if time_until_ready&.positive?
          minutes = (time_until_ready / 60.0).ceil
          return "#{minutes} minute#{'s' unless minutes == 1}"
        end

        return 'Within next retry cycle' if step.current_state == 'error' && retry_status == 'retry_eligible'
        return 'Depends on upstream steps' if blocking_reason == 'dependencies_not_satisfied'

        'Unknown'
      end

      # Calculate overall bottleneck impact on the task
      def calculate_overall_bottleneck_impact(bottleneck_analysis, task_context)
        return 'None' if bottleneck_analysis.empty?

        critical_count = bottleneck_analysis.count { |b| b[:severity] == 'Critical' }
        high_count = bottleneck_analysis.count { |b| b[:severity] == 'High' }

        return 'Severe' if critical_count >= 3 || task_context.failed_steps > (task_context.total_steps / 2)
        return 'High' if critical_count >= 1 || high_count >= 3
        return 'Medium' if high_count >= 1 || bottleneck_analysis.length >= 5

        'Low'
      end
    end
  end
end
