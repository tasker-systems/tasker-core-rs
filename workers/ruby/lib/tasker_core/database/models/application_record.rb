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

        # UUID v7 generation for models with UUID primary keys
        before_create :generate_uuid_v7

        def logger
          TaskerCore::Logging::Logger.instance
        end

        private

        def generate_uuid_v7
          # Only generate UUID v7 for models with UUID primary key
          return unless self.class.attribute_types[self.class.primary_key]&.type == :uuid

          # Generate UUID v7 if primary key is not already set
          public_send("#{self.class.primary_key}=", SecureRandom.uuid_v7) if public_send(self.class.primary_key).nil?
        end
      end
    end
  end
end
