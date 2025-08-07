# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Wrapper for the get_slowest_steps_v01 SQL function
      #
      # This function returns the slowest workflow steps within a specified time period
      # with duration metrics and optional filtering by namespace, task name, and version.
      #
      # @example Basic usage
      #   slowest_steps = Tasker::Functions::FunctionBasedSlowestSteps.call
      #   slowest_steps.each { |step| puts "#{step.step_name}: #{step.duration_seconds}s" }
      #
      # @example With filters and time period
      #   since_time = 24.hours.ago
      #   filtered_steps = Tasker::Functions::FunctionBasedSlowestSteps.call(
      #     since_timestamp: since_time,
      #     limit_count: 15,
      #     namespace_filter: 'inventory',
      #     task_name_filter: 'update_stock'
      #   )
      #
      class FunctionBasedSlowestSteps < FunctionWrapper
        # Individual slowest step result structure
        class SlowestStep < Dry::Struct
          attribute :workflow_step_id, Types::Integer
          attribute :task_id, Types::Integer
          attribute :step_name, Types::String
          attribute :task_name, Types::String
          attribute :namespace_name, Types::String
          attribute :version, Types::String
          attribute :duration_seconds, Types::Float
          attribute :attempts, Types::Integer
          attribute :created_at, Types::Params::DateTime
          attribute :completed_at, Types::Params::DateTime
          attribute :retryable, Types::Bool
          attribute :step_status, Types::String
        end

        # Call the get_slowest_steps_v01 SQL function
        #
        # @param since_timestamp [Time, nil] Start time for analysis (defaults to 24 hours ago in SQL)
        # @param limit_count [Integer] Maximum number of results to return (default: 10)
        # @param namespace_filter [String, nil] Filter by namespace name
        # @param task_name_filter [String, nil] Filter by task name
        # @param version_filter [String, nil] Filter by task version
        # @return [Array<SlowestStep>] Array of slowest step results
        # @raise [ActiveRecord::StatementInvalid] If the SQL function fails
        def self.call(since_timestamp: nil, limit_count: 10, namespace_filter: nil, task_name_filter: nil,
                      version_filter: nil)
          # Build SQL with proper parameter binding
          sql = 'SELECT * FROM get_slowest_steps_v01($1, $2, $3, $4, $5)'
          binds = [
            since_timestamp,
            limit_count,
            namespace_filter,
            task_name_filter,
            version_filter
          ]

          result = connection.select_all(sql, 'SlowestSteps Load', binds)

          result.map do |row|
            SlowestStep.new(
              workflow_step_id: row['workflow_step_id'].to_i,
              task_id: row['task_id'].to_i,
              step_name: row['step_name'].to_s,
              task_name: row['task_name'].to_s,
              namespace_name: row['namespace_name'].to_s,
              version: row['version'].to_s,
              duration_seconds: row['duration_seconds'].to_f,
              attempts: row['attempts'].to_i,
              created_at: Time.zone.parse(row['created_at'].to_s),
              completed_at: Time.zone.parse(row['completed_at'].to_s),
              retryable: ['t', true].include?(row['retryable']),
              step_status: row['step_status'].to_s
            )
          end
        end
      end
    end
  end
end
