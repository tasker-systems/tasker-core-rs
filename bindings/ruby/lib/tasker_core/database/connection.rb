# frozen_string_literal: true

require 'pg'
require 'singleton'

module TaskerCore
  module Database
    # Simple singleton database connection for registry operations
    #
    # This provides a lightweight connection wrapper specifically for 
    # the step handler registry to query named_tasks and configurations.
    class Connection
      include Singleton

      attr_reader :connection

      def initialize
        @connection = create_connection
      end

      # Execute a query and return one result
      #
      # @param sql [String] SQL query
      # @param params [Array] Query parameters
      # @return [Hash, nil] Single result hash or nil
      def query_one(sql, params = [])
        result = @connection.exec_params(sql, params)
        return nil if result.ntuples == 0
        
        result[0]
      rescue PG::Error => e
        raise TaskerCore::Error.new("Database query failed: #{e.message}")
      ensure
        result&.clear
      end

      # Execute a query and return all results
      #
      # @param sql [String] SQL query  
      # @param params [Array] Query parameters
      # @return [Array<Hash>] Array of result hashes
      def query_all(sql, params = [])
        result = @connection.exec_params(sql, params)
        result.to_a
      rescue PG::Error => e
        raise TaskerCore::Error.new("Database query failed: #{e.message}")
      ensure
        result&.clear
      end

      # Test if connection is still valid
      #
      # @return [Boolean] true if connection is healthy
      def healthy?
        @connection.exec('SELECT 1').ntuples == 1
      rescue
        false
      end

      # Close and recreate connection
      def reconnect!
        @connection&.close
        @connection = create_connection
      end

      private

      def create_connection
        database_url = ENV['DATABASE_URL'] || 'postgresql://localhost/tasker_development'
        PG.connect(database_url)
      rescue PG::Error => e
        raise TaskerCore::Error.new("Failed to connect to database: #{e.message}")
      end
    end
  end
end