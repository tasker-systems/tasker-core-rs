# frozen_string_literal: true

require_relative 'database/connection'
require_relative 'database/models'
require_relative 'database/functions'

module TaskerCore
  module Database
    # Factory method for creating SQL functions interface
    # This provides backward compatibility with the old SqlFunctions class
    # while using the new function-based architecture
    def self.create_sql_functions
      SqlFunctionsCompat.new
    end

    # Compatibility class that provides the same interface as the old SqlFunctions
    # but delegates to the new function-based architecture
    class SqlFunctionsCompat
      def close
        # No-op - ActiveRecord manages connections
      end

      # Add methods as needed for backward compatibility
      # Most functionality is now accessed directly through the function classes
    end
  end
end
