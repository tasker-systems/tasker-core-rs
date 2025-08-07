# frozen_string_literal: true

# typed: false

module TaskerCore
  module Database
    module Models
      # Abstract base class for all Tasker models that provides optional secondary database support.
      #
      # This class follows Rails multi-database conventions using the `connects_to` API.
      # When secondary database is enabled, it connects to a database named 'tasker' in database.yml.
      #
      # @example Basic usage with shared database (default)
      #   Tasker::Configuration.configuration do |config|
      #     config.database.enable_secondary_database = false
      #   end
      #
      # @example Using a dedicated Tasker database
      #   Tasker::Configuration.configuration do |config|
      #     config.database.enable_secondary_database = true
      #     config.database.name = :tasker
      #   end
      #
      #   # In database.yml:
      #   production:
      #     primary:
      #       database: my_primary_database
      #       adapter: postgresql
      #     tasker:
      #       database: my_tasker_database
      #       adapter: postgresql
      #
      class ApplicationRecord < ActiveRecord::Base
        self.abstract_class = true
        self.table_name_prefix = 'tasker_'
        def logger
          TaskerCore::Logging::Logger.instance
        end
      end
    end
  end
end
