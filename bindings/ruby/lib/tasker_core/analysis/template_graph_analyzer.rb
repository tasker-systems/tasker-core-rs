# frozen_string_literal: true

module Tasker
  module Analysis
    # Analyzes step template dependencies for workflow design validation
    #
    # This class provides comprehensive analysis of step template dependencies,
    # including cycle detection, topological sorting, and dependency visualization.
    # It's designed to help with workflow design validation and troubleshooting.
    #
    # The analyzer performs static analysis on step templates to identify potential
    # issues before workflow execution, including circular dependencies, dependency
    # depth analysis, and parallel execution opportunities.
    #
    # @example Basic usage
    #   analyzer = TemplateGraphAnalyzer.new(handler.step_templates)
    #   graph = analyzer.analyze
    #   puts "Cycles detected: #{graph[:cycles].any?}"
    #
    # @example Checking for specific issues
    #   analyzer = TemplateGraphAnalyzer.new(templates)
    #   if analyzer.has_cycles?
    #     puts "Circular dependencies found: #{analyzer.cycles}"
    #   end
    #
    # @since 2.2.0
    class TemplateGraphAnalyzer
      # @return [Array<Tasker::Types::StepTemplate>] The step templates being analyzed
      attr_reader :templates

      # Initialize the analyzer with step templates
      #
      # @param templates [Array<Tasker::Types::StepTemplate>] Step templates to analyze
      def initialize(templates)
        @templates = templates
        @dependency_map = nil
        @analysis_cache = nil
      end

      # Perform comprehensive dependency analysis
      #
      # @return [Hash] Comprehensive dependency analysis containing:
      #   - :nodes - Array of step information with dependencies
      #   - :edges - Array of dependency relationships
      #   - :topology - Topologically sorted step names
      #   - :cycles - Array of detected circular dependencies
      #   - :levels - Hash mapping steps to their dependency depth levels
      #   - :roots - Array of steps with no dependencies
      #   - :leaves - Array of steps with no dependents
      #   - :summary - Summary statistics
      def analyze
        return @analysis_cache if @analysis_cache

        nodes = build_nodes
        edges = build_edges
        dependency_map = build_dependency_map
        cycles = detect_cycles(dependency_map)
        topology = cycles.empty? ? topological_sort(dependency_map) : []
        levels = calculate_dependency_levels(dependency_map, topology)
        roots = find_root_steps
        leaves = find_leaf_steps(edges)

        @analysis_cache = {
          nodes: nodes,
          edges: edges,
          topology: topology,
          cycles: cycles,
          levels: levels,
          roots: roots,
          leaves: leaves,
          summary: build_summary(cycles, levels, edges)
        }
      end

      # Check if the workflow has circular dependencies
      #
      # @return [Boolean] True if cycles are detected
      def has_cycles?
        cycles.any?
      end

      # Get detected circular dependencies
      #
      # @return [Array<Array<String>>] Array of detected cycles
      def cycles
        analyze[:cycles]
      end

      # Get topological ordering of steps
      #
      # @return [Array<String>] Topologically sorted step names
      def topology
        analyze[:topology]
      end

      # Get dependency levels for all steps
      #
      # @return [Hash<String, Integer>] Step name to dependency level mapping
      def levels
        analyze[:levels]
      end

      # Get steps with no dependencies (workflow entry points)
      #
      # @return [Array<String>] Root step names
      def roots
        analyze[:roots]
      end

      # Get steps with no dependents (workflow exit points)
      #
      # @return [Array<String>] Leaf step names
      def leaves
        analyze[:leaves]
      end

      # Clear analysis cache (useful if templates change)
      #
      # @return [void]
      def clear_cache!
        @analysis_cache = nil
        @dependency_map = nil
      end

      private

      # Build nodes with dependency information
      #
      # @return [Array<Hash>] Node information for each step
      # @api private
      def build_nodes
        templates.map do |template|
          {
            name: template.name,
            description: template.description,
            handler_class: template.handler_class.name,
            dependencies: template.all_dependencies,
            dependency_count: template.all_dependencies.size
          }
        end
      end

      # Build edges (dependency relationships)
      #
      # @return [Array<Hash>] Edge information for dependencies
      # @api private
      def build_edges
        edges = []
        templates.each do |template|
          template.all_dependencies.each do |dependency|
            edges << {
              from: dependency,
              to: template.name,
              type: template.depends_on_step == dependency ? 'single' : 'multiple'
            }
          end
        end
        edges
      end

      # Build dependency map from step templates
      #
      # @return [Hash<String, Array<String>>] Map of step name to its dependencies
      # @api private
      def build_dependency_map
        return @dependency_map if @dependency_map

        @dependency_map = templates.each_with_object({}) do |template, map|
          map[template.name] = template.all_dependencies
        end
      end

      # Detect circular dependencies using depth-first search
      #
      # @param dependency_map [Hash<String, Array<String>>] Step dependencies
      # @return [Array<Array<String>>] Array of detected cycles
      # @api private
      def detect_cycles(dependency_map)
        cycles = []
        visited = Set.new
        rec_stack = Set.new

        dependency_map.each_key do |step|
          next if visited.include?(step)

          cycle = find_cycle_from_step(step, dependency_map, visited, rec_stack, [])
          cycles << cycle if cycle
        end

        cycles
      end

      # Find cycle starting from a specific step
      #
      # @param step [String] Starting step name
      # @param dependency_map [Hash] Step dependencies
      # @param visited [Set] Visited steps
      # @param rec_stack [Set] Recursion stack
      # @param path [Array] Current path
      # @return [Array<String>, nil] Cycle path or nil
      # @api private
      def find_cycle_from_step(step, dependency_map, visited, rec_stack, path)
        return nil if visited.include?(step)

        visited.add(step)
        rec_stack.add(step)
        current_path = path + [step]

        dependency_map[step].each do |dependency|
          if rec_stack.include?(dependency)
            # Found cycle - return the cycle portion
            cycle_start = current_path.index(dependency)
            return current_path[cycle_start..] + [dependency]
          end

          cycle = find_cycle_from_step(dependency, dependency_map, visited, rec_stack, current_path)
          return cycle if cycle
        end

        rec_stack.delete(step)
        nil
      end

      # Perform topological sort of steps using Kahn's algorithm
      #
      # @param dependency_map [Hash<String, Array<String>>] Step dependencies
      # @return [Array<String>] Topologically sorted step names
      # @api private
      def topological_sort(dependency_map)
        in_degree = Hash.new(0)
        reverse_deps = Hash.new { |h, k| h[k] = [] }

        # Calculate in-degrees and reverse dependencies
        dependency_map.each do |step, dependencies|
          in_degree[step] ||= 0
          dependencies.each do |dep|
            in_degree[step] += 1
            reverse_deps[dep] << step
          end
        end

        # Start with steps that have no dependencies
        queue = dependency_map.keys.select { |step| in_degree[step].zero? }
        result = []

        while queue.any?
          current = queue.shift
          result << current

          # Remove edges from current step to its dependents
          reverse_deps[current].each do |dependent|
            in_degree[dependent] -= 1
            queue << dependent if in_degree[dependent].zero?
          end
        end

        result
      end

      # Calculate dependency levels for each step
      #
      # @param dependency_map [Hash<String, Array<String>>] Step dependencies
      # @param topology [Array<String>] Topologically sorted steps
      # @return [Hash<String, Integer>] Step name to dependency level
      # @api private
      def calculate_dependency_levels(dependency_map, topology)
        levels = {}

        topology.each do |step|
          if dependency_map[step].empty?
            levels[step] = 0
          else
            max_dependency_level = dependency_map[step].map { |dep| levels[dep] || 0 }.max
            levels[step] = max_dependency_level + 1
          end
        end

        levels
      end

      # Find steps with no dependencies
      #
      # @return [Array<String>] Root step names
      # @api private
      def find_root_steps
        dependency_map = build_dependency_map
        dependency_map.keys.select { |name| dependency_map[name].empty? }
      end

      # Find steps with no dependents
      #
      # @param edges [Array<Hash>] Dependency edges
      # @return [Array<String>] Leaf step names
      # @api private
      def find_leaf_steps(edges)
        step_names = templates.map(&:name)
        step_names.reject { |name| edges.any? { |edge| edge[:from] == name } }
      end

      # Build summary statistics
      #
      # @param cycles [Array] Detected cycles
      # @param levels [Hash] Dependency levels
      # @param edges [Array] Dependency edges
      # @return [Hash] Summary statistics
      # @api private
      def build_summary(cycles, levels, edges)
        {
          total_steps: templates.size,
          total_dependencies: edges.size,
          has_cycles: cycles.any?,
          max_depth: levels.values.max || 0,
          parallel_branches: count_parallel_branches(levels)
        }
      end

      # Count parallel branches in the dependency graph
      #
      # @param levels [Hash<String, Integer>] Dependency levels
      # @return [Integer] Number of parallel execution branches
      # @api private
      def count_parallel_branches(levels)
        level_counts = levels.values.group_by(&:itself).transform_values(&:size)
        level_counts.values.max || 1
      end
    end
  end
end
