# frozen_string_literal: true

require 'pg'
require 'active_record'
require 'singleton'
require_relative '../config'

module TaskerCore
  module Database
    class Connection
      include Singleton

      attr_reader :connection

      def initialize
        establish_connection
        @connection = ActiveRecord::Base.connection.raw_connection
      end

      def execute(query)
        ActiveRecord::Base.connection.execute(query)
      end

      def close
        ActiveRecord::Base.connection.close
      end

      # Get the current database configuration
      def database_config
        @database_config ||= begin
          config = TaskerCore::Config.instance
          db_config = config.database_config

          # Check if database config is available
          if db_config.nil?
            raise TaskerCore::Errors::ConfigurationError,
                  'Database configuration not found. Ensure unified TOML config is loaded properly.'
          end
          db_config.to_h.deep_symbolize_keys
        end
      end

      # Get the current database name
      def database_name
        database_config[:database]
      end

      # Get the current database URL
      def database_url
        TaskerCore::Config.instance.database_url
      end

      # Check if connection is active
      def connected?
        ActiveRecord::Base.connected?
      end

      # Reconnect to database
      def reconnect!
        ActiveRecord::Base.connection.reconnect!
        @connection = ActiveRecord::Base.connection.raw_connection
      end

      private

      def establish_connection
        ActiveRecord::Base.establish_connection(database_config)
      rescue ActiveRecord::ConnectionNotEstablished => e
        raise TaskerCore::Errors::DatabaseError, "Failed to establish database connection: #{e.message}"
      rescue ActiveRecord::DatabaseConnectionError => e
        raise TaskerCore::Errors::DatabaseError, "Database connection error: #{e.message}"
      rescue TaskerCore::Errors::ConfigurationError => e
        raise TaskerCore::Errors::DatabaseError, "Database configuration error: #{e.message}"
      end
    end
  end
end
