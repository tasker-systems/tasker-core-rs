# frozen_string_literal: true

require 'pg'
require 'json'

module TaskerCore
  module Database
    # Ruby wrapper for TaskerCore SQL functions
    #
    # This provides Ruby access to the PostgreSQL functions created by the Rust core
    # for status queries, analytics, and dependency analysis without FFI coupling.
    #
    # All function names match the actual SQL functions defined in migrations/*.sql
    #
    # Examples:
    #   sql = SqlFunctions.new
    #   context = sql.task_execution_context(task_id)
    #   status = sql.step_readiness_status(task_id)
    #   health = sql.system_health_counts
    class SqlFunctions
      attr_reader :connection, :logger

      def initialize(connection: nil, logger: nil)
        @connection = connection || create_connection
        @logger = logger || TaskerCore::Logging::Logger.instance
      end

      # Get task execution context including progress and status
      # Uses: get_task_execution_context(input_task_id bigint)
      #
      # @param task_id [Integer] Task ID to analyze
      # @return [Hash] Task execution context data
      # @raise [TaskerCore::Error] if query fails
      def task_execution_context(task_id)
        logger.debug("📊 SQL: Getting task execution context for task_id: #{task_id}")

        result = connection.exec(
          "SELECT * FROM get_task_execution_context($1::BIGINT)",
          [task_id]
        )

        if result.ntuples > 0
          context = parse_task_execution_context(result[0])
          logger.debug("✅ SQL: Retrieved task execution context for task_id: #{task_id}")
          context
        else
          logger.warn("⚠️ SQL: No execution context found for task_id: #{task_id}")
          nil
        end
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get task execution context for #{task_id}: #{e.message}")
        raise Errors::DatabaseError, "Failed to get task execution context: #{e.message}"
      end

      # Get task execution contexts for multiple tasks in batch
      # Uses: get_task_execution_contexts_batch(input_task_ids bigint[])
      #
      # @param task_ids [Array<Integer>] Task IDs to analyze
      # @return [Array<Hash>] Array of task execution context data
      def task_execution_contexts_batch(task_ids)
        logger.debug("📊 SQL: Getting task execution contexts for #{task_ids.length} tasks")

        result = connection.exec(
          "SELECT * FROM get_task_execution_contexts_batch($1::BIGINT[])",
          [task_ids]
        )

        contexts = result.map { |row| parse_task_execution_context(row) }
        logger.debug("✅ SQL: Retrieved #{contexts.length} task execution contexts")
        contexts
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get task execution contexts batch: #{e.message}")
        raise TaskerCore::Errors::DatabaseError, "Failed to get task execution contexts: #{e.message}"
      end

      # Get step readiness status for a task
      # Uses: get_step_readiness_status(input_task_id bigint, step_ids bigint[] DEFAULT NULL)
      #
      # @param task_id [Integer] Task ID to analyze
      # @param step_ids [Array<Integer>] Optional specific step IDs (nil for all steps)
      # @return [Array<Hash>] Array of step readiness information
      def step_readiness_status(task_id, step_ids: nil)
        logger.debug("📊 SQL: Getting step readiness status for task_id: #{task_id}")

        result = connection.exec(
          "SELECT * FROM get_step_readiness_status($1::BIGINT, $2::BIGINT[])",
          [task_id, step_ids]
        )

        steps = result.map { |row| parse_step_readiness(row) }
        logger.debug("✅ SQL: Retrieved #{steps.length} step readiness entries for task_id: #{task_id}")
        steps
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get step readiness status for #{task_id}: #{e.message}")
        raise TaskerCore::Error, "Failed to get step readiness status: #{e.message}"
      end

      # Get step readiness status for multiple tasks in batch
      # Uses: get_step_readiness_status_batch(input_task_ids bigint[])
      #
      # @param task_ids [Array<Integer>] Task IDs to analyze
      # @return [Array<Hash>] Array of step readiness information
      def step_readiness_status_batch(task_ids)
        logger.debug("📊 SQL: Getting step readiness status batch for #{task_ids.length} tasks")

        result = connection.exec(
          "SELECT * FROM get_step_readiness_status_batch($1::BIGINT[])",
          [task_ids]
        )

        steps = result.map { |row| parse_step_readiness(row) }
        logger.debug("✅ SQL: Retrieved #{steps.length} step readiness entries for batch")
        steps
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get step readiness status batch: #{e.message}")
        raise TaskerCore::Error, "Failed to get step readiness status batch: #{e.message}"
      end

      # Get dependency levels for steps in a task
      # Uses: calculate_dependency_levels(input_task_id bigint)
      #
      # @param task_id [Integer] Task ID to analyze
      # @return [Array<Hash>] Array with workflow_step_id and dependency_level
      def calculate_dependency_levels(task_id)
        logger.debug("📊 SQL: Calculating dependency levels for task_id: #{task_id}")

        result = connection.exec(
          "SELECT * FROM calculate_dependency_levels($1::BIGINT)",
          [task_id]
        )

        levels = result.map { |row| parse_dependency_levels(row) }
        logger.debug("✅ SQL: Retrieved #{levels.length} dependency level entries for task_id: #{task_id}")
        levels
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to calculate dependency levels for #{task_id}: #{e.message}")
        raise TaskerCore::Error, "Failed to calculate dependency levels: #{e.message}"
      end

      # Get system health counts
      # Uses: get_system_health_counts_v01()
      #
      # @return [Hash] System health metrics including task and step counts by status
      def system_health_counts
        logger.debug("📊 SQL: Getting system health counts")

        result = connection.exec("SELECT * FROM get_system_health_counts_v01()")

        if result.ntuples > 0
          health = parse_system_health(result[0])
          logger.debug("✅ SQL: Retrieved system health counts")
          health
        else
          logger.warn("⚠️ SQL: No system health data found")
          {}
        end
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get system health counts: #{e.message}")
        raise TaskerCore::Error, "Failed to get system health counts: #{e.message}"
      end

      # Get analytics metrics for performance monitoring
      # Uses: get_analytics_metrics_v01(since_timestamp timestamp with time zone DEFAULT NULL)
      #
      # @param since_timestamp [Time] Optional timestamp to analyze from (nil for default period)
      # @return [Hash] Analytics data including performance metrics
      def analytics_metrics(since_timestamp: nil)
        logger.debug("📊 SQL: Getting analytics metrics")

        result = connection.exec(
          "SELECT * FROM get_analytics_metrics_v01($1::TIMESTAMP WITH TIME ZONE)",
          [since_timestamp&.utc&.iso8601]
        )

        if result.ntuples > 0
          metrics = parse_analytics_metrics(result[0])
          logger.debug("✅ SQL: Retrieved analytics metrics")
          metrics
        else
          logger.warn("⚠️ SQL: No analytics metrics found")
          {}
        end
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get analytics metrics: #{e.message}")
        raise TaskerCore::Error, "Failed to get analytics metrics: #{e.message}"
      end

      # Get slowest steps analysis
      # Uses: get_slowest_steps_v01(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)
      #
      # @param since_timestamp [Time] Optional timestamp to analyze from
      # @param limit_count [Integer] Number of slowest steps to return (default: 10)
      # @param namespace_filter [String] Optional namespace filter
      # @param task_name_filter [String] Optional task name filter
      # @param version_filter [String] Optional version filter
      # @return [Array<Hash>] Array of slowest step analysis
      def slowest_steps(since_timestamp: nil, limit_count: 10, namespace_filter: nil, task_name_filter: nil, version_filter: nil)
        logger.debug("📊 SQL: Getting slowest steps analysis (limit: #{limit_count})")

        result = connection.exec(
          "SELECT * FROM get_slowest_steps_v01($1::TIMESTAMP WITH TIME ZONE, $2::INTEGER, $3::TEXT, $4::TEXT, $5::TEXT)",
          [since_timestamp&.utc&.iso8601, limit_count, namespace_filter, task_name_filter, version_filter]
        )

        steps = result.map { |row| parse_slowest_steps(row) }
        logger.debug("✅ SQL: Retrieved #{steps.length} slowest step entries")
        steps
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get slowest steps analysis: #{e.message}")
        raise TaskerCore::Error, "Failed to get slowest steps analysis: #{e.message}"
      end

      # Get slowest tasks analysis
      # Uses: get_slowest_tasks_v01(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)
      #
      # @param since_timestamp [Time] Optional timestamp to analyze from
      # @param limit_count [Integer] Number of slowest tasks to return (default: 10)
      # @param namespace_filter [String] Optional namespace filter
      # @param task_name_filter [String] Optional task name filter
      # @param version_filter [String] Optional version filter
      # @return [Array<Hash>] Array of slowest task analysis
      def slowest_tasks(since_timestamp: nil, limit_count: 10, namespace_filter: nil, task_name_filter: nil, version_filter: nil)
        logger.debug("📊 SQL: Getting slowest tasks analysis (limit: #{limit_count})")

        result = connection.exec(
          "SELECT * FROM get_slowest_tasks_v01($1::TIMESTAMP WITH TIME ZONE, $2::INTEGER, $3::TEXT, $4::TEXT, $5::TEXT)",
          [since_timestamp&.utc&.iso8601, limit_count, namespace_filter, task_name_filter, version_filter]
        )

        tasks = result.map { |row| parse_slowest_tasks(row) }
        logger.debug("✅ SQL: Retrieved #{tasks.length} slowest task entries")
        tasks
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to get slowest tasks analysis: #{e.message}")
        raise TaskerCore::Error, "Failed to get slowest tasks analysis: #{e.message}"
      end

      # Check if a task is complete based on task execution context
      #
      # @param task_id [Integer] Task ID to check
      # @return [Boolean] true if task is complete
      def task_complete?(task_id)
        context = task_execution_context(task_id)
        return false unless context

        # Task is complete if all steps are completed and none are failed/pending/in_progress
        context[:total_steps] > 0 &&
          context[:completed_steps] == context[:total_steps] &&
          context[:failed_steps] == 0 &&
          context[:pending_steps] == 0 &&
          context[:in_progress_steps] == 0
      end

      # Get task progress summary using task execution context
      #
      # @param task_id [Integer] Task ID to analyze
      # @return [Hash] Task progress information
      def task_progress(task_id)
        context = task_execution_context(task_id)
        return nil unless context

        {
          task_id: context[:task_id],
          total_steps: context[:total_steps],
          completed_steps: context[:completed_steps],
          failed_steps: context[:failed_steps],
          in_progress_steps: context[:in_progress_steps],
          pending_steps: context[:pending_steps],
          ready_steps: context[:ready_steps],
          progress_percentage: context[:completion_percentage],
          current_status: context[:execution_status],
          health_status: context[:health_status]
        }
      end

      # Execute custom SQL function
      #
      # @param function_name [String] Name of SQL function to execute
      # @param params [Array] Parameters to pass to function
      # @return [Array<Hash>] Query results
      def execute_function(function_name, *params)
        logger.debug("📊 SQL: Executing function: #{function_name} with #{params.length} params")

        param_placeholders = params.each_with_index.map { |_, i| "$#{i + 1}" }.join(', ')
        sql = "SELECT * FROM #{function_name}(#{param_placeholders})"

        result = connection.exec(sql, params)
        rows = result.map { |row| row.to_h.transform_keys(&:to_sym) }

        logger.debug("✅ SQL: Function #{function_name} returned #{rows.length} rows")
        rows
      rescue PG::Error => e
        logger.error("❌ SQL: Failed to execute function #{function_name}: #{e.message}")
        raise TaskerCore::Error, "Failed to execute function #{function_name}: #{e.message}"
      end

      # Close the database connection
      def close
        connection&.close
      end

      private

      # Create database connection
      def create_connection
        # Try to use TaskerCore configuration if available
        if defined?(TaskerCore::Config)
          config = TaskerCore::Config.instance
          if config.respond_to?(:database_url) && config.database_url
            return PG.connect(config.database_url)
          end
        end

        # Fall back to DATABASE_URL environment variable
        database_url = ENV['DATABASE_URL']
        if database_url
          PG.connect(database_url)
        else
          raise TaskerCore::Error, "No database connection available. Set DATABASE_URL or configure TaskerCore."
        end
      rescue PG::Error => e
        raise TaskerCore::Error, "Failed to connect to database: #{e.message}"
      end

      # Parse task execution context result
      # Matches get_task_execution_context() return schema
      def parse_task_execution_context(row)
        {
          task_id: row['task_id']&.to_i,
          named_task_id: row['named_task_id']&.to_i,
          status: row['status'],
          total_steps: row['total_steps']&.to_i,
          pending_steps: row['pending_steps']&.to_i,
          in_progress_steps: row['in_progress_steps']&.to_i,
          completed_steps: row['completed_steps']&.to_i,
          failed_steps: row['failed_steps']&.to_i,
          ready_steps: row['ready_steps']&.to_i,
          execution_status: row['execution_status'],
          recommended_action: row['recommended_action'],
          completion_percentage: row['completion_percentage']&.to_f,
          health_status: row['health_status']
        }
      end

      # Parse step readiness result
      # Matches get_step_readiness_status() return schema
      def parse_step_readiness(row)
        {
          workflow_step_id: row['workflow_step_id']&.to_i,
          task_id: row['task_id']&.to_i,
          named_step_id: row['named_step_id']&.to_i,
          name: row['name'],
          current_state: row['current_state'],
          dependencies_satisfied: row['dependencies_satisfied'] == 't',
          retry_eligible: row['retry_eligible'] == 't',
          ready_for_execution: row['ready_for_execution'] == 't',
          last_failure_at: parse_timestamp(row['last_failure_at']),
          next_retry_at: parse_timestamp(row['next_retry_at']),
          total_parents: row['total_parents']&.to_i,
          completed_parents: row['completed_parents']&.to_i,
          attempts: row['attempts']&.to_i,
          retry_limit: row['retry_limit']&.to_i,
          backoff_request_seconds: row['backoff_request_seconds']&.to_i,
          last_attempted_at: parse_timestamp(row['last_attempted_at'])
        }
      end

      # Parse dependency levels result
      # Matches calculate_dependency_levels() return schema
      def parse_dependency_levels(row)
        {
          workflow_step_id: row['workflow_step_id']&.to_i,
          dependency_level: row['dependency_level']&.to_i
        }
      end

      # Parse system health result
      # Matches get_system_health_counts_v01() return schema
      def parse_system_health(row)
        {
          total_tasks: row['total_tasks']&.to_i,
          pending_tasks: row['pending_tasks']&.to_i,
          in_progress_tasks: row['in_progress_tasks']&.to_i,
          complete_tasks: row['complete_tasks']&.to_i,
          error_tasks: row['error_tasks']&.to_i,
          cancelled_tasks: row['cancelled_tasks']&.to_i,
          total_steps: row['total_steps']&.to_i,
          pending_steps: row['pending_steps']&.to_i,
          in_progress_steps: row['in_progress_steps']&.to_i,
          complete_steps: row['complete_steps']&.to_i,
          error_steps: row['error_steps']&.to_i,
          retryable_error_steps: row['retryable_error_steps']&.to_i,
          exhausted_retry_steps: row['exhausted_retry_steps']&.to_i,
          in_backoff_steps: row['in_backoff_steps']&.to_i,
          active_connections: row['active_connections']&.to_i,
          max_connections: row['max_connections']&.to_i
        }
      end

      # Parse analytics metrics result
      # Matches get_analytics_metrics_v01() return schema
      def parse_analytics_metrics(row)
        {
          active_tasks_count: row['active_tasks_count']&.to_i,
          total_namespaces_count: row['total_namespaces_count']&.to_i,
          unique_task_types_count: row['unique_task_types_count']&.to_i,
          system_health_score: row['system_health_score']&.to_f,
          task_throughput: row['task_throughput']&.to_i,
          completion_count: row['completion_count']&.to_i,
          error_count: row['error_count']&.to_i,
          completion_rate: row['completion_rate']&.to_f,
          error_rate: row['error_rate']&.to_f,
          avg_task_duration: row['avg_task_duration']&.to_f,
          avg_step_duration: row['avg_step_duration']&.to_f,
          step_throughput: row['step_throughput']&.to_i,
          analysis_period_start: parse_timestamp(row['analysis_period_start']),
          calculated_at: parse_timestamp(row['calculated_at'])
        }
      end

      # Parse slowest steps result
      # Matches get_slowest_steps_v01() return schema
      def parse_slowest_steps(row)
        {
          workflow_step_id: row['workflow_step_id']&.to_i,
          task_id: row['task_id']&.to_i,
          step_name: row['step_name'],
          task_name: row['task_name'],
          namespace_name: row['namespace_name'],
          version: row['version'],
          duration_seconds: row['duration_seconds']&.to_f,
          attempts: row['attempts']&.to_i,
          created_at: parse_timestamp(row['created_at']),
          completed_at: parse_timestamp(row['completed_at']),
          retryable: row['retryable'] == 't',
          step_status: row['step_status']
        }
      end

      # Parse slowest tasks result
      # Matches get_slowest_tasks_v01() return schema
      def parse_slowest_tasks(row)
        {
          task_id: row['task_id']&.to_i,
          task_name: row['task_name'],
          namespace_name: row['namespace_name'],
          version: row['version'],
          duration_seconds: row['duration_seconds']&.to_f,
          step_count: row['step_count']&.to_i,
          completed_steps: row['completed_steps']&.to_i,
          error_steps: row['error_steps']&.to_i,
          created_at: parse_timestamp(row['created_at']),
          completed_at: parse_timestamp(row['completed_at']),
          initiator: row['initiator'],
          source_system: row['source_system']
        }
      end

      # Parse timestamp string to Time object
      def parse_timestamp(timestamp_str)
        return nil if timestamp_str.nil? || timestamp_str.empty?
        Time.parse(timestamp_str)
      rescue ArgumentError
        nil
      end
    end
  end
end
