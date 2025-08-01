# frozen_string_literal: true

require_relative 'database/sql_functions'

module TaskerCore
  # Database module for SQL function access
  # 
  # This module provides Ruby access to PostgreSQL functions for status queries,
  # analytics, and dependency analysis without FFI coupling. This is the "read"
  # interface that complements the "write" interface of pgmq queues.
  module Database
    # Create a new SQL functions client
    # @param connection [PG::Connection] Optional database connection
    # @param logger [Logger] Optional logger
    # @return [SqlFunctions] New SQL functions client
    def self.create_sql_functions(connection: nil, logger: nil)
      SqlFunctions.new(connection: connection, logger: logger)
    end

    # Quick access to system health
    # @return [Hash] System health metrics
    def self.system_health
      client = create_sql_functions
      client.system_health_counts
    ensure
      client&.close
    end

    # Quick access to analytics metrics
    # @return [Hash] Analytics metrics
    def self.analytics
      client = create_sql_functions
      client.analytics_metrics
    ensure
      client&.close
    end

    # Check if a task is complete
    # @param task_id [Integer] Task ID to check
    # @return [Boolean] true if task is complete
    def self.task_complete?(task_id)
      client = create_sql_functions
      client.task_complete?(task_id)
    ensure
      client&.close
    end

    # Get task progress
    # @param task_id [Integer] Task ID to analyze
    # @return [Hash] Task progress information
    def self.task_progress(task_id)
      client = create_sql_functions
      client.task_progress(task_id)
    ensure
      client&.close
    end
  end
end