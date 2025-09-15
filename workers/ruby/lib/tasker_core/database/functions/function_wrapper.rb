# frozen_string_literal: true

module TaskerCore
  module Database
    module Functions
      # Utility class for wrapping SQL function results in ActiveRecord-like objects
      class FunctionWrapper
        include ActiveModel::Model
        include ActiveModel::Attributes

        def self.from_sql_function(sql, binds = [], name = nil)
          # Convert Ruby arrays to PostgreSQL array format for bind parameters
          processed_binds = binds.map { |bind| convert_array_bind(bind) }
          results = connection.select_all(sql, name, processed_binds)
          results.map { |row| new(row) }
        end

        def self.single_from_sql_function(sql, binds = [], name = nil)
          # Convert Ruby arrays to PostgreSQL array format for bind parameters
          processed_binds = binds.map { |bind| convert_array_bind(bind) }
          result = connection.select_one(sql, name, processed_binds)
          result ? new(result) : nil
        rescue ActiveRecord::ConnectionNotEstablished => e
          # Handle connection issues gracefully during shutdown/teardown
          TaskerCore::Logging::Logger.instance.warn("⚠️ DATABASE: Connection not established during function call (likely shutdown): #{e.message}")
          nil
        rescue PG::UnableToSend => e
          # Handle PostgreSQL connection issues
          TaskerCore::Logging::Logger.instance.warn("⚠️ DATABASE: Unable to send query (likely connection closed): #{e.message}")
          nil
        end

        def self.convert_array_bind(bind)
          if bind.is_a?(Array)
            # Convert Ruby array to PostgreSQL array format
            "{#{bind.join(',')}}"
          else
            bind
          end
        end

        def readonly?
          true
        end

        def self.connection
          ActiveRecord::Base.connection
        end
      end
    end
  end
end
