# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database::Functions
    # Wrapper for the get_system_health_counts_v01 SQL function
    #
    # This function provides comprehensive system health metrics in a single query,
    # returning counts for tasks, steps, retry states, and database connections.
    #
    # @example Basic usage
    #   health_data = Tasker::Functions::FunctionBasedSystemHealthCounts.call
    #   puts health_data.total_tasks
    #   puts health_data.active_connections
    #
    # @example Error handling
    #   begin
    #     health_data = Tasker::Functions::FunctionBasedSystemHealthCounts.call
    #   rescue ActiveRecord::StatementInvalid => e
    #     Rails.logger.error "Health check failed: #{e.message}"
    #   end
    class FunctionBasedSystemHealthCounts < FunctionWrapper
      # Health metrics result structure
      class HealthMetrics < Dry::Struct
        # Task counts
        attribute :total_tasks, Types::Integer
        attribute :pending_tasks, Types::Integer
        attribute :in_progress_tasks, Types::Integer
        attribute :complete_tasks, Types::Integer
        attribute :error_tasks, Types::Integer
        attribute :cancelled_tasks, Types::Integer

        # Step counts
        attribute :total_steps, Types::Integer
        attribute :pending_steps, Types::Integer
        attribute :in_progress_steps, Types::Integer
        attribute :complete_steps, Types::Integer
        attribute :error_steps, Types::Integer

        # Retry-specific counts
        attribute :retryable_error_steps, Types::Integer
        attribute :exhausted_retry_steps, Types::Integer
        attribute :in_backoff_steps, Types::Integer

        # Database metrics
        attribute :active_connections, Types::Integer
        attribute :max_connections, Types::Integer
      end

      # Call the get_system_health_counts_v01 SQL function
      #
      # @return [HealthMetrics] Health metrics with all system counts
      # @raise [ActiveRecord::StatementInvalid] If the SQL function fails
      def self.call
        sql = 'SELECT * FROM get_system_health_counts_v01()'
        result = connection.select_all(sql, 'SystemHealthCounts Load')

        if result.empty?
          # Return zero values if no data (shouldn't happen but defensive)
          HealthMetrics.new(
            total_tasks: 0, pending_tasks: 0, in_progress_tasks: 0,
            complete_tasks: 0, error_tasks: 0, cancelled_tasks: 0,
            total_steps: 0, pending_steps: 0, in_progress_steps: 0,
            complete_steps: 0, error_steps: 0, retryable_error_steps: 0,
            exhausted_retry_steps: 0, in_backoff_steps: 0,
            active_connections: 0, max_connections: 0
          )
        else
          # Convert the result to a structured HealthMetrics object
          row = result.first
          HealthMetrics.new(
            total_tasks: row['total_tasks'].to_i,
            pending_tasks: row['pending_tasks'].to_i,
            in_progress_tasks: row['in_progress_tasks'].to_i,
            complete_tasks: row['complete_tasks'].to_i,
            error_tasks: row['error_tasks'].to_i,
            cancelled_tasks: row['cancelled_tasks'].to_i,
            total_steps: row['total_steps'].to_i,
            pending_steps: row['pending_steps'].to_i,
            in_progress_steps: row['in_progress_steps'].to_i,
            complete_steps: row['complete_steps'].to_i,
            error_steps: row['error_steps'].to_i,
            retryable_error_steps: row['retryable_error_steps'].to_i,
            exhausted_retry_steps: row['exhausted_retry_steps'].to_i,
            in_backoff_steps: row['in_backoff_steps'].to_i,
            active_connections: row['active_connections'].to_i,
            max_connections: row['max_connections'].to_i
          )
        end
      end
    end
  end
end
