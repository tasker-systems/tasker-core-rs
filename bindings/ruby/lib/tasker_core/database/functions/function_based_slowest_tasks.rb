# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Wrapper for the get_slowest_tasks SQL function
      #
      # This function returns the slowest tasks within a specified time period
      # with performance metrics and optional filtering by namespace, task name, and version.
      #
      # @example Basic usage
      #   slowest_tasks = Tasker::Functions::FunctionBasedSlowestTasks.call
      #   slowest_tasks.each { |task| puts "#{task.task_name}: #{task.duration_seconds}s" }
      #
      # @example With filters and time period
      #   since_time = 24.hours.ago
      #   filtered_tasks = Tasker::Functions::FunctionBasedSlowestTasks.call(
      #     since_timestamp: since_time,
      #     limit_count: 5,
      #     namespace_filter: 'payments',
      #     task_name_filter: 'process_payment'
      #   )
      #
      class FunctionBasedSlowestTasks < FunctionWrapper
        # Individual slowest task result structure
        class SlowestTask < Dry::Struct
          attribute :task_uuid, Types::String
          attribute :task_name, Types::String
          attribute :namespace_name, Types::String
          attribute :version, Types::String
          attribute :duration_seconds, Types::Float
          attribute :step_count, Types::Integer
          attribute :completed_steps, Types::Integer
          attribute :error_steps, Types::Integer
          attribute :created_at, Types::Params::DateTime
          attribute :completed_at, Types::Params::DateTime.optional
          attribute :initiator, Types::String.optional
          attribute :source_system, Types::String.optional
        end

        # Call the get_slowest_tasks SQL function
        #
        # @param since_timestamp [Time, nil] Start time for analysis (defaults to 24 hours ago in SQL)
        # @param limit_count [Integer] Maximum number of results to return (default: 10)
        # @param namespace_filter [String, nil] Filter by namespace name
        # @param task_name_filter [String, nil] Filter by task name
        # @param version_filter [String, nil] Filter by task version
        # @return [Array<SlowestTask>] Array of slowest task results
        # @raise [ActiveRecord::StatementInvalid] If the SQL function fails
        def self.call(since_timestamp: nil, limit_count: 10, namespace_filter: nil, task_name_filter: nil,
                      version_filter: nil)
          # Build SQL with proper parameter binding
          sql = 'SELECT * FROM get_slowest_tasks($1, $2, $3, $4, $5)'
          binds = [
            since_timestamp,
            limit_count,
            namespace_filter,
            task_name_filter,
            version_filter
          ]

          result = connection.select_all(sql, 'SlowestTasks Load', binds)

          result.map do |row|
            SlowestTask.new(
              task_uuid: row['task_uuid'].to_s,
              task_name: row['task_name'].to_s,
              namespace_name: row['namespace_name'].to_s,
              version: row['version'].to_s,
              duration_seconds: row['duration_seconds'].to_f,
              step_count: row['step_count'].to_i,
              completed_steps: row['completed_steps'].to_i,
              error_steps: row['error_steps'].to_i,
              created_at: Time.zone.parse(row['created_at'].to_s),
              completed_at: row['completed_at'] ? Time.zone.parse(row['completed_at'].to_s) : nil,
              initiator: row['initiator'],
              source_system: row['source_system']
            )
          end
        end
      end
    end
  end
end
