# frozen_string_literal: true

require 'statesman/adapters/active_record'

# Configure Statesman to use ActiveRecord adapter for persistence
Statesman.configure do
  storage_adapter(Statesman::Adapters::ActiveRecord)
end
