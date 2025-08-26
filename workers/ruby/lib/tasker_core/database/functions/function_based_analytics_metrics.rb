# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Wrapper for the get_analytics_metrics SQL function
      #
      # This function provides comprehensive analytics metrics for performance monitoring,
      # including system overview, performance metrics, and duration calculations.
      #
      # @example Basic usage
      #   metrics = Tasker::Functions::FunctionBasedAnalyticsMetrics.call
      #   puts metrics.system_health_score
      #   puts metrics.completion_rate
      #
      # @example Time-bounded analysis
      #   since_time = 4.hours.ago
      #   metrics = Tasker::Functions::FunctionBasedAnalyticsMetrics.call(since_time)
      #   puts metrics.task_throughput
      #
      class FunctionBasedAnalyticsMetrics < FunctionWrapper
        # Analytics metrics result structure
        class AnalyticsMetrics < Dry::Struct
          # System overview metrics
          attribute :active_tasks_count, Types::Integer
          attribute :total_namespaces_count, Types::Integer
          attribute :unique_task_types_count, Types::Integer
          attribute :system_health_score, Types::Float

          # Performance metrics since timestamp
          attribute :task_throughput, Types::Integer
          attribute :completion_count, Types::Integer
          attribute :error_count, Types::Integer
          attribute :completion_rate, Types::Float
          attribute :error_rate, Types::Float

          # Duration metrics (in seconds)
          attribute :avg_task_duration, Types::Float
          attribute :avg_step_duration, Types::Float
          attribute :step_throughput, Types::Integer

          # Metadata
          attribute :analysis_period_start, Types::String
          attribute :calculated_at, Types::String
        end

        # Call the get_analytics_metrics SQL function
        #
        # @param since_timestamp [Time, nil] Start time for analysis (defaults to 1 hour ago in SQL)
        # @return [AnalyticsMetrics] Comprehensive analytics metrics
        # @raise [ActiveRecord::StatementInvalid] If the SQL function fails
        def self.call(since_timestamp = nil)
          if since_timestamp
            sql = 'SELECT * FROM get_analytics_metrics($1)'
            result = connection.select_all(sql, 'AnalyticsMetrics Load', [since_timestamp])
          else
            sql = 'SELECT * FROM get_analytics_metrics()'
            result = connection.select_all(sql, 'AnalyticsMetrics Load')
          end

          if result.empty?
            # Return zero values if no data (shouldn't happen but defensive)
            AnalyticsMetrics.new(
              active_tasks_count: 0,
              total_namespaces_count: 0,
              unique_task_types_count: 0,
              system_health_score: 1.0,
              task_throughput: 0,
              completion_count: 0,
              error_count: 0,
              completion_rate: 0.0,
              error_rate: 0.0,
              avg_task_duration: 0.0,
              avg_step_duration: 0.0,
              step_throughput: 0,
              analysis_period_start: (since_timestamp || 1.hour.ago).to_s,
              calculated_at: Time.current.to_s
            )
          else
            # Convert the result to a structured AnalyticsMetrics object
            row = result.first
            AnalyticsMetrics.new(
              active_tasks_count: row['active_tasks_count'].to_i,
              total_namespaces_count: row['total_namespaces_count'].to_i,
              unique_task_types_count: row['unique_task_types_count'].to_i,
              system_health_score: row['system_health_score'].to_f,
              task_throughput: row['task_throughput'].to_i,
              completion_count: row['completion_count'].to_i,
              error_count: row['error_count'].to_i,
              completion_rate: row['completion_rate'].to_f,
              error_rate: row['error_rate'].to_f,
              avg_task_duration: row['avg_task_duration'].to_f,
              avg_step_duration: row['avg_step_duration'].to_f,
              step_throughput: row['step_throughput'].to_i,
              analysis_period_start: row['analysis_period_start'].to_s,
              calculated_at: row['calculated_at'].to_s
            )
          end
        end
      end
    end
  end
end
